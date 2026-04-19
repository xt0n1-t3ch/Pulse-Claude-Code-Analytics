use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use cc_discord_presence::config::PresenceConfig;
use cc_discord_presence::cost;
use cc_discord_presence::discord::DiscordPresence;
use cc_discord_presence::session::{
    self, ClaudeSessionSnapshot, GitBranchCache, SessionParseCache, latest_limits_source,
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
        }
    }
}

#[derive(Default, Clone)]
struct CachedData {
    sessions: Vec<ClaudeSessionSnapshot>,
    usage: Option<CachedUsage>,
    usage_error: Option<String>,
    discord_status: String,
    discord_enabled: bool,
    discord_prefs: DiscordDisplayPrefs,
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

fn plan_name_from_key(key: &str) -> String {
    match key {
        "free" => "Free".to_string(),
        "pro" => "Pro".to_string(),
        "max_5x" => "MAX 5X".to_string(),
        "max_20x" => "MAX 20X".to_string(),
        "max" => "MAX".to_string(),
        "team" => "Team".to_string(),
        "enterprise" => "Enterprise".to_string(),
        other => other.to_uppercase(),
    }
}

fn plan_key_from_override(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => None,
        "free" => Some("free"),
        "pro" => Some("pro"),
        "max 5x" | "max_5x" => Some("max_5x"),
        "max 20x" | "max_20x" => Some("max_20x"),
        "max" => Some("max"),
        "team" => Some("team"),
        "enterprise" => Some("enterprise"),
        _ => None,
    }
}

fn resolved_plan_key(
    config: &PresenceConfig,
    override_name: Option<&str>,
    detected_key: Option<&str>,
) -> Option<String> {
    if let Some(name) = override_name {
        return Some(plan_key_from_override(name).unwrap_or("max").to_string());
    }
    config
        .plan
        .clone()
        .or_else(|| detected_key.map(str::to_string))
}

