use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::config;

/// Cache usage data for 5 minutes — it changes slowly and the endpoint
/// shares rate limits with Claude Code itself.
const USAGE_CACHE_TTL: Duration = Duration::from_secs(300);
/// Fallback backoff when Retry-After header is missing (reduced from 300s).
const USAGE_RATE_LIMIT_FALLBACK: Duration = Duration::from_secs(30);
const USAGE_API_TIMEOUT: Duration = Duration::from_secs(10);
const USAGE_API_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// Honest User-Agent identifying this tool and its version.
const USER_AGENT: &str = concat!("cc-discord-presence/", env!("CARGO_PKG_VERSION"));

/// Claude Code CLI's registered OAuth client ID (used for token refresh).
const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const TOKEN_REFRESH_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const TOKEN_REFRESH_COOLDOWN: Duration = Duration::from_secs(120);

/// Endpoint for enabling/disabling pay-per-use extra usage.
/// NOTE: This is inferred from the usage URL pattern (internal Anthropic API).
/// If the toggle is not working, open Chrome DevTools on claude.ai, click the
/// Extra Usage toggle, and update this constant with the correct URL + method.
const EXTRA_USAGE_TOGGLE_URL: &str = "https://api.anthropic.com/api/oauth/extra-usage";

/// Build a shared HTTP agent with the standard timeout.
fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(USAGE_API_TIMEOUT).build()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageData {
    pub five_hour: UsageWindow,
    pub seven_day: UsageWindow,
    // API returns "seven_day_sonnet"; keep alias for forward-compat
    #[serde(rename = "seven_day_sonnet", alias = "sonnet_free", default)]
    pub sonnet_free: Option<UsageWindow>,
    #[serde(default)]
    pub extra_usage: Option<ExtraUsage>,
}

impl UsageData {
    fn normalize_cached_units(mut self) -> Self {
        if let Some(extra) = self.extra_usage.as_mut()
            && extra.monthly_limit.unwrap_or_default() >= 1000.0
        {
            extra.monthly_limit = extra.monthly_limit.map(|v| v / 100.0);
            extra.used_credits = extra.used_credits.map(|v| v / 100.0);
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageWindow {
    pub utilization: f64,
    /// Reset timestamp — API returns `null` for windows that haven't started
    /// (e.g. `seven_day_sonnet` when the user hasn't used Sonnet yet).
    /// Must be Optional to avoid breaking the whole UsageData parse.
    #[serde(default)]
    pub resets_at: Option<DateTime<Utc>>,
}

/// Pay-per-use (extra) usage beyond the plan's included quota.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtraUsage {
    #[serde(default)]
    pub is_enabled: bool,
    /// Monthly spending limit in USD (e.g. 75.0)
    pub monthly_limit: Option<f64>,
    /// Credits consumed this month in USD (e.g. 48.89)
    pub used_credits: Option<f64>,
    /// Percent of monthly limit consumed (0–100)
    pub utilization: Option<f64>,
}

#[derive(Serialize, Deserialize)]
struct UsageCacheFile {
    fetched_at_unix: u64,
    data: UsageData,
}

#[derive(Debug, Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: OAuthCredentials,
}

#[derive(Debug, Deserialize)]
struct OAuthCredentials {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "expiresAt")]
    expires_at: i64,
    #[serde(rename = "refreshToken", default)]
    refresh_token: Option<String>,
    #[serde(rename = "subscriptionType", default)]
    subscription_type: Option<String>,
    #[serde(rename = "rateLimitTier", default)]
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthRefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct ApiUsageData {
    five_hour: UsageWindow,
    seven_day: UsageWindow,
    #[serde(rename = "seven_day_sonnet", alias = "sonnet_free", default)]
    sonnet_free: Option<UsageWindow>,
    #[serde(default)]
    extra_usage: Option<ApiExtraUsage>,
}

#[derive(Debug, Deserialize)]
struct ApiExtraUsage {
    #[serde(default)]
    is_enabled: bool,
    monthly_limit: Option<f64>,
    used_credits: Option<f64>,
    utilization: Option<f64>,
}

