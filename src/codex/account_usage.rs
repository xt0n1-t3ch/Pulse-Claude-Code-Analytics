use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, TimeZone, Utc};
use codex_presence_core::{
    CreditBalance, EffectiveLimitSelection, RateLimitEnvelope, RateLimits, UsageSnapshot,
    UsageWindow, classify_limit_scope, select_session_envelope_global_first,
    usage_snapshot_from_envelopes,
};
use serde::Deserialize;
use serde_json::Value;

use crate::codex::util::silent_command;

const CACHE_TTL: Duration = Duration::from_secs(30);
const RETRY_BACKOFF: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(8);

/// One coherent account quota snapshot from Codex's authenticated app-server.
/// The observation time belongs to the read itself because the protocol does
/// not attach timestamps to `account/rateLimits/read` responses.
#[derive(Clone, Debug)]
pub struct AccountUsageReading {
    pub envelopes: Vec<RateLimitEnvelope>,
    pub observed_at: DateTime<Utc>,
}

impl AccountUsageReading {
    pub fn usage_snapshot(&self) -> UsageSnapshot {
        usage_snapshot_from_envelopes("codex", "Codex account API", &self.envelopes)
    }

    pub fn effective_limits(&self) -> Option<EffectiveLimitSelection> {
        effective_limits_from_envelopes(&self.envelopes)
    }
}

/// Short-lived cache around the authenticated read. Pulse polls every five
/// seconds; spawning a new app-server for every UI tick would be wasteful and
/// would not make provider quota more accurate than this 30-second cadence.
#[derive(Default)]
pub struct AccountUsageManager {
    cached: Option<AccountUsageReading>,
    cached_at: Option<Instant>,
    last_attempt: Option<Instant>,
}

impl AccountUsageManager {
    pub fn get_usage(&mut self, force: bool) -> Result<AccountUsageReading> {
        let now = Instant::now();
        if !force
            && let (Some(cached), Some(cached_at)) = (&self.cached, self.cached_at)
            && now.duration_since(cached_at) < CACHE_TTL
        {
            return Ok(cached.clone());
        }

        if !force
            && let Some(last_attempt) = self.last_attempt
            && now.duration_since(last_attempt) < RETRY_BACKOFF
        {
            bail!("Codex account quota retry is cooling down");
        }

        self.last_attempt = Some(now);
        let reading = query_account_usage(RESPONSE_TIMEOUT)?;
        self.cached = Some(reading.clone());
        self.cached_at = Some(now);
        Ok(reading)
    }
}

pub fn effective_limits_from_envelopes(
    envelopes: &[RateLimitEnvelope],
) -> Option<EffectiveLimitSelection> {
    let selected = select_session_envelope_global_first(envelopes)?;
    Some(EffectiveLimitSelection {
        source_session_id: "account/rateLimits/read".to_string(),
        source_limit_id: selected.limit_id.clone(),
        source_scope: selected.scope,
        observed_at: selected.observed_at,
        limits: selected.limits,
        credits: selected.credits,
    })
}

/// JSONL is a fallback only while its quota event is genuinely recent. This
/// prevents an old global bucket from looking live after Codex starts emitting
/// `rate_limits: null` in newer token events.
pub fn fresh_envelopes(
    envelopes: impl IntoIterator<Item = RateLimitEnvelope>,
    now: DateTime<Utc>,
    max_age: chrono::Duration,
) -> Vec<RateLimitEnvelope> {
    envelopes
        .into_iter()
        .filter(|item| {
            item.observed_at
                .is_some_and(|observed| observed <= now && now - observed <= max_age)
        })
        .collect()
}

fn query_account_usage(timeout: Duration) -> Result<AccountUsageReading> {
    let mut child = codex_app_server_command()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start Codex app-server quota reader")?;

    let result = (|| {
        let mut stdin = child
            .stdin
            .take()
            .context("Codex app-server stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("Codex app-server stdout unavailable")?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(line) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        write_json_line(
            &mut stdin,
            &serde_json::json!({
                "id": 1,
                "method": "initialize",
                "params": { "clientInfo": { "name": "pulse", "version": env!("CARGO_PKG_VERSION") } }
            }),
        )?;
        let initialized = read_response(&rx, 1, timeout)?;
        if initialized.get("error").is_some() {
            bail!("Codex app-server rejected initialization");
        }

        write_json_line(
            &mut stdin,
            &serde_json::json!({ "method": "initialized", "params": {} }),
        )?;
        write_json_line(
            &mut stdin,
            &serde_json::json!({ "id": 2, "method": "account/rateLimits/read", "params": null }),
        )?;
        let response = read_response(&rx, 2, timeout)?;
        parse_rate_limits_response(&response.to_string(), Utc::now())
    })();

    stop_owned_child(&mut child);
    result
}

fn codex_app_server_command() -> Command {
    #[cfg(windows)]
    {
        // `codex` is commonly an npm .cmd shim on Windows. CreateProcess does
        // not execute batch files directly, so use cmd.exe with a fixed command.
        let mut command = silent_command("cmd.exe");
        command.args(["/d", "/s", "/c", "codex app-server --stdio"]);
        command
    }
    #[cfg(not(windows))]
    {
        let mut command = silent_command("codex");
        command.args(["app-server", "--stdio"]);
        command
    }
}

