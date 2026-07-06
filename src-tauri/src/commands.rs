use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use cc_discord_presence::codex::config::{PresenceConfig as CodexPresenceConfig, PresenceSurface};
use cc_discord_presence::codex::discord::DiscordPresence as CodexDiscordPresence;
use cc_discord_presence::codex::discord::presence_lines as codex_presence_lines;
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
    self, ClaudeSessionSnapshot, GitBranchCache, SessionParseCache, Speed, latest_limits_source,
    merge_statusline_into_sessions, preferred_active_session, read_statusline_data,
};
use cc_discord_presence::usage::UsageManager;
use serde::Serialize;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);
const STALE_THRESHOLD: Duration = Duration::from_secs(120);
const STICKY_WINDOW: Duration = Duration::from_secs(120);
const ACTIVE_CUTOFF: Duration = Duration::from_secs(600);
const IDLE_CUTOFF: Duration = Duration::from_secs(300);

static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
fn uptime_secs() -> u64 {
    START_TIME.get_or_init(Instant::now).elapsed().as_secs()
}

#[derive(Clone)]
struct DiscordDisplayPrefs {
    show_project: bool,
    show_branch: bool,
    show_model: bool,
    show_activity: bool,
    show_tokens: bool,
    show_cost: bool,
    show_limits: bool,
    show_context: bool,
    show_systems: bool,
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
            show_context: true,
            show_systems: true,
        }
    }
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
    discord_status: String,
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
        DetectedPlanTier::Pro => "pro",
        DetectedPlanTier::Unknown => "",
    }
}

