use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{debug, warn};

use crate::config;

const USAGE_CACHE_TTL: Duration = Duration::from_secs(30);
const USAGE_API_TIMEOUT: Duration = Duration::from_secs(10);
const USAGE_API_URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Debug, Clone, Deserialize)]
pub struct UsageData {
    pub five_hour: UsageWindow,
    pub seven_day: UsageWindow,
    #[serde(default)]
    pub sonnet_free: Option<UsageWindow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageWindow {
    pub utilization: f64,
    pub resets_at: DateTime<Utc>,
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
