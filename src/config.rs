use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const DEFAULT_STALE_SECONDS: u64 = 90;
const DEFAULT_POLL_SECONDS: u64 = 2;
const DEFAULT_ACTIVE_STICKY_SECONDS: u64 = 3600;
const MIN_ACTIVE_STICKY_SECONDS: u64 = 60;
const CONFIG_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_DISCORD_CLIENT_ID: &str = "1466664856261230716";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PresenceConfig {
    pub schema_version: u32,
    pub discord_client_id: Option<String>,
    pub plan: Option<String>,
    pub initialized: bool,
    pub privacy: PrivacyConfig,
    pub display: DisplayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {
    pub enabled: bool,
    pub show_project_name: bool,
    pub show_git_branch: bool,
    pub show_model: bool,
    pub show_tokens: bool,
    pub show_cost: bool,
    pub show_plan: bool,
    pub show_limits: bool,
    pub show_activity: bool,
    pub show_activity_target: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TerminalLogoMode {
    #[default]
    Auto,
    Ascii,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub large_image_key: String,
    pub large_text: String,
    pub small_image_key: String,
    pub small_text: String,
    pub activity_small_image_keys: ActivitySmallImageKeys,
    pub terminal_logo_mode: TerminalLogoMode,
    pub terminal_logo_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ActivitySmallImageKeys {
    pub thinking: Option<String>,
    pub reading: Option<String>,
    pub editing: Option<String>,
    pub running: Option<String>,
    pub waiting: Option<String>,
    pub idle: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    pub stale_threshold: Duration,
    pub active_sticky_window: Duration,
    pub poll_interval: Duration,
}

impl Default for PresenceConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            discord_client_id: Some(DEFAULT_DISCORD_CLIENT_ID.to_string()),
            plan: None,
            initialized: false,
            privacy: PrivacyConfig::default(),
            display: DisplayConfig::default(),
        }
    }
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            show_project_name: true,
            show_git_branch: true,
            show_model: true,
            show_tokens: true,
            show_cost: true,
            show_plan: true,
            show_limits: true,
            show_activity: true,
            show_activity_target: true,
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            large_image_key: "large".to_string(),
            large_text: "Claude Code".to_string(),
            small_image_key: String::new(),
            small_text: String::new(),
            activity_small_image_keys: ActivitySmallImageKeys::default(),
            terminal_logo_mode: TerminalLogoMode::Auto,
            terminal_logo_path: None,
        }
    }
}