fn write_json_line(stdin: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *stdin, value)
        .context("failed to encode Codex app-server request")?;
    stdin
        .write_all(b"\n")
        .context("failed to write Codex app-server request")?;
    stdin
        .flush()
        .context("failed to flush Codex app-server request")
}

fn read_response(rx: &Receiver<String>, id: i64, timeout: Duration) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("Codex app-server response timed out");
        }
        let line = rx
            .recv_timeout(remaining)
            .context("Codex app-server response stream closed")?;
        let value: Value =
            serde_json::from_str(&line).context("Codex app-server emitted invalid JSON")?;
        if value.get("id").and_then(Value::as_i64) == Some(id) {
            return Ok(value);
        }
    }
}

fn stop_owned_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[derive(Deserialize)]
struct RpcResponse {
    result: Option<AccountRateLimitsResult>,
    error: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountRateLimitsResult {
    rate_limits: WireRateLimitSnapshot,
    rate_limits_by_limit_id: Option<BTreeMap<String, WireRateLimitSnapshot>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRateLimitSnapshot {
    limit_id: Option<String>,
    limit_name: Option<String>,
    plan_type: Option<String>,
    primary: Option<WireUsageWindow>,
    secondary: Option<WireUsageWindow>,
    credits: Option<WireCredits>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireUsageWindow {
    used_percent: f64,
    window_duration_mins: Option<u64>,
    resets_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireCredits {
    balance: Option<String>,
    has_credits: bool,
    unlimited: bool,
}

pub fn parse_rate_limits_response(
    response: &str,
    observed_at: DateTime<Utc>,
) -> Result<AccountUsageReading> {
    let rpc: RpcResponse =
        serde_json::from_str(response).context("failed to decode Codex account quota response")?;
    if let Some(error) = rpc.error {
        return Err(anyhow!("Codex account quota request failed: {error}"));
    }
    let result = rpc
        .result
        .context("Codex account quota response has no result")?;
    let snapshots: Vec<WireRateLimitSnapshot> = match result.rate_limits_by_limit_id {
        Some(by_id) if !by_id.is_empty() => by_id.into_values().collect(),
        _ => vec![result.rate_limits],
    };

    let envelopes: Vec<RateLimitEnvelope> = snapshots
        .into_iter()
        .filter_map(|snapshot| wire_envelope(snapshot, observed_at))
        .collect();
    if !envelopes
        .iter()
        .any(|item| item.limits.primary.is_some() || item.limits.secondary.is_some())
    {
        bail!("Codex account quota response contains no quota windows");
    }

    Ok(AccountUsageReading {
        envelopes,
        observed_at,
    })
}

fn wire_envelope(
    snapshot: WireRateLimitSnapshot,
    observed_at: DateTime<Utc>,
) -> Option<RateLimitEnvelope> {
    let primary = snapshot.primary.and_then(wire_window);
    let secondary = snapshot.secondary.and_then(wire_window);
    let credits = snapshot.credits.map(|item| CreditBalance {
        balance: item.balance,
        has_credits: item.has_credits,
        unlimited: item.unlimited,
    });
    if primary.is_none() && secondary.is_none() && credits.is_none() {
        return None;
    }
    Some(RateLimitEnvelope {
        scope: classify_limit_scope(snapshot.limit_id.as_deref()),
        limit_id: snapshot.limit_id,
        limit_name: snapshot.limit_name,
        plan_type: snapshot.plan_type,
        observed_at: Some(observed_at),
        limits: RateLimits { primary, secondary },
        credits,
    })
}

fn wire_window(window: WireUsageWindow) -> Option<UsageWindow> {
    let window_minutes = window.window_duration_mins?;
    if window_minutes == 0 || !window.used_percent.is_finite() {
        return None;
    }
    let used_percent = window.used_percent.clamp(0.0, 100.0);
    Some(UsageWindow {
        used_percent,
        remaining_percent: (100.0 - used_percent).clamp(0.0, 100.0),
        window_minutes,
        resets_at: window
            .resets_at
            .and_then(|timestamp| Utc.timestamp_opt(timestamp, 0).single()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freshness_filter_drops_yesterdays_jsonl_quota() {
        let now = Utc.timestamp_opt(2_000, 0).single().unwrap();
        let stale = RateLimitEnvelope {
            observed_at: Utc.timestamp_opt(1_000, 0).single(),
            ..RateLimitEnvelope::default()
        };
        let fresh = RateLimitEnvelope {
            observed_at: Utc.timestamp_opt(1_950, 0).single(),
            ..RateLimitEnvelope::default()
        };

        let filtered = fresh_envelopes(vec![stale, fresh], now, chrono::Duration::minutes(15));
        assert_eq!(filtered.len(), 1);
        assert_eq!(
            filtered[0].observed_at,
            Utc.timestamp_opt(1_950, 0).single()
        );
    }

    #[test]
    fn owned_query_command_is_non_interactive() {
        let command = codex_app_server_command();
        let debug = format!("{command:?}");
        assert!(debug.contains("app-server"));
        assert!(debug.contains("stdio"));
    }
}