pub fn start_background_poller() {
    let data = Arc::clone(shared());

    if let Ok(mut d) = data.lock() {
        d.discord_enabled = true;
        if let Ok(cfg) = PresenceConfig::load_or_init() {
            d.discord_prefs = DiscordDisplayPrefs {
                show_project: cfg.privacy.show_project_name,
                show_branch: cfg.privacy.show_git_branch,
                show_model: cfg.privacy.show_model,
                show_activity: cfg.privacy.show_activity,
                show_tokens: cfg.privacy.show_tokens,
                show_cost: cfg.privacy.show_cost,
            };
        }
    }

    thread::spawn(move || {
        let mut git = GitBranchCache::new(Duration::from_secs(30));
        let mut parse = SessionParseCache::default();
        let mut usage_mgr = UsageManager::new();
        let mut config = PresenceConfig::load_or_init().unwrap_or_default();
        let mut discord = DiscordPresence::new(config.effective_client_id());

        loop {
            if let Ok(fresh) = PresenceConfig::load_or_init() {
                config = fresh;
            }
            let now = SystemTime::now();
            let cutoff = now
                .checked_sub(ACTIVE_CUTOFF)
                .unwrap_or(SystemTime::UNIX_EPOCH);

            let mut all = session::collect_active_sessions(
                &mut git,
                &mut parse,
                STALE_THRESHOLD,
                STICKY_WINDOW,
            )
            .unwrap_or_default();

            // Merge statusline data for more accurate activity info
            if let Some(sl) = read_statusline_data(&mut git) {
                merge_statusline_into_sessions(&mut all, sl);
            }

            let cutoff_chrono =
                chrono::Utc::now() - chrono::Duration::seconds(ACTIVE_CUTOFF.as_secs() as i64);
            let active: Vec<_> = all
                .into_iter()
                .filter(|s| {
                    if s.is_subagent {
                        return false;
                    }
                    if let Some(ts) = s.last_token_event_at {
                        return ts >= cutoff_chrono;
                    }
                    s.last_activity >= cutoff
                })
                .collect();

            // Honor a UI-initiated refresh request: drop the in-memory cache so
            // the next get_usage() call skips the TTL window and hits the API.
            let force_refresh = data
                .lock()
                .ok()
                .map(|mut d| {
                    let req = d.usage_refresh_requested;
                    if req {
                        d.usage_refresh_requested = false;
                    }
                    req
                })
                .unwrap_or(false);
            if force_refresh {
                usage_mgr.invalidate_cache();
                let _ = std::fs::remove_file(
                    cc_discord_presence::config::claude_home()
                        .join("discord-presence-usage-cache.json"),
                );
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

            // Discord Rich Presence — update if enabled, shutdown if disabled
            let (discord_enabled, prefs) = data
                .lock()
                .ok()
                .map(|d| (d.discord_enabled, d.discord_prefs.clone()))
                .unwrap_or((true, DiscordDisplayPrefs::default()));
            config.privacy.show_project_name = prefs.show_project;
            config.privacy.show_git_branch = prefs.show_branch;
            config.privacy.show_model = prefs.show_model;
            config.privacy.show_activity = prefs.show_activity;
            config.privacy.show_tokens = prefs.show_tokens;
            config.privacy.show_cost = prefs.show_cost;
            let override_name = plan_override().lock().ok().and_then(|guard| guard.clone());
            config.plan = resolved_plan_key(
                &config,
                override_name.as_deref(),
                detected_plan_key.as_deref(),
            );

            persist_live_session_snapshots(&active);
            let discord_status = if discord_enabled {
                let active_session = preferred_active_session(&active);
                let limits = latest_limits_source(&active).map(|s| &s.limits);
                let _ = discord.update(active_session, limits, usage.as_ref(), &config);
                discord.status().to_string()
            } else {
                // Disconnect from Discord IPC when disabled
                discord.shutdown();
                "Disabled".to_string()
            };

            if let Ok(mut d) = data.lock() {
                d.sessions = active;
                if cached_usage.is_some() {
                    d.usage = cached_usage;
                }
                d.usage_error = usage_error;
                d.discord_status = discord_status;
            }

            thread::sleep(REFRESH_INTERVAL);
        }
    });
}

fn read_sessions() -> Vec<ClaudeSessionSnapshot> {
    shared().lock().ok().map_or(vec![], |d| d.sessions.clone())
}

// ── Response types ──

#[derive(Serialize)]
pub struct HealthResponse {
    pub version: &'static str,
    pub uptime_seconds: u64,
    pub discord_status: String,
    pub discord_enabled: bool,
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
    pub subagent_count: usize,
    pub subagents: Vec<SubagentDetail>,
    pub tokens_per_sec: f64,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
}

#[derive(Serialize)]
pub struct RateLimitInfo {
    pub five_hour_pct: f64,
    pub five_hour_resets: String,
    pub seven_day_pct: f64,
    pub seven_day_resets: String,
    pub sonnet_pct: Option<f64>,
    pub sonnet_resets: Option<String>,
    pub extra_enabled: bool,
    pub extra_limit: Option<f64>,
    pub extra_used: Option<f64>,
    pub extra_pct: Option<f64>,
    pub source: String,
}

fn build_session_infos(snapshots: &[ClaudeSessionSnapshot]) -> Vec<SessionInfo> {
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
            let p = s.model.as_ref().map(|m| cost::model_pricing(m));
            let pure_inp = s
                .input_tokens
                .saturating_sub(s.cache_creation_tokens)
                .saturating_sub(s.cache_read_tokens);
            let ic = p
                .as_ref()
                .map_or(0.0, |p| pure_inp as f64 * p.input_per_million / 1_000_000.0);
            let oc = p.as_ref().map_or(0.0, |p| {
                s.output_tokens as f64 * p.output_per_million / 1_000_000.0
            });
            let cwc = p.as_ref().map_or(0.0, |p| {
                s.cache_creation_tokens as f64 * p.cache_write_per_million / 1_000_000.0
            });
            let crc = p.as_ref().map_or(0.0, |p| {
                s.cache_read_tokens as f64 * p.cache_read_per_million / 1_000_000.0
            });
            let tps = if s.total_api_duration_ms > 0 {
                s.output_tokens as f64 / (s.total_api_duration_ms as f64 / 1000.0)
            } else {
                0.0
            };
            let model_id_raw = s.model.clone().unwrap_or_default();
            let has_1m = cost::is_ga_1m_context(&model_id_raw)
                || model_id_raw.contains("[1m]")
                || s.max_turn_api_input > 200_000;
            let ctx_window = if has_1m { "1M" } else { "200K" }.to_string();
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
            let session_name = read_session_name(&s.session_id);
            SessionInfo {
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
                branch: s.git_branch.clone(),
                activity: s
                    .activity
                    .as_ref()
                    .map_or("Idle".into(), |a| a.action_text().to_string()),
                activity_target: s.activity.as_ref().and_then(|a| a.target.clone()),
                effort: s.reasoning_effort.label().to_string(),
                effort_explicit: s.reasoning_effort_explicit,
                is_idle,
                started_at: s.started_at.map(|t| t.to_rfc3339()),
                duration_secs,
                has_thinking: s.has_thinking_blocks,
                subagent_count: subagent_details.len(),
                subagents: subagent_details,
                tokens_per_sec: tps,
                input_cost: ic,
                output_cost: oc,
                cache_write_cost: cwc,
                cache_read_cost: crc,
            }
        })
        .collect()
}

