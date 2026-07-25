use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use cc_discord_presence::codex::config::{
    DesktopPresenceDesign, PresenceConfig as CodexPresenceConfig, PresenceSurface,
};
use cc_discord_presence::codex::discord::{
    DiscordPresence as CodexDiscordPresence, active_presence_presentation,
    idle_presence_presentation,
};
use cc_discord_presence::codex::model::SpeedMode as CodexSpeedMode;
use cc_discord_presence::codex::session::{
    self as codex_session, CodexSessionSnapshot, GitBranchCache as CodexGitBranchCache,
    SessionParseCache as CodexSessionParseCache,
};
use cc_discord_presence::codex::telemetry::plan::{DetectedPlanTier, PlanDetector};
use cc_discord_presence::codex::telemetry::service_tier::resolve_service_tier;
use cc_discord_presence::config::PresenceConfig;
use cc_discord_presence::cost;
use cc_discord_presence::discord::DiscordPresence as ClaudeDiscordPresence;
use cc_discord_presence::discord::presence_lines as claude_presence_lines;
use cc_discord_presence::provider::Provider;
use cc_discord_presence::session::{
    self, ClaudeSessionSnapshot, GitBranchCache, SessionParseCache, latest_limits_source,
    merge_statusline_into_sessions, preferred_active_session, read_statusline_data,
};
use cc_discord_presence::usage::UsageManager;
use codex_presence_core::{
    PresenceFieldId, QuotaScope, QuotaWindow, RateLimitScope, UsageSnapshot,
    usage_snapshot_from_envelopes,
};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::live::PublisherLease;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const STALE_THRESHOLD: Duration = Duration::from_secs(120);
const STICKY_WINDOW: Duration = Duration::from_secs(120);
const ACTIVE_CUTOFF: Duration = Duration::from_secs(600);
const IDLE_CUTOFF: Duration = Duration::from_secs(300);

static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static SNAPSHOT_POLLER_STARTED: AtomicBool = AtomicBool::new(false);
static INITIAL_SNAPSHOT_READY: AtomicBool = AtomicBool::new(false);
static SESSION_FINGERPRINTS: std::sync::OnceLock<Mutex<HashMap<String, u64>>> =
    std::sync::OnceLock::new();
fn uptime_secs() -> u64 {
    START_TIME.get_or_init(Instant::now).elapsed().as_secs()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DiscordDisplayPrefs {
    pub show_project: bool,
    pub show_branch: bool,
    pub show_model: bool,
    pub show_activity: bool,
    pub show_tokens: bool,
    pub show_cost: bool,
    pub show_limits: bool,
    pub show_credits: bool,
    pub show_context: bool,
    pub show_systems: bool,
}

impl Default for DiscordDisplayPrefs {
    fn default() -> Self {
        Self {
            show_project: true,
            show_branch: true,
            show_model: true,
            show_activity: true,
            show_tokens: false,
            show_cost: false,
            show_limits: true,
            show_credits: true,
            show_context: true,
            show_systems: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiscordSettings {
    pub provider: String,
    pub enabled: bool,
    pub status: String,
    pub publisher: String,
    pub display_prefs: DiscordDisplayPrefs,
    pub desktop_design: Option<String>,
    pub supports_desktop_design: bool,
    pub supports_field_order: bool,
    /// False for Claude: "Credits available" is a Codex account-balance field
    /// with no Claude equivalent, so the backend pins it off. The UI disables the
    /// row instead of offering a switch that silently snaps back.
    pub supports_credits: bool,
    pub field_order: Vec<String>,
}

#[derive(Default, Clone)]
enum ActiveSessions {
    #[default]
    None,
    Claude(Vec<ClaudeSessionSnapshot>),
    Codex(Vec<CodexSessionSnapshot>),
}

#[derive(Default, Clone)]
struct CachedData {
    active_provider: Provider,
    sessions: ActiveSessions,
    claude_usage: Option<CachedUsage>,
    claude_usage_error: Option<String>,
    codex_usage: Option<UsageSnapshot>,
    codex_limits: Option<codex_session::EffectiveLimitSelection>,
    discord_status: String,
    discord_publisher: String,
    discord_enabled: bool,
    discord_prefs: DiscordDisplayPrefs,
    codex_opencode_running: bool,
    codex_desktop_surface_running: bool,
    /// One-shot flag: when set by `refresh_usage` command, the background
    /// poller invalidates its usage cache on the next tick and forces a
    /// fresh API call. The flag is cleared after handling.
    usage_refresh_requested: bool,
}

#[derive(Clone)]
struct CachedUsage {
    five_hour_pct: f64,
    five_hour_resets: String,
    seven_day_pct: f64,
    seven_day_resets: String,
    sonnet_pct: Option<f64>,
    sonnet_resets: Option<String>,
    extra_enabled: bool,
    extra_limit: Option<f64>,
    extra_used: Option<f64>,
    extra_pct: Option<f64>,
    /// Provenance observed by `UsageManager` when these numbers were produced —
    /// the OAuth handshake or the on-disk cache. Carried through so the UI can
    /// state how it knows, instead of the old hard-coded `api` label.
    source: String,
}

static SHARED: std::sync::OnceLock<Arc<Mutex<CachedData>>> = std::sync::OnceLock::new();

fn shared() -> &'static Arc<Mutex<CachedData>> {
    SHARED.get_or_init(|| Arc::new(Mutex::new(CachedData::default())))
}

fn current_provider() -> Provider {
    shared()
        .lock()
        .ok()
        .map(|d| d.active_provider)
        .unwrap_or_else(cc_discord_presence::provider::load_active_provider)
}

fn plan_name_from_key(key: &str) -> String {
    cc_discord_presence::plan::name_from_key(key)
}

fn plan_key_from_override(name: &str) -> Option<&'static str> {
    cc_discord_presence::plan::key_from_override(name)
}

fn log_save_error(scope: &str, result: anyhow::Result<()>) {
    if let Err(err) = result {
        tracing::warn!(scope, error = %err, "failed to save Pulse configuration");
    }
}

fn codex_plan_key_from_tier(tier: DetectedPlanTier) -> &'static str {
    match tier {
        DetectedPlanTier::Free => "free",
        DetectedPlanTier::Go => "go",
        DetectedPlanTier::Plus => "plus",
        DetectedPlanTier::Business => "business",
        DetectedPlanTier::Enterprise => "enterprise",
        DetectedPlanTier::Pro5x => "pro_5x",
        DetectedPlanTier::Pro20x => "pro_20x",
        DetectedPlanTier::Unknown => "",
    }
}

/// Loads the master Rich Presence switch and the field visibility flags from the
/// active provider's on-disk config into the shared cache.
///
/// This is the only thing standing between a saved preference and a fresh
/// process: the Claude arm used to hard-code `discord_enabled = true`, so
/// pausing Rich Presence never survived a restart.
pub(crate) fn seed_discord_state_from_disk() {
    seed_discord_state_for(cc_discord_presence::provider::load_active_provider());
}

/// Seeds the cache for an explicitly named provider.
///
/// A provider switch must seed for the provider the user just chose. Re-reading
/// `pulse-provider.json` here would silently revert to the previous provider
/// whenever that file could not be written, leaving the UI branded for one
/// provider while presence and snapshots served another.
pub(crate) fn seed_discord_state_for(provider: Provider) {
    let Ok(mut data) = shared().lock() else {
        return;
    };
    data.active_provider = provider;
    match data.active_provider {
        Provider::Claude => {
            if let Ok(config) = PresenceConfig::load_or_init() {
                data.discord_enabled = config.presence_enabled;
                data.discord_prefs = claude_display_prefs(&config);
            }
        }
        Provider::Codex => {
            if let Ok(config) = CodexPresenceConfig::load_or_init() {
                data.discord_enabled = config.presence_enabled;
                data.discord_prefs = codex_display_prefs(&config);
            }
        }
    }
}

pub fn start_background_poller(app: tauri::AppHandle) {
    INITIAL_SNAPSHOT_READY.store(false, Ordering::Release);
    SNAPSHOT_POLLER_STARTED.store(true, Ordering::Release);
    let data = Arc::clone(shared());

    seed_discord_state_from_disk();

    thread::spawn(move || {
        let mut claude_git = GitBranchCache::new(Duration::from_secs(30));
        let mut claude_parse = SessionParseCache::default();
        let mut codex_git = CodexGitBranchCache::new(Duration::from_secs(30));
        let mut codex_parse = CodexSessionParseCache::default();
        let mut usage_mgr = UsageManager::new();
        let mut claude_config = PresenceConfig::load_or_init().unwrap_or_default();
        let mut codex_config = CodexPresenceConfig::load_or_init().unwrap_or_default();
        let mut claude_discord = ClaudeDiscordPresence::new(claude_config.effective_client_id());
        let mut codex_discord = CodexDiscordPresence::new(codex_config.effective_client_id());
        let mut codex_plan_detector = PlanDetector::new();
        let mut claude_publisher = PublisherLease::new(cc_discord_presence::config::lock_path());
        let mut codex_publisher =
            PublisherLease::new(cc_discord_presence::codex::config::lock_path());
        let mut last_snapshot_hash = None;

        loop {
            let provider = current_provider();
            let (discord_enabled, prefs, force_refresh) = data
                .lock()
                .ok()
                .map(|mut d| {
                    d.active_provider = provider;
                    let req = d.usage_refresh_requested;
                    if req {
                        d.usage_refresh_requested = false;
                    }
                    (d.discord_enabled, d.discord_prefs.clone(), req)
                })
                .unwrap_or((true, DiscordDisplayPrefs::default(), false));

            let (discord_status, discord_publisher) = match provider {
                Provider::Claude => {
                    codex_publisher.release();
                    let publisher_owned = claude_publisher.try_acquire().unwrap_or_else(|error| {
                        tracing::warn!(error = %error, "failed to acquire Claude publisher lease");
                        false
                    });
                    if let Ok(fresh) = PresenceConfig::load_or_init() {
                        claude_config = fresh;
                    }

                    let now = SystemTime::now();
                    let cutoff = now
                        .checked_sub(ACTIVE_CUTOFF)
                        .unwrap_or(SystemTime::UNIX_EPOCH);

                    let mut all = session::collect_active_sessions(
                        &mut claude_git,
                        &mut claude_parse,
                        STALE_THRESHOLD,
                        STICKY_WINDOW,
                    )
                    .unwrap_or_default();

                    if let Some(sl) = read_statusline_data(&mut claude_git) {
                        merge_statusline_into_sessions(&mut all, sl);
                    }

                    let cutoff_chrono = chrono::Utc::now()
                        - chrono::Duration::seconds(ACTIVE_CUTOFF.as_secs() as i64);
                    let active: Vec<_> = all
                        .into_iter()
                        .filter(|s| is_claude_presence_candidate(s, cutoff, cutoff_chrono))
                        .collect();

                    if force_refresh {
                        usage_mgr.invalidate_cache();
                        let usage_cache_path = cc_discord_presence::config::claude_home()
                            .join("discord-presence-usage-cache.json");
                        if let Err(err) = std::fs::remove_file(&usage_cache_path)
                            && err.kind() != std::io::ErrorKind::NotFound
                        {
                            tracing::warn!(
                                path = %usage_cache_path.display(),
                                error = %err,
                                "failed to remove usage cache"
                            );
                        }
                    }

                    let usage = usage_mgr.get_usage();
                    let usage_source = usage_mgr
                        .last_usage_origin()
                        .map(|origin| origin.label())
                        .unwrap_or_else(|| "unknown source".to_string());
                    let detected_plan_key = usage_mgr.detected_plan_key();
                    let cached_usage = usage.as_ref().map(|u| {
                        let fmt_reset = |dt: Option<chrono::DateTime<chrono::Utc>>| -> String {
                            dt.map(|d| d.to_rfc3339())
                                .unwrap_or_else(|| "N/A".to_string())
                        };
                        CachedUsage {
                            five_hour_pct: u.five_hour.utilization,
                            five_hour_resets: fmt_reset(u.five_hour.resets_at),
                            seven_day_pct: u.seven_day.utilization,
                            seven_day_resets: fmt_reset(u.seven_day.resets_at),
                            sonnet_pct: u.sonnet_free.as_ref().map(|s| s.utilization),
                            sonnet_resets: u.sonnet_free.as_ref().map(|s| fmt_reset(s.resets_at)),
                            extra_enabled: u.extra_usage.as_ref().is_some_and(|e| e.is_enabled),
                            extra_limit: u.extra_usage.as_ref().and_then(|e| e.monthly_limit),
                            extra_used: u.extra_usage.as_ref().and_then(|e| e.used_credits),
                            extra_pct: u.extra_usage.as_ref().and_then(|e| e.utilization),
                            source: usage_source.clone(),
                        }
                    });
                    let usage_error = usage_mgr.error_hint_with_countdown();

                    apply_claude_display_prefs(&mut claude_config, &prefs);
                    let manual_plan = PresenceConfig::load_or_init()
                        .ok()
                        .and_then(|cfg| cfg.plan)
                        .filter(|p| !p.trim().is_empty());
                    claude_config.plan = manual_plan.or_else(|| detected_plan_key.clone());

                    persist_live_claude_snapshots(&active);
                    let status = if discord_enabled && publisher_owned {
                        let active_session = preferred_active_session(&active);
                        let limits = latest_limits_source(&active).map(|s| &s.limits);
                        if let Err(err) = claude_discord.update(
                            active_session,
                            limits,
                            usage.as_ref(),
                            &claude_config,
                        ) {
                            tracing::warn!(error = %err, "failed to update Claude Discord presence");
                        }
                        codex_discord.shutdown();
                        claude_discord.status().to_string()
                    } else if !discord_enabled {
                        claude_discord.shutdown();
                        codex_discord.shutdown();
                        "Disabled".to_string()
                    } else {
                        claude_discord.shutdown();
                        codex_discord.shutdown();
                        "Controlled by external daemon".to_string()
                    };

                    if let Ok(mut d) = data.lock() {
                        d.sessions = ActiveSessions::Claude(active);
                        d.claude_usage = cached_usage;
                        d.claude_usage_error = usage_error;
                    }
                    (
                        status,
                        if publisher_owned {
                            "pulse"
                        } else {
                            "external_daemon"
                        }
                        .to_string(),
                    )
                }
                Provider::Codex => {
                    claude_publisher.release();
                    let publisher_owned = codex_publisher.try_acquire().unwrap_or_else(|error| {
                        tracing::warn!(error = %error, "failed to acquire Codex publisher lease");
                        false
                    });
                    if let Ok(fresh) = CodexPresenceConfig::load_or_init() {
                        codex_config = fresh;
                    }
                    let discord_enabled = codex_config.presence_enabled;

                    apply_codex_display_prefs(&mut codex_config, &prefs, CreditsMirror::Apply);

                    let sessions_roots = cc_discord_presence::codex::config::sessions_paths();
                    let active = codex_session::collect_active_sessions_multi(
                        &sessions_roots,
                        STALE_THRESHOLD,
                        STICKY_WINDOW,
                        &mut codex_git,
                        &mut codex_parse,
                        &codex_config.pricing,
                    )
                    .unwrap_or_default();
                    let usage_envelopes = codex_parse.rate_limit_envelopes();
                    let codex_usage = (!usage_envelopes.is_empty()).then(|| {
                        usage_snapshot_from_envelopes(
                            Provider::Codex.as_str(),
                            "Codex JSONL rate_limits",
                            &usage_envelopes,
                        )
                    });
                    let effective_limits = codex_parse.latest_limits_source();

                    let resolved_plan = codex_plan_detector
                        .resolve_from_envelopes(&usage_envelopes, &codex_config.openai_plan);
                    let resolved_service_tier = resolve_service_tier();
                    let opencode_running =
                        cc_discord_presence::codex::process::is_opencode_running();
                    let codex_desktop_running =
                        cc_discord_presence::codex::process::is_desktop_surface_running();
                    let surface_override = codex_fallback_surface(codex_desktop_running);

                    persist_live_codex_snapshots(&active, &codex_config, surface_override);
                    let status = if discord_enabled && publisher_owned {
                        let active_session = codex_session::preferred_active_session(&active);
                        let limits = effective_limits.as_ref().map(|item| &item.limits);
                        if let Err(err) = codex_discord.update(
                            active_session,
                            limits,
                            &resolved_plan,
                            &resolved_service_tier,
                            &codex_config,
                            surface_override,
                        ) {
                            tracing::warn!(error = %err, "failed to update Codex Discord presence");
                        }
                        claude_discord.shutdown();
                        codex_discord.status().to_string()
                    } else if !discord_enabled {
                        claude_discord.shutdown();
                        codex_discord.shutdown();
                        "Disabled".to_string()
                    } else {
                        claude_discord.shutdown();
                        codex_discord.shutdown();
                        "Controlled by external daemon".to_string()
                    };

                    if let Ok(mut d) = data.lock() {
                        d.discord_enabled = discord_enabled;
                        d.sessions = ActiveSessions::Codex(active);
                        d.codex_opencode_running = opencode_running;
                        d.codex_desktop_surface_running = codex_desktop_running;
                        d.claude_usage = None;
                        d.claude_usage_error = None;
                        d.codex_usage = codex_usage;
                        d.codex_limits = effective_limits;
                    }
                    (
                        status,
                        if publisher_owned {
                            "pulse"
                        } else {
                            "external_daemon"
                        }
                        .to_string(),
                    )
                }
            };

            if let Ok(mut d) = data.lock() {
                d.discord_status = discord_status;
                d.discord_publisher = discord_publisher;
            }
            INITIAL_SNAPSHOT_READY.store(true, Ordering::Release);

            if let Ok(snapshot) = get_app_snapshot() {
                let snapshot_hash = app_snapshot_fingerprint(&snapshot);
                if last_snapshot_hash != Some(snapshot_hash) {
                    if let Err(error) = app.emit("pulse://snapshot", &snapshot) {
                        tracing::warn!(error = %error, "failed to emit Pulse snapshot");
                    }
                    last_snapshot_hash = Some(snapshot_hash);
                }
            }

            thread::sleep(REFRESH_INTERVAL);
        }
    });
}

fn is_claude_presence_candidate(
    session: &ClaudeSessionSnapshot,
    cutoff: SystemTime,
    cutoff_chrono: chrono::DateTime<chrono::Utc>,
) -> bool {
    if session.is_subagent {
        return false;
    }

    session
        .last_token_event_at
        .is_some_and(|ts| ts >= cutoff_chrono)
        || session.last_activity >= cutoff
}

fn read_claude_sessions() -> Vec<ClaudeSessionSnapshot> {
    shared()
        .lock()
        .ok()
        .map_or_else(Vec::new, |d| match &d.sessions {
            ActiveSessions::Claude(sessions) => sessions.clone(),
            _ => Vec::new(),
        })
}

fn read_codex_sessions() -> Vec<CodexSessionSnapshot> {
    shared()
        .lock()
        .ok()
        .map_or_else(Vec::new, |d| match &d.sessions {
            ActiveSessions::Codex(sessions) => sessions.clone(),
            _ => Vec::new(),
        })
}

fn read_codex_desktop_surface_running() -> bool {
    shared()
        .lock()
        .ok()
        .is_some_and(|d| d.codex_desktop_surface_running)
}

fn codex_fallback_surface(desktop_surface_running: bool) -> PresenceSurface {
    if desktop_surface_running {
        PresenceSurface::Desktop
    } else {
        PresenceSurface::Cli
    }
}

fn codex_session_surface(
    session: &CodexSessionSnapshot,
    fallback_surface: PresenceSurface,
) -> PresenceSurface {
    session.detected_surface().unwrap_or(fallback_surface)
}

fn current_live_session_infos() -> Vec<SessionInfo> {
    match current_provider() {
        Provider::Claude => build_claude_session_infos(&read_claude_sessions()),
        Provider::Codex => {
            let config = CodexPresenceConfig::load_or_init().unwrap_or_default();
            build_codex_session_infos(
                &read_codex_sessions(),
                &config,
                codex_fallback_surface(read_codex_desktop_surface_running()),
            )
        }
    }
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub version: &'static str,
    pub uptime_seconds: u64,
    pub discord_status: String,
    pub discord_enabled: bool,
}

#[derive(Serialize)]
pub struct AppSnapshot {
    pub revision: u32,
    pub health: HealthResponse,
    pub metrics: MetricsResponse,
    pub sessions: Vec<SessionInfo>,
    pub rate_limits: Option<RateLimitInfo>,
    pub discord_preview: DiscordPresencePreview,
    pub discord_settings: DiscordSettings,
    pub plan: PlanInfo,
}

#[tauri::command]
pub fn get_app_snapshot() -> Result<AppSnapshot, String> {
    let deadline = Instant::now() + REFRESH_INTERVAL + Duration::from_secs(1);
    while SNAPSHOT_POLLER_STARTED.load(Ordering::Acquire)
        && !INITIAL_SNAPSHOT_READY.load(Ordering::Acquire)
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(25));
    }
    Ok(AppSnapshot {
        revision: 1,
        health: get_health(),
        metrics: get_metrics(),
        sessions: get_live_sessions(),
        rate_limits: get_rate_limits(),
        discord_preview: get_discord_preview(),
        discord_settings: get_discord_settings()?,
        plan: get_plan_info(),
    })
}

