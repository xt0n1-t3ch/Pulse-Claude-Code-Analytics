use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use discord_rich_presence::activity::{Activity, Assets, Timestamps};
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::config::PresenceConfig;
use crate::session::{ActivityKind, ClaudeSessionSnapshot, RateLimits};
use crate::usage::UsageData;
use crate::util::{format_cost, format_tokens};

pub struct DiscordPresence {
    client_id: Option<String>,
    client: Option<DiscordIpcClient>,
    last_status: String,
    last_sent: Option<(String, String)>,
    last_publish_at: Option<Instant>,
    known_asset_keys: Option<HashSet<String>>,
    last_asset_refresh_at: Option<Instant>,
    // Keepalive fields
    last_heartbeat_at: Option<Instant>,
    reconnect_backoff: Duration,
    last_reconnect_attempt: Option<Instant>,
    consecutive_errors: u32,
    idle_start_epoch: Option<i64>,
}

const DISCORD_MIN_PUBLISH_INTERVAL: Duration = Duration::from_secs(2);
const DISCORD_ASSET_REFRESH_INTERVAL: Duration = Duration::from_secs(300);
const DISCORD_ASSET_FETCH_TIMEOUT: Duration = Duration::from_secs(2);

/// Force re-send activity at this interval even if payload unchanged,
/// preventing Discord from dropping the IPC connection due to inactivity.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Minimum backoff between reconnection attempts after IPC errors.
const RECONNECT_MIN_BACKOFF: Duration = Duration::from_secs(5);

/// Maximum backoff cap for exponential reconnection backoff.
const RECONNECT_MAX_BACKOFF: Duration = Duration::from_secs(60);

impl DiscordPresence {
    pub fn new(client_id: Option<String>) -> Self {
        let last_status = if client_id.is_some() {
            "Disconnected".to_string()
        } else {
            "Missing CC_DISCORD_CLIENT_ID".to_string()
        };
        Self {
            client_id,
            client: None,
            last_status,
            last_sent: None,
            last_publish_at: None,
            known_asset_keys: None,
            last_asset_refresh_at: None,
            last_heartbeat_at: None,
            reconnect_backoff: RECONNECT_MIN_BACKOFF,
            last_reconnect_attempt: None,
            consecutive_errors: 0,
            idle_start_epoch: None,
        }
    }

    pub fn status(&self) -> &str {
        &self.last_status
    }

    pub fn update(
        &mut self,
        active_session: Option<&ClaudeSessionSnapshot>,
        effective_limits: Option<&RateLimits>,
        api_usage: Option<&UsageData>,
        config: &PresenceConfig,
    ) -> Result<()> {
        if self.client_id.is_none() {
            self.last_status = "Missing CC_DISCORD_CLIENT_ID".to_string();
            return Ok(());
        }

        // Try to connect; silently skip during backoff period
        if let Err(_err) = self.ensure_connected() {
            return Ok(());
        }
        if self.client.is_none() {
            return Ok(());
        }

        self.refresh_asset_keys_if_needed();

        let needs_heartbeat = self
            .last_heartbeat_at
            .map(|t| t.elapsed() >= HEARTBEAT_INTERVAL)
            .unwrap_or(true);

        match active_session {
            Some(session) => {
                self.idle_start_epoch = None;
                let (details, state) = presence_lines(session, effective_limits, api_usage, config);
                let payload = (details.clone(), state.clone());

                // Allow re-send when heartbeat interval has elapsed (keepalive)
                if self.last_sent.as_ref() == Some(&payload) && !needs_heartbeat {
                    self.last_status = "Connected".to_string();
                    return Ok(());
                }
                if let Some(last_publish) = self.last_publish_at {
                    if last_publish.elapsed() < DISCORD_MIN_PUBLISH_INTERVAL {
                        self.last_status = "Connected".to_string();
                        return Ok(());
                    }
                }

                let (small_image_key, small_text) = small_asset_for_activity(session, config);
                let resolved_large_key = resolve_image_key(
                    &config.display.large_image_key,
                    self.known_asset_keys.as_ref(),
                );
                let resolved_small_key =
                    resolve_image_key(&small_image_key, self.known_asset_keys.as_ref());
                let (large_image_key, small_image_key) =
                    normalize_asset_pair(resolved_large_key, resolved_small_key);
                let activity = build_activity(
                    &details,
                    &state,
                    session,
                    large_image_key.as_deref(),
                    non_empty_trimmed(&config.display.large_text),
                    small_image_key.as_deref(),
                    non_empty_trimmed(&small_text),
                );
                let client = self
                    .client
                    .as_mut()
                    .ok_or_else(|| anyhow!("Discord IPC client unexpectedly missing"))?;
                if let Err(err) = client
                    .set_activity(activity)
                    .context("failed to set Discord activity")
                {
                    self.handle_ipc_error(&err.to_string());
                    return Err(err);
                }
                self.last_sent = Some(payload);
                self.last_publish_at = Some(Instant::now());
                self.last_heartbeat_at = Some(Instant::now());
                self.last_status = "Connected".to_string();
            }
            None => {
                // Show idle presence instead of clearing — keeps Rich Presence visible
                let idle_start = *self
                    .idle_start_epoch
                    .get_or_insert_with(|| Utc::now().timestamp().max(0));

                let details = "Claude Code".to_string();
                let state = "Waiting for session".to_string();
                let payload = (details.clone(), state.clone());

                if self.last_sent.as_ref() == Some(&payload) && !needs_heartbeat {
                    self.last_status = "Connected (idle)".to_string();
                    return Ok(());
                }
                if let Some(last_publish) = self.last_publish_at {
                    if last_publish.elapsed() < DISCORD_MIN_PUBLISH_INTERVAL {
                        self.last_status = "Connected (idle)".to_string();
                        return Ok(());
                    }
                }

                let resolved_large_key = resolve_image_key(
                    &config.display.large_image_key,
                    self.known_asset_keys.as_ref(),
                );

                let mut activity = Activity::new()
                    .details(&details)
                    .state(&state)
                    .timestamps(Timestamps::new().start(idle_start));

                if let Some(ref key) = resolved_large_key {
                    let mut assets = Assets::new().large_image(key.as_str());
                    if let Some(text) = non_empty_trimmed(&config.display.large_text) {
                        assets = assets.large_text(text);
                    }
                    activity = activity.assets(assets);
                }

                let client = self
                    .client
                    .as_mut()
                    .ok_or_else(|| anyhow!("Discord IPC client unexpectedly missing"))?;
                if let Err(err) = client
                    .set_activity(activity)
                    .context("failed to set Discord idle activity")
                {
                    self.handle_ipc_error(&err.to_string());
                    return Err(err);
                }
                self.last_sent = Some(payload);
                self.last_publish_at = Some(Instant::now());
                self.last_heartbeat_at = Some(Instant::now());
                self.last_status = "Connected (idle)".to_string();
            }
        }

        Ok(())
    }

