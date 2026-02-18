use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::config;

const USAGE_CACHE_TTL: Duration = Duration::from_secs(30);
const USAGE_API_TIMEOUT: Duration = Duration::from_secs(10);
const USAGE_API_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// Endpoint for enabling/disabling pay-per-use extra usage.
/// NOTE: This is inferred from the usage URL pattern (internal Anthropic API).
/// If the toggle is not working, open Chrome DevTools on claude.ai, click the
/// Extra Usage toggle, and update this constant with the correct URL + method.
const EXTRA_USAGE_TOGGLE_URL: &str = "https://api.anthropic.com/api/oauth/extra-usage";

#[derive(Debug, Clone, Deserialize)]
pub struct UsageData {
    pub five_hour: UsageWindow,
    pub seven_day: UsageWindow,
    // API returns "seven_day_sonnet"; keep alias for forward-compat
    #[serde(rename = "seven_day_sonnet", alias = "sonnet_free", default)]
    pub sonnet_free: Option<UsageWindow>,
    #[serde(default)]
    pub extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageWindow {
    pub utilization: f64,
    pub resets_at: DateTime<Utc>,
}

/// Pay-per-use (extra) usage beyond the plan's included quota.
#[derive(Debug, Clone, Deserialize, Default)]
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
    #[serde(rename = "subscriptionType", default)]
    subscription_type: Option<String>,
    #[serde(rename = "rateLimitTier", default)]
    #[allow(dead_code)]
    rate_limit_tier: Option<String>,
}

pub struct UsageManager {
    cached_usage: Option<UsageData>,
    last_fetch: Option<Instant>,
    credentials: Option<CredentialsFile>,
    subscription_type_cache: Option<String>,
}

impl UsageManager {
    pub fn new() -> Self {
        Self {
            cached_usage: None,
            last_fetch: None,
            credentials: None,
            subscription_type_cache: None,
        }
    }

    pub fn get_usage(&mut self) -> Option<UsageData> {
        if let Some(ref usage) = self.cached_usage {
            if let Some(last) = self.last_fetch {
                if last.elapsed() < USAGE_CACHE_TTL {
                    return Some(usage.clone());
                }
            }
        }

        self.fetch_usage()
    }

    pub fn invalidate_cache(&mut self) {
        self.last_fetch = None;
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

    fn fetch_usage(&mut self) -> Option<UsageData> {
        self.ensure_credentials();
        let creds = self.credentials.as_ref()?;

        if creds.claude_ai_oauth.expires_at < Utc::now().timestamp_millis() {
            debug!("OAuth token expired, reloading credentials");
            self.credentials = None;
            self.ensure_credentials();
            let _ = self.credentials.as_ref()?;
        }

        let creds = self.credentials.as_ref()?;
        let agent = ureq::AgentBuilder::new().timeout(USAGE_API_TIMEOUT).build();

        let response = agent
            .get(USAGE_API_URL)
            .set(
                "Authorization",
                &format!("Bearer {}", creds.claude_ai_oauth.access_token),
            )
            .set("anthropic-beta", "oauth-2025-04-20")
            .set("User-Agent", "claude-code/2.0.31")
            .call();

        match response {
            Ok(resp) => {
                let body = match resp.into_string() {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("Failed to read usage API response: {e}");
                        return self.cached_usage.clone();
                    }
                };

                match serde_json::from_str::<UsageData>(&body) {
                    Ok(usage) => {
                        self.cached_usage = Some(usage.clone());
                        self.last_fetch = Some(Instant::now());
                        Some(usage)
                    }
                    Err(e) => {
                        warn!("Failed to parse usage API response: {e}");
                        self.cached_usage.clone()
                    }
                }
            }
            Err(ureq::Error::Status(401, _)) => {
                debug!("Usage API returned 401, clearing credentials");
                self.credentials = None;
                self.cached_usage.clone()
            }
            Err(e) => {
                debug!("Usage API request failed: {e}");
                self.cached_usage.clone()
            }
        }
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

/// Spawns a background thread that disables Extra Usage, waits 3 seconds, then re-enables it.
///
/// This is a fire-and-forget safety measure triggered when a new charge is detected.
/// Both toggle calls are logged; on failure the toggle is a no-op (charge already happened).
pub fn spawn_extra_usage_toggle_cycle(access_token: String) {
    std::thread::spawn(move || {
        let agent = ureq::AgentBuilder::new().timeout(USAGE_API_TIMEOUT).build();

        let do_toggle = |enabled: bool| {
            let body = format!("{{\"enabled\":{enabled}}}");
            let result = agent
                .put(EXTRA_USAGE_TOGGLE_URL)
                .set("Authorization", &format!("Bearer {access_token}"))
                .set("anthropic-beta", "oauth-2025-04-20")
                .set("Content-Type", "application/json")
                .set("User-Agent", "claude-code/2.0.31")
                .send_string(&body);

            match result {
                Ok(resp) => {
                    debug!(
                        enabled,
                        status = resp.status(),
                        "extra usage toggle response"
                    );
                }
                Err(ureq::Error::Status(status, resp)) => {
                    let body = resp.into_string().unwrap_or_default();
                    warn!(enabled, status, body = %body, "extra usage toggle HTTP error");
                }
                Err(e) => {
                    warn!(enabled, error = %e, "extra usage toggle request failed");
                }
            }
        };

        do_toggle(false);
        std::thread::sleep(Duration::from_secs(3));
        do_toggle(true);
    });
}