fn app_snapshot_fingerprint(snapshot: &AppSnapshot) -> u64 {
    semantic_snapshot_fingerprint(serde_json::to_value(snapshot).unwrap_or_default())
}

fn semantic_snapshot_fingerprint(mut value: serde_json::Value) -> u64 {
    if let Some(root) = value.as_object_mut() {
        if let Some(health) = root
            .get_mut("health")
            .and_then(serde_json::Value::as_object_mut)
        {
            health.remove("uptime_seconds");
        }
        if let Some(preview) = root
            .get_mut("discord_preview")
            .and_then(serde_json::Value::as_object_mut)
        {
            preview.remove("duration_secs");
        }
        if let Some(sessions) = root
            .get_mut("sessions")
            .and_then(serde_json::Value::as_array_mut)
        {
            for session in sessions {
                if let Some(session) = session.as_object_mut() {
                    session.remove("duration_secs");
                    session.remove("tokens_per_sec");
                }
            }
        }
    }
    let mut hasher = DefaultHasher::new();
    value.to_string().hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiscordPresencePreview {
    pub provider: String,
    pub app_name: String,
    pub details: String,
    pub state: String,
    pub large_image_key: String,
    pub large_text: String,
    pub small_image_key: Option<String>,
    pub small_text: Option<String>,
    pub has_session: bool,
    pub duration_secs: u64,
}

#[derive(Serialize)]
pub struct MetricsResponse {
    pub total_cost: f64,
    pub input_tokens: u64,
    pub pure_input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
    pub session_count: usize,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
    pub cache_hit_ratio: f64,
    pub models: Vec<ModelMetric>,
}

#[derive(Serialize)]
pub struct ModelMetric {
    pub model: String,
    pub sessions: usize,
    pub cost: f64,
    pub tokens: u64,
}

#[derive(Serialize)]
pub struct SubagentDetail {
    pub agent_type: String,
    pub model: String,
    pub tokens: u64,
    pub cost: f64,
    pub activity: String,
}

fn read_session_name(session_id: &str) -> Option<String> {
    let meta_dir = cc_discord_presence::config::claude_home()
        .join("usage-data")
        .join("session-meta");
    let meta_file = meta_dir.join(format!("{}.json", session_id));
    let data = std::fs::read_to_string(meta_file).ok()?;
    let json: serde_json::Value = serde_json::from_str(&data).ok()?;
    let prompt = json.get("first_prompt")?.as_str()?;
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return None;
    }
    let truncated = if trimmed.len() > 80 {
        let end = trimmed
            .char_indices()
            .take(80)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(80);
        format!("{}...", &trimmed[..end])
    } else {
        trimmed.to_string()
    };
    Some(truncated)
}

#[derive(Serialize)]
pub struct SessionInfo {
    pub provider: String,
    pub app_name: Option<String>,
    pub session_id: String,
    pub session_name: Option<String>,
    pub project: String,
    pub model: String,
    pub model_id: String,
    pub context_window: String,
    pub cost: f64,
    pub tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    pub context_used_tokens: u64,
    pub context_window_tokens: u64,
    pub branch: Option<String>,
    pub activity: String,
    pub activity_target: Option<String>,
    pub effort: String,
    /// True when the effort came from an explicit JSONL injection. False means
    /// we only have the `settings.json` default and the live Claude Desktop
    /// composer selection may differ (Claude Desktop keeps it in memory).
    pub effort_explicit: bool,
    pub is_idle: bool,
    pub started_at: Option<String>,
    pub duration_secs: u64,
    pub has_thinking: bool,
    pub workflow_label: Option<String>,
    pub subagent_count: usize,
    pub subagents: Vec<SubagentDetail>,
    pub tokens_per_sec: f64,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
    /// Speed tier of the most recent turn ("fast"/"standard").
    pub speed: String,
    /// True when the most recent turn ran in fast mode (priority speed).
    pub fast: bool,
    /// Service tier of the most recent turn ("priority"/"standard"), display only.
    pub service_tier: Option<String>,
    /// This session's model's currently-active introductory-pricing window, if
    /// any. `None` both for models with no promo and for a promo'd model once
    /// its window has closed — the frontend never computes its own expiry.
    pub intro_pricing: Option<cost::IntroPricingBadge>,
    /// True when this session's model uses a newer tokenizer that bills more
    /// tokens than its predecessor for the same input text at an unchanged
    /// per-token rate (currently: Opus 4.7+, Claude Sonnet 5).
    pub has_inflated_tokenizer: bool,
}

#[derive(Serialize)]
pub struct RateLimitInfo {
    pub provider: String,
    pub usage: Option<UsageSnapshot>,
    pub five_hour_pct: f64,
    pub five_hour_resets: String,
    pub five_hour_label: String,
    pub five_hour_window_minutes: Option<u64>,
    pub seven_day_pct: f64,
    pub seven_day_resets: String,
    pub seven_day_label: String,
    pub seven_day_window_minutes: Option<u64>,
    pub sonnet_pct: Option<f64>,
    pub sonnet_resets: Option<String>,
    pub extra_enabled: bool,
    pub extra_limit: Option<f64>,
    pub extra_used: Option<f64>,
    pub extra_pct: Option<f64>,
    pub source: String,
}

fn format_codex_window_label(minutes: Option<u64>, fallback: &str) -> String {
    match minutes {
        Some(300) => "5h Window".into(),
        Some(10080) => "7d Window".into(),
        Some(1440) => "24h Window".into(),
        Some(value) if value > 0 => {
            if value % 1440 == 0 {
                format!("{}d Window", value / 1440)
            } else if value % 60 == 0 {
                format!("{}h Window", value / 60)
            } else {
                format!("{value}m Window")
            }
        }
        _ => fallback.into(),
    }
}

fn build_claude_session_infos(snapshots: &[ClaudeSessionSnapshot]) -> Vec<SessionInfo> {
    let now = SystemTime::now();
    let idle = now
        .checked_sub(IDLE_CUTOFF)
        .unwrap_or(SystemTime::UNIX_EPOCH);

    snapshots
        .iter()
        .map(|s| {
            let is_idle = s.last_activity < idle;
            let duration_secs = s
                .started_at
                .map(|st| (chrono::Utc::now() - st).num_seconds().max(0) as u64)
                .unwrap_or(0);
            let model_id_for_speed = s.model.clone().unwrap_or_default();
            let fast = s.speed.is_fast() && cost::is_fast_capable(&model_id_for_speed);
            let ic = s.input_cost;
            let oc = s.output_cost;
            let cwc = s.cache_write_cost;
            let crc = s.cache_read_cost;
            let tps = if s.total_api_duration_ms > 0 {
                s.output_tokens as f64 / (s.total_api_duration_ms as f64 / 1000.0)
            } else {
                0.0
            };
            let model_id_raw = s.model.clone().unwrap_or_default();
            let intro_pricing = cost::active_intro_pricing(&model_id_raw, chrono::Utc::now());
            let has_inflated_tokenizer = cost::has_inflated_tokenizer(&model_id_raw);
            let has_1m = cost::is_ga_1m_context(&model_id_raw)
                || model_id_raw.contains("[1m]")
                || s.max_turn_api_input > 200_000;
            let ctx_window_tokens = if has_1m { 1_000_000 } else { 200_000 };
            let ctx_used_tokens = s.current_context_tokens.min(ctx_window_tokens);
            let ctx_window = if has_1m { "1M" } else { "200K" }.to_string();
            let activity = s
                .activity
                .as_ref()
                .map_or("Idle".into(), |a| a.action_text().to_string());
            let activity = if !is_idle && activity == "Idle" {
                "Thinking".to_string()
            } else {
                activity
            };
            let subagent_details: Vec<SubagentDetail> = s
                .subagents
                .iter()
                .map(|sa| SubagentDetail {
                    agent_type: sa.agent_type.clone(),
                    model: sa
                        .model_display
                        .clone()
                        .or(sa.model.as_ref().map(|m| cost::model_display_name(m)))
                        .unwrap_or_else(|| "Unknown".into()),
                    tokens: sa.tokens,
                    cost: sa.cost,
                    activity: sa
                        .activity
                        .as_ref()
                        .map_or("Idle".into(), |a| a.action_text().to_string()),
                })
                .collect();
            let background_agent_count = s.background_work.active_agent_count;
            let subagent_count = subagent_details.len().max(background_agent_count);
            let workflow_label = if s.background_work.workflow_active {
                Some("ULTRACODE".to_string())
            } else {
                None
            };
            let session_name = read_session_name(&s.session_id);
            SessionInfo {
                provider: Provider::Claude.as_str().to_string(),
                app_name: None,
                session_id: s.session_id.clone(),
                session_name,
                project: s.project_name.clone(),
                model: s
                    .model_display
                    .clone()
                    .or(s.model.as_ref().map(|m| cost::model_display_name(m)))
                    .unwrap_or_else(|| "Unknown".into()),
                model_id: model_id_raw,
                context_window: ctx_window,
                cost: s.total_cost,
                tokens: s.session_total_tokens.unwrap_or(0),
                input_tokens: s.input_tokens,
                output_tokens: s.output_tokens,
                cache_write_tokens: s.cache_creation_tokens,
                cache_read_tokens: s.cache_read_tokens,
                context_used_tokens: ctx_used_tokens,
                context_window_tokens: ctx_window_tokens,
                branch: s.git_branch.clone(),
                activity,
                activity_target: s.activity.as_ref().and_then(|a| a.target.clone()),
                effort: s.reasoning_effort.label().to_string(),
                effort_explicit: s.reasoning_effort_explicit,
                is_idle,
                started_at: s.started_at.map(|t| t.to_rfc3339()),
                duration_secs,
                has_thinking: s.has_thinking_blocks,
                workflow_label,
                subagent_count,
                subagents: subagent_details,
                tokens_per_sec: tps,
                input_cost: ic,
                output_cost: oc,
                cache_write_cost: cwc,
                cache_read_cost: crc,
                speed: s.speed.as_str().to_string(),
                fast,
                service_tier: s.service_tier.clone(),
                intro_pricing,
                has_inflated_tokenizer,
            }
        })
        .collect()
}

fn build_codex_session_infos(
    snapshots: &[CodexSessionSnapshot],
    config: &CodexPresenceConfig,
    fallback_surface: PresenceSurface,
) -> Vec<SessionInfo> {
    let now = SystemTime::now();
    let idle = now
        .checked_sub(IDLE_CUTOFF)
        .unwrap_or(SystemTime::UNIX_EPOCH);

    snapshots
        .iter()
        .map(|s| {
            let is_idle = s.last_activity < idle;
            let duration_secs = s
                .started_at
                .map(|st| (chrono::Utc::now() - st).num_seconds().max(0) as u64)
                .unwrap_or(0);
            let model_id_raw = s.model.clone().unwrap_or_default();
            let model_key = model_id_raw.clone();
            let fast = s.speed.mode == CodexSpeedMode::Fast;
            let display_name = if model_key.is_empty() {
                "Unknown".to_string()
            } else {
                cc_discord_presence::codex::util::format_model_display(
                    &model_key,
                    s.reasoning_effort,
                    fast,
                )
            };
            let context_window = s
                .context_window
                .as_ref()
                .map(|snapshot| snapshot.window_tokens)
                .or_else(|| {
                    cc_discord_presence::codex::cost::default_model_context_window(&model_key)
                })
                .unwrap_or(0);
            let context_window_label = if context_window == 0 {
                "Unknown".to_string()
            } else {
                cc_discord_presence::codex::util::format_tokens(context_window)
            };
            let input_total = codex_total_input_tokens(s);
            let cached_input = s.cached_input_tokens_total;
            let context_used_tokens = s
                .context_window
                .as_ref()
                .map(|snapshot| snapshot.used_tokens.min(context_window))
                .unwrap_or(0);
            let activity = s
                .activity
                .as_ref()
                .map_or("Idle".into(), |a| a.action_text().to_string());
            let activity = if !is_idle && activity == "Idle" {
                "Thinking".to_string()
            } else {
                activity
            };
            let activity_target = s.activity.as_ref().and_then(|a| a.target.clone());
            let surface = codex_session_surface(s, fallback_surface);
            SessionInfo {
                provider: Provider::Codex.as_str().to_string(),
                app_name: Some(
                    surface
                        .label(config.display.desktop_presence_design)
                        .to_string(),
                ),
                session_id: s.session_id.clone(),
                session_name: None,
                project: s.project_name.clone(),
                model: display_name,
                model_id: model_key,
                context_window: context_window_label,
                cost: s.known_cost_usd.unwrap_or(0.0),
                tokens: s
                    .session_total_tokens
                    .unwrap_or(input_total + s.output_tokens_total),
                input_tokens: input_total,
                output_tokens: s.output_tokens_total,
                cache_write_tokens: 0,
                cache_read_tokens: cached_input,
                context_used_tokens,
                context_window_tokens: context_window,
                branch: s.git_branch.clone(),
                activity,
                activity_target,
                effort: s
                    .reasoning_effort
                    .map(|effort| effort.label().to_string())
                    .unwrap_or_else(|| "Unknown".to_string()),
                effort_explicit: s.reasoning_effort.is_some(),
                is_idle,
                started_at: s.started_at.map(|t| t.to_rfc3339()),
                duration_secs,
                has_thinking: s.reasoning_effort.is_some(),
                workflow_label: None,
                subagent_count: 0,
                subagents: Vec::new(),
                tokens_per_sec: 0.0,
                input_cost: s.cost_breakdown.input_cost_usd,
                output_cost: s.cost_breakdown.output_cost_usd,
                cache_write_cost: 0.0,
                cache_read_cost: s.cost_breakdown.cached_input_cost_usd,
                speed: s.speed.mode.label().to_ascii_lowercase(),
                fast,
                service_tier: s.speed.known.then(|| {
                    if fast {
                        "priority".to_string()
                    } else {
                        "standard".to_string()
                    }
                }),
                intro_pricing: None,
                has_inflated_tokenizer: false,
            }
        })
        .collect()
}

fn codex_total_input_tokens(snapshot: &CodexSessionSnapshot) -> u64 {
    snapshot
        .input_tokens_total
        .max(snapshot.cached_input_tokens_total)
}

fn persist_live_session_infos(provider: Provider, result: &[SessionInfo]) {
    let active_ids: Vec<String> = result.iter().map(|s| s.session_id.clone()).collect();
    let active_keys: HashSet<String> = active_ids
        .iter()
        .map(|id| format!("{}:{id}", provider.as_str()))
        .collect();
    let fingerprints = SESSION_FINGERPRINTS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut fingerprints = fingerprints.lock().ok();
    let mut changed_count = 0;
    for s in result {
        let key = format!("{}:{}", provider.as_str(), s.session_id);
        let fingerprint = persistent_session_fingerprint(s);
        let unchanged = fingerprints
            .as_ref()
            .and_then(|items| items.get(&key))
            .is_some_and(|previous| *previous == fingerprint);
        if !unchanged {
            crate::db::upsert_session(s);
            changed_count += 1;
            if let Some(items) = fingerprints.as_mut() {
                items.insert(key, fingerprint);
            }
        }
    }
    if let Some(items) = fingerprints.as_mut() {
        let provider_prefix = format!("{}:", provider.as_str());
        items.retain(|key, _| !key.starts_with(&provider_prefix) || active_keys.contains(key));
    }
    crate::db::mark_inactive(provider.as_str(), &active_ids);
    crate::db::checkpoint_wal_after_writes(changed_count);
}

fn persistent_session_fingerprint(session: &SessionInfo) -> u64 {
    let mut value = serde_json::to_value(session).unwrap_or_default();
    if let Some(session) = value.as_object_mut() {
        // These values advance with wall-clock time even when no telemetry changed.
        // They are presentation-only and must not force SQLite writes every poll.
        session.remove("duration_secs");
        session.remove("tokens_per_sec");
    }
    let mut hasher = DefaultHasher::new();
    value.to_string().hash(&mut hasher);
    hasher.finish()
}

fn persist_live_claude_snapshots(snapshots: &[ClaudeSessionSnapshot]) {
    let result = build_claude_session_infos(snapshots);
    persist_live_session_infos(Provider::Claude, &result);
}