impl From<ApiUsageData> for UsageData {
    fn from(value: ApiUsageData) -> Self {
        Self {
            five_hour: value.five_hour,
            seven_day: value.seven_day,
            sonnet_free: value.sonnet_free,
            extra_usage: value.extra_usage.map(|extra| ExtraUsage {
                is_enabled: extra.is_enabled,
                monthly_limit: extra.monthly_limit.map(|v| v / 100.0),
                used_credits: extra.used_credits.map(|v| v / 100.0),
                utilization: extra.utilization,
            }),
        }
    }
}

pub fn detect_plan_key(
    subscription_type: Option<&str>,
    rate_limit_tier: Option<&str>,
) -> Option<&'static str> {
    let sub = subscription_type.unwrap_or("").trim().to_ascii_lowercase();
    let tier = rate_limit_tier.unwrap_or("").trim().to_ascii_lowercase();

    if sub.is_empty() && tier.is_empty() {
        return None;
    }
    if sub.contains("team") || tier.contains("team") {
        return Some("team");
    }
    if sub.contains("20x") || tier.contains("20x") {
        return Some("max_20x");
    }
    if sub.contains("5x") || tier.contains("5x") {
        return Some("max_5x");
    }
    if sub.contains("pro") || tier.contains("pro") {
        return Some("pro");
    }
    if sub == "max" || sub.contains("claude_max") || tier.contains("max") {
        return Some("max");
    }
    if sub.contains("free") || tier.contains("free") {
        return Some("free");
    }
    None
}

pub struct UsageManager {
    cached_usage: Option<UsageData>,
    last_fetch: Option<Instant>,
    credentials: Option<CredentialsFile>,
    subscription_type_cache: Option<String>,
    last_refresh_attempt: Option<Instant>,
    /// Backoff until this instant after a 429 rate-limit response.
    rate_limit_until: Option<Instant>,
    /// Human-readable status for TUI display.
    last_error_hint: Option<String>,
    /// Shared HTTP agent for connection reuse across API calls.
    agent: ureq::Agent,
    /// Number of fetch attempts — use shorter cache TTL for first 3 fetches.
    fetch_count: u32,
}

impl UsageManager {
    pub fn new() -> Self {
        Self {
            cached_usage: None,
            last_fetch: None,
            credentials: None,
            subscription_type_cache: None,
            last_refresh_attempt: None,
            rate_limit_until: None,
            last_error_hint: None,
            agent: http_agent(),
            fetch_count: 0,
        }
    }

    fn try_read_file_cache() -> Option<UsageData> {
        let path = crate::config::usage_cache_path();
        let raw = std::fs::read_to_string(path).ok()?;
        let cache: UsageCacheFile = serde_json::from_str(&raw).ok()?;
        let now_unix = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        if now_unix.saturating_sub(cache.fetched_at_unix) < USAGE_CACHE_TTL.as_secs() {
            Some(cache.data.normalize_cached_units())
        } else {
            None
        }
    }

    fn write_file_cache(data: &UsageData) {
        let path = crate::config::usage_cache_path();
        let fetched_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let Ok(json) = serde_json::to_string(&UsageCacheFile {
            fetched_at_unix,
            data: data.clone(),
        }) else {
            return;
        };
        let _ = std::fs::write(path, json);
    }

    pub fn get_usage(&mut self) -> Option<UsageData> {
        // Shorter cache TTL for first 3 fetches to get initial data faster
        let ttl = if self.fetch_count < 3 {
            Duration::from_secs(30)
        } else {
            USAGE_CACHE_TTL
        };
        if let Some(ref usage) = self.cached_usage
            && let Some(last) = self.last_fetch
            && last.elapsed() < ttl
        {
            return Some(usage.clone());
        }

        if let Some(cached) = Self::try_read_file_cache() {
            self.cached_usage = Some(cached.clone());
            self.last_fetch = Some(Instant::now());
            self.rate_limit_until = None;
            return Some(cached);
        }

        // Respect rate-limit backoff
        if let Some(until) = self.rate_limit_until
            && Instant::now() < until
        {
            return self.cached_usage.clone();
        }

        self.fetch_usage()
    }