    pub fn shutdown(&mut self) {
        let _ = self.clear_activity();
        if let Some(client) = self.client.as_mut() {
            let _ = client.close();
        }
        self.client = None;
        self.last_sent = None;
        self.last_publish_at = None;
        self.last_heartbeat_at = None;
        self.last_asset_refresh_at = None;
        self.idle_start_epoch = None;
        self.consecutive_errors = 0;
        self.reconnect_backoff = RECONNECT_MIN_BACKOFF;
        if self.client_id.is_some() {
            self.last_status = "Disconnected".to_string();
        }
    }

    fn clear_activity(&mut self) -> Result<()> {
        if let Some(client) = self.client.as_mut() {
            if let Err(err) = client
                .clear_activity()
                .context("failed to clear Discord activity")
            {
                self.handle_ipc_error(&err.to_string());
                return Err(err);
            }
        }
        Ok(())
    }

    fn ensure_connected(&mut self) -> Result<()> {
        if self.client.is_some() {
            return Ok(());
        }

        let Some(client_id) = self.client_id.clone() else {
            return Ok(());
        };

        // Respect reconnection backoff — skip silently if too soon
        if let Some(last_attempt) = self.last_reconnect_attempt {
            if last_attempt.elapsed() < self.reconnect_backoff {
                return Ok(());
            }
        }

        self.last_reconnect_attempt = Some(Instant::now());

        let mut client = DiscordIpcClient::new(&client_id);
        match client
            .connect()
            .context("failed to connect to Discord IPC (is Discord desktop open?)")
        {
            Ok(()) => {
                self.client = Some(client);
                self.consecutive_errors = 0;
                self.reconnect_backoff = RECONNECT_MIN_BACKOFF;
                self.last_sent = None; // Force re-send after reconnect
                self.last_heartbeat_at = None;
                self.last_status = "Connected".to_string();
                Ok(())
            }
            Err(err) => {
                self.increase_backoff();
                self.last_status =
                    format!("Reconnecting in {}s...", self.reconnect_backoff.as_secs());
                Err(err)
            }
        }
    }

    fn refresh_asset_keys_if_needed(&mut self) {
        let Some(client_id) = self.client_id.as_deref() else {
            return;
        };
        if let Some(last_refresh) = self.last_asset_refresh_at {
            if last_refresh.elapsed() < DISCORD_ASSET_REFRESH_INTERVAL {
                return;
            }
        }

        self.last_asset_refresh_at = Some(Instant::now());
        if let Ok(asset_keys) = fetch_discord_asset_keys(client_id) {
            self.known_asset_keys = Some(asset_keys);
        }
    }