fn persist_live_codex_snapshots(
    snapshots: &[CodexSessionSnapshot],
    config: &CodexPresenceConfig,
    fallback_surface: PresenceSurface,
) {
    let result = build_codex_session_infos(snapshots, config, fallback_surface);
    persist_live_session_infos(Provider::Codex, &result);
}

#[tauri::command]
pub fn get_health() -> HealthResponse {
    let (discord_status, discord_enabled) = shared()
        .lock()
        .ok()
        .map(|d| (d.discord_status.clone(), d.discord_enabled))
        .unwrap_or_else(|| ("Unknown".into(), true));
    HealthResponse {
        version: env!("CARGO_PKG_VERSION"),
        uptime_seconds: uptime_secs(),
        discord_status,
        discord_enabled,
    }
}

#[tauri::command]
pub fn set_discord_enabled(enabled: bool) -> Result<DiscordSettings, String> {
    // Persist for whichever provider is active. Claude used to skip this branch
    // entirely, so the master switch lived only in `shared()` and every restart
    // silently turned Rich Presence back on.
    match current_provider() {
        Provider::Codex => {
            let mut config =
                CodexPresenceConfig::load_or_init().map_err(|error| error.to_string())?;
            config.presence_enabled = enabled;
            config.save().map_err(|error| error.to_string())?;
        }
        Provider::Claude => {
            let mut config = PresenceConfig::load_or_init().map_err(|error| error.to_string())?;
            config.presence_enabled = enabled;
            config.save().map_err(|error| error.to_string())?;
        }
    }

    let mut data = shared()
        .lock()
        .map_err(|_| "Discord settings state is unavailable".to_string())?;
    data.discord_enabled = enabled;
    drop(data);
    get_discord_settings()
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn set_discord_display_prefs(
    show_project: bool,
    show_branch: bool,
    show_model: bool,
    show_activity: bool,
    show_tokens: bool,
    show_cost: bool,
    show_limits: bool,
    show_credits: bool,
    show_context: bool,
    show_systems: bool,
) -> Result<DiscordSettings, String> {
    let prefs = DiscordDisplayPrefs {
        show_project,
        show_branch,
        show_model,
        show_activity,
        show_tokens,
        show_cost,
        show_limits,
        show_credits,
        show_context,
        show_systems,
    };
    // Both provider configs are kept in sync so the field switches mean the same
    // thing after a provider swap, but the order matters. The ACTIVE provider is
    // written first and its failure is fatal; the mirror is only attempted once
    // the user's own config has landed. Writing both up front meant a failed
    // active write still persisted the mirror, so the change reappeared after a
    // provider swap even though the UI had reported it as failed and rolled back.
    let save_claude = || -> Result<(), String> {
        let mut config = PresenceConfig::load_or_init().map_err(|error| error.to_string())?;
        apply_claude_display_prefs(&mut config, &prefs);
        config.save().map_err(|error| error.to_string())
    };
    let credits = if current_provider() == Provider::Codex {
        CreditsMirror::Apply
    } else {
        CreditsMirror::Preserve
    };
    let save_codex = || -> Result<(), String> {
        let mut config = CodexPresenceConfig::load_or_init().map_err(|error| error.to_string())?;
        apply_codex_display_prefs(&mut config, &prefs, credits);
        config.save().map_err(|error| error.to_string())
    };

    match current_provider() {
        Provider::Claude => {
            save_claude()?;
            log_mirror_error(Provider::Codex, save_codex());
        }
        Provider::Codex => {
            save_codex()?;
            log_mirror_error(Provider::Claude, save_claude());
        }
    }

    shared()
        .lock()
        .map_err(|_| "Discord settings state is unavailable".to_string())?
        .discord_prefs = prefs;
    get_discord_settings()
}

/// Reports a failed write to the *inactive* provider's config without failing the
/// user's action. The active provider is the one on screen; losing the mirror is
/// a degraded sync, not a lost setting.
fn log_mirror_error(provider: Provider, result: Result<(), String>) {
    if let Err(error) = result {
        tracing::warn!(
            provider = provider.as_str(),
            error = %error,
            "failed to mirror Discord display preferences to the inactive provider"
        );
    }
}

#[tauri::command]
pub fn set_codex_desktop_design(design: String) -> Result<DiscordSettings, String> {
    let design = match design.trim().to_ascii_lowercase().as_str() {
        "codex_app" => DesktopPresenceDesign::CodexApp,
        "chatgpt_app" => DesktopPresenceDesign::ChatGptApp,
        _ => return Err("Desktop design must be codex_app or chatgpt_app".to_string()),
    };
    let mut config = CodexPresenceConfig::load_or_init().map_err(|error| error.to_string())?;
    config.display.desktop_presence_design = design;
    config.save().map_err(|error| error.to_string())?;
    get_discord_settings()
}

/// Ask the background poller to drop its usage cache and hit the API on the
/// next tick. The UI's refresh button is wired to this — real data within ~5s.
#[tauri::command]
pub fn refresh_usage() {
    if let Ok(mut d) = shared().lock() {
        d.usage_refresh_requested = true;
    }
}

#[tauri::command]
pub fn get_metrics() -> MetricsResponse {
    let sessions = current_live_session_infos();
    let (mut cost, mut inp, mut out, mut cw, mut cr, mut tot) = (0.0, 0u64, 0u64, 0u64, 0u64, 0u64);
    for s in &sessions {
        cost += s.cost;
        inp += s.input_tokens;
        out += s.output_tokens;
        cw += s.cache_write_tokens;
        cr += s.cache_read_tokens;
        tot += s.tokens;
    }

    let (mut ic, mut oc, mut cwc, mut crc) = (0.0, 0.0, 0.0, 0.0);
    let mut model_map: std::collections::HashMap<String, (usize, f64, u64)> =
        std::collections::HashMap::new();
    for s in &sessions {
        ic += s.input_cost;
        oc += s.output_cost;
        cwc += s.cache_write_cost;
        crc += s.cache_read_cost;

        let entry = model_map.entry(s.model.clone()).or_insert((0, 0.0, 0));
        entry.0 += 1;
        entry.1 += s.cost;
        entry.2 += s.tokens;
    }

    let pure_inp_total = inp.saturating_sub(cw).saturating_sub(cr);
    let cache_total = cr as f64 + pure_inp_total as f64;
    let cache_hit_ratio = if cache_total > 0.0 {
        cr as f64 / cache_total * 100.0
    } else {
        0.0
    };

    let mut models: Vec<ModelMetric> = model_map
        .into_iter()
        .map(|(model, (sessions, cost, tokens))| ModelMetric {
            model,
            sessions,
            cost,
            tokens,
        })
        .collect();
    models.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    MetricsResponse {
        total_cost: cost,
        input_tokens: inp,
        pure_input_tokens: inp.saturating_sub(cw).saturating_sub(cr),
        output_tokens: out,
        cache_write_tokens: cw,
        cache_read_tokens: cr,
        total_tokens: tot,
        session_count: sessions.len(),
        input_cost: ic,
        output_cost: oc,
        cache_write_cost: cwc,
        cache_read_cost: crc,
        cache_hit_ratio,
        models,
    }
}

#[tauri::command]
pub fn get_live_sessions() -> Vec<SessionInfo> {
    current_live_session_infos()
}

#[tauri::command]
pub fn get_discord_preview() -> DiscordPresencePreview {
    let data = shared()
        .lock()
        .ok()
        .map(|data| data.clone())
        .unwrap_or_default();
    build_discord_presence_preview(&data)
}

fn build_discord_presence_preview(data: &CachedData) -> DiscordPresencePreview {
    match data.active_provider {
        Provider::Claude => {
            let mut config = PresenceConfig::load_or_init().unwrap_or_default();
            apply_claude_display_prefs(&mut config, &data.discord_prefs);
            let sessions = match &data.sessions {
                ActiveSessions::Claude(sessions) => sessions.as_slice(),
                _ => &[],
            };
            build_claude_discord_preview(sessions, &config)
        }
        Provider::Codex => {
            let mut config = CodexPresenceConfig::load_or_init().unwrap_or_default();
            apply_codex_display_prefs(&mut config, &data.discord_prefs, CreditsMirror::Apply);
            let sessions = match &data.sessions {
                ActiveSessions::Codex(sessions) => sessions.as_slice(),
                _ => &[],
            };
            build_codex_discord_preview(sessions, &config, data.codex_desktop_surface_running)
        }
    }
}

fn apply_claude_display_prefs(config: &mut PresenceConfig, prefs: &DiscordDisplayPrefs) {
    config.privacy.show_project_name = prefs.show_project;
    config.privacy.show_git_branch = prefs.show_branch;
    config.privacy.show_model = prefs.show_model;
    config.privacy.show_activity = prefs.show_activity;
    config.privacy.show_tokens = prefs.show_tokens;
    config.privacy.show_cost = prefs.show_cost;
    config.privacy.show_limits = prefs.show_limits;
    config.privacy.show_context = prefs.show_context;
    config.privacy.show_systems = prefs.show_systems;
}

/// Applies the shared field switches to the Codex config.
///
/// `credits` is passed separately because it is Codex-only. A payload composed
/// while Claude is active always carries `show_credits = false` — Claude has no
/// such field — so copying it here silently disabled Credits on the Codex side
/// every time an unrelated Claude toggle was saved.
fn apply_codex_display_prefs(
    config: &mut CodexPresenceConfig,
    prefs: &DiscordDisplayPrefs,
    credits: CreditsMirror,
) {
    config.privacy.show_project_name = prefs.show_project;
    config.privacy.show_git_branch = prefs.show_branch;
    config.privacy.show_model = prefs.show_model;
    config.privacy.show_activity = prefs.show_activity;
    config.privacy.show_tokens = prefs.show_tokens;
    config.privacy.show_cost = prefs.show_cost;
    config.privacy.show_limits = prefs.show_limits;
    if credits == CreditsMirror::Apply {
        config.privacy.show_credits = prefs.show_credits;
    }
    config.privacy.show_context = prefs.show_context;
    config.privacy.show_systems = prefs.show_systems;
}

/// Whether the incoming payload actually carries a meaningful `show_credits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CreditsMirror {
    /// Composed while Codex was active — the value is the user's choice.
    Apply,
    /// Composed while a provider without Credits was active — keep what Codex has.
    Preserve,
}

fn claude_display_prefs(config: &PresenceConfig) -> DiscordDisplayPrefs {
    DiscordDisplayPrefs {
        show_project: config.privacy.show_project_name,
        show_branch: config.privacy.show_git_branch,
        show_model: config.privacy.show_model,
        show_activity: config.privacy.show_activity,
        show_tokens: config.privacy.show_tokens,
        show_cost: config.privacy.show_cost,
        show_limits: config.privacy.show_limits,
        show_credits: false,
        show_context: config.privacy.show_context,
        show_systems: config.privacy.show_systems,
    }
}

fn codex_display_prefs(config: &CodexPresenceConfig) -> DiscordDisplayPrefs {
    DiscordDisplayPrefs {
        show_project: config.privacy.show_project_name,
        show_branch: config.privacy.show_git_branch,
        show_model: config.privacy.show_model,
        show_activity: config.privacy.show_activity,
        show_tokens: config.privacy.show_tokens,
        show_cost: config.privacy.show_cost,
        show_limits: config.privacy.show_limits,
        show_credits: config.privacy.show_credits,
        show_context: config.privacy.show_context,
        show_systems: config.privacy.show_systems,
    }
}

fn desktop_design_key(design: DesktopPresenceDesign) -> &'static str {
    match design {
        DesktopPresenceDesign::CodexApp => "codex_app",
        DesktopPresenceDesign::ChatGptApp => "chatgpt_app",
    }
}