pub fn start_background_poller() {
    let data = Arc::clone(shared());

    if let Ok(mut d) = data.lock() {
        d.active_provider = cc_discord_presence::provider::load_active_provider();
        d.discord_enabled = true;
        if let Ok(cfg) = PresenceConfig::load_or_init() {
            d.discord_prefs = DiscordDisplayPrefs {
                show_project: cfg.privacy.show_project_name,
                show_branch: cfg.privacy.show_git_branch,
                show_model: cfg.privacy.show_model,
                show_activity: cfg.privacy.show_activity,
                show_tokens: cfg.privacy.show_tokens,
                show_cost: cfg.privacy.show_cost,
                show_limits: cfg.privacy.show_limits,
                show_context: cfg.privacy.show_context,
                show_systems: cfg.privacy.show_systems,
            };
        }
    }

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

            let discord_status = match provider {
                Provider::Claude => {
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
                    let status = if discord_enabled {
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
                    } else {
                        claude_discord.shutdown();
                        codex_discord.shutdown();
                        "Disabled".to_string()
                    };

                    if let Ok(mut d) = data.lock() {
                        d.sessions = ActiveSessions::Claude(active);
                        d.claude_usage = cached_usage;
                        d.claude_usage_error = usage_error;
                    }
                    status
                }
                Provider::Codex => {
                    if let Ok(fresh) = CodexPresenceConfig::load_or_init() {
                        codex_config = fresh;
                    }

                    apply_codex_display_prefs(&mut codex_config, &prefs);

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

                    let resolved_plan = codex_plan_detector
                        .resolve_from_sessions(&active, &codex_config.openai_plan);
                    let resolved_service_tier = resolve_service_tier();
                    let opencode_running =
                        cc_discord_presence::codex::process::is_opencode_running();
                    let codex_desktop_running =
                        cc_discord_presence::codex::process::is_desktop_surface_running();
                    let surface_override = if codex_desktop_running {
                        PresenceSurface::Desktop
                    } else {
                        PresenceSurface::Default
                    };

                    persist_live_codex_snapshots(
                        &active,
                        resolved_service_tier.is_fast(),
                        opencode_running,
                    );
                    let status = if discord_enabled {
                        let active_session = codex_session::preferred_active_session(&active);
                        let effective_limits = codex_session::latest_limits_source(&active);
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
                    } else {
                        claude_discord.shutdown();
                        codex_discord.shutdown();
                        "Disabled".to_string()
                    };

                    if let Ok(mut d) = data.lock() {
                        d.sessions = ActiveSessions::Codex(active);
                        d.codex_opencode_running = opencode_running;
                        d.codex_desktop_surface_running = codex_desktop_running;
                        d.claude_usage = None;
                        d.claude_usage_error = None;
                    }
                    status
                }
            };

            if let Ok(mut d) = data.lock() {
                d.discord_status = discord_status;
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

fn current_live_session_infos() -> Vec<SessionInfo> {
    match current_provider() {
        Provider::Claude => build_claude_session_infos(&read_claude_sessions()),
        Provider::Codex => {
            let fast_mode = resolve_service_tier().is_fast();
            build_codex_session_infos(
                &read_codex_sessions(),
                fast_mode,
                read_codex_desktop_surface_running(),
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiscordPresencePreview {
    pub provider: String,
    pub app_name: String,
    pub details: String,
    pub state: String,
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
    fast_mode: bool,
    desktop_surface_running: bool,
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
            let model_key = if model_id_raw.is_empty() {
                "gpt-5-codex".to_string()
            } else {
                model_id_raw.clone()
            };
            let display_name = cc_discord_presence::codex::util::format_model_display(
                &model_key,
                s.reasoning_effort,
                fast_mode,
            );
            let speed = cc_discord_presence::codex::cost::speed_multiplier(&model_key, fast_mode);
            let context_window = s
                .context_window
                .as_ref()
                .map(|snapshot| snapshot.window_tokens)
                .or_else(|| {
                    cc_discord_presence::codex::cost::default_model_context_window(&model_key)
                })
                .unwrap_or(cc_discord_presence::codex::cost::CODEX_OAUTH_CONTEXT_WINDOW);
            let context_window_label = if context_window >= 1_000_000 {
                "1M".to_string()
            } else if context_window >= 400_000 {
                "400K".to_string()
            } else {
                format!("{}K", context_window / 1_000)
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
            SessionInfo {
                provider: Provider::Codex.as_str().to_string(),
                app_name: if desktop_surface_running || s.is_desktop_surface() {
                    Some("Codex App".to_string())
                } else {
                    None
                },
                session_id: s.session_id.clone(),
                session_name: None,
                project: s.project_name.clone(),
                model: display_name,
                model_id: model_key,
                context_window: context_window_label,
                cost: s.total_cost_usd * speed,
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
                subagent_count: usize::from(s.is_subagent),
                subagents: Vec::new(),
                tokens_per_sec: 0.0,
                input_cost: s.cost_breakdown.input_cost_usd * speed,
                output_cost: s.cost_breakdown.output_cost_usd * speed,
                cache_write_cost: 0.0,
                cache_read_cost: s.cost_breakdown.cached_input_cost_usd * speed,
                speed: Speed::from_fast(fast_mode).as_str().to_string(),
                fast: fast_mode,
                service_tier: None,
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
    for s in result {
        crate::db::upsert_session(s);
        crate::db::update_daily_stats(s);
    }
    crate::db::mark_inactive(provider.as_str(), &active_ids);
}

fn persist_live_claude_snapshots(snapshots: &[ClaudeSessionSnapshot]) {
    let result = build_claude_session_infos(snapshots);
    persist_live_session_infos(Provider::Claude, &result);
}

fn persist_live_codex_snapshots(
    snapshots: &[CodexSessionSnapshot],
    fast_mode: bool,
    opencode_running: bool,
) {
    let result = build_codex_session_infos(snapshots, fast_mode, opencode_running);
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
pub fn set_discord_enabled(enabled: bool) {
    if let Ok(mut d) = shared().lock() {
        d.discord_enabled = enabled;
    }
}

#[tauri::command]
pub fn set_discord_display_prefs(
    show_project: bool,
    show_branch: bool,
    show_model: bool,
    show_activity: bool,
    show_tokens: bool,
    show_cost: bool,
    show_limits: bool,
    show_context: bool,
    show_systems: bool,
) {
    if let Ok(mut d) = shared().lock() {
        d.discord_prefs = DiscordDisplayPrefs {
            show_project,
            show_branch,
            show_model,
            show_activity,
            show_tokens,
            show_cost,
            show_limits,
            show_context,
            show_systems,
        };
    }
    if let Ok(mut cfg) = PresenceConfig::load_or_init() {
        apply_claude_display_prefs(
            &mut cfg,
            &DiscordDisplayPrefs {
                show_project,
                show_branch,
                show_model,
                show_activity,
                show_tokens,
                show_cost,
                show_limits,
                show_context,
                show_systems,
            },
        );
        log_save_error("claude-display-prefs", cfg.save());
    }
    if let Ok(mut cfg) = CodexPresenceConfig::load_or_init() {
        apply_codex_display_prefs(
            &mut cfg,
            &DiscordDisplayPrefs {
                show_project,
                show_branch,
                show_model,
                show_activity,
                show_tokens,
                show_cost,
                show_limits,
                show_context,
                show_systems,
            },
        );
        log_save_error("codex-display-prefs", cfg.save());
    }
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
            apply_codex_display_prefs(&mut config, &data.discord_prefs);
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

fn apply_codex_display_prefs(config: &mut CodexPresenceConfig, prefs: &DiscordDisplayPrefs) {
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
        has_session: true,
        duration_secs: claude_duration_secs(session),
    }
}

fn build_codex_discord_preview(
    sessions: &[CodexSessionSnapshot],
    config: &CodexPresenceConfig,
    desktop_surface_running: bool,
) -> DiscordPresencePreview {
    let app_name = if desktop_surface_running {
        "Codex App"
    } else {
        "Codex"
    };
    let Some(session) = codex_session::preferred_active_session(sessions) else {
        return DiscordPresencePreview {
            provider: Provider::Codex.as_str().to_string(),
            app_name: app_name.to_string(),
            details: app_name.to_string(),
            state: "Idling...".to_string(),
            has_session: false,
            duration_secs: 0,
        };
    };

    let resolved_service_tier = resolve_service_tier();
    let resolved_plan = PlanDetector::new().resolve_from_sessions(sessions, &config.openai_plan);
    let effective_limits = codex_session::latest_limits_source(sessions);
    let limits = effective_limits.as_ref().map(|item| &item.limits);
    let (details, state) = codex_presence_lines(
        session,
        limits,
        &resolved_plan,
        &resolved_service_tier,
        config,
    );

    DiscordPresencePreview {
        provider: Provider::Codex.as_str().to_string(),
        app_name: app_name.to_string(),
        details,
        state,
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
                    source: "api".into(),
                });
            }

            if let ActiveSessions::Claude(sessions) = &data.sessions
                && let Some(source) = session::latest_limits_source(sessions)
                && let Some(primary) = source.limits.primary.as_ref()
            {
                let secondary = source.limits.secondary.as_ref();
                return Some(RateLimitInfo {
                    provider: Provider::Claude.as_str().to_string(),
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
            let sessions = match &data.sessions {
                ActiveSessions::Codex(sessions) => sessions,
                _ => {
                    return Some(RateLimitInfo {
                        provider: Provider::Codex.as_str().to_string(),
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
                        source: "no codex telemetry yet".into(),
                    });
                }
            };
            if let Some(selected) = codex_session::latest_limits_source(sessions)
                && let Some(primary) = selected.limits.primary.as_ref()
            {
                let secondary = selected.limits.secondary.as_ref();
                return Some(RateLimitInfo {
                    provider: Provider::Codex.as_str().to_string(),
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
                    "pro" => Some(cc_discord_presence::codex::config::OpenAiPlanTier::Pro),
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
        if let Ok(mut d) = shared().lock() {
            d.active_provider = provider;
        }
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
        autocompact_buffer: (context_window as f64 * 0.033) as u64,
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
    let system_prompt: u64 = 10_000;
    let system_tools: u64 = 6_000;
    let known = system_prompt + system_tools + memory_total + skills_total + mcp_total;
    let used_tokens = if current_context_tokens > 0 {
        current_context_tokens.min(ctx_window)
    } else {
        (known + u64::from(selected.is_some()) * 1_000).min(ctx_window)
    };
    let messages = used_tokens.saturating_sub(known);
    let autocompact_buffer = (ctx_window as f64 * 0.033) as u64;
    let free_space = ctx_window.saturating_sub(used_tokens.saturating_add(autocompact_buffer));

    ContextBreakdown {
        model,
        context_window: ctx_window,
        used_tokens,
        free_space,
        autocompact_buffer,
        system_prompt,
        system_tools,
        memory_files,
        memory_total,
        skills,
        skills_total,
        messages,
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
    let autocompact_buffer = (ctx_window as f64 * 0.033) as u64;
    let system_prompt = memory_total.min(used_tokens);
    let known = system_prompt + skills_total + mcp_total;
    let messages = used_tokens.saturating_sub(known);
    let free_space = ctx_window.saturating_sub(used_tokens.saturating_add(autocompact_buffer));

    ContextBreakdown {
        model,
        context_window: ctx_window,
        used_tokens,
        free_space,
        autocompact_buffer,
        system_prompt,
        system_tools: 0,
        memory_files,
        memory_total,
        skills,
        skills_total,
        messages,
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
) -> crate::analyzers::model_routing::ModelRoutingReport {
    offload(move || crate::analyzers::model_routing::analyze(&analyzer_sessions(days))).await
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
    let routing = crate::analyzers::model_routing::analyze(sessions);
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
        routing: &routing,
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
    pub days: i64,
    pub total_sessions: usize,
    pub recommendations: Vec<crate::analyzers::recommendations::Recommendation>,
    pub trace_overview: crate::analyzers::trace_overview::TraceOverview,
    pub tool_frequency: crate::analyzers::tool_frequency::ToolFrequencyReport,
    pub prompt_complexity: crate::analyzers::prompt_complexity::PromptComplexityReport,
    pub session_health: crate::analyzers::session_health::SessionHealthReport,
    pub cache_health: crate::analyzers::cache_health::CacheHealthReport,
    pub model_routing: crate::analyzers::model_routing::ModelRoutingReport,
    pub inflection_points: Vec<crate::analyzers::inflection::InflectionPoint>,
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
        build_reports_bundle_from_roots(provider, days, sessions, claude_roots, codex_roots)
    })
    .await
}

pub fn build_reports_bundle_from_roots(
    provider: Provider,
    days: Option<i64>,
    sessions: Vec<crate::db::HistoricalSession>,
    claude_roots: Vec<PathBuf>,
    codex_roots: Vec<PathBuf>,
) -> ReportsBundle {
    let traces = crate::analyzers::session_trace::load_session_traces_from_roots(
        &sessions,
        claude_roots,
        codex_roots,
    );

    let cache_health = crate::analyzers::cache_health::analyze_for_provider(provider, &sessions);
    let model_routing = crate::analyzers::model_routing::analyze(&sessions);
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
        routing: &model_routing,
        inflections: &inflection_points,
        tool_frequency: Some(&tool_frequency),
        prompt_complexity: Some(&prompt_complexity),
        session_health: Some(&session_health),
    };
    let recommendations = crate::analyzers::recommendations::generate(&ctx);

    ReportsBundle {
        provider: provider.as_str().to_string(),
        days: days.unwrap_or(30),
        total_sessions: sessions.len(),
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
    use super::{
        build_claude_context_breakdown, build_claude_session_infos, build_codex_discord_preview,
        build_codex_session_infos, codex_plan_key_from_tier, codex_total_input_tokens,
        plan_key_from_override,
    };
    use cc_discord_presence::codex::config::PresenceConfig as TestCodexPresenceConfig;
    use cc_discord_presence::codex::cost::{PricingSource, TokenCostBreakdown};
    use cc_discord_presence::codex::session::CodexSessionSnapshot;
    use cc_discord_presence::codex::telemetry::limits::RateLimits;
    use cc_discord_presence::codex::telemetry::plan::DetectedPlanTier;
    use cc_discord_presence::config::PresenceConfig as TestClaudePresenceConfig;
    use cc_discord_presence::cost;
    use cc_discord_presence::session::{ClaudeSessionSnapshot, DataSource, ReasoningEffort, Speed};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

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

    #[test]
    fn display_prefs_are_saved_for_claude_and_codex_together() {
        let temp =
            std::env::temp_dir().join(format!("pulse-display-prefs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        let claude_home = temp.join("claude");
        let codex_home = temp.join("codex");
        std::fs::create_dir_all(&claude_home).expect("claude home");
        std::fs::create_dir_all(&codex_home).expect("codex home");
        unsafe {
            std::env::set_var("CLAUDE_HOME", &claude_home);
            std::env::set_var("CODEX_HOME", &codex_home);
        }

        super::set_discord_display_prefs(true, false, true, true, true, true, false, false, true);

        let claude = TestClaudePresenceConfig::load_or_init().expect("claude config");
        let codex = TestCodexPresenceConfig::load_or_init().expect("codex config");

        assert!(!claude.privacy.show_git_branch);
        assert!(!codex.privacy.show_git_branch);
        assert!(claude.privacy.show_cost);
        assert!(codex.privacy.show_cost);
        assert!(!claude.privacy.show_limits);
        assert!(!codex.privacy.show_limits);
        assert!(!claude.privacy.show_context);
        assert!(!codex.privacy.show_context);
        assert!(claude.privacy.show_systems);
        assert!(codex.privacy.show_systems);
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
        let standard = build_codex_session_infos(&[sample_codex_snapshot()], false, false);

        assert!(standard[0].intro_pricing.is_none());
        assert!(!standard[0].has_inflated_tokenizer);
    }

    #[test]
    fn codex_session_info_counts_subagent_source_safely() {
        let mut snapshot = sample_codex_snapshot();
        snapshot.is_subagent = true;

        let infos = build_codex_session_infos(&[snapshot], false, false);

        assert_eq!(infos[0].subagent_count, 1);
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
        assert!(
            breakdown.free_space > 0,
            "a session that genuinely emptied out after compaction must show real free space, \
             not 0 (the exact symptom Tony reported: a CRITICAL \"100% full\" recommendation \
             for a session that isn't actually full right now)"
        );
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
        assert_eq!(codex_plan_key_from_tier(DetectedPlanTier::Pro), "pro");
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
            cost_breakdown: TokenCostBreakdown {
                input_cost_usd: 0.0,
                cached_input_cost_usd: 0.0,
                output_cost_usd: 0.0,
                cached_input_savings_usd: 0.0,
            },
            pricing_source: PricingSource::Fallback,
            context_window: None,
            limits: RateLimits::default(),
            rate_limit_envelopes: Vec::new(),
            activity: None,
            started_at: None,
            last_token_event_at: None,
            last_activity: SystemTime::UNIX_EPOCH,
            source_file: PathBuf::from("C:/Users/xt0n1/.codex/sessions/sample.jsonl"),
            is_subagent: false,
        }
    }

    #[test]
    fn codex_total_input_tokens_uses_telemetry_total_without_double_counting_cache() {
        let snapshot = sample_codex_snapshot();
        assert_eq!(codex_total_input_tokens(&snapshot), 54_626_018);
    }

    #[test]
    fn codex_session_info_keeps_oauth_context_at_400k_for_gpt_5_4() {
        let infos = build_codex_session_infos(&[sample_codex_snapshot()], false, false);

        assert_eq!(infos[0].context_window, "400K");
        assert_eq!(infos[0].context_window_tokens, 400_000);
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
    fn fast_mode_scales_codex_cost_by_speed_multiplier() {
        use super::build_codex_session_infos;

        let mut snapshot = sample_codex_snapshot();
        snapshot.model = Some("gpt-5.5".into());
        snapshot.total_cost_usd = 4.0;
        snapshot.cost_breakdown = TokenCostBreakdown {
            input_cost_usd: 1.0,
            cached_input_cost_usd: 0.5,
            output_cost_usd: 2.5,
            cached_input_savings_usd: 4.5,
        };

        let standard = build_codex_session_infos(&[snapshot.clone()], false, false);
        let fast = build_codex_session_infos(&[snapshot], true, false);

        assert!((standard[0].cost - 4.0).abs() < 0.0001);
        assert!((fast[0].cost - 10.0).abs() < 0.0001);
        assert!((fast[0].input_cost - 2.5).abs() < 0.0001);
        assert!((fast[0].output_cost - 6.25).abs() < 0.0001);
        assert!((fast[0].cache_read_cost - 1.25).abs() < 0.0001);
    }
}