    fn handle_ipc_error(&mut self, message: &str) {
        self.client = None;
        self.increase_backoff();
        self.last_status = format!("Discord error: {}", compact_error(message));
    }

    fn increase_backoff(&mut self) {
        self.consecutive_errors = self.consecutive_errors.saturating_add(1);
        let backoff_secs = RECONNECT_MIN_BACKOFF
            .as_secs()
            .saturating_mul(1u64 << self.consecutive_errors.min(4));
        self.reconnect_backoff =
            Duration::from_secs(backoff_secs.min(RECONNECT_MAX_BACKOFF.as_secs()));
    }
}

fn compact_error(input: &str) -> String {
    const MAX: usize = 96;
    if input.len() <= MAX {
        return input.to_string();
    }
    let mut end = MAX.saturating_sub(3);
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &input[..end])
}

fn build_activity<'a>(
    details: &'a str,
    state: &'a str,
    session: &'a ClaudeSessionSnapshot,
    large_image_key: Option<&'a str>,
    large_text: Option<&'a str>,
    small_image_key: Option<&'a str>,
    small_text: Option<&'a str>,
) -> Activity<'a> {
    let start = session
        .started_at
        .unwrap_or_else(Utc::now)
        .timestamp()
        .max(0);

    let mut activity = Activity::new()
        .details(details)
        .state(state)
        .timestamps(Timestamps::new().start(start));

    let mut assets = Assets::new();
    let mut has_assets = false;

    if let Some(image_key) = large_image_key {
        assets = assets.large_image(image_key);
        has_assets = true;
        if let Some(text) = large_text {
            assets = assets.large_text(text);
        }
    }

    if let Some(image_key) = small_image_key {
        assets = assets.small_image(image_key);
        has_assets = true;
        if let Some(text) = small_text {
            assets = assets.small_text(text);
        }
    }

    if has_assets {
        activity = activity.assets(assets);
    }

    activity
}

fn presence_lines(
    session: &ClaudeSessionSnapshot,
    effective_limits: Option<&RateLimits>,
    api_usage: Option<&UsageData>,
    config: &PresenceConfig,
) -> (String, String) {
    if config.privacy.enabled {
        return (
            "Using Claude Code".to_string(),
            "In a coding session".to_string(),
        );
    }

    let project_label = if config.privacy.show_project_name {
        if config.privacy.show_git_branch {
            if let Some(branch) = &session.git_branch {
                format!("{} ({branch})", session.project_name)
            } else {
                session.project_name.clone()
            }
        } else {
            session.project_name.clone()
        }
    } else {
        "private project".to_string()
    };

    let details = if config.privacy.show_activity {
        if let Some(activity) = &session.activity {
            format!(
                "{} \u{2022} {}",
                activity.to_text(config.privacy.show_activity_target),
                project_label
            )
        } else if config.privacy.show_project_name {
            format!("Working on {}", project_label)
        } else {
            "Working in Claude Code".to_string()
        }
    } else if config.privacy.show_project_name {
        format!("Working on {}", project_label)
    } else {
        "Working in Claude Code".to_string()
    };

    let mut state_parts: Vec<String> = Vec::new();

    if config.privacy.show_model {
        if let Some(display) = &session.model_display {
            let model_id = session.model.as_deref().unwrap_or("");
            let tokens = session.session_total_tokens.unwrap_or(0);
            state_parts.push(crate::cost::model_display_with_context(
                model_id, display, tokens,
            ));
        } else if let Some(model) = &session.model {
            state_parts.push(model.clone());
        }
    }

    if config.privacy.show_tokens {
        for part in token_state_parts(session) {
            state_parts.push(part);
        }
    }

    if config.privacy.show_cost && session.total_cost > 0.0 {
        state_parts.push(format!("Cost: {}", format_cost(session.total_cost)));
    }

    if config.privacy.show_plan {
        if let Some(plan_name) = config.plan_display_name() {
            state_parts.push(plan_name.to_string());
        }
    }

    if config.privacy.show_limits {
        render_limits_to_state(&mut state_parts, effective_limits, api_usage);
    }

    if config.privacy.show_cost {
        render_extra_usage_to_state(&mut state_parts, api_usage);
    }

    let state = compact_join_prioritized(&state_parts, 128);

    (truncate_for_discord(&details), state)
}

fn token_state_parts(session: &ClaudeSessionSnapshot) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(last) = session.last_turn_tokens {
        if last > 0 {
            parts.push(format!("Last response {}", format_tokens(last)));
        }
    }
    if let Some(total) = session.session_total_tokens {
        if total > 0 {
            parts.push(format!("Session total {}", format_tokens(total)));
        }
    }
    parts
}