fn build_discord_settings(
    provider: Provider,
    cached: &CachedData,
    claude_config: Option<&PresenceConfig>,
    codex_config: Option<&CodexPresenceConfig>,
) -> DiscordSettings {
    let (enabled, display_prefs, desktop_design, supports_desktop_design, field_order) =
        match provider {
            Provider::Claude => (
                claude_config
                    .map(|config| config.presence_enabled)
                    .unwrap_or(cached.discord_enabled),
                claude_config
                    .map(claude_display_prefs)
                    .unwrap_or_else(|| cached.discord_prefs.clone()),
                None,
                false,
                Vec::new(),
            ),
            Provider::Codex => (
                codex_config
                    .map(|config| config.presence_enabled)
                    .unwrap_or(cached.discord_enabled),
                codex_config
                    .map(codex_display_prefs)
                    .unwrap_or_else(|| cached.discord_prefs.clone()),
                codex_config.map(|config| {
                    desktop_design_key(config.display.desktop_presence_design).to_string()
                }),
                true,
                codex_config
                    .map(|config| {
                        config
                            .display
                            .presence_layout
                            .fields
                            .iter()
                            .map(|item| item.field.as_str().to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
        };

    DiscordSettings {
        provider: provider.as_str().to_string(),
        enabled,
        status: cached.discord_status.clone(),
        publisher: cached.discord_publisher.clone(),
        display_prefs,
        desktop_design,
        supports_desktop_design,
        supports_field_order: provider == Provider::Codex,
        supports_credits: provider == Provider::Codex,
        field_order,
    }
}

#[tauri::command]
pub fn set_discord_field_order(order: Vec<String>) -> Result<DiscordSettings, String> {
    if current_provider() != Provider::Codex {
        return Err("Field ordering is currently available for Codex presence".to_string());
    }
    let parsed: Vec<PresenceFieldId> = order
        .iter()
        .map(|value| {
            PresenceFieldId::parse(value).ok_or_else(|| format!("Unknown presence field: {value}"))
        })
        .collect::<Result<_, _>>()?;
    let unique: HashSet<PresenceFieldId> = parsed.iter().copied().collect();
    if parsed.len() != PresenceFieldId::ALL.len() || unique.len() != PresenceFieldId::ALL.len() {
        return Err("Field order must contain every field exactly once".to_string());
    }
    let mut config = CodexPresenceConfig::load_or_init().map_err(|error| error.to_string())?;
    config.display.presence_layout.fields.sort_by_key(|item| {
        parsed
            .iter()
            .position(|field| *field == item.field)
            .unwrap_or(usize::MAX)
    });
    config.save().map_err(|error| error.to_string())?;
    get_discord_settings()
}

#[tauri::command]
pub fn get_discord_settings() -> Result<DiscordSettings, String> {
    let provider = current_provider();
    let cached = shared()
        .lock()
        .map_err(|_| "Discord settings state is unavailable".to_string())?
        .clone();
    match provider {
        Provider::Claude => {
            let config = PresenceConfig::load_or_init().map_err(|error| error.to_string())?;
            Ok(build_discord_settings(
                provider,
                &cached,
                Some(&config),
                None,
            ))
        }
        Provider::Codex => {
            let config = CodexPresenceConfig::load_or_init().map_err(|error| error.to_string())?;
            Ok(build_discord_settings(
                provider,
                &cached,
                None,
                Some(&config),
            ))
        }
    }
}

fn build_claude_discord_preview(
    sessions: &[ClaudeSessionSnapshot],
    config: &PresenceConfig,
) -> DiscordPresencePreview {
    let Some(session) = preferred_active_session(sessions) else {
        return DiscordPresencePreview {
            provider: Provider::Claude.as_str().to_string(),
            app_name: "Claude Code".to_string(),
            details: "Claude Code".to_string(),
            state: "Waiting for session".to_string(),
            large_image_key: config.display.large_image_key.clone(),
            large_text: config.display.large_text.clone(),
            small_image_key: None,
            small_text: None,
            has_session: false,
            duration_secs: 0,
        };
    };

    let limits = latest_limits_source(sessions).map(|source| &source.limits);
    let (details, state, _tooltip) = claude_presence_lines(session, limits, None, config);

    DiscordPresencePreview {
        provider: Provider::Claude.as_str().to_string(),
        app_name: "Claude Code".to_string(),
        details,
        state,
        large_image_key: config.display.large_image_key.clone(),
        large_text: config.display.large_text.clone(),
        small_image_key: None,
        small_text: None,
        has_session: true,
        duration_secs: claude_duration_secs(session),
    }
}

fn build_codex_discord_preview(
    sessions: &[CodexSessionSnapshot],
    config: &CodexPresenceConfig,
    desktop_surface_running: bool,
) -> DiscordPresencePreview {
    let fallback_surface = codex_fallback_surface(desktop_surface_running);
    let Some(session) = codex_session::preferred_active_session(sessions) else {
        let presentation = idle_presence_presentation(fallback_surface, config);
        return DiscordPresencePreview {
            provider: Provider::Codex.as_str().to_string(),
            app_name: presentation.app_name,
            details: presentation.details,
            state: presentation.state,
            large_image_key: presentation.large_image_key,
            large_text: presentation.large_text,
            small_image_key: presentation.small_image_key,
            small_text: presentation.small_text,
            has_session: false,
            duration_secs: 0,
        };
    };

    let resolved_service_tier = resolve_service_tier();
    let resolved_plan = PlanDetector::new().resolve_from_sessions(sessions, &config.openai_plan);
    let effective_limits = codex_session::latest_limits_source(sessions);
    let limits = effective_limits.as_ref().map(|item| &item.limits);
    let presentation = active_presence_presentation(
        codex_session_surface(session, fallback_surface),
        session,
        limits,
        &resolved_plan,
        &resolved_service_tier,
        config,
    );

    DiscordPresencePreview {
        provider: Provider::Codex.as_str().to_string(),
        app_name: presentation.app_name,
        details: presentation.details,
        state: presentation.state,
        large_image_key: presentation.large_image_key,
        large_text: presentation.large_text,
        small_image_key: presentation.small_image_key,
        small_text: presentation.small_text,
        has_session: true,
        duration_secs: codex_duration_secs(session),
    }
}

fn claude_duration_secs(session: &ClaudeSessionSnapshot) -> u64 {
    session
        .started_at
        .map(|started_at| (chrono::Utc::now() - started_at).num_seconds().max(0) as u64)
        .unwrap_or(0)
}

fn codex_duration_secs(session: &CodexSessionSnapshot) -> u64 {
    session
        .started_at
        .map(|started_at| (chrono::Utc::now() - started_at).num_seconds().max(0) as u64)
        .unwrap_or(0)
}

#[tauri::command]
pub fn get_rate_limits() -> Option<RateLimitInfo> {
    let data = shared().lock().ok()?;
    match data.active_provider {
        Provider::Claude => {
            if let Some(u) = data.claude_usage.as_ref() {
                return Some(RateLimitInfo {
                    provider: Provider::Claude.as_str().to_string(),
                    usage: Some(UsageSnapshot {
                        provider: Provider::Claude.as_str().to_string(),
                        scopes: vec![QuotaScope {
                            id: Some("claude".to_string()),
                            name: Some("Account quota".to_string()),
                            kind: RateLimitScope::GlobalCodex,
                            windows: vec![
                                QuotaWindow {
                                    window_minutes: 300,
                                    used_percent: u.five_hour_pct,
                                    remaining_percent: (100.0 - u.five_hour_pct).clamp(0.0, 100.0),
                                    resets_at: None,
                                },
                                QuotaWindow {
                                    window_minutes: 10080,
                                    used_percent: u.seven_day_pct,
                                    remaining_percent: (100.0 - u.seven_day_pct).clamp(0.0, 100.0),
                                    resets_at: None,
                                },
                            ],
                        }],
                        credits: None,
                        observed_at: None,
                        source: u.source.clone(),
                    }),
                    five_hour_pct: u.five_hour_pct,
                    five_hour_resets: u.five_hour_resets.clone(),
                    five_hour_label: "5-hour window".into(),
                    five_hour_window_minutes: Some(300),
                    seven_day_pct: u.seven_day_pct,
                    seven_day_resets: u.seven_day_resets.clone(),
                    seven_day_label: "All Models".into(),
                    seven_day_window_minutes: Some(10080),
                    sonnet_pct: u.sonnet_pct,
                    sonnet_resets: u.sonnet_resets.clone(),
                    extra_enabled: u.extra_enabled,
                    extra_limit: u.extra_limit,
                    extra_used: u.extra_used,
                    extra_pct: u.extra_pct,
                    source: u.source.clone(),
                });
            }

            if let ActiveSessions::Claude(sessions) = &data.sessions
                && let Some(source) = session::latest_limits_source(sessions)
                && let Some(primary) = source.limits.primary.as_ref()
            {
                let secondary = source.limits.secondary.as_ref();
                return Some(RateLimitInfo {
                    provider: Provider::Claude.as_str().to_string(),
                    usage: Some(UsageSnapshot {
                        provider: Provider::Claude.as_str().to_string(),
                        scopes: vec![QuotaScope {
                            id: Some("claude".to_string()),
                            name: Some("Account quota".to_string()),
                            kind: RateLimitScope::GlobalCodex,
                            windows: std::iter::once(primary)
                                .chain(secondary.into_iter())
                                .map(|window| QuotaWindow {
                                    window_minutes: window.window_minutes,
                                    used_percent: window.used_percent,
                                    remaining_percent: window.remaining_percent,
                                    resets_at: window.resets_at,
                                })
                                .collect(),
                        }],
                        credits: None,
                        observed_at: source.last_token_event_at,
                        source: match source.source {
                            session::DataSource::Statusline => "statusline".to_string(),
                            session::DataSource::Jsonl => "jsonl".to_string(),
                        },
                    }),
                    five_hour_pct: primary.used_percent,
                    five_hour_resets: primary
                        .resets_at
                        .map_or("N/A".into(), |d| d.format("%H:%M UTC").to_string()),
                    five_hour_label: "5-hour window".into(),
                    five_hour_window_minutes: Some(primary.window_minutes),
                    seven_day_pct: secondary.map_or(0.0, |s| s.used_percent),
                    seven_day_resets: secondary
                        .and_then(|s| s.resets_at)
                        .map_or("N/A".into(), |d| d.format("%H:%M UTC").to_string()),
                    seven_day_label: "All Models".into(),
                    seven_day_window_minutes: secondary.map(|s| s.window_minutes),
                    sonnet_pct: None,
                    sonnet_resets: None,
                    extra_enabled: false,
                    extra_limit: None,
                    extra_used: None,
                    extra_pct: None,
                    source: data
                        .claude_usage_error
                        .clone()
                        .unwrap_or_else(|| "session".into()),
                });
            }

            let hint = data
                .claude_usage_error
                .clone()
                .unwrap_or_else(|| "no data yet".into());
            Some(RateLimitInfo {
                provider: Provider::Claude.as_str().to_string(),
                usage: None,
                five_hour_pct: 0.0,
                five_hour_resets: "N/A".into(),
                five_hour_label: "5-hour window".into(),
                five_hour_window_minutes: None,
                seven_day_pct: 0.0,
                seven_day_resets: "N/A".into(),
                seven_day_label: "All Models".into(),
                seven_day_window_minutes: None,
                sonnet_pct: None,
                sonnet_resets: None,
                extra_enabled: false,
                extra_limit: None,
                extra_used: None,
                extra_pct: None,
                source: hint,
            })
        }
        Provider::Codex => {
            let usage = data.codex_usage.clone();
            if let Some(selected) = data.codex_limits.as_ref()
                && let Some(primary) = selected.limits.primary.as_ref()
            {
                let secondary = selected.limits.secondary.as_ref();
                return Some(RateLimitInfo {
                    provider: Provider::Codex.as_str().to_string(),
                    usage,
                    five_hour_pct: primary.used_percent,
                    five_hour_resets: primary.resets_at.map_or("N/A".into(), |d| d.to_rfc3339()),
                    five_hour_label: format_codex_window_label(
                        Some(primary.window_minutes),
                        "Primary Window",
                    ),
                    five_hour_window_minutes: Some(primary.window_minutes),
                    seven_day_pct: secondary.map_or(0.0, |s| s.used_percent),
                    seven_day_resets: secondary
                        .and_then(|s| s.resets_at)
                        .map_or("N/A".into(), |d| d.to_rfc3339()),
                    seven_day_label: format_codex_window_label(
                        secondary.map(|s| s.window_minutes),
                        "Secondary Window",
                    ),
                    seven_day_window_minutes: secondary.map(|s| s.window_minutes),
                    sonnet_pct: None,
                    sonnet_resets: None,
                    extra_enabled: false,
                    extra_limit: None,
                    extra_used: None,
                    extra_pct: None,
                    source: selected.source_label(),
                });
            }
            Some(RateLimitInfo {
                provider: Provider::Codex.as_str().to_string(),
                usage,
                five_hour_pct: 0.0,
                five_hour_resets: "N/A".into(),
                five_hour_label: "5h Window".into(),
                five_hour_window_minutes: None,
                seven_day_pct: 0.0,
                seven_day_resets: "N/A".into(),
                seven_day_label: "7d Window".into(),
                seven_day_window_minutes: None,
                sonnet_pct: None,
                sonnet_resets: None,
                extra_enabled: false,
                extra_limit: None,
                extra_used: None,
                extra_pct: None,
                source: "codex telemetry unavailable".into(),
            })
        }
    }
}

#[derive(Serialize)]
pub struct DiscordUserInfo {
    pub user_id: String,
    pub username: String,
    pub discriminator: String,
    pub avatar_hash: String,
    pub avatar_url: String,
    pub banner_hash: Option<String>,
    pub banner_url: Option<String>,
}

fn discord_leveldb_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let variants = ["discord", "discordcanary", "discordptb"];

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            for v in &variants {
                dirs.push(
                    PathBuf::from(&appdata)
                        .join(v)
                        .join("Local Storage/leveldb"),
                );
            }
        }
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            let pascal = ["Discord", "DiscordCanary", "DiscordPTB"];
            for v in &pascal {
                dirs.push(
                    PathBuf::from(&localappdata)
                        .join(v)
                        .join("Local Storage/leveldb"),
                );
            }
        }
    }

    #[cfg(target_os = "macos")]
    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(&home);
        let variants_mac = [
            "discord",
            "discordcanary",
            "discordptb",
            "Discord",
            "Discord Canary",
            "Discord PTB",
        ];
        for v in &variants_mac {
            dirs.push(
                home_path
                    .join("Library/Application Support")
                    .join(v)
                    .join("Local Storage/leveldb"),
            );
        }
    }

    #[cfg(target_os = "linux")]
    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(&home);
        for v in &variants {
            dirs.push(
                home_path
                    .join(".config")
                    .join(v)
                    .join("Local Storage/leveldb"),
            );
        }
        let flatpak_ids = [
            "com.discordapp.Discord",
            "com.discordapp.DiscordCanary",
            "com.discordapp.DiscordPTB",
        ];
        for id in &flatpak_ids {
            dirs.push(
                home_path
                    .join(".var/app")
                    .join(id)
                    .join("config/discord/Local Storage/leveldb"),
            );
        }
        for v in &variants {
            dirs.push(
                home_path
                    .join("snap")
                    .join(v)
                    .join("current/.config")
                    .join(v)
                    .join("Local Storage/leveldb"),
            );
        }
    }

    dirs
}

#[tauri::command]
pub fn get_discord_user() -> Option<DiscordUserInfo> {
    let leveldb_dir = discord_leveldb_dirs().into_iter().find(|d| d.exists())?;

    let mut entries: Vec<_> = std::fs::read_dir(&leveldb_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".ldb") || name.ends_with(".log")
        })
        .collect();

    entries.sort_by(|a, b| {
        let ta = a
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let tb = b
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
        tb.cmp(&ta)
    });

    for entry in entries {
        let data = std::fs::read(entry.path()).ok()?;
        if let Some(user) = extract_discord_user(&data) {
            return Some(user);
        }
    }
    None
}

fn extract_discord_user(data: &[u8]) -> Option<DiscordUserInfo> {
    let needle = b"\"id\":\"";
    let mut pos = 0;
    while pos < data.len().saturating_sub(100) {
        if let Some(offset) = data[pos..].windows(needle.len()).position(|w| w == needle) {
            let start = pos + offset;
            let id_start = start + needle.len();
            if let Some(id_end) = data[id_start..].iter().position(|&b| b == b'"') {
                let id_bytes = &data[id_start..id_start + id_end];
                if id_bytes.len() >= 17 && id_bytes.iter().all(|b| b.is_ascii_digit()) {
                    let user_id = String::from_utf8_lossy(id_bytes).to_string();
                    let chunk_end = (start + 600).min(data.len());
                    let chunk = &data[start..chunk_end];
                    let chunk_str = String::from_utf8_lossy(chunk);

                    let username = match extract_json_field(&chunk_str, "username") {
                        Some(u) if !u.is_empty() => u,
                        _ => {
                            pos = start + 1;
                            continue;
                        }
                    };

                    let discriminator = extract_json_field(&chunk_str, "discriminator")
                        .filter(|d| !d.is_empty())
                        .unwrap_or_else(|| "0".to_string());

                    let avatar_hash = extract_json_field(&chunk_str, "avatar")
                        .filter(|h| !h.is_empty())
                        .unwrap_or_default();

                    let avatar_url = if avatar_hash.is_empty() {
                        default_avatar_url(&user_id, &discriminator)
                    } else {
                        let ext = if avatar_hash.starts_with("a_") {
                            "gif"
                        } else {
                            "png"
                        };
                        format!(
                            "https://cdn.discordapp.com/avatars/{}/{}.{}?size=256",
                            user_id, avatar_hash, ext
                        )
                    };

                    let (banner_hash, banner_url) = match extract_json_field(&chunk_str, "banner") {
                        Some(bh) if !bh.is_empty() => {
                            let ext = if bh.starts_with("a_") { "gif" } else { "png" };
                            let url = format!(
                                "https://cdn.discordapp.com/banners/{}/{}.{}?size=600",
                                user_id, bh, ext
                            );
                            (Some(bh), Some(url))
                        }
                        _ => (None, None),
                    };

                    return Some(DiscordUserInfo {
                        user_id,
                        username,
                        discriminator,
                        avatar_hash,
                        avatar_url,
                        banner_hash,
                        banner_url,
                    });
                }
            }
            pos = start + 1;
        } else {
            break;
        }
    }
    None
}

/// Build the CDN URL for Discord's built-in default avatars.
///
/// - New username system (discriminator "0"): index = (user_id >> 22) % 6
/// - Legacy discriminator system: index = discriminator % 5
fn default_avatar_url(user_id: &str, discriminator: &str) -> String {
    let index = if discriminator == "0" {
        user_id.parse::<u64>().map(|id| (id >> 22) % 6).unwrap_or(0)
    } else {
        discriminator
            .parse::<u32>()
            .map(|d| u64::from(d % 5))
            .unwrap_or(0)
    };
    format!("https://cdn.discordapp.com/embed/avatars/{index}.png")
}

fn extract_json_field(text: &str, field: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", field);
    let start = text.find(&pattern)? + pattern.len();
    let end = start + text[start..].find('"')?;
    Some(text[start..end].to_string())
}

#[derive(Serialize)]
pub struct PlanInfo {
    pub provider: String,
    pub plan_key: String,
    pub plan_name: String,
    pub detected: bool,
}

#[tauri::command]
pub fn get_plan_info() -> PlanInfo {
    match current_provider() {
        Provider::Claude => {
            if let Ok(cfg) = PresenceConfig::load_or_init()
                && let Some(plan) = cfg.plan.as_deref().filter(|plan| !plan.trim().is_empty())
            {
                return PlanInfo {
                    provider: Provider::Claude.as_str().to_string(),
                    plan_key: plan.to_string(),
                    plan_name: plan_name_from_key(plan),
                    detected: false,
                };
            }

            let mut usage_mgr = UsageManager::new();
            let plan_key = usage_mgr.detected_plan_key().unwrap_or_default();
            let plan_name = if plan_key.is_empty() {
                "Unknown".to_string()
            } else {
                plan_name_from_key(&plan_key)
            };

            PlanInfo {
                provider: Provider::Claude.as_str().to_string(),
                plan_key,
                plan_name,
                detected: true,
            }
        }
        Provider::Codex => {
            let config = CodexPresenceConfig::load_or_init().unwrap_or_default();
            let sessions = read_codex_sessions();
            let mut detector = PlanDetector::new();
            let resolved = detector.resolve_from_sessions(&sessions, &config.openai_plan);
            PlanInfo {
                provider: Provider::Codex.as_str().to_string(),
                plan_key: codex_plan_key_from_tier(resolved.tier).to_string(),
                plan_name: resolved.label(config.openai_plan.show_price),
                detected: !matches!(
                    resolved.source,
                    cc_discord_presence::codex::telemetry::plan::DetectedPlanSource::Manual
                ),
            }
        }
    }
}

#[tauri::command]
pub fn set_plan_override(plan: String) {
    match current_provider() {
        Provider::Claude => {
            if let Ok(mut cfg) = PresenceConfig::load_or_init() {
                cfg.plan = plan_key_from_override(&plan).map(str::to_string);
                log_save_error("claude-plan-override", cfg.save());
            }
        }
        Provider::Codex => {
            if let Ok(mut cfg) = CodexPresenceConfig::load_or_init() {
                let normalized = plan.trim().to_ascii_lowercase();
                let tier = match normalized.as_str() {
                    "" | "auto" => None,
                    "free" => Some(cc_discord_presence::codex::config::OpenAiPlanTier::Free),
                    "go" => Some(cc_discord_presence::codex::config::OpenAiPlanTier::Go),
                    "plus" => Some(cc_discord_presence::codex::config::OpenAiPlanTier::Plus),
                    "team" => Some(cc_discord_presence::codex::config::OpenAiPlanTier::Business),
                    "pro_5x" | "pro5x" => {
                        Some(cc_discord_presence::codex::config::OpenAiPlanTier::Pro5x)
                    }
                    "pro" | "pro_20x" | "pro20x" => {
                        Some(cc_discord_presence::codex::config::OpenAiPlanTier::Pro20x)
                    }
                    "business" => {
                        Some(cc_discord_presence::codex::config::OpenAiPlanTier::Business)
                    }
                    "enterprise" => {
                        Some(cc_discord_presence::codex::config::OpenAiPlanTier::Enterprise)
                    }
                    _ => None,
                };
                if let Some(tier) = tier {
                    cfg.openai_plan.mode =
                        cc_discord_presence::codex::config::OpenAiPlanMode::Manual;
                    cfg.openai_plan.tier = tier;
                } else {
                    cfg.openai_plan.mode = cc_discord_presence::codex::config::OpenAiPlanMode::Auto;
                }
                log_save_error("codex-plan-override", cfg.save());
            }
        }
    }
}

#[derive(Serialize)]
pub struct ProviderInfo {
    pub active_provider: String,
}

#[derive(Serialize)]
pub struct ProviderCopyInfo {
    pub provider: String,
    pub provider_label: String,
    pub instruction_file: String,
    pub home_dir: String,
    pub sessions_store: String,
    pub fix_label: String,
    pub global_state_source: String,
}

#[tauri::command]
pub fn get_active_provider() -> ProviderInfo {
    ProviderInfo {
        active_provider: current_provider().as_str().to_string(),
    }
}

#[tauri::command]
pub fn get_provider_copy() -> ProviderCopyInfo {
    let provider = current_provider();
    ProviderCopyInfo {
        provider: provider.as_str().to_string(),
        provider_label: provider.display_name().to_string(),
        instruction_file: provider.instruction_file_name().to_string(),
        home_dir: provider.home_dir_name().to_string(),
        sessions_store: provider.sessions_glob_label().to_string(),
        fix_label: provider.fix_action_label().to_string(),
        global_state_source: provider.global_state_label().to_string(),
    }
}

#[tauri::command]
pub fn set_active_provider(provider: String) {
    if let Some(provider) = Provider::parse(&provider) {
        if let Err(err) = cc_discord_presence::provider::save_active_provider(provider) {
            tracing::warn!(provider = provider.as_str(), error = %err, "failed to save active provider");
        }
        // The switch flags and field visibility belong to the provider, not to
        // the session — re-read them so the Discord view reflects the config
        // that is now in charge instead of the previous provider's cache. Seed
        // for the requested provider, not for whatever is on disk: if the write
        // above failed, the user's choice still governs this process.
        seed_discord_state_for(provider);
    }
}

#[tauri::command]
pub fn get_session_history(
    days: Option<i64>,
    project: Option<String>,
    limit: Option<i64>,
) -> Vec<crate::db::HistoricalSession> {
    crate::db::get_session_history(days, project.as_deref(), limit)
}

#[tauri::command]
pub fn get_session_history_filtered(
    from_iso: Option<String>,
    to_iso: Option<String>,
    project: Option<String>,
    model: Option<String>,
    min_cost: Option<f64>,
    max_cost: Option<f64>,
    limit: Option<i64>,
) -> Vec<crate::db::HistoricalSession> {
    crate::db::get_session_history_filtered(
        from_iso.as_deref(),
        to_iso.as_deref(),
        project.as_deref(),
        model.as_deref(),
        min_cost,
        max_cost,
        limit,
    )
}

#[tauri::command]
pub fn get_sessions_by_hour_range(
    start_hour: i64,
    end_hour: i64,
    days: Option<i64>,
) -> Vec<crate::db::HistoricalSession> {
    crate::db::get_sessions_by_hour_range(start_hour, end_hour, days)
}

