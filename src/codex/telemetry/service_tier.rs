use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::codex::config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServiceTier {
    Fast,
    #[default]
    Standard,
}

impl ServiceTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fast => "Fast",
            Self::Standard => "Standard",
        }
    }

    pub fn fast_mode_label(self) -> &'static str {
        match self {
            Self::Fast => "On",
            Self::Standard => "Off",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedServiceTier {
    pub tier: ServiceTier,
    pub raw_tier: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub source_path: Option<PathBuf>,
}

impl Default for ResolvedServiceTier {
    fn default() -> Self {
        Self {
            tier: ServiceTier::Standard,
            raw_tier: None,
            observed_at: None,
            source_path: None,
        }
    }
}

impl ResolvedServiceTier {
    pub fn is_fast(&self) -> bool {
        matches!(self.tier, ServiceTier::Fast)
    }

    pub fn fast_mode_label(&self) -> &'static str {
        self.tier.fast_mode_label()
    }
}

pub fn resolve_service_tier() -> ResolvedServiceTier {
    resolve_service_tier_from_sources(&config::config_toml_paths(), &config::global_state_paths())
}

/// Resolve the active service tier from every known source, preferring the
/// freshest by file modification time. Current Codex versions persist the
/// active tier as `service_tier` in `config.toml`; older Codex App builds wrote
/// `default-service-tier` into `.codex-global-state.json`, kept here as a
/// fallback so the signal works across versions.
fn resolve_service_tier_from_sources(
    config_toml_paths: &[PathBuf],
    global_state_paths: &[PathBuf],
) -> ResolvedServiceTier {
    config_toml_paths
        .iter()
        .filter_map(|path| load_service_tier_from_config_toml(path))
        .chain(
            global_state_paths
                .iter()
                .filter_map(|path| load_service_tier_from_global_state(path)),
        )
        .max_by_key(|candidate| {
            candidate
                .observed_at
                .map(|value| value.timestamp_millis())
                .unwrap_or(i64::MIN)
        })
        .unwrap_or_default()
}

/// Map a raw Codex service-tier string to the Fast/Standard distinction shown in
/// the presence. `priority` is OpenAI's expedited (fast) tier; `fast` is the
/// legacy Codex App literal. Everything else (`default`, `flex`, `auto`,
/// `standard`, …) is Standard.
fn service_tier_from_raw(raw: &str) -> ServiceTier {
    match raw.trim().to_ascii_lowercase().as_str() {
        "fast" | "priority" => ServiceTier::Fast,
        _ => ServiceTier::Standard,
    }
}

fn load_service_tier_from_config_toml(path: &Path) -> Option<ResolvedServiceTier> {
    let modified = fs::metadata(path).ok()?.modified().ok();
    let raw = fs::read_to_string(path).ok()?;
    let raw_tier = parse_root_service_tier(&raw)?;

    Some(ResolvedServiceTier {
        tier: service_tier_from_raw(&raw_tier),
        raw_tier: Some(raw_tier),
        observed_at: modified.map(DateTime::<Utc>::from),
        source_path: Some(path.to_path_buf()),
    })
}

/// Extract the root-level `service_tier` value from a `config.toml`. Only keys
/// declared before the first `[table]` header are root keys in TOML, so parsing
/// stops there. Handles quoted/bare values and trailing comments without pulling
/// in a TOML dependency.
fn parse_root_service_tier(contents: &str) -> Option<String> {
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            break;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() != "service_tier" {
            continue;
        }
        let value = value.trim();
        let extracted = if let Some(rest) = value.strip_prefix('"') {
            rest.split('"').next().unwrap_or("")
        } else if let Some(rest) = value.strip_prefix('\'') {
            rest.split('\'').next().unwrap_or("")
        } else {
            value.split('#').next().unwrap_or(value).trim()
        };
        let extracted = extracted.trim();
        if extracted.is_empty() {
            return None;
        }
        return Some(extracted.to_ascii_lowercase());
    }
    None
}

