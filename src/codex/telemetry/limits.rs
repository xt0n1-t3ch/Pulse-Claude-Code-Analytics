use std::time::SystemTime;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitScope {
    GlobalCodex,
    ModelScoped,
    #[default]
    Other,
}

impl RateLimitScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::GlobalCodex => "global",
            Self::ModelScoped => "model",
            Self::Other => "other",
        }
    }

    pub fn as_slug(self) -> &'static str {
        match self {
            Self::GlobalCodex => "global_codex",
            Self::ModelScoped => "model_scoped",
            Self::Other => "other",
        }
    }

    pub const fn preference(self) -> u8 {
        match self {
            Self::GlobalCodex => 3,
            Self::ModelScoped => 2,
            Self::Other => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageWindow {
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub window_minutes: u64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimits {
    pub primary: Option<UsageWindow>,
    pub secondary: Option<UsageWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimitEnvelope {
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub plan_type: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub scope: RateLimitScope,
    pub limits: RateLimits,
}

#[derive(Debug, Clone)]
pub struct SessionLimitCandidate {
    pub session_id: String,
    pub session_last_activity: SystemTime,
    pub envelope: RateLimitEnvelope,
}

#[derive(Debug, Clone)]
pub struct EffectiveLimitSelection {
    pub source_session_id: String,
    pub source_limit_id: Option<String>,
    pub source_scope: RateLimitScope,
    pub observed_at: Option<DateTime<Utc>>,
    pub limits: RateLimits,
}

impl EffectiveLimitSelection {
    pub fn source_label(&self) -> String {
        match self.source_scope {
            RateLimitScope::GlobalCodex => "Global account quota (/codex)".to_string(),
            RateLimitScope::ModelScoped => {
                let id = self.source_limit_id.as_deref().unwrap_or("unknown");
                format!("Model-specific quota ({id})")
            }
            RateLimitScope::Other => {
                let id = self.source_limit_id.as_deref().unwrap_or("unknown");
                format!("Quota stream ({id})")
            }
        }
    }
}

pub fn limits_present(limits: &RateLimits) -> bool {
    limits.primary.is_some() || limits.secondary.is_some()
}

pub fn classify_limit_scope(limit_id: Option<&str>) -> RateLimitScope {
    let normalized = limit_id
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if normalized == "codex" {
        return RateLimitScope::GlobalCodex;
    }
    if normalized.starts_with("codex_") {
        return RateLimitScope::ModelScoped;
    }
    RateLimitScope::Other
}

pub fn parse_rate_limit_envelope(
    value: Option<&Value>,
    observed_at: Option<DateTime<Utc>>,
) -> Option<RateLimitEnvelope> {
    let value = value?;
    let limits = RateLimits {
        primary: parse_usage_window(value.get("primary")),
        secondary: parse_usage_window(value.get("secondary")),
    };
    if !limits_present(&limits) {
        return None;
    }

    let limit_id = str_at(value, &["limit_id"]);
    let limit_name = str_at(value, &["limit_name"]);
    let plan_type = str_at(value, &["plan_type"]);
    Some(RateLimitEnvelope {
        scope: classify_limit_scope(limit_id.as_deref()),
        limit_id,
        limit_name,
        plan_type,
        observed_at,
        limits,
    })
}

pub fn select_session_envelope_global_first(
    envelopes: &[RateLimitEnvelope],
) -> Option<RateLimitEnvelope> {
    let global = envelopes
        .iter()
        .filter(|item| item.scope == RateLimitScope::GlobalCodex)
        .filter_map(best_scoped_envelope);
    if let Some(best) = global.max_by_key(envelope_rank_key_ref) {
        return Some(best.clone());
    }

    envelopes
        .iter()
        .filter_map(best_scoped_envelope)
        .max_by_key(envelope_rank_key_ref)
        .cloned()
}

pub fn select_effective_limits_global_first(
    candidates: &[SessionLimitCandidate],
) -> Option<EffectiveLimitSelection> {
    let has_global = candidates.iter().any(|item| {
        item.envelope.scope == RateLimitScope::GlobalCodex && limits_present(&item.envelope.limits)
    });

    let selected = candidates.iter().filter(|item| {
        if has_global {
            item.envelope.scope == RateLimitScope::GlobalCodex
        } else {
            limits_present(&item.envelope.limits)
        }
    });
    let selected = selected.max_by_key(|item| {
        (
            envelope_rank_key(&item.envelope),
            system_time_rank(item.session_last_activity),
        )
    })?;

    Some(EffectiveLimitSelection {
        source_session_id: selected.session_id.clone(),
        source_limit_id: selected.envelope.limit_id.clone(),
        source_scope: selected.envelope.scope,
        observed_at: selected.envelope.observed_at,
        limits: selected.envelope.limits.clone(),
    })
}

fn best_scoped_envelope(envelope: &RateLimitEnvelope) -> Option<&RateLimitEnvelope> {
    limits_present(&envelope.limits).then_some(envelope)
}

fn envelope_rank_key(envelope: &RateLimitEnvelope) -> (i64, i64, String, String) {
    let observed = envelope
        .observed_at
        .map(|ts| ts.timestamp_millis())
        .unwrap_or(i64::MIN);
    let scope = match envelope.scope {
        RateLimitScope::GlobalCodex => 3,
        RateLimitScope::ModelScoped => 2,
        RateLimitScope::Other => 1,
    };
    let id = envelope.limit_id.clone().unwrap_or_default();
    let plan = envelope.plan_type.clone().unwrap_or_default();
    (observed, scope, id, plan)
}

fn envelope_rank_key_ref(envelope: &&RateLimitEnvelope) -> (i64, i64, String, String) {
    envelope_rank_key(envelope)
}

fn system_time_rank(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or(i64::MIN)
}

fn parse_usage_window(value: Option<&Value>) -> Option<UsageWindow> {
    let value = value?;
    let used_percent = clamp_percent(float_at(value, &["used_percent"]).unwrap_or(0.0));
    let remaining_percent = clamp_percent(100.0 - used_percent);

    Some(UsageWindow {
        used_percent,
        remaining_percent,
        window_minutes: uint_at(value, &["window_minutes"]).unwrap_or(0),
        resets_at: int_at(value, &["resets_at"])
            .and_then(|epoch| Utc.timestamp_opt(epoch, 0).single()),
    })
}

fn clamp_percent(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 100.0)
}

fn str_at(value: &Value, path: &[&str]) -> Option<String> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor
        .as_str()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

fn uint_at(value: &Value, path: &[&str]) -> Option<u64> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor
        .as_u64()
        .or_else(|| cursor.as_i64().and_then(|n| (n >= 0).then_some(n as u64)))
}

fn int_at(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor
        .as_i64()
        .or_else(|| cursor.as_u64().map(|n| n as i64))
}

fn float_at(value: &Value, path: &[&str]) -> Option<f64> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor
        .as_f64()
        .or_else(|| cursor.as_u64().map(|n| n as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_envelope_extracts_scope_and_windows() {
        let payload = serde_json::json!({
            "limit_id": "codex",
            "limit_name": "Global",
            "plan_type": "pro",
            "primary": {"used_percent": 20.0, "window_minutes": 300, "resets_at": 1772311817},
            "secondary": {"used_percent": 21.0, "window_minutes": 10080, "resets_at": 1772766565}
        });
        let parsed = parse_rate_limit_envelope(Some(&payload), Utc.timestamp_opt(10, 0).single())
            .expect("envelope");
        assert_eq!(parsed.scope, RateLimitScope::GlobalCodex);
        assert_eq!(parsed.limit_id.as_deref(), Some("codex"));
        assert_eq!(parsed.plan_type.as_deref(), Some("pro"));
        assert_eq!(
            parsed
                .limits
                .secondary
                .as_ref()
                .expect("secondary")
                .remaining_percent,
            79.0
        );
    }

    #[test]
    fn selection_prefers_global_codex_when_available() {
        let now = SystemTime::now();
        let candidates = vec![
            SessionLimitCandidate {
                session_id: "s1".to_string(),
                session_last_activity: now,
                envelope: RateLimitEnvelope {
                    limit_id: Some("codex_bengalfox".to_string()),
                    limit_name: Some("Spark".to_string()),
                    plan_type: None,
                    observed_at: Utc.timestamp_opt(200, 0).single(),
                    scope: RateLimitScope::ModelScoped,
                    limits: RateLimits {
                        primary: Some(UsageWindow {
                            used_percent: 0.0,
                            remaining_percent: 100.0,
                            window_minutes: 300,
                            resets_at: None,
                        }),
                        secondary: Some(UsageWindow {
                            used_percent: 13.0,
                            remaining_percent: 87.0,
                            window_minutes: 10080,
                            resets_at: None,
                        }),
                    },
                },
            },
            SessionLimitCandidate {
                session_id: "s1".to_string(),
                session_last_activity: now,
                envelope: RateLimitEnvelope {
                    limit_id: Some("codex".to_string()),
                    limit_name: None,
                    plan_type: Some("pro".to_string()),
                    observed_at: Utc.timestamp_opt(199, 0).single(),
                    scope: RateLimitScope::GlobalCodex,
                    limits: RateLimits {
                        primary: Some(UsageWindow {
                            used_percent: 8.0,
                            remaining_percent: 92.0,
                            window_minutes: 300,
                            resets_at: None,
                        }),
                        secondary: Some(UsageWindow {
                            used_percent: 21.0,
                            remaining_percent: 79.0,
                            window_minutes: 10080,
                            resets_at: None,
                        }),
                    },
                },
            },
        ];

        let selected = select_effective_limits_global_first(&candidates).expect("selection");
        assert_eq!(selected.source_limit_id.as_deref(), Some("codex"));
        assert_eq!(selected.source_scope, RateLimitScope::GlobalCodex);
        assert_eq!(
            selected
                .limits
                .secondary
                .as_ref()
                .expect("secondary")
                .remaining_percent,
            79.0
        );
    }
}