#[tauri::command]
pub fn search_sessions(query: String, limit: Option<i64>) -> Vec<crate::db::HistoricalSession> {
    crate::db::search_sessions(&query, limit)
}

#[tauri::command]
pub fn get_daily_stats(days: Option<i64>) -> Vec<crate::db::DailyStat> {
    crate::db::get_daily_stats(days)
}

#[tauri::command]
pub fn get_analytics_summary() -> crate::db::AnalyticsSummary {
    crate::db::get_analytics_summary()
}

#[derive(Serialize, Clone)]
pub struct ContextFileEntry {
    pub name: String,
    pub tokens: u64,
}

#[derive(Serialize, Clone)]
pub struct ContextBreakdown {
    pub model: String,
    pub context_window: u64,
    pub used_tokens: u64,
    pub free_space: u64,
    pub autocompact_buffer: u64,
    pub system_prompt: u64,
    pub system_tools: u64,
    pub memory_files: Vec<ContextFileEntry>,
    pub memory_total: u64,
    pub skills: Vec<ContextFileEntry>,
    pub skills_total: u64,
    pub messages: u64,
    pub mcp_tools: Vec<ContextFileEntry>,
    pub mcp_total: u64,
}

#[derive(Serialize)]
pub struct SessionContextBreakdown {
    pub session_id: String,
    pub project: String,
    pub model_id: String,
    pub is_idle: bool,
    pub activity: String,
    pub breakdown: ContextBreakdown,
}

#[derive(Serialize)]
pub struct SessionContextUsage {
    pub session_id: String,
    pub project: String,
    pub model: String,
    pub model_display: String,
    pub used_tokens: u64,
    pub window_tokens: u64,
    pub utilization_pct: f64,
    pub recommendation: String,
}

const CONTEXT_WATCH_PCT: f64 = 50.0;
const CONTEXT_COMPACT_SOON_PCT: f64 = 80.0;
const CONTEXT_COMPACT_NOW_PCT: f64 = 95.0;

fn context_utilization_pct(used_tokens: u64, window_tokens: u64) -> f64 {
    if window_tokens == 0 {
        0.0
    } else {
        ((used_tokens as f64 / window_tokens as f64) * 100.0).clamp(0.0, 100.0)
    }
}

fn context_recommendation(utilization_pct: f64) -> String {
    if utilization_pct >= CONTEXT_COMPACT_NOW_PCT {
        "Context is nearly full — compact now or start a fresh session before the next turn.".into()
    } else if utilization_pct >= CONTEXT_COMPACT_SOON_PCT {
        "Context is filling up — plan to compact soon to avoid an autocompact mid-task.".into()
    } else if utilization_pct >= CONTEXT_WATCH_PCT {
        "Context is past half — keep an eye on it and compact when you shift topics.".into()
    } else {
        "Context is healthy — plenty of headroom for this session.".into()
    }
}

fn estimate_tokens(text: &str) -> u64 {
    (text.len() as f64 / 3.5).ceil() as u64
}

fn estimate_tokens_from_file(path: &std::path::Path) -> u64 {
    std::fs::read_to_string(path)
        .map(|s| estimate_tokens(&s))
        .unwrap_or(0)
}

fn resolve_instruction_include(
    base_file: &std::path::Path,
    raw: &str,
) -> Option<std::path::PathBuf> {
    let trimmed = raw.trim().trim_matches('"').trim_matches('\'');
    if trimmed.is_empty() {
        return None;
    }
    let candidate = std::path::PathBuf::from(trimmed);
    if candidate.is_absolute() {
        Some(candidate)
    } else {
        base_file.parent().map(|parent| parent.join(candidate))
    }
}

fn discover_instruction_includes(
    base_file: &std::path::Path,
    content: &str,
) -> Vec<std::path::PathBuf> {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            if let Some(include) = trimmed.strip_prefix('@') {
                return resolve_instruction_include(base_file, include);
            }
            if let Some(include) = trimmed.strip_prefix("file:") {
                return resolve_instruction_include(base_file, include);
            }
            None
        })
        .collect()
}

fn label_context_file(
    path: &std::path::Path,
    provider: Provider,
    project_root: Option<&std::path::Path>,
    project_name: Option<&str>,
) -> String {
    let provider_home = provider.home_path();
    if path.starts_with(&provider_home)
        && let Ok(relative) = path.strip_prefix(&provider_home)
    {
        let relative = relative.to_string_lossy().replace('\\', "/");
        return format!("{}/{}", provider.home_dir_name(), relative);
    }
    if let (Some(root), Some(name)) = (project_root, project_name)
        && path.starts_with(root)
        && let Ok(relative) = path.strip_prefix(root)
    {
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.is_empty() {
            return name.to_string();
        }
        return format!("{name}/{relative}");
    }
    path.to_string_lossy().replace('\\', "/")
}

fn collect_instruction_tree(
    provider: Provider,
    root_file: &std::path::Path,
    project_root: Option<&std::path::Path>,
    project_name: Option<&str>,
    seen: &mut HashSet<std::path::PathBuf>,
    out: &mut Vec<ContextFileEntry>,
) {
    let canonical = std::fs::canonicalize(root_file).unwrap_or_else(|_| root_file.to_path_buf());
    if !seen.insert(canonical.clone()) {
        return;
    }
    let Ok(content) = std::fs::read_to_string(&canonical) else {
        return;
    };
    let tokens = estimate_tokens(&content);
    if tokens > 0 {
        let name = label_context_file(&canonical, provider, project_root, project_name);
        out.push(ContextFileEntry { name, tokens });
    }
    for include in discover_instruction_includes(&canonical, &content) {
        if include.exists() {
            collect_instruction_tree(provider, &include, project_root, project_name, seen, out);
        }
    }
}

fn collect_skills_from_dir(skills_dir: &std::path::Path) -> Vec<ContextFileEntry> {
    let mut skills = Vec::new();
    if skills_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(skills_dir)
    {
        let mut dirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        dirs.sort_by_key(|e| e.file_name());
        for entry in dirs {
            let skill_file = entry.path().join("SKILL.md");
            if skill_file.exists() {
                let tokens = estimate_tokens_from_file(&skill_file);
                if tokens > 0 {
                    skills.push(ContextFileEntry {
                        name: entry.file_name().to_string_lossy().to_string(),
                        tokens,
                    });
                }
            }
        }
    }
    skills.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.name.cmp(&b.name)));
    skills
}

fn collect_codex_mcp_tools(config_path: &std::path::Path) -> Vec<ContextFileEntry> {
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return Vec::new();
    };

    let mut tools = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_block = String::new();

    let flush = |name: &mut Option<String>, block: &mut String, out: &mut Vec<ContextFileEntry>| {
        if let Some(name) = name.take() {
            let tokens = estimate_tokens(block).max(20);
            out.push(ContextFileEntry { name, tokens });
        }
        block.clear();
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            flush(&mut current_name, &mut current_block, &mut tools);

            if let Some(name) = trimmed
                .strip_prefix("[mcp_servers.")
                .and_then(|value| value.strip_suffix(']'))
                .map(|value| value.trim_matches('"').trim_matches('\'').to_string())
                .filter(|value| !value.is_empty())
            {
                current_name = Some(name);
                current_block.push_str(line);
                current_block.push('\n');
            }
            continue;
        }

        if current_name.is_some() {
            current_block.push_str(line);
            current_block.push('\n');
        }
    }

    flush(&mut current_name, &mut current_block, &mut tools);
    tools.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.name.cmp(&b.name)));
    tools
}

fn empty_context_breakdown(model: &str, context_window: u64) -> ContextBreakdown {
    ContextBreakdown {
        model: model.to_string(),
        context_window,
        used_tokens: 0,
        free_space: context_window,
        autocompact_buffer: 0,
        system_prompt: 0,
        system_tools: 0,
        memory_files: Vec::new(),
        memory_total: 0,
        skills: Vec::new(),
        skills_total: 0,
        messages: 0,
        mcp_tools: Vec::new(),
        mcp_total: 0,
    }
}

fn is_claude_session_idle(session: &ClaudeSessionSnapshot, idle: SystemTime) -> bool {
    session.last_activity < idle
}

fn is_codex_session_idle(session: &CodexSessionSnapshot, idle: SystemTime) -> bool {
    session.last_activity < idle
}

fn claude_context_window(session: &ClaudeSessionSnapshot) -> u64 {
    let model_id = session.model.as_deref().unwrap_or("");
    if cost::is_ga_1m_context(model_id)
        || model_id.contains("[1m]")
        || session.max_turn_api_input > 200_000
    {
        1_000_000
    } else {
        200_000
    }
}

fn claude_context_model(session: &ClaudeSessionSnapshot) -> String {
    session
        .model_display
        .clone()
        .or(session.model.as_ref().map(|m| cost::model_display_name(m)))
        .unwrap_or_else(|| "Unknown".into())
}

fn collect_claude_memory_files(
    claude_home: &std::path::Path,
    selected: Option<&ClaudeSessionSnapshot>,
) -> Vec<ContextFileEntry> {
    let mut memory_files = Vec::new();
    let global_claude_md = claude_home.join("CLAUDE.md");
    if global_claude_md.exists() {
        let tokens = estimate_tokens_from_file(&global_claude_md);
        if tokens > 0 {
            memory_files.push(ContextFileEntry {
                name: ".claude/CLAUDE.md".into(),
                tokens,
            });
        }
    }

    let rules_dir = claude_home.join("rules");
    if rules_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&rules_dir)
    {
        let mut files: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "md" || ext == "txt")
            })
            .collect();
        files.sort_by_key(|e| e.file_name());
        for entry in files {
            let path = entry.path();
            let tokens = estimate_tokens_from_file(&path);
            if tokens > 0 {
                memory_files.push(ContextFileEntry {
                    name: format!(
                        ".claude/rules/{}",
                        path.file_name().unwrap().to_string_lossy()
                    ),
                    tokens,
                });
            }
        }
    }

    if let Some(session) = selected {
        let project_claude = session.cwd.join("CLAUDE.md");
        if project_claude.exists() {
            let tokens = estimate_tokens_from_file(&project_claude);
            let name = format!("{}/CLAUDE.md", session.project_name);
            if tokens > 0 && !memory_files.iter().any(|f| f.name == name) {
                memory_files.push(ContextFileEntry { name, tokens });
            }
        }
    }

    memory_files.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.name.cmp(&b.name)));
    memory_files
}

fn collect_claude_mcp_tools(settings_file: &std::path::Path) -> Vec<ContextFileEntry> {
    let mut mcp_tools = Vec::new();
    if settings_file.exists()
        && let Ok(data) = std::fs::read_to_string(settings_file)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&data)
        && let Some(servers) = json.get("mcpServers").and_then(|v| v.as_object())
    {
        for (name, config) in servers {
            let config_str = serde_json::to_string(config).unwrap_or_default();
            let tokens = estimate_tokens(&config_str).max(20);
            mcp_tools.push(ContextFileEntry {
                name: name.clone(),
                tokens,
            });
        }
    }
    mcp_tools.sort_by_key(|f| std::cmp::Reverse(f.tokens));
    mcp_tools
}

fn build_claude_context_breakdown(selected: Option<&ClaudeSessionSnapshot>) -> ContextBreakdown {
    let claude_home = cc_discord_presence::config::claude_home();
    let model = selected.map_or_else(|| "Unknown".into(), claude_context_model);
    let ctx_window = selected.map_or(200_000, claude_context_window);
    let current_context_tokens = selected.map_or(0, |s| s.current_context_tokens);
    let memory_files = collect_claude_memory_files(&claude_home, selected);
    let memory_total: u64 = memory_files.iter().map(|f| f.tokens).sum();
    let skills = collect_skills_from_dir(&claude_home.join("skills"));
    let skills_total: u64 = skills.iter().map(|f| f.tokens).sum();
    let mcp_tools = collect_claude_mcp_tools(&claude_home.join("settings.json"));
    let mcp_total: u64 = mcp_tools.iter().map(|f| f.tokens).sum();
    let used_tokens = current_context_tokens.min(ctx_window);
    let free_space = ctx_window.saturating_sub(used_tokens);

    ContextBreakdown {
        model,
        context_window: ctx_window,
        used_tokens,
        free_space,
        autocompact_buffer: 0,
        system_prompt: 0,
        system_tools: 0,
        memory_files,
        memory_total,
        skills,
        skills_total,
        messages: used_tokens,
        mcp_tools,
        mcp_total,
    }
}

fn build_codex_context_breakdown(selected: Option<&CodexSessionSnapshot>) -> ContextBreakdown {
    let codex_home = cc_discord_presence::codex::config::codex_home();
    let model = selected
        .and_then(|s| s.model.clone())
        .map(|model| {
            cc_discord_presence::codex::util::format_model_display(
                &model,
                selected.and_then(|session| session.reasoning_effort),
                resolve_service_tier().is_fast(),
            )
        })
        .unwrap_or_else(|| "Codex".to_string());
    let ctx_window = selected
        .and_then(|s| s.context_window.as_ref().map(|w| w.window_tokens))
        .or_else(|| {
            selected
                .and_then(|s| s.model.as_deref())
                .and_then(cc_discord_presence::codex::cost::default_model_context_window)
        })
        .unwrap_or(400_000);
    let used_tokens = selected
        .and_then(|s| {
            s.context_window
                .as_ref()
                .map(|w| w.used_tokens.min(ctx_window))
        })
        .unwrap_or(0);
    let mut memory_files = Vec::new();
    let mut seen_instruction_files = HashSet::new();
    let global_agents = codex_home.join("AGENTS.md");
    if global_agents.exists() {
        collect_instruction_tree(
            Provider::Codex,
            &global_agents,
            None,
            None,
            &mut seen_instruction_files,
            &mut memory_files,
        );
    }
    let generated_instructions = codex_home.join("generated-model-instructions.md");
    if generated_instructions.exists() {
        collect_instruction_tree(
            Provider::Codex,
            &generated_instructions,
            None,
            None,
            &mut seen_instruction_files,
            &mut memory_files,
        );
    }
    if let Some(session) = selected {
        let project_agents = session.cwd.join("AGENTS.md");
        if project_agents.exists() {
            collect_instruction_tree(
                Provider::Codex,
                &project_agents,
                Some(&session.cwd),
                Some(&session.project_name),
                &mut seen_instruction_files,
                &mut memory_files,
            );
        }
    }
    memory_files.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.name.cmp(&b.name)));
    let memory_total: u64 = memory_files.iter().map(|f| f.tokens).sum();
    let skills = collect_skills_from_dir(&codex_home.join("skills"));
    let skills_total: u64 = skills.iter().map(|f| f.tokens).sum();
    let mcp_tools = collect_codex_mcp_tools(&codex_home.join("config.toml"));
    let mcp_total: u64 = mcp_tools.iter().map(|f| f.tokens).sum();
    let free_space = ctx_window.saturating_sub(used_tokens);

    ContextBreakdown {
        model,
        context_window: ctx_window,
        used_tokens,
        free_space,
        autocompact_buffer: 0,
        system_prompt: 0,
        system_tools: 0,
        memory_files,
        memory_total,
        skills,
        skills_total,
        messages: used_tokens,
        mcp_tools,
        mcp_total,
    }
}

fn selected_claude_context_sessions<'a>(
    sessions: &'a [ClaudeSessionSnapshot],
    session_ids: Option<&[String]>,
) -> Vec<&'a ClaudeSessionSnapshot> {
    if let Some(ids) = session_ids
        && !ids.is_empty()
    {
        let mut seen = HashSet::new();
        return ids
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .filter_map(|id| sessions.iter().find(|s| s.session_id == *id))
            .collect();
    }

    let idle = SystemTime::now()
        .checked_sub(IDLE_CUTOFF)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let active: Vec<_> = sessions
        .iter()
        .filter(|session| !is_claude_session_idle(session, idle))
        .collect();
    if active.is_empty() {
        preferred_active_session(sessions).into_iter().collect()
    } else {
        active
    }
}

fn selected_codex_context_sessions<'a>(
    sessions: &'a [CodexSessionSnapshot],
    session_ids: Option<&[String]>,
) -> Vec<&'a CodexSessionSnapshot> {
    if let Some(ids) = session_ids
        && !ids.is_empty()
    {
        let mut seen = HashSet::new();
        return ids
            .iter()
            .filter(|id| seen.insert((*id).clone()))
            .filter_map(|id| sessions.iter().find(|s| s.session_id == *id))
            .collect();
    }

    let idle = SystemTime::now()
        .checked_sub(IDLE_CUTOFF)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let active: Vec<_> = sessions
        .iter()
        .filter(|session| !is_codex_session_idle(session, idle))
        .collect();
    if active.is_empty() {
        codex_session::preferred_active_session(sessions)
            .into_iter()
            .collect()
    } else {
        active
    }
}

fn claude_context_entry(
    session: &ClaudeSessionSnapshot,
    idle: SystemTime,
) -> SessionContextBreakdown {
    SessionContextBreakdown {
        session_id: session.session_id.clone(),
        project: session.project_name.clone(),
        model_id: session.model.clone().unwrap_or_default(),
        is_idle: is_claude_session_idle(session, idle),
        activity: session
            .activity
            .as_ref()
            .map_or("Idle".into(), |a| a.action_text().to_string()),
        breakdown: build_claude_context_breakdown(Some(session)),
    }
}

fn codex_context_entry(
    session: &CodexSessionSnapshot,
    idle: SystemTime,
) -> SessionContextBreakdown {
    SessionContextBreakdown {
        session_id: session.session_id.clone(),
        project: session.project_name.clone(),
        model_id: session.model.clone().unwrap_or_default(),
        is_idle: is_codex_session_idle(session, idle),
        activity: session
            .activity
            .as_ref()
            .map_or("Idle".into(), |a| a.action_text().to_string()),
        breakdown: build_codex_context_breakdown(Some(session)),
    }
}

#[tauri::command]
pub fn get_context_breakdown(session_id: Option<String>) -> ContextBreakdown {
    get_context_breakdowns(session_id.map(|id| vec![id]))
        .into_iter()
        .next()
        .map(|entry| entry.breakdown)
        .unwrap_or_else(|| match current_provider() {
            Provider::Claude => empty_context_breakdown("Unknown", 200_000),
            Provider::Codex => empty_context_breakdown("Codex", 400_000),
        })
}

#[tauri::command]
pub fn get_context_breakdowns(session_ids: Option<Vec<String>>) -> Vec<SessionContextBreakdown> {
    let idle = SystemTime::now()
        .checked_sub(IDLE_CUTOFF)
        .unwrap_or(SystemTime::UNIX_EPOCH);

    match current_provider() {
        Provider::Claude => {
            let sessions = read_claude_sessions();
            selected_claude_context_sessions(&sessions, session_ids.as_deref())
                .into_iter()
                .map(|session| claude_context_entry(session, idle))
                .collect()
        }
        Provider::Codex => {
            let sessions = read_codex_sessions();
            selected_codex_context_sessions(&sessions, session_ids.as_deref())
                .into_iter()
                .map(|session| codex_context_entry(session, idle))
                .collect()
        }
    }
}