fn render_extra_usage_to_state(parts: &mut Vec<String>, api_usage: Option<&UsageData>) {
    let Some(usage) = api_usage else { return };
    let Some(ref extra) = usage.extra_usage else {
        return;
    };
    if !extra.is_enabled {
        return;
    }
    if let (Some(spent), Some(limit)) = (extra.used_credits, extra.monthly_limit) {
        // API returns values in cents — divide by 100 to get USD
        let spent_usd = spent / 100.0;
        let limit_usd = limit / 100.0;
        let pct = extra.utilization.unwrap_or(0.0);
        parts.push(format!(
            "Extra ${:.2}/${:.2} ({:.0}%)",
            spent_usd, limit_usd, pct
        ));
    }
}

fn render_limits_to_state(
    parts: &mut Vec<String>,
    effective_limits: Option<&RateLimits>,
    api_usage: Option<&UsageData>,
) {
    // Prefer API usage data
    if let Some(usage) = api_usage {
        let five_hr_pct = (100.0 - usage.five_hour.utilization).max(0.0);
        parts.push(format!("5h left {:.0}%", five_hr_pct));
        let seven_day_pct = (100.0 - usage.seven_day.utilization).max(0.0);
        parts.push(format!("7d left {:.0}%", seven_day_pct));
        return;
    }

    // Fall back to JSONL limits
    if let Some(limits) = effective_limits {
        if let Some(primary) = &limits.primary {
            parts.push(format!("5h left {:.0}%", primary.remaining_percent));
        }
        if let Some(secondary) = &limits.secondary {
            parts.push(format!("7d left {:.0}%", secondary.remaining_percent));
        }
    }
}

fn small_asset_for_activity(
    session: &ClaudeSessionSnapshot,
    config: &PresenceConfig,
) -> (String, String) {
    let activity_kind = session
        .activity
        .as_ref()
        .map(|a| &a.kind)
        .unwrap_or(&ActivityKind::Idle);

    let keys = &config.display.activity_small_image_keys;
    let override_key = match activity_kind {
        ActivityKind::Thinking => keys.thinking.as_deref(),
        ActivityKind::ReadingFile => keys.reading.as_deref(),
        ActivityKind::EditingFile => keys.editing.as_deref(),
        ActivityKind::RunningCommand => keys.running.as_deref(),
        ActivityKind::WaitingInput => keys.waiting.as_deref(),
        ActivityKind::Idle => keys.idle.as_deref(),
    };

    let image_key = override_key
        .unwrap_or(&config.display.small_image_key)
        .to_string();

    let text = match activity_kind {
        ActivityKind::Thinking => "Thinking",
        ActivityKind::ReadingFile => "Reading files",
        ActivityKind::EditingFile => "Editing files",
        ActivityKind::RunningCommand => "Running command",
        ActivityKind::WaitingInput => "Waiting for input",
        ActivityKind::Idle => &config.display.small_text,
    };

    (image_key, text.to_string())
}

fn truncate_for_discord(s: &str) -> String {
    if s.len() <= 128 {
        s.to_string()
    } else {
        // Walk back to a valid UTF-8 char boundary to avoid panicking on multi-byte chars
        let mut end = 125;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

fn compact_join_prioritized(parts: &[String], max_len: usize) -> String {
    if parts.is_empty() {
        return String::new();
    }

    let joined = parts.join(" | ");
    if joined.len() <= max_len {
        return joined;
    }

    // Drop parts from the end until it fits
    for count in (1..parts.len()).rev() {
        let attempt = parts[..count].join(" | ");
        if attempt.len() <= max_len {
            return attempt;
        }
    }

    truncate_for_discord(&parts[0])
}

fn non_empty_trimmed(s: &str) -> Option<&str> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn resolve_image_key(key: &str, known_keys: Option<&HashSet<String>>) -> Option<String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(known) = known_keys {
        if !known.contains(trimmed) {
            return None;
        }
    }
    Some(trimmed.to_string())
}

fn normalize_asset_pair(
    large: Option<String>,
    small: Option<String>,
) -> (Option<String>, Option<String>) {
    // Discord requires large_image if small_image is set
    if small.is_some() && large.is_none() {
        return (small, None);
    }
    (large, small)
}

#[derive(Debug, Deserialize)]
struct DiscordAssetEntry {
    name: String,
}

fn fetch_discord_asset_keys(client_id: &str) -> Result<HashSet<String>> {
    let url = format!("https://discord.com/api/v10/oauth2/applications/{client_id}/assets");

    let agent = ureq::AgentBuilder::new()
        .timeout(DISCORD_ASSET_FETCH_TIMEOUT)
        .build();

    let response = agent.get(&url).call()?;
    let body = response.into_string()?;
    let entries: Vec<DiscordAssetEntry> = serde_json::from_str(&body)?;

    Ok(entries.into_iter().map(|e| e.name).collect())
}