impl PresenceConfig {
    pub fn load_or_init() -> Result<Self> {
        let cfg_path = config_path();
        if let Some(parent) = cfg_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        if cfg_path.exists() {
            let raw = fs::read_to_string(&cfg_path)
                .with_context(|| format!("failed to read {}", cfg_path.display()))?;
            let mut parsed: PresenceConfig = serde_json::from_str(&raw)
                .with_context(|| format!("invalid JSON in {}", cfg_path.display()))?;
            if parsed.normalize_and_migrate() {
                parsed.save()?;
            }
            Ok(parsed)
        } else {
            let cfg = PresenceConfig::default();
            cfg.save()?;
            Ok(cfg)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let data = serde_json::to_string_pretty(self)?;
        fs::write(&path, data).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn effective_client_id(&self) -> Option<String> {
        let from_env = env::var("CC_DISCORD_CLIENT_ID")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        if from_env.is_some() {
            return from_env;
        }

        self.discord_client_id
            .as_ref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    pub fn plan_display_name(&self) -> Option<&str> {
        match self.plan.as_deref()? {
            "free" => Some("Free"),
            "pro" => Some("Pro ($20/mo)"),
            "max_5x" => Some("Max 5x ($100/mo)"),
            "max_20x" => Some("Max 20x ($200/mo)"),
            _ => None,
        }
    }

    pub fn toggle_privacy(&mut self) -> bool {
        self.privacy.enabled = !self.privacy.enabled;
        self.privacy.enabled
    }

    fn normalize_and_migrate(&mut self) -> bool {
        let mut changed = false;

        if self.schema_version < CONFIG_SCHEMA_VERSION {
            self.schema_version = CONFIG_SCHEMA_VERSION;
            changed = true;
        }

        if is_missing(&self.discord_client_id) {
            self.discord_client_id = Some(DEFAULT_DISCORD_CLIENT_ID.to_string());
            changed = true;
        }

        if self.display.large_image_key.trim().is_empty() {
            self.display.large_image_key = DisplayConfig::default().large_image_key;
            changed = true;
        }
        if self.display.large_text.trim().is_empty() {
            self.display.large_text = DisplayConfig::default().large_text;
            changed = true;
        }
        // small_image_key and small_text are intentionally allowed to be empty
        // (empty = no small image displayed in Discord)
        for item in [
            &mut self.display.activity_small_image_keys.thinking,
            &mut self.display.activity_small_image_keys.reading,
            &mut self.display.activity_small_image_keys.editing,
            &mut self.display.activity_small_image_keys.running,
            &mut self.display.activity_small_image_keys.waiting,
            &mut self.display.activity_small_image_keys.idle,
        ] {
            if normalize_optional_string(item) {
                changed = true;
            }
        }
        if self
            .display
            .terminal_logo_path
            .as_deref()
            .is_some_and(|path| path.trim().is_empty())
        {
            self.display.terminal_logo_path = None;
            changed = true;
        }

        changed
    }
}

pub fn runtime_settings() -> RuntimeSettings {
    let sticky_seconds = env_u64(
        "CC_PRESENCE_ACTIVE_STICKY_SECONDS",
        DEFAULT_ACTIVE_STICKY_SECONDS,
    )
    .max(MIN_ACTIVE_STICKY_SECONDS);
    RuntimeSettings {
        stale_threshold: Duration::from_secs(env_u64(
            "CC_PRESENCE_STALE_SECONDS",
            DEFAULT_STALE_SECONDS,
        )),
        active_sticky_window: Duration::from_secs(sticky_seconds),
        poll_interval: Duration::from_secs(env_u64(
            "CC_PRESENCE_POLL_SECONDS",
            DEFAULT_POLL_SECONDS,
        )),
    }
}

pub fn claude_home() -> PathBuf {
    if let Ok(custom) = env::var("CLAUDE_HOME") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
}

pub fn projects_path() -> PathBuf {
    claude_home().join("projects")
}

pub fn projects_paths() -> Vec<PathBuf> {
    let mut ordered: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    push_unique_path(&mut ordered, &mut seen, projects_path());

    #[cfg(windows)]
    {
        for candidate in windows_wsl_projects_candidates() {
            push_unique_path(&mut ordered, &mut seen, candidate);
        }
    }

    #[cfg(all(unix, not(windows)))]
    {
        for candidate in wsl_windows_projects_candidates() {
            push_unique_path(&mut ordered, &mut seen, candidate);
        }
    }

    ordered
}

pub fn statusline_data_path() -> PathBuf {
    claude_home().join("discord-presence-data.json")
}

pub fn credentials_path() -> PathBuf {
    claude_home().join(".credentials.json")
}

pub fn config_path() -> PathBuf {
    claude_home().join("discord-presence-config.json")
}

pub fn lock_path() -> PathBuf {
    claude_home().join("cc-discord-presence.lock")
}

pub fn instance_meta_path() -> PathBuf {
    claude_home().join("cc-discord-presence.instance.json")
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn is_missing(value: &Option<String>) -> bool {
    value.as_ref().map(|v| v.trim().is_empty()).unwrap_or(true)
}

fn normalize_optional_string(value: &mut Option<String>) -> bool {
    if let Some(item) = value.as_mut() {
        let trimmed = item.trim().to_string();
        if trimmed.is_empty() {
            *value = None;
            return true;
        }
        if *item != trimmed {
            *item = trimmed;
            return true;
        }
    }
    false
}

fn push_unique_path(paths: &mut Vec<PathBuf>, seen: &mut HashSet<String>, candidate: PathBuf) {
    if candidate.as_os_str().is_empty() {
        return;
    }
    let key = path_key(&candidate);
    if seen.insert(key) {
        paths.push(candidate);
    }
}

fn path_key(path: &Path) -> String {
    #[cfg(windows)]
    {
        return path
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
    }

    #[cfg(not(windows))]
    {
        path.to_string_lossy().to_string()
    }
}

#[cfg(all(unix, not(windows)))]
fn wsl_windows_projects_candidates() -> Vec<PathBuf> {
    if !running_in_wsl() {
        return Vec::new();
    }

    let mut candidates = Vec::new();

    if let Ok(profile) = env::var("USERPROFILE") {
        let profile = profile.trim();
        if !profile.is_empty() {
            candidates.push(PathBuf::from(profile).join(".claude").join("projects"));
        }
    }

    if let Ok(username) = env::var("USERNAME").or_else(|_| env::var("USER")) {
        let username = username.trim();
        if !username.is_empty() {
            candidates.push(
                PathBuf::from("/mnt/c/Users")
                    .join(username)
                    .join(".claude")
                    .join("projects"),
            );
        }
    }

    candidates
}

#[cfg(all(unix, not(windows)))]
fn running_in_wsl() -> bool {
    if env::var_os("WSL_DISTRO_NAME").is_some() {
        return true;
    }
    fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|value| value.to_ascii_lowercase().contains("microsoft"))
        .unwrap_or(false)
}

#[cfg(windows)]
fn windows_wsl_projects_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let distros = windows_wsl_distro_names();
    for distro in distros {
        if let Some(home) = wsl_home_for_distro(&distro) {
            candidates.push(wsl_home_to_unc_projects_path(&distro, &home));
            continue;
        }
        if let Some(username) = env::var("USERNAME")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            let fallback = format!(
                r"\\wsl.localhost\{}\home\{}\.claude\projects",
                distro, username
            );
            candidates.push(PathBuf::from(fallback));
        }
    }
    candidates
}

#[cfg(windows)]
fn windows_wsl_distro_names() -> Vec<String> {
    let output = Command::new("wsl.exe")
        .args(["-l", "-q"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    decode_windows_text_output(&output.stdout)
        .lines()
        .map(|line| line.trim().trim_start_matches('*').trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

#[cfg(windows)]
fn wsl_home_for_distro(distro: &str) -> Option<String> {
    let output = Command::new("wsl.exe")
        .args(["-d", distro, "--", "sh", "-lc", "printf %s \"$HOME\""])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let home = decode_windows_text_output(&output.stdout)
        .trim()
        .to_string();
    if home.is_empty() {
        None
    } else {
        Some(home)
    }
}

#[cfg(windows)]
fn wsl_home_to_unc_projects_path(distro: &str, home: &str) -> PathBuf {
    let mut unc = format!(r"\\wsl.localhost\{}", distro);
    for part in home.trim().trim_start_matches('/').split('/') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        unc.push('\\');
        unc.push_str(part);
    }
    unc.push_str(r"\.claude\projects");
    PathBuf::from(unc)
}

#[cfg(windows)]
fn decode_windows_text_output(bytes: &[u8]) -> String {
    let has_interleaved_nuls = bytes
        .iter()
        .skip(1)
        .step_by(2)
        .take(64)
        .any(|byte| *byte == 0);

    if bytes.starts_with(&[0xFF, 0xFE]) || has_interleaved_nuls {
        let mut utf16: Vec<u16> = Vec::with_capacity(bytes.len() / 2);
        let mut chunks = bytes.chunks_exact(2);
        for chunk in &mut chunks {
            utf16.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        return String::from_utf16_lossy(&utf16);
    }

    String::from_utf8_lossy(bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_client_id_is_returned() {
        let cfg = PresenceConfig {
            discord_client_id: Some("from-config".to_string()),
            ..PresenceConfig::default()
        };
        assert_eq!(cfg.effective_client_id().as_deref(), Some("from-config"));
    }

    #[test]
    fn plan_display_names() {
        let mut cfg = PresenceConfig::default();
        cfg.plan = Some("pro".to_string());
        assert_eq!(cfg.plan_display_name(), Some("Pro ($20/mo)"));

        cfg.plan = Some("max_20x".to_string());
        assert_eq!(cfg.plan_display_name(), Some("Max 20x ($200/mo)"));

        cfg.plan = None;
        assert_eq!(cfg.plan_display_name(), None);
    }

    #[test]
    fn migration_sets_default_client_id_when_missing() {
        let mut cfg = PresenceConfig {
            schema_version: 0,
            discord_client_id: None,
            plan: None,
            initialized: false,
            privacy: PrivacyConfig::default(),
            display: DisplayConfig::default(),
        };

        let changed = cfg.normalize_and_migrate();

        assert!(changed);
        assert_eq!(cfg.schema_version, 1);
        assert_eq!(
            cfg.discord_client_id.as_deref(),
            Some(DEFAULT_DISCORD_CLIENT_ID)
        );
    }

    #[test]
    fn display_defaults_to_auto_logo_mode() {
        let cfg = PresenceConfig::default();
        assert_eq!(cfg.display.terminal_logo_mode, TerminalLogoMode::Auto);
        assert_eq!(cfg.display.terminal_logo_path, None);
    }

    #[test]
    fn privacy_toggle() {
        let mut cfg = PresenceConfig::default();
        assert!(!cfg.privacy.enabled);
        assert!(cfg.toggle_privacy());
        assert!(cfg.privacy.enabled);
        assert!(!cfg.toggle_privacy());
        assert!(!cfg.privacy.enabled);
    }
}