#[tauri::command]
pub fn get_sessions_context_usage(days: Option<i64>) -> Vec<SessionContextUsage> {
    let mut seen = HashSet::new();
    let mut rows: Vec<SessionContextUsage> = get_context_breakdowns(None)
        .into_iter()
        .filter(|entry| seen.insert(entry.session_id.clone()))
        .map(|entry| {
            let breakdown = entry.breakdown;
            let used_tokens = breakdown.used_tokens.min(breakdown.context_window);
            let utilization_pct = context_utilization_pct(used_tokens, breakdown.context_window);
            SessionContextUsage {
                session_id: entry.session_id,
                project: entry.project,
                model: entry.model_id,
                model_display: breakdown.model,
                used_tokens,
                window_tokens: breakdown.context_window,
                utilization_pct,
                recommendation: context_recommendation(utilization_pct),
            }
        })
        .collect();

    rows.extend(
        crate::db::get_session_history(Some(days.unwrap_or(30)), None, Some(5000))
            .into_iter()
            .filter(|s| seen.insert(s.id.clone()))
            .map(|s| {
                let window_tokens = if s.window_tokens > 0 {
                    s.window_tokens as u64
                } else if cost::is_ga_1m_context(&s.model_id) || s.context_window == "1M" {
                    1_000_000
                } else {
                    200_000
                };
                let used_tokens = (s.used_tokens.max(0) as u64).min(window_tokens);
                let utilization_pct = context_utilization_pct(used_tokens, window_tokens);
                SessionContextUsage {
                    session_id: s.id,
                    project: s.project,
                    model: s.model_id,
                    model_display: s.model,
                    used_tokens,
                    window_tokens,
                    utilization_pct,
                    recommendation: context_recommendation(utilization_pct),
                }
            }),
    );
    rows
}

#[tauri::command]
pub fn get_project_stats(days: Option<i64>) -> Vec<crate::db::ProjectStat> {
    crate::db::get_project_stats(days)
}

#[tauri::command]
pub fn get_hourly_activity(days: Option<i64>) -> Vec<crate::db::HourlyActivity> {
    crate::db::get_hourly_activity(days)
}

#[tauri::command]
pub fn get_top_sessions(
    limit: Option<i64>,
    days: Option<i64>,
) -> Vec<crate::db::HistoricalSession> {
    crate::db::get_top_sessions(limit, days)
}

#[tauri::command]
pub fn get_cost_forecast() -> crate::db::CostForecast {
    crate::db::get_cost_forecast()
}

#[tauri::command]
pub fn get_budget_status() -> crate::db::BudgetStatus {
    crate::db::get_budget_status()
}

#[tauri::command]
pub fn set_budget(monthly_budget: f64, alert_threshold_pct: Option<f64>) {
    crate::db::set_budget(monthly_budget, alert_threshold_pct);
}

#[tauri::command]
pub fn get_model_distribution(days: Option<i64>) -> Vec<(String, i64, f64)> {
    crate::db::get_model_distribution(days)
}

#[tauri::command]
pub fn export_all_data() -> serde_json::Value {
    crate::db::export_all_data()
}

#[tauri::command]
pub fn clear_history() -> i64 {
    crate::db::clear_history()
}

#[tauri::command]
pub fn get_db_size() -> u64 {
    crate::db::get_db_size_bytes()
}

#[tauri::command]
pub async fn generate_html_report(days: Option<i64>, project: Option<String>) -> String {
    offload(move || crate::report::generate_html_report(days, project.as_deref())).await
}

#[tauri::command]
pub async fn generate_markdown_report(days: Option<i64>, project: Option<String>) -> String {
    offload(move || crate::report::generate_markdown_report(days, project.as_deref())).await
}

async fn offload<T, F>(work: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .expect("analyzer blocking task panicked")
}

fn analyzer_sessions(days: Option<i64>) -> Vec<crate::db::HistoricalSession> {
    crate::db::get_session_history(Some(days.unwrap_or(30)), None, Some(5000))
}

/// Analyzer session page for a fixed window, exposed for the debug-only dev
/// bridge so it can answer analyzer commands without an async runtime.
#[cfg(debug_assertions)]
pub fn analyzer_sessions_for(days: i64) -> Vec<crate::db::HistoricalSession> {
    analyzer_sessions(Some(days))
}

/// Active analyzer provider, exposed for the debug-only dev bridge.
#[cfg(debug_assertions)]
pub fn analyzer_provider_for_bridge() -> Provider {
    analyzer_provider()
}

fn analyzer_roots() -> (Vec<PathBuf>, Vec<PathBuf>) {
    (
        cc_discord_presence::config::projects_paths(),
        cc_discord_presence::codex::config::sessions_paths(),
    )
}

fn analyzer_traces(
    sessions: &[crate::db::HistoricalSession],
) -> std::collections::HashMap<String, crate::analyzers::session_trace::SessionTrace> {
    crate::analyzers::session_trace::load_session_traces(sessions)
}

fn analyzer_provider() -> Provider {
    current_provider()
}

#[tauri::command]
pub async fn get_cache_health(
    days: Option<i64>,
) -> crate::analyzers::cache_health::CacheHealthReport {
    let provider = analyzer_provider();
    offload(move || {
        crate::analyzers::cache_health::analyze_for_provider(provider, &analyzer_sessions(days))
    })
    .await
}

#[tauri::command]
pub async fn get_inflection_points(
    days: Option<i64>,
) -> Vec<crate::analyzers::inflection::InflectionPoint> {
    let provider = analyzer_provider();
    offload(move || {
        crate::analyzers::inflection::detect_for_provider(provider, &analyzer_sessions(days))
    })
    .await
}

#[tauri::command]
pub async fn get_model_routing(
    days: Option<i64>,
) -> Option<crate::analyzers::model_routing::ModelRoutingReport> {
    let provider = analyzer_provider();
    offload(move || {
        provider
            .capabilities()
            .model_routing
            .then(|| crate::analyzers::model_routing::analyze(&analyzer_sessions(days)))
    })
    .await
}

#[tauri::command]
pub async fn get_tool_frequency(
    days: Option<i64>,
) -> crate::analyzers::tool_frequency::ToolFrequencyReport {
    offload(move || {
        let sessions = analyzer_sessions(days);
        let traces = analyzer_traces(&sessions);
        crate::analyzers::tool_frequency::analyze(&sessions, &traces)
    })
    .await
}

#[tauri::command]
pub async fn get_prompt_complexity(
    days: Option<i64>,
) -> crate::analyzers::prompt_complexity::PromptComplexityReport {
    offload(move || {
        let sessions = analyzer_sessions(days);
        let traces = analyzer_traces(&sessions);
        crate::analyzers::prompt_complexity::analyze(&sessions, &traces)
    })
    .await
}

#[tauri::command]
pub async fn get_session_health(
    days: Option<i64>,
) -> crate::analyzers::session_health::SessionHealthReport {
    offload(move || {
        let sessions = analyzer_sessions(days);
        let traces = analyzer_traces(&sessions);
        let tool_frequency = crate::analyzers::tool_frequency::analyze(&sessions, &traces);
        let prompt_complexity = crate::analyzers::prompt_complexity::analyze(&sessions, &traces);
        crate::analyzers::session_health::analyze(
            &sessions,
            &traces,
            &tool_frequency,
            &prompt_complexity,
        )
    })
    .await
}

#[tauri::command]
pub async fn get_trace_overview(
    days: Option<i64>,
) -> crate::analyzers::trace_overview::TraceOverview {
    let provider = analyzer_provider();
    offload(move || {
        let sessions = analyzer_sessions(days);
        let traces = analyzer_traces(&sessions);
        let cache = crate::analyzers::cache_health::analyze_for_provider(provider, &sessions);
        crate::analyzers::trace_overview::build(
            provider,
            &sessions,
            &traces,
            cache.trend_weighted_ratio,
        )
    })
    .await
}

#[tauri::command]
pub async fn get_recommendations(
    days: Option<i64>,
) -> Vec<crate::analyzers::recommendations::Recommendation> {
    let provider = analyzer_provider();
    offload(move || {
        let sessions = analyzer_sessions(days);
        let traces = analyzer_traces(&sessions);
        recommendations_from_traces(provider, &sessions, &traces)
    })
    .await
}

/// Look up a recommendation by id and return its `fix_prompt` so the frontend
/// can `navigator.clipboard.writeText(...)` it. Returns an empty string if
/// no matching recommendation exists for the current data window.
#[tauri::command]
pub async fn copy_fix_prompt(rec_id: String) -> String {
    let provider = analyzer_provider();
    offload(move || {
        let sessions = analyzer_sessions(None);
        let traces = analyzer_traces(&sessions);
        recommendations_from_traces(provider, &sessions, &traces)
            .into_iter()
            .find(|r| r.id == rec_id)
            .map(|r| r.fix_prompt)
            .unwrap_or_default()
    })
    .await
}

fn recommendations_from_traces(
    provider: Provider,
    sessions: &[crate::db::HistoricalSession],
    traces: &std::collections::HashMap<String, crate::analyzers::session_trace::SessionTrace>,
) -> Vec<crate::analyzers::recommendations::Recommendation> {
    let cache = crate::analyzers::cache_health::analyze_for_provider(provider, sessions);
    let routing = provider
        .capabilities()
        .model_routing
        .then(|| crate::analyzers::model_routing::analyze(sessions));
    let inflections = crate::analyzers::inflection::detect_for_provider(provider, sessions);
    let tool_frequency = crate::analyzers::tool_frequency::analyze(sessions, traces);
    let prompt_complexity = crate::analyzers::prompt_complexity::analyze(sessions, traces);
    let session_health = crate::analyzers::session_health::analyze(
        sessions,
        traces,
        &tool_frequency,
        &prompt_complexity,
    );
    let ctx = crate::analyzers::recommendations::AnalysisContext {
        provider,
        sessions,
        cache: &cache,
        routing: routing.as_ref(),
        inflections: &inflections,
        tool_frequency: Some(&tool_frequency),
        prompt_complexity: Some(&prompt_complexity),
        session_health: Some(&session_health),
    };
    crate::analyzers::recommendations::generate(&ctx)
}

#[derive(Serialize)]
pub struct ReportsBundle {
    pub provider: String,
    pub capabilities: cc_discord_presence::provider::ProviderCapabilities,
    pub days: i64,
    pub total_sessions: usize,
    pub recommendations: Vec<crate::analyzers::recommendations::Recommendation>,
    pub trace_overview: crate::analyzers::trace_overview::TraceOverview,
    pub tool_frequency: crate::analyzers::tool_frequency::ToolFrequencyReport,
    pub prompt_complexity: crate::analyzers::prompt_complexity::PromptComplexityReport,
    pub session_health: crate::analyzers::session_health::SessionHealthReport,
    pub cache_health: crate::analyzers::cache_health::CacheHealthReport,
    pub model_routing: Option<crate::analyzers::model_routing::ModelRoutingReport>,
    pub inflection_points: Vec<crate::analyzers::inflection::InflectionPoint>,
    /// Daily spend series for the requested window, oldest first, with every
    /// day in range present (zero-filled). The Reports timeline plots this
    /// directly, so gaps must be explicit days rather than missing points.
    pub daily_costs: Vec<DailyCostPoint>,
}

/// One day on the Reports cost timeline.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DailyCostPoint {
    pub date: String,
    pub cost: f64,
    pub sessions: i64,
}

#[tauri::command]
pub async fn get_reports_bundle(days: Option<i64>, project: Option<String>) -> ReportsBundle {
    let provider = analyzer_provider();
    let (claude_roots, codex_roots) = analyzer_roots();
    offload(move || {
        let sessions = crate::db::get_session_history(
            Some(days.unwrap_or(30)),
            project.as_deref(),
            Some(5000),
        );
        let daily_costs = window_daily_costs(days.unwrap_or(30), project.as_deref());
        build_reports_bundle_from_roots(
            provider,
            days,
            sessions,
            daily_costs,
            claude_roots,
            codex_roots,
        )
    })
    .await
}

/// Synchronous form of [`get_reports_bundle`] for callers that are already off
/// the UI thread (currently the debug-only dev bridge).
pub fn reports_bundle_blocking(days: i64, project: Option<String>) -> ReportsBundle {
    let provider = analyzer_provider();
    let (claude_roots, codex_roots) = analyzer_roots();
    let sessions = crate::db::get_session_history(Some(days), project.as_deref(), Some(5000));
    let daily_costs = window_daily_costs(days, project.as_deref());
    build_reports_bundle_from_roots(
        provider,
        Some(days),
        sessions,
        daily_costs,
        claude_roots,
        codex_roots,
    )
}

/// Window-wide cost totals for the Cost Analysis KPIs.
///
/// The view lists a capped page of recent sessions for its table, but a KPI
/// that says "Total spent (30d)" has to cover the whole window. Summing the
/// visible page instead understates real spend by exactly the sessions that
/// did not fit — which is how the screen came to report $7.73 against an
/// actual $7,371.35.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct CostTotals {
    pub days: i64,
    pub sessions: usize,
    pub total_cost: f64,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
    pub total_tokens: i64,
    pub cache_read_tokens: i64,
    /// Input tokens that were neither written to nor read from cache. Paired
    /// with `input_cost` this yields the window's true per-token input rate,
    /// which is what the cache-savings estimate needs.
    pub pure_input_tokens: i64,
    /// Window-wide spend per model, highest first. Computed here so the
    /// breakdown always reconciles with `total_cost` instead of describing
    /// only the page of sessions the table happens to show.
    pub by_model: Vec<CostSlice>,
    /// Window-wide spend per project, highest first.
    pub by_project: Vec<CostSlice>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct CostSlice {
    pub label: String,
    pub cost: f64,
    pub sessions: usize,
}

#[tauri::command]
pub async fn get_cost_totals(days: Option<i64>, project: Option<String>) -> CostTotals {
    offload(move || cost_totals_blocking(days.unwrap_or(30), project)).await
}

pub fn cost_totals_blocking(days: i64, project: Option<String>) -> CostTotals {
    // No limit: these are aggregates over the whole window by definition.
    let sessions = crate::db::get_session_history(Some(days), project.as_deref(), None);
    aggregate_cost_totals(days, &sessions)
}

fn aggregate_cost_totals(days: i64, sessions: &[crate::db::HistoricalSession]) -> CostTotals {
    use std::collections::HashMap;

    let mut totals = CostTotals {
        days,
        sessions: sessions.len(),
        ..Default::default()
    };
    let mut by_model: HashMap<&str, (f64, usize)> = HashMap::new();
    let mut by_project: HashMap<&str, (f64, usize)> = HashMap::new();

    for s in sessions {
        totals.total_cost += s.total_cost;
        totals.input_cost += s.input_cost;
        totals.output_cost += s.output_cost;
        totals.cache_write_cost += s.cache_write_cost;
        totals.cache_read_cost += s.cache_read_cost;
        totals.total_tokens += s.total_tokens;
        totals.cache_read_tokens += s.cache_read_tokens;
        totals.pure_input_tokens +=
            (s.input_tokens - s.cache_write_tokens - s.cache_read_tokens).max(0);

        let m = by_model.entry(s.model.as_str()).or_insert((0.0, 0));
        m.0 += s.total_cost;
        m.1 += 1;
        let p = by_project.entry(s.project.as_str()).or_insert((0.0, 0));
        p.0 += s.total_cost;
        p.1 += 1;
    }

    totals.by_model = into_sorted_slices(by_model);
    totals.by_project = into_sorted_slices(by_project);
    totals
}

/// Sorts a label -> (cost, sessions) map into descending-cost slices.
fn into_sorted_slices(map: std::collections::HashMap<&str, (f64, usize)>) -> Vec<CostSlice> {
    let mut slices: Vec<CostSlice> = map
        .into_iter()
        .map(|(label, (cost, sessions))| CostSlice {
            label: label.to_string(),
            cost,
            sessions,
        })
        .collect();
    slices.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            // Stable tie-break so equal-cost rows do not reshuffle between polls.
            .then_with(|| a.label.cmp(&b.label))
    });
    slices
}

/// First calendar date plotted for a `days`-long window, inclusive.
///
/// The series spans today plus `days - 1` earlier dates. Aggregating from this
/// date keeps the boundary day whole, so a session that lands earlier in the
/// day than the rolling `now - days` cutoff still appears on the curve that
/// describes it.
fn window_start_date(days: i64) -> chrono::NaiveDate {
    let span = days.max(1);
    chrono::Utc::now().date_naive() - chrono::Duration::days(span - 1)
}

/// Reads window-wide daily spend straight from SQLite.
///
/// Separate from the analyzer session page on purpose: the analyzers accept a
/// capped page of recent rows, but a timeline built from that page would
/// zero-fill days whose sessions were discarded.
fn window_daily_costs(days: i64, project: Option<&str>) -> Vec<crate::db::DailyCostRow> {
    let from = window_start_date(days).format("%Y-%m-%d").to_string();
    crate::db::get_daily_costs(&from, project)
}