fn persist_live_session_infos(result: &[SessionInfo]) {
    let active_ids: Vec<String> = result.iter().map(|s| s.session_id.clone()).collect();
    for s in result {
        crate::db::upsert_session(s);
        crate::db::update_daily_stats(s);
    }
    crate::db::mark_inactive(&active_ids);
}

fn persist_live_session_snapshots(snapshots: &[ClaudeSessionSnapshot]) {
    let result = build_session_infos(snapshots);
    persist_live_session_infos(&result);
}

// ── Commands (all instant — read from cache) ──

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
) {
    if let Ok(mut d) = shared().lock() {
        d.discord_prefs = DiscordDisplayPrefs {
            show_project,
            show_branch,
            show_model,
            show_activity,
            show_tokens,
            show_cost,
        };
    }
    if let Ok(mut cfg) = PresenceConfig::load_or_init() {
        cfg.privacy.show_project_name = show_project;
        cfg.privacy.show_git_branch = show_branch;
        cfg.privacy.show_model = show_model;
        cfg.privacy.show_activity = show_activity;
        cfg.privacy.show_tokens = show_tokens;
        cfg.privacy.show_cost = show_cost;
        let _ = cfg.save();
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
    let sessions = read_sessions();
    let (mut cost, mut inp, mut out, mut cw, mut cr, mut tot) = (0.0, 0u64, 0u64, 0u64, 0u64, 0u64);
    for s in &sessions {
        cost += s.total_cost;
        inp += s.input_tokens;
        out += s.output_tokens;
        cw += s.cache_creation_tokens;
        cr += s.cache_read_tokens;
        tot += s.session_total_tokens.unwrap_or(0);
    }

    let pricing = |model: &Option<String>| -> (f64, f64, f64, f64) {
        model
            .as_ref()
            .map(|m| {
                let p = cost::model_pricing(m);
                (
                    p.input_per_million,
                    p.output_per_million,
                    p.cache_write_per_million,
                    p.cache_read_per_million,
                )
            })
            .unwrap_or((5.0, 25.0, 6.25, 0.50))
    };

    let (mut ic, mut oc, mut cwc, mut crc) = (0.0, 0.0, 0.0, 0.0);
    let mut model_map: std::collections::HashMap<String, (usize, f64, u64)> =
        std::collections::HashMap::new();
    for s in &sessions {
        let (ip, op, wp, rp) = pricing(&s.model);
        let pure = s
            .input_tokens
            .saturating_sub(s.cache_creation_tokens)
            .saturating_sub(s.cache_read_tokens);
        ic += pure as f64 * ip / 1_000_000.0;
        oc += s.output_tokens as f64 * op / 1_000_000.0;
        cwc += s.cache_creation_tokens as f64 * wp / 1_000_000.0;
        crc += s.cache_read_tokens as f64 * rp / 1_000_000.0;

        let model_name = s
            .model_display
            .clone()
            .or(s.model.as_ref().map(|m| cost::model_display_name(m)))
            .unwrap_or_else(|| "Unknown".into());
        let entry = model_map.entry(model_name).or_insert((0, 0.0, 0));
        entry.0 += 1;
        entry.1 += s.total_cost;
        entry.2 += s.session_total_tokens.unwrap_or(0);
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
    let snapshots = read_sessions();
    let result = build_session_infos(&snapshots);
    persist_live_session_infos(&result);
    result
}

#[tauri::command]
pub fn get_rate_limits() -> Option<RateLimitInfo> {
    let data = shared().lock().ok()?;

    // Prefer UsageManager (real API data)
    if let Some(u) = data.usage.as_ref() {
        return Some(RateLimitInfo {
            five_hour_pct: u.five_hour_pct,
            five_hour_resets: u.five_hour_resets.clone(),
            seven_day_pct: u.seven_day_pct,
            seven_day_resets: u.seven_day_resets.clone(),
            sonnet_pct: u.sonnet_pct,
            sonnet_resets: u.sonnet_resets.clone(),
            extra_enabled: u.extra_enabled,
            extra_limit: u.extra_limit,
            extra_used: u.extra_used,
            extra_pct: u.extra_pct,
            source: "api".into(),
        });
    }

    // Fallback to session JSONL headers
    if let Some(source) = session::latest_limits_source(&data.sessions) {
        if let Some(primary) = source.limits.primary.as_ref() {
            let secondary = source.limits.secondary.as_ref();
            return Some(RateLimitInfo {
                five_hour_pct: primary.used_percent,
                five_hour_resets: primary
                    .resets_at
                    .map_or("N/A".into(), |d| d.format("%H:%M UTC").to_string()),
                seven_day_pct: secondary.map_or(0.0, |s| s.used_percent),
                seven_day_resets: secondary
                    .and_then(|s| s.resets_at)
                    .map_or("N/A".into(), |d| d.format("%H:%M UTC").to_string()),
                sonnet_pct: None,
                sonnet_resets: None,
                extra_enabled: false,
                extra_limit: None,
                extra_used: None,
                extra_pct: None,
                source: data.usage_error.clone().unwrap_or_else(|| "session".into()),
            });
        }
    }

    // Return zeroed data with error hint so the UI doesn't show "Waiting..." forever
    let hint = data
        .usage_error
        .clone()
        .unwrap_or_else(|| "no data yet".into());
    Some(RateLimitInfo {
        five_hour_pct: 0.0,
        five_hour_resets: "N/A".into(),
        seven_day_pct: 0.0,
        seven_day_resets: "N/A".into(),
        sonnet_pct: None,
        sonnet_resets: None,
        extra_enabled: false,
        extra_limit: None,
        extra_used: None,
        extra_pct: None,
        source: hint,
    })
}

// ── Discord User from local LevelDB ──

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
        // Portable/per-user installs land under %LOCALAPPDATA% with PascalCase names.
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
        // Both lowercase and display-name variants have been observed across versions.
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
        // Standard install via package manager or AppImage.
        for v in &variants {
            dirs.push(
                home_path
                    .join(".config")
                    .join(v)
                    .join("Local Storage/leveldb"),
            );
        }
        // Flatpak — sandbox places config under ~/.var/app/<app-id>/config/.
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
        // Snap — config lives under ~/snap/<app>/current/.config/.
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

    // Sort by modified time descending — newest first for freshest avatar hash
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
    // Search for the MultiAccountStore user JSON pattern
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

                    // "0" discriminator means the account uses the new unique-username
                    // system (no #tag). Empty / missing → normalize to "0".
                    let discriminator = extract_json_field(&chunk_str, "discriminator")
                        .filter(|d| !d.is_empty())
                        .unwrap_or_else(|| "0".to_string());

                    let avatar_hash = extract_json_field(&chunk_str, "avatar")
                        .filter(|h| !h.is_empty())
                        .unwrap_or_default();

                    // If the user has no custom avatar, Discord serves a default
                    // based on (user_id >> 22) % 6 (new usernames) or discriminator % 5 (legacy).
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

// ── Plan Detection ──

#[derive(Serialize)]
pub struct PlanInfo {
    pub plan_name: String,
    pub detected: bool,
}

static PLAN_OVERRIDE: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
    std::sync::OnceLock::new();

fn plan_override() -> &'static std::sync::Mutex<Option<String>> {
    PLAN_OVERRIDE.get_or_init(|| std::sync::Mutex::new(None))
}

#[tauri::command]
pub fn get_plan_info() -> PlanInfo {
    if let Ok(guard) = plan_override().lock() {
        if let Some(ref name) = *guard {
            return PlanInfo {
                plan_name: name.clone(),
                detected: false,
            };
        }
    }

    if let Ok(cfg) = PresenceConfig::load_or_init()
        && let Some(plan) = cfg.plan.as_deref()
    {
        return PlanInfo {
            plan_name: plan_name_from_key(plan),
            detected: false,
        };
    }

    let mut usage_mgr = UsageManager::new();
    let plan_name = usage_mgr
        .detected_plan_key()
        .map(|key| plan_name_from_key(&key))
        .unwrap_or_else(|| "Unknown".to_string());

    PlanInfo {
        plan_name,
        detected: true,
    }
}

#[tauri::command]
pub fn set_plan_override(plan: String) {
    if let Ok(mut guard) = plan_override().lock() {
        if plan_key_from_override(&plan).is_none() {
            *guard = None;
        } else {
            *guard = Some(plan_name_from_key(plan_key_from_override(&plan).unwrap()));
        }
    }
}

// ── Historical Analytics (SQLite) ──

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

// ── Context Breakdown (like /context) ──

#[derive(Serialize)]
pub struct ContextFileEntry {
    pub name: String,
    pub tokens: u64,
}

#[derive(Serialize)]
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

fn estimate_tokens(text: &str) -> u64 {
    (text.len() as f64 / 3.5).ceil() as u64
}

fn estimate_tokens_from_file(path: &std::path::Path) -> u64 {
    std::fs::read_to_string(path)
        .map(|s| estimate_tokens(&s))
        .unwrap_or(0)
}

#[tauri::command]
pub fn get_context_breakdown() -> ContextBreakdown {
    let claude_home = cc_discord_presence::config::claude_home();
    let sessions = read_sessions();

    // Determine model & context window from active session
    let (model, ctx_window) = sessions
        .first()
        .map(|s| {
            let name = s
                .model_display
                .clone()
                .or(s.model.as_ref().map(|m| cost::model_display_name(m)))
                .unwrap_or_else(|| "Unknown".into());
            let model_id = s.model.clone().unwrap_or_default();
            let is_1m = cost::is_ga_1m_context(&model_id)
                || model_id.contains("[1m]")
                || s.max_turn_api_input > 200_000;
            let window: u64 = if is_1m { 1_000_000 } else { 200_000 };
            (name, window)
        })
        .unwrap_or(("Unknown".into(), 200_000));

    // Use max single-turn input as best estimate for current context window usage
    let latest_input: u64 = sessions
        .iter()
        .map(|s| s.max_turn_api_input)
        .max()
        .unwrap_or(0);

    // Memory files: CLAUDE.md + rules
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
    if rules_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&rules_dir) {
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
    }

    // Project-specific CLAUDE.md from active sessions
    for s in &sessions {
        let project_claude = s.cwd.join("CLAUDE.md");
        if project_claude.exists() {
            let tokens = estimate_tokens_from_file(&project_claude);
            let name = format!("{}/CLAUDE.md", s.project_name);
            if tokens > 0 && !memory_files.iter().any(|f| f.name == name) {
                memory_files.push(ContextFileEntry { name, tokens });
            }
        }
    }

    let memory_total: u64 = memory_files.iter().map(|f| f.tokens).sum();

    // Skills
    let mut skills = Vec::new();
    let skills_dir = claude_home.join("skills");
    if skills_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
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
    }
    let skills_total: u64 = skills.iter().map(|f| f.tokens).sum();

    // MCP tools — read from settings.json
    let mut mcp_tools = Vec::new();
    let settings_file = claude_home.join("settings.json");
    if settings_file.exists() {
        if let Ok(data) = std::fs::read_to_string(&settings_file) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(servers) = json.get("mcpServers").and_then(|v| v.as_object()) {
                    for (name, config) in servers {
                        let config_str = serde_json::to_string(config).unwrap_or_default();
                        let tokens = estimate_tokens(&config_str).max(20);
                        mcp_tools.push(ContextFileEntry {
                            name: name.clone(),
                            tokens,
                        });
                    }
                }
            }
        }
    }
    mcp_tools.sort_by(|a, b| b.tokens.cmp(&a.tokens));
    let mcp_total: u64 = mcp_tools.iter().map(|f| f.tokens).sum();

    // System prompt estimate (~10k base for Claude Code)
    let system_prompt: u64 = 10_000;
    // System tools estimate (~6k for built-in tools)
    let system_tools: u64 = 6_000;

    // Calculate used and free
    let known = system_prompt + system_tools + memory_total + skills_total + mcp_total;
    let used = if latest_input > 0 {
        latest_input
    } else {
        known + 1_000
    };
    let messages = used.saturating_sub(known);
    let autocompact_buffer = (ctx_window as f64 * 0.033) as u64;
    let free_space = ctx_window.saturating_sub(used + autocompact_buffer);

    ContextBreakdown {
        model,
        context_window: ctx_window,
        used_tokens: used,
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
pub fn generate_html_report(days: Option<i64>, project: Option<String>) -> String {
    crate::report::generate_html_report(days, project.as_deref())
}

#[tauri::command]
pub fn generate_markdown_report(days: Option<i64>, project: Option<String>) -> String {
    crate::report::generate_markdown_report(days, project.as_deref())
}

// ── cchubber-style analyzers (Phase 3) ──────────────────────────────────

fn analyzer_sessions(days: Option<i64>) -> Vec<crate::db::HistoricalSession> {
    crate::db::get_session_history(Some(days.unwrap_or(30)), None, Some(5000))
}

fn analyzer_traces(
    sessions: &[crate::db::HistoricalSession],
) -> std::collections::HashMap<String, crate::analyzers::session_trace::SessionTrace> {
    crate::analyzers::session_trace::load_session_traces(sessions)
}

#[tauri::command]
pub fn get_cache_health(days: Option<i64>) -> crate::analyzers::cache_health::CacheHealthReport {
    crate::analyzers::cache_health::analyze(&analyzer_sessions(days))
}

#[tauri::command]
pub fn get_inflection_points(
    days: Option<i64>,
) -> Vec<crate::analyzers::inflection::InflectionPoint> {
    crate::analyzers::inflection::detect(&analyzer_sessions(days))
}

#[tauri::command]
pub fn get_model_routing(days: Option<i64>) -> crate::analyzers::model_routing::ModelRoutingReport {
    crate::analyzers::model_routing::analyze(&analyzer_sessions(days))
}

#[tauri::command]
pub fn get_tool_frequency(
    days: Option<i64>,
) -> crate::analyzers::tool_frequency::ToolFrequencyReport {
    let sessions = analyzer_sessions(days);
    let traces = analyzer_traces(&sessions);
    crate::analyzers::tool_frequency::analyze(&sessions, &traces)
}

#[tauri::command]
pub fn get_prompt_complexity(
    days: Option<i64>,
) -> crate::analyzers::prompt_complexity::PromptComplexityReport {
    let sessions = analyzer_sessions(days);
    let traces = analyzer_traces(&sessions);
    crate::analyzers::prompt_complexity::analyze(&sessions, &traces)
}

#[tauri::command]
pub fn get_session_health(
    days: Option<i64>,
) -> crate::analyzers::session_health::SessionHealthReport {
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
}

#[tauri::command]
pub fn get_recommendations(
    days: Option<i64>,
) -> Vec<crate::analyzers::recommendations::Recommendation> {
    let sessions = analyzer_sessions(days);
    let traces = analyzer_traces(&sessions);
    let cache = crate::analyzers::cache_health::analyze(&sessions);
    let routing = crate::analyzers::model_routing::analyze(&sessions);
    let inflections = crate::analyzers::inflection::detect(&sessions);
    let tool_frequency = crate::analyzers::tool_frequency::analyze(&sessions, &traces);
    let prompt_complexity = crate::analyzers::prompt_complexity::analyze(&sessions, &traces);
    let session_health = crate::analyzers::session_health::analyze(
        &sessions,
        &traces,
        &tool_frequency,
        &prompt_complexity,
    );
    let ctx = crate::analyzers::recommendations::AnalysisContext {
        sessions: &sessions,
        cache: &cache,
        routing: &routing,
        inflections: &inflections,
        tool_frequency: Some(&tool_frequency),
        prompt_complexity: Some(&prompt_complexity),
        session_health: Some(&session_health),
    };
    crate::analyzers::recommendations::generate(&ctx)
}

/// Look up a recommendation by id and return its `fix_prompt` so the frontend
/// can `navigator.clipboard.writeText(...)` it. Returns an empty string if
/// no matching recommendation exists for the current data window.
#[tauri::command]
pub fn copy_fix_prompt(rec_id: String) -> String {
    let sessions = analyzer_sessions(None);
    let traces = analyzer_traces(&sessions);
    let cache = crate::analyzers::cache_health::analyze(&sessions);
    let routing = crate::analyzers::model_routing::analyze(&sessions);
    let inflections = crate::analyzers::inflection::detect(&sessions);
    let tool_frequency = crate::analyzers::tool_frequency::analyze(&sessions, &traces);
    let prompt_complexity = crate::analyzers::prompt_complexity::analyze(&sessions, &traces);
    let session_health = crate::analyzers::session_health::analyze(
        &sessions,
        &traces,
        &tool_frequency,
        &prompt_complexity,
    );
    let ctx = crate::analyzers::recommendations::AnalysisContext {
        sessions: &sessions,
        cache: &cache,
        routing: &routing,
        inflections: &inflections,
        tool_frequency: Some(&tool_frequency),
        prompt_complexity: Some(&prompt_complexity),
        session_health: Some(&session_health),
    };
    crate::analyzers::recommendations::generate(&ctx)
        .into_iter()
        .find(|r| r.id == rec_id)
        .map(|r| r.fix_prompt)
        .unwrap_or_default()
}