fn load_service_tier_from_global_state(path: &Path) -> Option<ResolvedServiceTier> {
    let modified = fs::metadata(path).ok()?.modified().ok();
    let raw = fs::read_to_string(path).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    let raw_tier = parsed
        .get("electron-persisted-atom-state")
        .and_then(|value| value.get("default-service-tier"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_ascii_lowercase();

    Some(ResolvedServiceTier {
        tier: service_tier_from_raw(&raw_tier),
        raw_tier: Some(raw_tier),
        observed_at: modified.map(DateTime::<Utc>::from),
        source_path: Some(path.to_path_buf()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn defaults_to_standard_when_nothing_is_found() {
        let resolved = resolve_service_tier_from_sources(&[], &[]);
        assert_eq!(resolved.tier, ServiceTier::Standard);
        assert!(!resolved.is_fast());
    }

    #[test]
    fn reads_fast_service_tier_from_global_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(".codex-global-state.json");
        fs::write(
            &path,
            r#"{"electron-persisted-atom-state":{"default-service-tier":"fast"}}"#,
        )
        .expect("write");

        let resolved = resolve_service_tier_from_sources(&[], &[path]);
        assert_eq!(resolved.tier, ServiceTier::Fast);
        assert_eq!(resolved.raw_tier.as_deref(), Some("fast"));
        assert!(resolved.is_fast());
    }

    #[test]
    fn prefers_the_freshest_valid_global_state_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let older = dir.path().join("older.json");
        let newer = dir.path().join("newer.json");
        fs::write(
            &older,
            r#"{"electron-persisted-atom-state":{"default-service-tier":"standard"}}"#,
        )
        .expect("write older");
        std::thread::sleep(Duration::from_millis(10));
        fs::write(
            &newer,
            r#"{"electron-persisted-atom-state":{"default-service-tier":"fast"}}"#,
        )
        .expect("write newer");

        let resolved = resolve_service_tier_from_sources(&[], &[older, newer]);
        assert_eq!(resolved.tier, ServiceTier::Fast);
        assert_eq!(resolved.raw_tier.as_deref(), Some("fast"));
    }

    #[test]
    fn reads_fast_service_tier_from_config_toml() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "model = \"gpt-5.5\"\nservice_tier = \"fast\"  # fast mode on\n[tui]\nfoo = 1\n",
        )
        .expect("write");

        let resolved = resolve_service_tier_from_sources(&[path], &[]);
        assert_eq!(resolved.tier, ServiceTier::Fast);
        assert_eq!(resolved.raw_tier.as_deref(), Some("fast"));
    }

    #[test]
    fn maps_priority_tier_to_fast() {
        assert_eq!(service_tier_from_raw("priority"), ServiceTier::Fast);
        assert_eq!(service_tier_from_raw("Priority"), ServiceTier::Fast);
        assert_eq!(service_tier_from_raw("fast"), ServiceTier::Fast);
        assert_eq!(service_tier_from_raw("default"), ServiceTier::Standard);
        assert_eq!(service_tier_from_raw("flex"), ServiceTier::Standard);
        assert_eq!(service_tier_from_raw("auto"), ServiceTier::Standard);
    }

    #[test]
    fn parse_root_service_tier_handles_quotes_comments_and_tables() {
        assert_eq!(
            parse_root_service_tier("service_tier = \"default\"\n").as_deref(),
            Some("default")
        );
        assert_eq!(
            parse_root_service_tier("service_tier='fast' # bare quote\n").as_deref(),
            Some("fast")
        );
        assert_eq!(
            parse_root_service_tier("service_tier = priority\n").as_deref(),
            Some("priority")
        );
        // A `service_tier` inside a table is not a root key and must be ignored.
        assert_eq!(
            parse_root_service_tier("[profiles.x]\nservice_tier = \"fast\"\n"),
            None
        );
        assert_eq!(parse_root_service_tier("model = \"gpt-5.5\"\n"), None);
    }

    #[test]
    fn config_toml_wins_when_fresher_than_global_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let state = dir.path().join(".codex-global-state.json");
        let toml = dir.path().join("config.toml");
        fs::write(
            &state,
            r#"{"electron-persisted-atom-state":{"default-service-tier":"standard"}}"#,
        )
        .expect("write state");
        std::thread::sleep(Duration::from_millis(10));
        fs::write(&toml, "service_tier = \"fast\"\n").expect("write toml");

        let resolved = resolve_service_tier_from_sources(&[toml], &[state]);
        assert_eq!(resolved.tier, ServiceTier::Fast);
    }
}