/// Zero-fills SQL daily totals into one point per day, oldest first.
///
/// Zero-filling matters for the timeline: a day with no sessions is a real
/// observation (you did not spend), not a missing sample. Plotting only the
/// days that exist would silently compress idle stretches and distort the
/// shape of the curve.
fn daily_cost_series(rows: &[crate::db::DailyCostRow], days: i64) -> Vec<DailyCostPoint> {
    use std::collections::HashMap;

    let by_date: HashMap<&str, (f64, i64)> = rows
        .iter()
        .map(|row| (row.date.as_str(), (row.cost, row.sessions)))
        .collect();

    let span = days.max(1);
    let start = window_start_date(days);
    (0..span)
        .map(|offset| {
            let date = (start + chrono::Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string();
            let (cost, sessions) = by_date.get(date.as_str()).copied().unwrap_or((0.0, 0));
            DailyCostPoint {
                date,
                cost,
                sessions,
            }
        })
        .collect()
}

pub fn build_reports_bundle_from_roots(
    provider: Provider,
    days: Option<i64>,
    sessions: Vec<crate::db::HistoricalSession>,
    daily_costs: Vec<crate::db::DailyCostRow>,
    claude_roots: Vec<PathBuf>,
    codex_roots: Vec<PathBuf>,
) -> ReportsBundle {
    let traces = crate::analyzers::session_trace::load_session_traces_from_roots(
        &sessions,
        claude_roots,
        codex_roots,
    );

    let cache_health = crate::analyzers::cache_health::analyze_for_provider(provider, &sessions);
    let capabilities = provider.capabilities();
    let model_routing = capabilities
        .model_routing
        .then(|| crate::analyzers::model_routing::analyze(&sessions));
    let inflection_points = crate::analyzers::inflection::detect_for_provider(provider, &sessions);
    let tool_frequency = crate::analyzers::tool_frequency::analyze(&sessions, &traces);
    let prompt_complexity = crate::analyzers::prompt_complexity::analyze(&sessions, &traces);
    let session_health = crate::analyzers::session_health::analyze(
        &sessions,
        &traces,
        &tool_frequency,
        &prompt_complexity,
    );
    let trace_overview = crate::analyzers::trace_overview::build(
        provider,
        &sessions,
        &traces,
        cache_health.trend_weighted_ratio,
    );
    let ctx = crate::analyzers::recommendations::AnalysisContext {
        provider,
        sessions: &sessions,
        cache: &cache_health,
        routing: model_routing.as_ref(),
        inflections: &inflection_points,
        tool_frequency: Some(&tool_frequency),
        prompt_complexity: Some(&prompt_complexity),
        session_health: Some(&session_health),
    };
    let recommendations = crate::analyzers::recommendations::generate(&ctx);

    ReportsBundle {
        provider: provider.as_str().to_string(),
        capabilities,
        days: days.unwrap_or(30),
        total_sessions: sessions.len(),
        daily_costs: daily_cost_series(&daily_costs, days.unwrap_or(30)),
        recommendations,
        trace_overview,
        tool_frequency,
        prompt_complexity,
        session_health,
        cache_health,
        model_routing,
        inflection_points,
    }
}

#[cfg(test)]
mod tests {
    use super::aggregate_cost_totals;
    use super::{
        build_claude_context_breakdown, build_claude_session_infos, build_codex_discord_preview,
        build_codex_session_infos, codex_plan_key_from_tier, codex_total_input_tokens,
        daily_cost_series, plan_key_from_override, semantic_snapshot_fingerprint,
    };
    use cc_discord_presence::codex::config::{
        DesktopPresenceDesign, DisplayConfig, OpenAiPlanMode,
        PresenceConfig as TestCodexPresenceConfig, PresenceSurface as TestPresenceSurface,
        PrivacyConfig,
    };
    use cc_discord_presence::codex::cost::{
        CostAttribution, PricingSource, PricingStatus, TokenCostBreakdown,
    };
    use cc_discord_presence::codex::model::{SessionSpeed, SpeedMode, SpeedSource};
    use cc_discord_presence::codex::session::{
        CodexSessionSnapshot, ContextWindowSnapshot, ContextWindowSource,
    };
    use cc_discord_presence::codex::telemetry::limits::RateLimits;
    use cc_discord_presence::codex::telemetry::plan::DetectedPlanTier;
    use cc_discord_presence::config::PresenceConfig as TestClaudePresenceConfig;
    use cc_discord_presence::cost;
    use cc_discord_presence::session::{ClaudeSessionSnapshot, DataSource, ReasoningEffort, Speed};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    #[test]
    fn snapshot_fingerprint_ignores_wall_clock_fields_but_not_telemetry() {
        let first = serde_json::json!({
            "health": {"uptime_seconds": 10, "status": "ok"},
            "discord_preview": {"duration_secs": 10, "state": "7d 96%"},
            "sessions": [{"duration_secs": 10, "tokens_per_sec": 4.5, "tokens": 100}]
        });
        let second = serde_json::json!({
            "health": {"uptime_seconds": 15, "status": "ok"},
            "discord_preview": {"duration_secs": 15, "state": "7d 96%"},
            "sessions": [{"duration_secs": 15, "tokens_per_sec": 3.0, "tokens": 100}]
        });
        let changed = serde_json::json!({
            "health": {"uptime_seconds": 15, "status": "ok"},
            "discord_preview": {"duration_secs": 15, "state": "7d 95%"},
            "sessions": [{"duration_secs": 15, "tokens_per_sec": 3.0, "tokens": 101}]
        });

        assert_eq!(
            semantic_snapshot_fingerprint(first),
            semantic_snapshot_fingerprint(second)
        );
        assert_ne!(
            semantic_snapshot_fingerprint(serde_json::json!({
                "health": {"uptime_seconds": 15, "status": "ok"},
                "discord_preview": {"duration_secs": 15, "state": "7d 96%"},
                "sessions": [{"duration_secs": 15, "tokens_per_sec": 3.0, "tokens": 100}]
            })),
            semantic_snapshot_fingerprint(changed)
        );
    }

    #[test]
    fn usage_snapshot_transport_uses_public_scope_labels() {
        let snapshot = codex_presence_core::UsageSnapshot {
            provider: "codex".to_string(),
            scopes: vec![codex_presence_core::QuotaScope {
                id: Some("codex".to_string()),
                name: None,
                kind: codex_presence_core::RateLimitScope::GlobalCodex,
                windows: Vec::new(),
            }],
            credits: None,
            observed_at: None,
            source: "fixture".to_string(),
        };
        let value = serde_json::to_value(snapshot).expect("usage snapshot JSON");
        assert_eq!(value["scopes"][0]["kind"], "global");
    }

    fn sample_claude_snapshot(model_id: &str) -> ClaudeSessionSnapshot {
        ClaudeSessionSnapshot {
            session_id: format!("{model_id}-session"),
            cwd: PathBuf::from("D:/X/Pulse"),
            project_name: "pulse".into(),
            git_branch: None,
            model: Some(model_id.to_string()),
            model_display: Some(cost::model_display_name(model_id)),
            session_total_tokens: Some(120_000),
            last_turn_tokens: Some(2_000),
            session_delta_tokens: None,
            input_tokens: 100_000,
            output_tokens: 20_000,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            max_turn_api_input: 100_000,
            current_context_tokens: 100_000,
            reasoning_effort: ReasoningEffort::High,
            reasoning_effort_explicit: true,
            has_thinking_blocks: false,
            speed: Speed::Standard,
            service_tier: None,
            total_cost: 2.0,
            input_cost: 1.0,
            output_cost: 1.0,
            cache_write_cost: 0.0,
            cache_read_cost: 0.0,
            total_api_duration_ms: 0,
            limits: cc_discord_presence::session::RateLimits::default(),
            activity: None,
            started_at: None,
            last_token_event_at: None,
            last_activity: SystemTime::now(),
            source: DataSource::Jsonl,
            source_file: PathBuf::from("session.jsonl"),
            background_work: cc_discord_presence::workflow_state::BackgroundWorkInfo::default(),
            subagents: Vec::new(),
            is_subagent: false,
            parent_session_id: None,
        }
    }

    #[test]
    fn claude_session_info_carries_the_real_time_sonnet_5_intro_pricing_badge() {
        let snapshots = [sample_claude_snapshot("claude-sonnet-5")];
        let infos = build_claude_session_infos(&snapshots);
        let expected = cost::active_intro_pricing("claude-sonnet-5", chrono::Utc::now());

        assert_eq!(infos[0].intro_pricing, expected);
    }

    /// `CLAUDE_HOME` / `CODEX_HOME` are process-global, so every test that
    /// redirects them must hold this lock or they clobber each other under
    /// libtest's default multi-threaded runner.
    static HOME_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Redirects both provider homes at a fresh temp tree and returns the guard
    /// that keeps other home-mutating tests out until the caller is done.
    fn isolated_homes(tag: &str) -> (std::sync::MutexGuard<'static, ()>, PathBuf, PathBuf) {
        let guard = HOME_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = std::env::temp_dir().join(format!("pulse-{tag}-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        let claude_home = temp.join("claude");
        let codex_home = temp.join("codex");
        std::fs::create_dir_all(&claude_home).expect("claude home");
        std::fs::create_dir_all(&codex_home).expect("codex home");
        unsafe {
            std::env::set_var("CLAUDE_HOME", &claude_home);
            std::env::set_var("CODEX_HOME", &codex_home);
        }
        (guard, claude_home, codex_home)
    }

    #[test]
    fn claude_rich_presence_toggle_survives_a_restart() {
        let (_guard, _claude_home, _codex_home) = isolated_homes("claude-rp-toggle");
        super::set_active_provider("claude".to_string());

        let settings = super::set_discord_enabled(false).expect("disable Claude presence");
        assert!(!settings.enabled, "the command must report the new state");

        // Restart equivalent: nothing but the on-disk config survives, and the
        // startup path is what seeds the in-memory cache.
        let claude = TestClaudePresenceConfig::load_or_init().expect("claude config");
        assert!(
            !claude.presence_enabled,
            "turning Rich Presence off must be written to the Claude config"
        );

        if let Ok(mut data) = super::shared().lock() {
            data.discord_enabled = true; // pretend a fresh process default
        }
        super::seed_discord_state_from_disk();

        // The poller decides whether to publish to Discord from this cached flag,
        // so seeding it wrong is what makes a paused presence come back to life.
        let cached_enabled = super::shared()
            .lock()
            .expect("shared state")
            .discord_enabled;
        assert!(
            !cached_enabled,
            "startup must seed the cached presence flag from disk, not force it on"
        );

        let after_restart = super::get_discord_settings().expect("settings after restart");
        assert!(
            !after_restart.enabled,
            "startup must honour the persisted Claude presence flag instead of forcing it on"
        );
    }

    #[test]
    fn claude_display_prefs_persist_even_when_the_codex_mirror_write_fails() {
        let (_guard, _claude_home, codex_home) = isolated_homes("mirror-failure");
        super::set_active_provider("claude".to_string());

        // Make the Codex home unusable: a regular file where a directory is
        // expected, so `CodexPresenceConfig::load_or_init` cannot succeed.
        std::fs::remove_dir_all(&codex_home).expect("clear codex home");
        std::fs::write(&codex_home, b"not a directory").expect("occupy codex home");

        let settings = super::set_discord_display_prefs(
            true, false, true, true, true, true, true, false, true, true,
        )
        .expect("a broken Codex mirror must not fail the active provider's save");

        assert!(
            !settings.display_prefs.show_branch,
            "the returned canonical payload must reflect the change the user made"
        );
        let claude = TestClaudePresenceConfig::load_or_init().expect("claude config");
        assert!(
            !claude.privacy.show_git_branch,
            "the Claude config must keep the change even though the mirror write failed"
        );
    }

    #[test]
    fn saving_claude_toggles_preserves_the_codex_credits_preference() {
        let (_guard, _claude_home, _codex_home) = isolated_homes("credits-mirror");

        // Codex owns Credits and has it on.
        super::set_active_provider("codex".to_string());
        super::set_discord_display_prefs(
            true, true, true, true, true, true, true, true, true, true,
        )
        .expect("save under codex");
        assert!(
            TestCodexPresenceConfig::load_or_init()
                .expect("codex config")
                .privacy
                .show_credits
        );

        // Claude has no Credits field, so its payload always carries `false`.
        // Saving an unrelated Claude toggle must not disable Codex Credits.
        super::set_active_provider("claude".to_string());
        super::set_discord_display_prefs(
            true, false, true, true, true, true, true, false, true, true,
        )
        .expect("save under claude");

        let codex = TestCodexPresenceConfig::load_or_init().expect("codex config");
        assert!(
            codex.privacy.show_credits,
            "a Claude-side save must not clobber a Codex-only preference"
        );
        assert!(
            !codex.privacy.show_git_branch,
            "shared fields still mirror across providers"
        );
    }

    #[test]
    fn discord_controls_are_saved_for_claude_and_codex_together() {
        let (_guard, _claude_home, _codex_home) = isolated_homes("display-prefs");

        super::set_discord_display_prefs(
            true, false, true, true, true, true, false, true, false, true,
        )
        .expect("save display preferences");
        super::set_active_provider("codex".to_string());
        let settings = super::set_discord_enabled(false).expect("disable Codex presence");

        let claude = TestClaudePresenceConfig::load_or_init().expect("claude config");
        let codex = TestCodexPresenceConfig::load_or_init().expect("codex config");

        assert!(!claude.privacy.show_git_branch);
        assert!(!codex.privacy.show_git_branch);
        assert!(claude.privacy.show_cost);
        assert!(codex.privacy.show_cost);
        assert!(!claude.privacy.show_limits);
        assert!(!codex.privacy.show_limits);
        assert!(codex.privacy.show_credits);
        assert!(!claude.privacy.show_context);
        assert!(!codex.privacy.show_context);
        assert!(claude.privacy.show_systems);
        assert!(codex.privacy.show_systems);
        assert!(!codex.presence_enabled);
        assert!(!settings.enabled);
    }

    #[test]
    fn codex_discord_settings_reflect_persisted_privacy_design_and_publisher() {
        let config = TestCodexPresenceConfig {
            presence_enabled: false,
            privacy: PrivacyConfig {
                show_git_branch: false,
                ..PrivacyConfig::default()
            },
            display: DisplayConfig {
                desktop_presence_design: DesktopPresenceDesign::ChatGptApp,
                ..DisplayConfig::default()
            },
            ..TestCodexPresenceConfig::default()
        };
        let cached = super::CachedData {
            active_provider: cc_discord_presence::provider::Provider::Codex,
            discord_status: "Controlled by external daemon".to_string(),
            discord_publisher: "external_daemon".to_string(),
            discord_enabled: true,
            ..Default::default()
        };

        let settings = super::build_discord_settings(
            cc_discord_presence::provider::Provider::Codex,
            &cached,
            None,
            Some(&config),
        );

        assert_eq!(settings.provider, "codex");
        assert!(!settings.enabled);
        assert_eq!(settings.publisher, "external_daemon");
        assert!(!settings.display_prefs.show_branch);
        assert_eq!(settings.desktop_design.as_deref(), Some("chatgpt_app"));
        assert!(settings.supports_desktop_design);
    }

    #[test]
    fn claude_session_info_surfaces_ultracode_background_work_safely() {
        let mut snapshot = sample_claude_snapshot("claude-opus-4-8");
        snapshot.background_work = cc_discord_presence::workflow_state::BackgroundWorkInfo {
            workflow_active: true,
            active_agent_count: 2,
            latest_signal_at: Some(SystemTime::now()),
        };

        let infos = build_claude_session_infos(&[snapshot]);

        assert_eq!(infos[0].workflow_label.as_deref(), Some("ULTRACODE"));
        assert_eq!(infos[0].subagent_count, 2);
        assert!(infos[0].subagents.is_empty());
    }

    #[test]
    fn claude_session_info_does_not_call_plain_thinking_a_workflow() {
        let mut snapshot = sample_claude_snapshot("claude-opus-4-8");
        snapshot.has_thinking_blocks = true;
        snapshot.background_work =
            cc_discord_presence::workflow_state::BackgroundWorkInfo::default();

        let infos = build_claude_session_infos(&[snapshot]);

        assert_eq!(infos[0].workflow_label, None);
    }

    #[test]
    fn discord_preview_uses_the_same_claude_presence_lines_as_publish() {
        let mut snapshot = sample_claude_snapshot("claude-opus-4-8");
        snapshot.project_name = "PropertyAlpha-Agent".to_string();
        snapshot.git_branch = Some("feat/marketplace-addtochat-liveview-management".to_string());
        snapshot.background_work = cc_discord_presence::workflow_state::BackgroundWorkInfo {
            workflow_active: true,
            active_agent_count: 1,
            latest_signal_at: Some(SystemTime::now()),
        };

        let mut config = TestClaudePresenceConfig {
            plan: Some("max_20x".to_string()),
            ..Default::default()
        };
        super::apply_claude_display_prefs(
            &mut config,
            &super::DiscordDisplayPrefs {
                show_project: true,
                show_branch: false,
                show_model: true,
                show_activity: true,
                show_tokens: true,
                show_cost: true,
                show_limits: false,
                show_credits: false,
                show_context: false,
                show_systems: true,
            },
        );

        let preview = super::build_claude_discord_preview(&[snapshot], &config);

        assert!(preview.has_session);
        assert!(preview.details.contains("PropertyAlpha-Agent"));
        assert!(!preview.details.contains("feat/marketplace"));
        assert!(preview.state.contains("ULTRACODE"));
        assert!(preview.state.contains("1 agent"));
        assert!(!preview.state.contains("5h"));
        assert!(!preview.state.contains("7d"));
        assert!(!preview.state.contains("Ctx"));
    }

    #[test]
    fn claude_presence_candidate_keeps_background_work_when_token_event_is_stale() {
        let mut snapshot = sample_claude_snapshot("claude-opus-4-8");
        snapshot.last_token_event_at = Some(chrono::Utc::now() - chrono::Duration::minutes(20));
        snapshot.last_activity = SystemTime::now();
        snapshot.background_work = cc_discord_presence::workflow_state::BackgroundWorkInfo {
            workflow_active: true,
            active_agent_count: 1,
            latest_signal_at: Some(snapshot.last_activity),
        };

        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(600))
            .expect("cutoff");
        let cutoff_chrono = chrono::Utc::now() - chrono::Duration::seconds(600);

        assert!(super::is_claude_presence_candidate(
            &snapshot,
            cutoff,
            cutoff_chrono
        ));
    }

    #[test]
    fn claude_session_info_has_no_intro_pricing_badge_for_a_model_with_no_promo() {
        let snapshots = [sample_claude_snapshot("claude-sonnet-4-6")];
        let infos = build_claude_session_infos(&snapshots);

        assert!(infos[0].intro_pricing.is_none());
    }

    #[test]
    fn claude_session_info_flags_inflated_tokenizer_for_sonnet_5_and_opus_4_7_plus() {
        for model_id in ["claude-sonnet-5", "claude-opus-4-8"] {
            let snapshots = [sample_claude_snapshot(model_id)];
            let infos = build_claude_session_infos(&snapshots);
            assert!(infos[0].has_inflated_tokenizer, "{model_id}");
        }
    }

    #[test]
    fn claude_session_info_does_not_flag_inflated_tokenizer_for_sonnet_4_6() {
        let snapshots = [sample_claude_snapshot("claude-sonnet-4-6")];
        let infos = build_claude_session_infos(&snapshots);

        assert!(!infos[0].has_inflated_tokenizer);
    }

    #[test]
    fn codex_session_info_never_carries_an_intro_pricing_badge_or_inflated_tokenizer_flag() {
        let standard = build_codex_session_infos(
            &[sample_codex_snapshot()],
            &TestCodexPresenceConfig::default(),
            TestPresenceSurface::Cli,
        );

        assert!(standard[0].intro_pricing.is_none());
        assert!(!standard[0].has_inflated_tokenizer);
    }

    #[test]
    fn codex_session_info_does_not_invent_subagent_count() {
        let infos = build_codex_session_infos(
            &[sample_codex_snapshot()],
            &TestCodexPresenceConfig::default(),
            TestPresenceSurface::Cli,
        );

        assert_eq!(infos[0].subagent_count, 0);
        assert!(infos[0].subagents.is_empty());
        assert_eq!(infos[0].workflow_label, None);
    }

    #[test]
    fn session_info_context_used_tokens_reflects_current_fill_not_the_historical_peak() {
        let snapshot = ClaudeSessionSnapshot {
            max_turn_api_input: 999_486,
            current_context_tokens: 25_500,
            ..sample_claude_snapshot("claude-sonnet-5")
        };
        let infos = build_claude_session_infos(&[snapshot]);

        assert_eq!(
            infos[0].context_used_tokens, 25_500,
            "the live-session ctx-1m badge must show current fill, not the all-time peak"
        );
        assert_eq!(
            infos[0].context_window_tokens, 1_000_000,
            "the 1M-vs-200K window-size decision is unaffected -- it stays keyed off the \
             historical peak (max_turn_api_input), which correctly never decreases"
        );
    }

    #[test]
    fn context_breakdown_used_tokens_reflects_current_fill_not_the_historical_peak() {
        let snapshot = ClaudeSessionSnapshot {
            max_turn_api_input: 999_486,
            current_context_tokens: 25_500,
            ..sample_claude_snapshot("claude-sonnet-5")
        };
        let breakdown = build_claude_context_breakdown(Some(&snapshot));

        assert_eq!(
            breakdown.used_tokens, 25_500,
            "the Context Window view must show current fill, not the session's all-time peak"
        );
        assert_eq!(breakdown.context_window, 1_000_000);
        assert_eq!(breakdown.system_prompt, 0);
        assert_eq!(breakdown.system_tools, 0);
        assert_eq!(breakdown.autocompact_buffer, 0);
        assert_eq!(breakdown.messages, breakdown.used_tokens);
        assert_eq!(breakdown.free_space, 1_000_000 - 25_500);
        assert!(
            breakdown.free_space > 0,
            "a session that genuinely emptied out after compaction must show real free space, \
             not 0 (the exact symptom Tony reported: a CRITICAL \"100% full\" recommendation \
             for a session that isn't actually full right now)"
        );

        let empty = super::empty_context_breakdown("Claude", 200_000);
        assert_eq!(empty.used_tokens, 0);
        assert_eq!(empty.autocompact_buffer, 0);
        assert_eq!(empty.free_space, 200_000);
    }

    #[test]
    fn plan_key_from_override_accepts_display_labels_and_auto() {
        assert_eq!(plan_key_from_override("Max 20x ($200/mo)"), Some("max_20x"));
        assert_eq!(plan_key_from_override("Max 5x ($100/mo)"), Some("max_5x"));
        assert_eq!(plan_key_from_override("  Team plan  "), Some("team"));
        assert_eq!(plan_key_from_override("enterprise"), Some("enterprise"));
        assert_eq!(plan_key_from_override("pro monthly"), Some("pro"));
        assert_eq!(plan_key_from_override("free"), Some("free"));
        assert_eq!(plan_key_from_override("Max"), Some("max"));
        assert_eq!(plan_key_from_override("auto"), None);
        assert_eq!(plan_key_from_override(""), None);
    }

    #[test]
    fn codex_plan_key_maps_detected_tiers_to_frontend_contract() {
        assert_eq!(codex_plan_key_from_tier(DetectedPlanTier::Free), "free");
        assert_eq!(codex_plan_key_from_tier(DetectedPlanTier::Go), "go");
        assert_eq!(codex_plan_key_from_tier(DetectedPlanTier::Plus), "plus");
        assert_eq!(
            codex_plan_key_from_tier(DetectedPlanTier::Business),
            "business"
        );
        assert_eq!(
            codex_plan_key_from_tier(DetectedPlanTier::Enterprise),
            "enterprise"
        );
        assert_eq!(codex_plan_key_from_tier(DetectedPlanTier::Pro5x), "pro_5x");
        assert_eq!(
            codex_plan_key_from_tier(DetectedPlanTier::Pro20x),
            "pro_20x"
        );
        assert_eq!(codex_plan_key_from_tier(DetectedPlanTier::Unknown), "");
    }

    fn sample_codex_snapshot() -> CodexSessionSnapshot {
        CodexSessionSnapshot {
            session_id: "session".into(),
            cwd: PathBuf::from("D:/X/Web Development/MCP Servers/cc-discord-presence"),
            project_name: "pulse".into(),
            git_branch: None,
            model: Some("gpt-5.4".into()),
            originator: None,
            source: None,
            reasoning_effort: None,
            speed: SessionSpeed::default(),
            approval_policy: None,
            sandbox_policy: None,
            session_total_tokens: Some(54_764_083),
            last_turn_tokens: None,
            session_delta_tokens: None,
            input_tokens_total: 54_626_018,
            cached_input_tokens_total: 52_219_136,
            output_tokens_total: 138_065,
            last_input_tokens: None,
            last_cached_input_tokens: None,
            last_output_tokens: None,
            total_cost_usd: 0.0,
            known_cost_usd: None,
            cost_breakdown: TokenCostBreakdown {
                input_cost_usd: 0.0,
                cache_write_cost_usd: 0.0,
                cached_input_cost_usd: 0.0,
                output_cost_usd: 0.0,
                cached_input_savings_usd: 0.0,
            },
            pricing_source: PricingSource::Fallback,
            pricing_status: PricingStatus::Unavailable,
            cost_attribution: CostAttribution::SingleModel,
            cost_breakdown_reconciled: false,
            context_window: None,
            limits: RateLimits::default(),
            rate_limit_envelopes: Vec::new(),
            activity: None,
            started_at: None,
            last_token_event_at: None,
            last_activity: SystemTime::UNIX_EPOCH,
            source_file: PathBuf::from("C:/Users/xt0n1/.codex/sessions/sample.jsonl"),
        }
    }

    #[test]
    fn codex_total_input_tokens_uses_telemetry_total_without_double_counting_cache() {
        let snapshot = sample_codex_snapshot();
        assert_eq!(codex_total_input_tokens(&snapshot), 54_626_018);
    }

    #[test]
    fn codex_session_info_uses_the_canonical_effective_context_for_gpt_5_4() {
        // The snapshot carries no observed context window, so resolution falls
        // back to the model catalogue. Pin it here: without an explicit window
        // the resolver also consults ~/.codex/models_cache.json, which makes
        // the expected value depend on whichever Codex build last wrote that
        // file on the developer's machine.
        let mut snapshot = sample_codex_snapshot();
        snapshot.context_window = Some(ContextWindowSnapshot {
            raw_window_tokens: 272_000,
            window_tokens: 258_400,
            effective_percent: Some(95),
            used_tokens: 0,
            remaining_tokens: 258_400,
            remaining_percent: 100.0,
            source: ContextWindowSource::Event,
            raw_source: ContextWindowSource::Event,
        });

        let infos = build_codex_session_infos(
            &[snapshot],
            &TestCodexPresenceConfig::default(),
            TestPresenceSurface::Cli,
        );

        assert_eq!(infos[0].context_window, "258.4K");
        assert_eq!(infos[0].context_window_tokens, 258_400);
    }

    #[test]
    fn codex_discord_preview_idles_as_codex_app_when_desktop_is_running() {
        let config = TestCodexPresenceConfig::default();
        let preview = build_codex_discord_preview(&[], &config, true);

        assert_eq!(preview.app_name, "Codex App");
        assert_eq!(preview.details, "Codex App");
        assert_eq!(preview.state, "Idling...");
        assert!(!preview.has_session);
    }

    #[test]
    fn codex_discord_preview_uses_selected_chatgpt_identity_and_hides_branch() {
        let mut snapshot = sample_codex_snapshot();
        snapshot.model = Some("gpt-5.6-sol".into());
        snapshot.reasoning_effort = Some(cc_discord_presence::codex::model::ReasoningEffort::Max);
        snapshot.git_branch = Some("main".into());
        snapshot.originator = Some("Codex Desktop".into());
        let mut config = TestCodexPresenceConfig::default();
        config.display.desktop_presence_design = DesktopPresenceDesign::ChatGptApp;
        config.privacy.show_git_branch = false;
        config.openai_plan.mode = OpenAiPlanMode::Manual;

        let preview = build_codex_discord_preview(&[snapshot], &config, true);

        assert_eq!(preview.app_name, "ChatGPT App");
        assert_eq!(preview.large_image_key, "codex-logo");
        assert_eq!(preview.large_text, "ChatGPT App");
        assert!(!preview.details.contains("main"));
        assert!(
            preview
                .state
                .starts_with("GPT-5.6 Sol · Max | Pro 20x ($200/month)")
        );
    }

    #[test]
    fn context_recommendation_maps_each_tier_at_boundaries() {
        use super::{
            CONTEXT_COMPACT_NOW_PCT, CONTEXT_COMPACT_SOON_PCT, CONTEXT_WATCH_PCT,
            context_recommendation,
        };

        assert_eq!(CONTEXT_WATCH_PCT, 50.0);
        assert_eq!(CONTEXT_COMPACT_SOON_PCT, 80.0);
        assert_eq!(CONTEXT_COMPACT_NOW_PCT, 95.0);

        let healthy = context_recommendation(49.0);
        let watch = context_recommendation(50.0);
        let soon = context_recommendation(80.0);
        let now = context_recommendation(95.0);
        let full = context_recommendation(100.0);

        assert!(healthy.contains("healthy"));
        assert!(watch.contains("half"));
        assert!(soon.contains("compact soon"));
        assert!(now.contains("compact now"));
        assert!(full.contains("compact now"));

        assert_ne!(healthy, watch);
        assert_ne!(watch, soon);
        assert_ne!(soon, now);
    }

    #[test]
    fn codex_session_info_does_not_multiply_canonical_fast_cost_twice() {
        use super::build_codex_session_infos;

        let mut snapshot = sample_codex_snapshot();
        snapshot.model = Some("gpt-5.5".into());
        snapshot.speed = SessionSpeed::explicit(SpeedMode::Fast, SpeedSource::ThreadSettings);
        snapshot.total_cost_usd = 4.0;
        snapshot.known_cost_usd = Some(4.0);
        snapshot.pricing_status = PricingStatus::Exact;
        snapshot.cost_breakdown = TokenCostBreakdown {
            input_cost_usd: 1.0,
            cache_write_cost_usd: 0.0,
            cached_input_cost_usd: 0.5,
            output_cost_usd: 2.5,
            cached_input_savings_usd: 4.5,
        };

        let info = build_codex_session_infos(
            &[snapshot],
            &TestCodexPresenceConfig::default(),
            TestPresenceSurface::Cli,
        );

        assert!((info[0].cost - 4.0).abs() < 0.0001);
        assert!((info[0].input_cost - 1.0).abs() < 0.0001);
        assert!((info[0].output_cost - 2.5).abs() < 0.0001);
        assert!((info[0].cache_read_cost - 0.5).abs() < 0.0001);
        assert!(info[0].fast);
    }

    // ---- Reports cost timeline ---------------------------------------

    /// Builds a history row that landed `days_ago` days before today.
    /// A SQL daily total placed `days_ago` calendar days before today.
    fn cost_row(days_ago: i64, cost: f64, sessions: i64) -> crate::db::DailyCostRow {
        let date = chrono::Utc::now().date_naive() - chrono::Duration::days(days_ago);
        crate::db::DailyCostRow {
            date: date.format("%Y-%m-%d").to_string(),
            cost,
            sessions,
        }
    }

    /// A stored session dated `days_ago` calendar days before today.
    fn session_on_day(days_ago: i64, cost: f64) -> crate::db::HistoricalSession {
        let date = chrono::Utc::now().date_naive() - chrono::Duration::days(days_ago);
        crate::db::HistoricalSession {
            started_at: Some(format!("{}T12:00:00Z", date.format("%Y-%m-%d"))),
            total_cost: cost,
            ..Default::default()
        }
    }

    #[test]
    fn daily_cost_series_returns_one_point_per_day_oldest_first() {
        let series = daily_cost_series(&[], 7);
        assert_eq!(series.len(), 7);
        let dates: Vec<&str> = series.iter().map(|p| p.date.as_str()).collect();
        let mut sorted = dates.clone();
        sorted.sort_unstable();
        assert_eq!(dates, sorted, "series must run oldest to newest");
    }

    /// A day without sessions is a real "you spent nothing" observation, so it
    /// must appear as a zero point rather than vanish from the timeline.
    #[test]
    fn daily_cost_series_zero_fills_days_without_sessions() {
        let series = daily_cost_series(&[cost_row(1, 12.5, 1)], 5);
        assert_eq!(series.len(), 5);
        let zero_days = series.iter().filter(|p| p.cost == 0.0).count();
        assert_eq!(zero_days, 4);
        let spend_day = series.iter().find(|p| p.cost > 0.0).expect("spend day");
        assert!((spend_day.cost - 12.5).abs() < 0.000_001);
        assert_eq!(spend_day.sessions, 1);
    }

    #[test]
    fn daily_cost_series_carries_the_aggregated_day_total() {
        let series = daily_cost_series(&[cost_row(2, 8.0, 3)], 5);
        let day = series.iter().find(|p| p.sessions > 0).expect("busy day");
        assert!((day.cost - 8.0).abs() < 0.000_001);
        assert_eq!(day.sessions, 3);
    }

    #[test]
    fn daily_cost_series_ignores_sessions_outside_the_window() {
        // 60 days ago is outside a 7-day window; the series stays flat.
        let series = daily_cost_series(&[cost_row(60, 40.0, 1)], 7);
        assert_eq!(series.len(), 7);
        assert!(series.iter().all(|p| p.cost == 0.0));
    }

    /// The first plotted date is the same day the SQL aggregate starts from,
    /// so a session on the boundary day is charted instead of silently
    /// disagreeing with the analyzers that counted it.
    #[test]
    fn daily_cost_series_plots_the_boundary_day_of_the_window() {
        let series = daily_cost_series(&[cost_row(6, 7.25, 2)], 7);
        let first = series.first().expect("first day");
        assert_eq!(first.date, cost_row(6, 0.0, 0).date);
        assert!((first.cost - 7.25).abs() < 0.000_001);
        assert_eq!(first.sessions, 2);
    }

    // ---- Cost Analysis KPI totals ------------------------------------

    /// KPIs must cover every session in the window, not just the page the
    /// table happens to display.
    #[test]
    fn cost_totals_sum_every_session_in_the_window() {
        let sessions: Vec<_> = (0..250).map(|i| session_on_day(i % 30, 2.0)).collect();
        let totals = aggregate_cost_totals(30, &sessions);
        assert_eq!(totals.sessions, 250);
        assert!((totals.total_cost - 500.0).abs() < 0.000_001);
    }

    /// The four cost categories must reconcile with the headline total, or the
    /// "Cost by type" breakdown silently disagrees with "Total spent".
    #[test]
    fn cost_totals_categories_reconcile_with_the_headline() {
        let mut a = session_on_day(1, 0.0);
        a.input_cost = 1.5;
        a.output_cost = 2.0;
        a.cache_write_cost = 0.75;
        a.cache_read_cost = 0.25;
        a.total_cost = 4.5;

        let totals = aggregate_cost_totals(30, &[a]);
        let by_category = totals.input_cost
            + totals.output_cost
            + totals.cache_write_cost
            + totals.cache_read_cost;
        assert!((by_category - totals.total_cost).abs() < 0.000_001);
    }

    #[test]
    fn cost_totals_are_zero_for_an_empty_window() {
        let totals = aggregate_cost_totals(7, &[]);
        assert_eq!(totals.sessions, 0);
        assert_eq!(totals.total_cost, 0.0);
        assert_eq!(totals.days, 7);
    }

    /// The cache-savings estimate multiplies a token count by a per-token rate.
    /// Both must come from this aggregate, so `pure_input_tokens` has to be the
    /// denominator that matches `input_cost` over the same sessions.
    #[test]
    fn cost_totals_expose_pure_input_tokens_for_the_input_rate() {
        let mut s = session_on_day(1, 0.0);
        s.input_tokens = 100_000;
        s.cache_write_tokens = 30_000;
        s.cache_read_tokens = 50_000;
        s.input_cost = 0.1;

        let totals = aggregate_cost_totals(30, &[s]);
        // 100k total input minus 30k written minus 50k read = 20k pure.
        assert_eq!(totals.pure_input_tokens, 20_000);
        assert_eq!(totals.cache_read_tokens, 50_000);

        let rate = totals.input_cost / totals.pure_input_tokens as f64;
        assert!((rate - 0.000_005).abs() < 1e-9, "rate was {rate}");
    }

    /// Cache-heavy sessions can report more cached than raw input tokens; the
    /// pure-input count must clamp at zero rather than going negative and
    /// flipping the derived rate.
    #[test]
    fn cost_totals_never_report_negative_pure_input() {
        let mut s = session_on_day(1, 0.0);
        s.input_tokens = 1_000;
        s.cache_write_tokens = 5_000;
        s.cache_read_tokens = 9_000;

        let totals = aggregate_cost_totals(30, &[s]);
        assert_eq!(totals.pure_input_tokens, 0);
    }

    /// Per-model and per-project breakdowns must reconcile with the headline.
    /// Summing only the visible page made the bars claim $7.72 on a window
    /// whose real spend was $7,371.
    #[test]
    fn cost_totals_breakdowns_reconcile_with_the_headline() {
        let mut a = session_on_day(1, 10.0);
        a.model = "Claude Opus 5".into();
        a.project = "pulse".into();
        let mut b = session_on_day(2, 4.0);
        b.model = "Claude Sonnet 5".into();
        b.project = "pulse".into();
        let mut c = session_on_day(3, 6.0);
        c.model = "Claude Opus 5".into();
        c.project = "other".into();

        let totals = aggregate_cost_totals(30, &[a, b, c]);

        let model_sum: f64 = totals.by_model.iter().map(|m| m.cost).sum();
        let project_sum: f64 = totals.by_project.iter().map(|p| p.cost).sum();
        assert!((model_sum - totals.total_cost).abs() < 0.000_001);
        assert!((project_sum - totals.total_cost).abs() < 0.000_001);

        // Highest spend first, with sessions grouped rather than listed.
        assert_eq!(totals.by_model[0].label, "Claude Opus 5");
        assert!((totals.by_model[0].cost - 16.0).abs() < 0.000_001);
        assert_eq!(totals.by_model[0].sessions, 2);
        assert_eq!(totals.by_project[0].label, "pulse");
        assert!((totals.by_project[0].cost - 14.0).abs() < 0.000_001);
    }
}