    pub fn invalidate_cache(&mut self) {
        self.last_fetch = None;
        self.rate_limit_until = None;
    }

    /// Returns a hint about why usage data is unavailable (for TUI display).
    /// If rate-limited, shows a live countdown.
    pub fn error_hint_with_countdown(&self) -> Option<String> {
        if let Some(until) = self.rate_limit_until {
            let now = Instant::now();
            if until > now {
                let remaining = (until - now).as_secs();
                if remaining > 0 {
                    return Some(format!("refreshing in {}s", remaining));
                }
            }
            return None;
        }
        self.last_error_hint.clone()
    }

    /// Returns a clone of the current OAuth access token, if credentials are loaded.
    pub fn get_access_token(&mut self) -> Option<String> {
        self.ensure_credentials();
        self.credentials
            .as_ref()
            .map(|c| c.claude_ai_oauth.access_token.clone())
    }

    pub fn subscription_type(&mut self) -> Option<String> {
        if self.subscription_type_cache.is_some() {
            return self.subscription_type_cache.clone();
        }
        self.ensure_credentials();
        if let Some(ref creds) = self.credentials {
            self.subscription_type_cache = creds.claude_ai_oauth.subscription_type.clone();
        }
        self.subscription_type_cache.clone()
    }

    pub fn detected_plan_key(&mut self) -> Option<String> {
        self.ensure_credentials();
        let creds = self.credentials.as_ref()?;
        detect_plan_key(
            creds.claude_ai_oauth.subscription_type.as_deref(),
            creds.claude_ai_oauth.rate_limit_tier.as_deref(),
        )
        .map(str::to_string)
    }

    // ── Core API call + response handling (shared by all fetch paths) ──────

    /// Send an authenticated GET to the usage API using the current credentials.
    fn call_usage_api(&self) -> Option<Result<ureq::Response, ureq::Error>> {
        let creds = self.credentials.as_ref()?;
        Some(
            self.agent
                .get(USAGE_API_URL)
                .set(
                    "Authorization",
                    &format!("Bearer {}", creds.claude_ai_oauth.access_token),
                )
                .set("anthropic-beta", "oauth-2025-04-20")
                .set("User-Agent", USER_AGENT)
                .call(),
        )
    }

    /// Parse a usage API response, updating cache and error state.
    fn handle_usage_response(
        &mut self,
        response: Result<ureq::Response, ureq::Error>,
    ) -> Option<UsageData> {
        match response {
            Ok(resp) => {
                let body = match resp.into_string() {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Failed to read usage API response: {e}");
                        self.last_error_hint = Some("bad response".to_string());
                        return self.cached_usage.clone();
                    }
                };

                match serde_json::from_str::<ApiUsageData>(&body) {
                    Ok(parsed) => {
                        let usage: UsageData = parsed.into();
                        self.cached_usage = Some(usage.clone());
                        self.last_fetch = Some(Instant::now());
                        self.last_error_hint = None;
                        self.rate_limit_until = None;
                        Self::write_file_cache(&usage);
                        Some(usage)
                    }
                    Err(e) => {
                        warn!("Failed to parse usage API response: {e}");
                        self.last_error_hint = Some("parse error".to_string());
                        self.cached_usage.clone()
                    }
                }
            }
            Err(ureq::Error::Status(429, resp)) => {
                let retry_after = Self::parse_retry_after(&resp);
                debug!(
                    retry_after_secs = retry_after.as_secs(),
                    "Usage API rate limited"
                );
                self.rate_limit_until = Some(Instant::now() + retry_after);
                let secs = retry_after.as_secs();
                self.last_error_hint = Some(format!("refreshing in {secs}s"));
                self.cached_usage.clone()
            }
            Err(e) => {
                debug!("Usage API request failed: {e}");
                self.last_error_hint = Some("API unreachable".to_string());
                self.cached_usage.clone()
            }
        }
    }

    // ── Fetch orchestration ───────────────────────────────────────────────

    fn fetch_usage(&mut self) -> Option<UsageData> {
        self.fetch_count = self.fetch_count.saturating_add(1);
        self.ensure_credentials();
        if self.credentials.is_none() {
            self.last_error_hint = Some("no credentials — check .credentials.json".to_string());
            return None;
        }

        // Refresh expired tokens before calling the API
        if let Some(ref creds) = self.credentials
            && creds.claude_ai_oauth.expires_at < Utc::now().timestamp_millis()
        {
            debug!("OAuth token expired, attempting refresh");
            if !self.try_refresh_token() {
                self.credentials = None;
                self.ensure_credentials();
                if let Some(ref creds) = self.credentials {
                    if creds.claude_ai_oauth.expires_at < Utc::now().timestamp_millis() {
                        debug!("Token still expired after reload, skipping API call");
                        self.last_error_hint = Some("token expired".to_string());
                        return self.cached_usage.clone();
                    }
                } else {
                    self.last_error_hint = Some("token expired, refresh failed".to_string());
                    return self.cached_usage.clone();
                }
            }
        }

        let response = match self.call_usage_api() {
            Some(r) => r,
            None => return self.cached_usage.clone(),
        };

        // On 401, try refreshing the token once and retry
        if let Err(ureq::Error::Status(401, _)) = &response {
            debug!("Usage API returned 401, attempting token refresh");
            self.last_error_hint = Some("re-authenticating...".to_string());
            if self.try_refresh_token()
                && let Some(retry_response) = self.call_usage_api()
            {
                return self.handle_usage_response(retry_response);
            }
            self.credentials = None;
            self.last_error_hint = Some("auth failed — re-login to claude.ai".to_string());
            return self.cached_usage.clone();
        }

        self.handle_usage_response(response)
    }

    /// Parse the Retry-After header from a 429 response.
    fn parse_retry_after(resp: &ureq::Response) -> Duration {
        resp.header("retry-after")
            .and_then(|v| v.parse::<u64>().ok())
            .map(|secs| Duration::from_secs(secs + 5)) // reduced safety margin (was 15s)
            .unwrap_or(USAGE_RATE_LIMIT_FALLBACK)
    }

    /// Attempt to refresh the OAuth token using the refresh_token.
    /// Returns true if the token was successfully refreshed and credentials updated.
    fn try_refresh_token(&mut self) -> bool {
        // Respect cooldown to avoid hammering the endpoint
        if let Some(last) = self.last_refresh_attempt
            && last.elapsed() < TOKEN_REFRESH_COOLDOWN
        {
            return false;
        }
        self.last_refresh_attempt = Some(Instant::now());

        let refresh_token = self
            .credentials
            .as_ref()
            .and_then(|c| c.claude_ai_oauth.refresh_token.clone());

        let Some(refresh_token) = refresh_token else {
            debug!("No refresh token available");
            return false;
        };

        debug!("Refreshing OAuth token");
        let body = serde_json::json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": OAUTH_CLIENT_ID,
        });

        let result = self
            .agent
            .post(TOKEN_REFRESH_URL)
            .set("Content-Type", "application/json")
            .send_string(&body.to_string());

        match result {
            Ok(resp) => {
                let body_str = match resp.into_string() {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Failed to read token refresh response: {e}");
                        return false;
                    }
                };

                let refresh_resp = match serde_json::from_str::<OAuthRefreshResponse>(&body_str) {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("Failed to parse token refresh response: {e}");
                        return false;
                    }
                };

                let expires_at =
                    Utc::now().timestamp_millis() + (refresh_resp.expires_in as i64 * 1000);

                // Write updated credentials back to file atomically
                if self.save_refreshed_credentials(&refresh_resp, expires_at) {
                    // Reload from the file we just wrote
                    self.credentials = None;
                    self.ensure_credentials();
                    debug!(
                        expires_in = refresh_resp.expires_in,
                        "OAuth token refreshed successfully"
                    );
                    true
                } else {
                    false
                }
            }
            Err(ureq::Error::Status(status, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                debug!(status, body = %body, "Token refresh HTTP error");
                false
            }
            Err(e) => {
                debug!(error = %e, "Token refresh request failed");
                false
            }
        }
    }

    /// Read the credentials file as raw JSON, update the token fields, and write back.
    /// This preserves all other fields (scopes, subscriptionType, etc.).
    fn save_refreshed_credentials(
        &self,
        refresh_resp: &OAuthRefreshResponse,
        expires_at: i64,
    ) -> bool {
        let cred_path = config::credentials_path();
        let Ok(data) = std::fs::read_to_string(&cred_path) else {
            return false;
        };
        let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&data) else {
            return false;
        };

        if let Some(oauth) = json
            .get_mut("claudeAiOauth")
            .and_then(|v| v.as_object_mut())
        {
            oauth.insert(
                "accessToken".to_string(),
                serde_json::Value::String(refresh_resp.access_token.clone()),
            );
            oauth.insert(
                "expiresAt".to_string(),
                serde_json::Value::Number(serde_json::Number::from(expires_at)),
            );
            if let Some(ref new_refresh) = refresh_resp.refresh_token {
                oauth.insert(
                    "refreshToken".to_string(),
                    serde_json::Value::String(new_refresh.clone()),
                );
            }
        } else {
            return false;
        }

        let Ok(updated) = serde_json::to_string_pretty(&json) else {
            return false;
        };
        if std::fs::write(&cred_path, updated).is_err() {
            warn!(
                "Failed to write refreshed credentials to {}",
                cred_path.display()
            );
            return false;
        }
        true
    }

    fn ensure_credentials(&mut self) {
        if self.credentials.is_some() {
            return;
        }

        let cred_path = config::credentials_path();
        let Ok(data) = std::fs::read_to_string(&cred_path) else {
            debug!("Cannot read credentials file: {}", cred_path.display());
            return;
        };

        match serde_json::from_str::<CredentialsFile>(&data) {
            Ok(creds) => {
                self.subscription_type_cache = creds.claude_ai_oauth.subscription_type.clone();
                self.credentials = Some(creds);
            }
            Err(e) => {
                warn!("Cannot parse credentials: {e}");
            }
        }
    }
}

impl Default for UsageManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Claude.ai session-cookie toggle helpers ───────────────────────────────────

/// Fetches the first organization UUID accessible with the given claude.ai session cookie.
fn get_org_uuid(session_key: &str) -> Option<String> {
    let resp = http_agent()
        .get("https://claude.ai/api/organizations")
        .set("Cookie", &format!("sessionKey={session_key}"))
        .set("User-Agent", "Mozilla/5.0")
        .call()
        .ok()?;
    let text = resp.into_string().ok()?;
    let orgs: Vec<serde_json::Value> = serde_json::from_str(&text).ok()?;
    orgs.into_iter()
        .next()?
        .get("uuid")?
        .as_str()
        .map(str::to_string)
}

/// Sends a PATCH to the claude.ai overage_spend_limit endpoint.
/// Returns `true` on HTTP success, `false` otherwise (caller falls back to Bearer path).
fn toggle_extra_usage_via_session(session_key: &str, org_uuid: &str, enabled: bool) -> bool {
    let url = format!("https://claude.ai/api/organizations/{org_uuid}/overage_spend_limit");
    let body = format!("{{\"is_enabled\":{enabled}}}");
    let result = http_agent()
        .request("PATCH", &url)
        .set("Cookie", &format!("sessionKey={session_key}"))
        .set("Content-Type", "application/json")
        .set("User-Agent", "Mozilla/5.0")
        .send_string(&body);
    match result {
        Ok(resp) => {
            debug!(
                enabled,
                status = resp.status(),
                "extra usage toggle via session cookie"
            );
            true
        }
        Err(ureq::Error::Status(status, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            debug!(enabled, status, body = %body, "extra usage session toggle HTTP error");
            false
        }
        Err(e) => {
            debug!(enabled, error = %e, "extra usage session toggle failed");
            false
        }
    }
}

// ── Public toggle entry point ─────────────────────────────────────────────────

/// Spawns a background thread that disables Extra Usage, waits 3 seconds, then re-enables it.
///
/// Prefers the `claude.ai` session-cookie path (real endpoint).
/// Falls back to the Bearer-token path as a secondary attempt; any HTTP errors there
/// are logged at `debug` level only (the endpoint may not exist).
pub fn spawn_extra_usage_toggle_cycle(access_token: String, session_key: Option<String>) {
    std::thread::spawn(move || {
        // Resolve org UUID once — only needed for the session-cookie path.
        let org_uuid = session_key.as_deref().and_then(get_org_uuid);

        let agent = http_agent();

        let do_toggle = |enabled: bool| {
            // Session-cookie path (real claude.ai endpoint).
            if let (Some(sk), Some(uuid)) = (&session_key, &org_uuid)
                && toggle_extra_usage_via_session(sk, uuid, enabled)
            {
                return;
            }

            // Bearer-token fallback (endpoint may 404 — debug only, no WARN).
            let body = format!("{{\"enabled\":{enabled}}}");
            let result = agent
                .put(EXTRA_USAGE_TOGGLE_URL)
                .set("Authorization", &format!("Bearer {access_token}"))
                .set("anthropic-beta", "oauth-2025-04-20")
                .set("Content-Type", "application/json")
                .set("User-Agent", USER_AGENT)
                .send_string(&body);
            match result {
                Ok(resp) => {
                    debug!(
                        enabled,
                        status = resp.status(),
                        "extra usage bearer toggle response"
                    );
                }
                Err(ureq::Error::Status(status, resp)) => {
                    let body = resp.into_string().unwrap_or_default();
                    debug!(enabled, status, body = %body, "extra usage bearer toggle HTTP error");
                }
                Err(e) => {
                    debug!(enabled, error = %e, "extra usage bearer toggle failed");
                }
            }
        };

        do_toggle(false);
        std::thread::sleep(Duration::from_secs(3));
        do_toggle(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_usage_extra_usage_is_normalized_to_usd() {
        let parsed: ApiUsageData = serde_json::from_str(
            r#"{
                "five_hour": { "utilization": 1.0, "resets_at": null },
                "seven_day": { "utilization": 2.0, "resets_at": null },
                "extra_usage": {
                    "is_enabled": true,
                    "monthly_limit": 20000,
                    "used_credits": 20035.0,
                    "utilization": 100.0
                }
            }"#,
        )
        .unwrap();

        let usage: UsageData = parsed.into();
        let extra = usage.extra_usage.unwrap();
        assert_eq!(extra.monthly_limit, Some(200.0));
        assert_eq!(extra.used_credits, Some(200.35));
    }

    #[test]
    fn cached_usage_cents_are_normalized_once() {
        let usage = UsageData {
            five_hour: UsageWindow {
                utilization: 0.0,
                resets_at: None,
            },
            seven_day: UsageWindow {
                utilization: 0.0,
                resets_at: None,
            },
            sonnet_free: None,
            extra_usage: Some(ExtraUsage {
                is_enabled: true,
                monthly_limit: Some(20000.0),
                used_credits: Some(20035.0),
                utilization: Some(100.0),
            }),
        }
        .normalize_cached_units();

        let extra = usage.extra_usage.unwrap();
        assert_eq!(extra.monthly_limit, Some(200.0));
        assert_eq!(extra.used_credits, Some(200.35));
    }

    #[test]
    fn detect_plan_key_prefers_tier_detail_for_max() {
        assert_eq!(
            detect_plan_key(Some("max"), Some("default_claude_max_20x")),
            Some("max_20x")
        );
        assert_eq!(detect_plan_key(Some("claude_pro_2025"), None), Some("pro"));
        assert_eq!(detect_plan_key(Some("team"), None), Some("team"));
    }
}
