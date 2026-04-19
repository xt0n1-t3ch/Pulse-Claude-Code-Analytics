use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use walkdir::WalkDir;

use crate::config;
use crate::cost;

// ── Public Types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    #[default]
    Idle,
    Thinking,
    ReadingFile,
    EditingFile,
    RunningCommand,
    WaitingInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Low,
    #[default]
    Medium,
    High,
    /// "Extra High" — Opus 4.7+ exclusive (API `effort: xhigh`).
    /// Serialized as `extra_high` for config/JSON; API value is `xhigh`.
    ExtraHigh,
    Max,
}

impl ReasoningEffort {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::ExtraHigh => "Extra High",
            Self::Max => "Max",
        }
    }

    /// Anthropic API value for the `effort` parameter.
    /// Low → "low", Medium → "medium", High → "high", ExtraHigh → "xhigh", Max → "max".
    pub fn api_value(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::ExtraHigh => "xhigh",
            Self::Max => "max",
        }
    }

    /// Parse from API string (accepts "xhigh", "extra_high", "extrahigh" for ExtraHigh).
    pub fn from_api(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" | "med" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" | "x-high" | "extra_high" | "extrahigh" | "extra high" => Some(Self::ExtraHigh),
            "max" | "maximum" => Some(Self::Max),
            _ => None,
        }
    }

    /// True when this effort level enables extended thinking (High, ExtraHigh, or Max).
    pub fn is_high(&self) -> bool {
        matches!(self, Self::High | Self::ExtraHigh | Self::Max)
    }

    /// Short display for tight contexts (Discord state line, badges).
    /// Extra High → "X-High", others use `label()`.
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Med",
            Self::High => "High",
            Self::ExtraHigh => "X-High",
            Self::Max => "Max",
        }
    }
}

impl std::fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Fallback: read default effort from `settings.json`.
/// The primary source is JSONL extraction + thinking-block inference; this is only
/// used as the initial default when a session starts parsing.
pub fn read_claude_effort_level() -> ReasoningEffort {
    let path = crate::config::claude_home().join("settings.json");
    let Ok(data) = std::fs::read_to_string(&path) else {
        return ReasoningEffort::default();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) else {
        return ReasoningEffort::default();
    };
    match json.get("effortLevel").and_then(|v| v.as_str()) {
        Some(s) => ReasoningEffort::from_api(s).unwrap_or_default(),
        None => ReasoningEffort::Medium,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivitySnapshot {
    pub kind: ActivityKind,
    pub target: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub last_active_at: Option<DateTime<Utc>>,
    pub last_effective_signal_at: Option<DateTime<Utc>>,
    pub idle_candidate_at: Option<DateTime<Utc>>,
    pub pending_calls: usize,
}

impl ActivitySnapshot {
    pub fn action_text(&self) -> &'static str {
        match self.kind {
            ActivityKind::Thinking => "Thinking",
            ActivityKind::ReadingFile => "Reading",
            ActivityKind::EditingFile => "Editing",
            ActivityKind::RunningCommand => "Running command",
            ActivityKind::WaitingInput => "Waiting for input",
            ActivityKind::Idle => "Idle",
        }
    }

    pub fn to_text(&self, show_target: bool) -> String {
        if show_target && let Some(ref target) = self.target {
            let trimmed = target.trim();
            if !trimmed.is_empty() {
                let short = shorten_activity_target(&self.kind, trimmed);
                return format!("{} {}", self.action_text(), short);
            }
        }
        self.action_text().to_string()
    }
}

/// Condenses an activity target to its most readable short form for Discord/UI display.
/// Files → filename only. Commands → first token (command name) only.
fn shorten_activity_target(kind: &ActivityKind, target: &str) -> String {
    match kind {
        ActivityKind::ReadingFile | ActivityKind::EditingFile => std::path::Path::new(target)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(target)
            .to_string(),
        ActivityKind::RunningCommand => target
            .split_whitespace()
            .next()
            .unwrap_or(target)
            .to_string(),
        _ => target.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataSource {
    Statusline,
    Jsonl,
}

/// Information about a subagent spawned by a parent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentInfo {
    pub agent_type: String,
    pub model: Option<String>,
    pub model_display: Option<String>,
    pub activity: Option<ActivitySnapshot>,
    pub tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RateLimits {
    pub primary: Option<UsageWindowLimits>,
    pub secondary: Option<UsageWindowLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageWindowLimits {
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub window_minutes: u64,
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSessionSnapshot {
    pub session_id: String,
    pub cwd: PathBuf,
    pub project_name: String,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub model_display: Option<String>,
    pub session_total_tokens: Option<u64>,
    pub last_turn_tokens: Option<u64>,
    pub session_delta_tokens: Option<u64>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    /// Maximum per-turn total API input (input + cache_creation + cache_read) across the session.
    /// Only a single turn exceeding 200K reliably indicates 1M extended context usage.
    pub max_turn_api_input: u64,
    pub reasoning_effort: ReasoningEffort,
    /// True when the effort was read from an explicit JSONL system-reminder
    /// injection (reasoning effort level: X / <reasoning_effort>NN</…>).
    /// False when we only have the settings.json default — in that case the
    /// effort might not match what the user has selected in Claude Desktop's
    /// composer (which lives in Electron memory only).
    #[serde(default)]
    pub reasoning_effort_explicit: bool,
    pub has_thinking_blocks: bool,
    pub total_cost: f64,
    pub total_api_duration_ms: u64,
    pub limits: RateLimits,
    pub activity: Option<ActivitySnapshot>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_token_event_at: Option<DateTime<Utc>>,
    pub last_activity: SystemTime,
    pub source: DataSource,
    pub source_file: PathBuf,
    /// Subagents spawned by this session (populated after collection).
    #[serde(default)]
    pub subagents: Vec<SubagentInfo>,
    /// True if this session IS a subagent (not a parent session).
    #[serde(default)]
    pub is_subagent: bool,
    /// Parent session ID for subagent sessions.
    #[serde(default)]
    pub parent_session_id: Option<String>,
}

impl ClaudeSessionSnapshot {
    /// True when extended thinking is active with High reasoning effort (ULTRATHINK mode).
    pub fn is_ultrathinking(&self) -> bool {
        self.has_thinking_blocks && matches!(self.reasoning_effort, ReasoningEffort::Max)
    }
}

// ── Git Branch Cache ──────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct GitBranchCache {
    ttl: Duration,
    entries: HashMap<PathBuf, CachedBranch>,
}

#[derive(Debug, Clone)]
struct CachedBranch {
    value: Option<String>,
    expires_at: Instant,
}

impl GitBranchCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: HashMap::new(),
        }
    }

    pub fn get(&mut self, project_path: &Path) -> Option<String> {
        if project_path.as_os_str().is_empty() || !project_path.exists() {
            return None;
        }

        let key = project_path.to_path_buf();
        if let Some(cached) = self.entries.get(&key)
            && Instant::now() < cached.expires_at
        {
            return cached.value.clone();
        }

        let value = fetch_git_branch(project_path);
        self.entries.insert(
            key,
            CachedBranch {
                value: value.clone(),
                expires_at: Instant::now() + self.ttl,
            },
        );
        value
    }
}

fn fetch_git_branch(project_path: &Path) -> Option<String> {
    let output = crate::util::silent_command("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(project_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        // Detached HEAD — try short SHA
        let sha_output = crate::util::silent_command("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(project_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;

        if sha_output.status.success() {
            let sha = String::from_utf8_lossy(&sha_output.stdout)
                .trim()
                .to_string();
            if !sha.is_empty() {
                return Some(sha);
            }
        }
        return None;
    }

    Some(branch)
}

// ── Session Parse Cache ───────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct SessionParseCache {
    entries: HashMap<PathBuf, CachedSessionEntry>,
}

#[derive(Debug)]
struct CachedSessionEntry {
    cursor: u64,
    file_len: u64,
    modified: SystemTime,
    accumulator: SessionAccumulator,
    snapshot: Option<ClaudeSessionSnapshot>,
}

impl CachedSessionEntry {
    fn new(modified: SystemTime, default_effort: ReasoningEffort) -> Self {
        Self {
            cursor: 0,
            file_len: 0,
            modified,
            accumulator: SessionAccumulator::with_default_effort(default_effort),
            snapshot: None,
        }
    }

    fn reset(&mut self, modified: SystemTime, default_effort: ReasoningEffort) {
        self.cursor = 0;
        self.file_len = 0;
        self.modified = modified;
        self.accumulator = SessionAccumulator::with_default_effort(default_effort);
        self.snapshot = None;
    }
}

// ── Session Accumulator ───────────────────────────────────────────────────

#[derive(Debug, Default)]
struct SessionAccumulator {
    #[allow(dead_code)]
    session_id: Option<String>,
    cwd: Option<PathBuf>,
    started_at: Option<DateTime<Utc>>,
    model: Option<String>,
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cache_creation_tokens: u64,
    total_cache_read_tokens: u64,
    /// Maximum per-turn API input (input + cache_creation + cache_read) seen so far.
    max_turn_api_input: u64,
    total_cost: f64,
    session_total_tokens: Option<u64>,
    previous_session_total_tokens: Option<u64>,
    last_turn_tokens: Option<u64>,
    limits: RateLimits,
    last_token_event_at: Option<DateTime<Utc>>,
    activity_tracker: ActivityTracker,
    reasoning_effort: ReasoningEffort,
    reasoning_effort_explicitly_set: bool,
    has_thinking_blocks: bool,
    /// Most recent full file path from Read/Write/Edit tool calls.
    /// Used to determine active workspace in multi-workspace VS Code setups.
    last_file_target: Option<PathBuf>,
}

impl SessionAccumulator {
    fn with_default_effort(effort: ReasoningEffort) -> Self {
        Self {
            reasoning_effort: effort,
            ..Default::default()
        }
    }
}

// ── Activity Tracker ──────────────────────────────────────────────────────

const IDLE_DEBOUNCE_SECS: i64 = 45;

#[derive(Debug, Default)]
struct ActivityTracker {
    snapshot: Option<ActivitySnapshot>,
    pending_calls: HashMap<String, PendingActivity>,
    last_event_at: Option<DateTime<Utc>>,
    last_effective_signal_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct PendingActivity {
    #[allow(dead_code)]
    kind: ActivityKind,
    #[allow(dead_code)]
    target: Option<String>,
}

impl ActivityTracker {
    fn observe_timestamp(&mut self, observed_at: Option<DateTime<Utc>>) {
        if let Some(ts) = observed_at {
            self.last_event_at = max_datetime(self.last_event_at, Some(ts));
        }
    }

    fn observe_effective_signal(&mut self, observed_at: Option<DateTime<Utc>>) {
        self.observe_timestamp(observed_at);
        self.last_effective_signal_at = max_datetime(self.last_effective_signal_at, observed_at);
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.last_effective_signal_at =
                max_datetime(snapshot.last_effective_signal_at, observed_at);
        }
    }

    fn mark_activity(
        &mut self,
        kind: ActivityKind,
        target: Option<String>,
        observed_at: Option<DateTime<Utc>>,
    ) {
        self.observe_effective_signal(observed_at);
        let previous_active = self.snapshot.as_ref().and_then(|item| item.last_active_at);
        let last_active_at = max_datetime(previous_active, observed_at);
        let idle_candidate_at = if self.pending_calls.is_empty()
            && !matches!(kind, ActivityKind::Idle | ActivityKind::WaitingInput)
        {
            last_active_at
        } else {
            None
        };

        self.snapshot = Some(ActivitySnapshot {
            kind,
            target,
            observed_at,
            last_active_at,
            last_effective_signal_at: self.last_effective_signal_at,
            idle_candidate_at,
            pending_calls: self.pending_calls.len(),
        });
    }

    fn note_commentary(&mut self, observed_at: Option<DateTime<Utc>>) {
        self.observe_effective_signal(observed_at);
        let should_promote = self.snapshot.as_ref().is_none_or(|snapshot| {
            matches!(
                snapshot.kind,
                ActivityKind::Idle | ActivityKind::WaitingInput
            )
        });
        if should_promote {
            self.mark_activity(ActivityKind::Thinking, None, observed_at);
        }
    }

    fn register_call(
        &mut self,
        call_id: &str,
        kind: ActivityKind,
        target: Option<String>,
        observed_at: Option<DateTime<Utc>>,
    ) {
        self.pending_calls.insert(
            call_id.to_string(),
            PendingActivity {
                kind: kind.clone(),
                target: target.clone(),
            },
        );
        self.mark_activity(kind, target, observed_at);
    }

    fn resolve_call(&mut self, call_id: &str, observed_at: Option<DateTime<Utc>>) {
        self.pending_calls.remove(call_id);
        self.observe_effective_signal(observed_at);

        if self.pending_calls.is_empty() {
            if let Some(snapshot) = self.snapshot.as_mut() {
                snapshot.pending_calls = 0;
                snapshot.idle_candidate_at = observed_at;
            }
        } else if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.pending_calls = self.pending_calls.len();
        }
    }

    fn apply_idle_debounce(&mut self, now: DateTime<Utc>) {
        let Some(ref snapshot) = self.snapshot else {
            return;
        };
        if matches!(
            snapshot.kind,
            ActivityKind::Idle | ActivityKind::WaitingInput
        ) {
            return;
        }
        if !self.pending_calls.is_empty() {
            return;
        }
        let Some(idle_candidate) = snapshot.idle_candidate_at else {
            return;
        };
        let elapsed = now.signed_duration_since(idle_candidate).num_seconds();
        if elapsed >= IDLE_DEBOUNCE_SECS {
            self.mark_activity(ActivityKind::Idle, None, Some(now));
        }
    }

    fn finalize(&self) -> Option<ActivitySnapshot> {
        self.snapshot.clone()
    }
}

fn max_datetime(a: Option<DateTime<Utc>>, b: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    match (a, b) {
        (Some(va), Some(vb)) => Some(va.max(vb)),
        (Some(va), None) => Some(va),
        (None, Some(vb)) => Some(vb),
        (None, None) => None,
    }
}

// ── Statusline Data Reader ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct StatusLineData {
    session_id: String,
    cwd: String,
    model: StatusLineModel,
    workspace: StatusLineWorkspace,
    cost: StatusLineCost,
    context_window: StatusLineContextWindow,
}

#[derive(Debug, Deserialize)]
struct StatusLineModel {
    id: String,
    display_name: String,
}

#[derive(Debug, Deserialize)]
struct StatusLineWorkspace {
    #[serde(default)]
    #[allow(dead_code)]
    current_dir: String,
    #[serde(default)]
    project_dir: String,
}

#[derive(Debug, Deserialize)]
struct StatusLineCost {
    total_cost_usd: f64,
    #[serde(default)]
    total_duration_ms: i64,
    #[serde(default)]
    total_api_duration_ms: i64,
}

#[derive(Debug, Deserialize)]
struct StatusLineContextWindow {
    #[serde(default)]
    total_input_tokens: u64,
    #[serde(default)]
    total_output_tokens: u64,
}

pub fn read_statusline_data(git_cache: &mut GitBranchCache) -> Option<ClaudeSessionSnapshot> {
    let data_path = config::statusline_data_path();

    let file_info = fs::metadata(&data_path).ok()?;
    let modified = file_info.modified().ok()?;

    // If file hasn't been modified in 60 seconds, session is likely closed
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    if age > Duration::from_secs(60) {
        return None;
    }

    let data = fs::read_to_string(&data_path).ok()?;
    let status: StatusLineData = serde_json::from_str(&data).ok()?;

    // Validate session_id
    if status.session_id.is_empty()
        || status.session_id.len() < 10
        || !status.session_id.contains('-')
    {
        return None;
    }

    let project_path = if !status.workspace.project_dir.is_empty() {
        status.workspace.project_dir.clone()
    } else if !status.cwd.is_empty() {
        status.cwd.clone()
    } else {
        return None;
    };

    let sanitized_model_id = non_synthetic_model(&status.model.id);
    let sanitized_display_name = non_synthetic_display_name(&status.model.display_name);

    // Validate model
    if sanitized_model_id.is_none() && sanitized_display_name.is_none() {
        return None;
    }
    if status.model.display_name == "Test" {
        return None;
    }

    let project_name = derive_project_name(Path::new(&project_path));

    let git_branch = git_cache.get(Path::new(&project_path));
    let total_tokens =
        status.context_window.total_input_tokens + status.context_window.total_output_tokens;

    // Calculate session start from duration
    let started_at = if status.cost.total_duration_ms > 0 {
        Some(Utc::now() - chrono::Duration::milliseconds(status.cost.total_duration_ms))
    } else {
        None
    };

    Some(ClaudeSessionSnapshot {
        session_id: status.session_id,
        cwd: PathBuf::from(&project_path),
        project_name,
        git_branch,
        model: sanitized_model_id.clone(),
        model_display: sanitized_display_name.or_else(|| {
            sanitized_model_id
                .as_ref()
                .map(|model_id| cost::model_display_name(model_id).to_string())
        }),
        session_total_tokens: Some(total_tokens),
        last_turn_tokens: None,
        session_delta_tokens: None,
        input_tokens: status.context_window.total_input_tokens,
        output_tokens: status.context_window.total_output_tokens,
        cache_creation_tokens: 0, // statusline doesn't expose cache breakdown
        cache_read_tokens: 0,
        max_turn_api_input: 0, // statusline doesn't expose per-turn data
        reasoning_effort: read_claude_effort_level(),
        reasoning_effort_explicit: false,
        has_thinking_blocks: false,
        total_cost: status.cost.total_cost_usd,
        total_api_duration_ms: status.cost.total_api_duration_ms.max(0) as u64,
        limits: RateLimits::default(),
        activity: None,
        started_at,
        last_token_event_at: None,
        last_activity: modified,
        source: DataSource::Statusline,
        source_file: data_path,
        subagents: Vec::new(),
        is_subagent: false,
        parent_session_id: None,
    })
}

/// Merge a statusline-sourced session into JSONL-parsed sessions.
/// Statusline wins for cost/model/totals; JSONL wins for granular token data + activity.
pub fn merge_statusline_into_sessions(
    sessions: &mut Vec<ClaudeSessionSnapshot>,
    statusline: ClaudeSessionSnapshot,
) {
    let existing_idx = sessions
        .iter()
        .position(|s| s.session_id == statusline.session_id);
    if let Some(idx) = existing_idx {
        let jsonl = &sessions[idx];
        let merged_model = statusline.model.clone().or_else(|| jsonl.model.clone());
        let merged_model_display = statusline
            .model_display
            .clone()
            .or_else(|| jsonl.model_display.clone());
        let merged = ClaudeSessionSnapshot {
            last_turn_tokens: statusline.last_turn_tokens.or(jsonl.last_turn_tokens),
            session_delta_tokens: statusline
                .session_delta_tokens
                .or(jsonl.session_delta_tokens),
            max_turn_api_input: jsonl.max_turn_api_input.max(statusline.max_turn_api_input),
            activity: statusline.activity.clone().or(jsonl.activity.clone()),
            last_token_event_at: statusline.last_token_event_at.or(jsonl.last_token_event_at),
            model: merged_model,
            model_display: merged_model_display,
            // JSONL wins — statusline doesn't expose these fields
            reasoning_effort: jsonl.reasoning_effort,
            reasoning_effort_explicit: jsonl.reasoning_effort_explicit,
            has_thinking_blocks: jsonl.has_thinking_blocks,
            ..statusline
        };
        sessions[idx] = merged;
    } else {
        sessions.insert(0, statusline);
    }
}

// ── JSONL Session Parsing ─────────────────────────────────────────────────

/// Claude Code JSONL message format
#[derive(Debug, Deserialize)]
struct JsonlMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    message: Option<JsonlMessageContent>,
    #[serde(default)]
    is_api_error_message: bool,
}

#[derive(Debug, Deserialize)]
struct JsonlMessageContent {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<JsonlUsage>,
    #[serde(default)]
    content: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonlUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

pub fn collect_active_sessions(
    git_cache: &mut GitBranchCache,
    parse_cache: &mut SessionParseCache,
    stale_threshold: Duration,
    active_sticky_window: Duration,
) -> Result<Vec<ClaudeSessionSnapshot>> {
    let projects_roots = config::projects_paths();
    let ide_workspaces = config::read_ide_workspace_folders();
    collect_active_sessions_multi(
        &projects_roots,
        stale_threshold,
        active_sticky_window,
        git_cache,
        parse_cache,
        &ide_workspaces,
    )
}

pub fn collect_active_sessions_multi(
    projects_roots: &[PathBuf],
    stale_threshold: Duration,
    active_sticky_window: Duration,
    git_cache: &mut GitBranchCache,
    parse_cache: &mut SessionParseCache,
    ide_workspaces: &[PathBuf],
) -> Result<Vec<ClaudeSessionSnapshot>> {
    let now = SystemTime::now();
    let stale_cutoff = now
        .checked_sub(stale_threshold)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let sticky_cutoff = now
        .checked_sub(active_sticky_window)
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let default_effort = read_claude_effort_level();
    let mut sessions = Vec::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();

    for projects_root in projects_roots {
        if !projects_root.exists() {
            continue;
        }

        for entry in WalkDir::new(projects_root)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            let path = entry.path();
            if !entry.file_type().is_file() {
                continue;
            }
            // Detect subagent files but don't skip them — we parse and merge
            let is_subagent_file = path
                .components()
                .any(|component| component.as_os_str().eq_ignore_ascii_case("subagents"));
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            seen_paths.insert(path.to_path_buf());

            let metadata = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            let modified = match metadata.modified() {
                Ok(m) => m,
                Err(_) => continue,
            };

            // Extract encoded directory name for session ID
            let encoded_dir = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Some(mut snapshot) = parse_session_file_cached(
                path,
                &metadata,
                modified,
                &encoded_dir,
                git_cache,
                parse_cache,
                default_effort,
                ide_workspaces,
            )? {
                // Mark subagent sessions with parent info
                if is_subagent_file {
                    snapshot.is_subagent = true;
                    // Parent session ID = directory name above "subagents/"
                    // Path: .../projects/{slug}/{session-uuid}/subagents/agent-{id}.jsonl
                    snapshot.parent_session_id = path
                        .parent() // subagents/
                        .and_then(|p| p.parent()) // {session-uuid}/
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .filter(|id| is_valid_session_id(id));
                    // Read agent type from .meta.json sibling
                    let meta_path = path.with_extension("meta.json");
                    if let Ok(meta_data) = fs::read_to_string(&meta_path)
                        && let Ok(meta_json) = serde_json::from_str::<serde_json::Value>(&meta_data)
                        && let Some(agent_type) =
                            meta_json.get("agentType").and_then(|v| v.as_str())
                    {
                        // Store agent type in a temporary field via project_name
                        // We'll use it during merge; project_name will be overridden by parent
                        snapshot.project_name = format!("subagent:{}", agent_type);
                    }
                }

                let recency = session_recency(&snapshot, modified);
                snapshot.last_activity = recency;
                if should_include_session(&snapshot, recency, stale_cutoff, sticky_cutoff) {
                    sessions.push(snapshot);
                }
            }
        }
    }

    parse_cache
        .entries
        .retain(|path, _| seen_paths.contains(path));
    sessions = dedupe_sessions_by_id(sessions);

    // Post-process: merge subagent data into parent sessions
    merge_subagents_into_parents(&mut sessions);

    sessions.sort_by_key(|session| Reverse(session_rank_key(session)));
    Ok(sessions)
}

#[allow(clippy::too_many_arguments)]
fn parse_session_file_cached(
    path: &Path,
    metadata: &std::fs::Metadata,
    modified: SystemTime,
    encoded_dir: &str,
    git_cache: &mut GitBranchCache,
    parse_cache: &mut SessionParseCache,
    default_effort: ReasoningEffort,
    ide_workspaces: &[PathBuf],
) -> Result<Option<ClaudeSessionSnapshot>> {
    let file_len = metadata.len();
    let path_buf = path.to_path_buf();

    let entry = parse_cache
        .entries
        .entry(path_buf.clone())
        .or_insert_with(|| CachedSessionEntry::new(modified, default_effort));

    // Reset if file was truncated or modified time changed significantly
    if file_len < entry.file_len || modified != entry.modified {
        entry.reset(modified, default_effort);
    }

    // No new data
    if file_len == entry.cursor && entry.snapshot.is_some() {
        return Ok(entry.snapshot.clone());
    }

    // Read new lines from cursor
    let mut file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;

    if entry.cursor > 0 {
        file.seek(SeekFrom::Start(entry.cursor)).with_context(|| {
            format!(
                "failed to seek to cursor {} in {}",
                entry.cursor,
                path.display()
            )
        })?;
    }

    let reader = BufReader::new(&file);
    let mut new_cursor = entry.cursor;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        new_cursor += line.len() as u64 + 1; // +1 for newline

        if line.trim().is_empty() {
            continue;
        }

        let msg: JsonlMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(_) => continue,
        };

        process_jsonl_message(&mut entry.accumulator, &msg);
    }

    entry.cursor = new_cursor;
    entry.file_len = file_len;

    // Build snapshot from accumulator — extract values before mutable borrow
    let model_id = match entry.accumulator.model.as_deref() {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            entry.snapshot = None;
            return Ok(None);
        }
    };

    let cwd = entry
        .accumulator
        .cwd
        .clone()
        .unwrap_or_else(|| decode_project_path(encoded_dir));

    // Smart project name: if cwd is a drive root (multi-workspace VS Code),
    // use IDE workspace folders + recent file activity for accurate detection.
    let effective_project_root = if config::is_drive_root(&cwd) {
        entry
            .accumulator
            .last_file_target
            .as_deref()
            .and_then(|target| config::find_best_workspace(target, ide_workspaces))
    } else {
        None
    };
    let project_name = derive_project_name(effective_project_root.as_deref().unwrap_or(&cwd));

    let git_branch_path = effective_project_root.as_deref().unwrap_or(&cwd);
    let git_branch = git_cache.get(git_branch_path);
    let total_tokens = entry.accumulator.total_input_tokens + entry.accumulator.total_output_tokens;

    let session_delta = entry.accumulator.session_total_tokens.and_then(|current| {
        entry
            .accumulator
            .previous_session_total_tokens
            .map(|prev| current.saturating_sub(prev))
    });

    let session_id = entry
        .accumulator
        .session_id
        .clone()
        .filter(|session_id| is_valid_session_id(session_id))
        .unwrap_or_else(|| {
            if encoded_dir.is_empty() {
                cwd.to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "_")
            } else {
                format!("encoded_{encoded_dir}")
            }
        });

    let last_turn_tokens = entry.accumulator.last_turn_tokens;
    let input_tokens = entry.accumulator.total_input_tokens;
    let output_tokens = entry.accumulator.total_output_tokens;
    let cache_creation_tokens = entry.accumulator.total_cache_creation_tokens;
    let cache_read_tokens = entry.accumulator.total_cache_read_tokens;
    let max_turn_api_input = entry.accumulator.max_turn_api_input;
    let total_cost = entry.accumulator.total_cost;
    let limits = entry.accumulator.limits.clone();
    let started_at = entry.accumulator.started_at;
    let last_token_event_at = entry.accumulator.last_token_event_at;

    // Apply idle debounce (mutable borrow)
    entry
        .accumulator
        .activity_tracker
        .apply_idle_debounce(Utc::now());

    let snapshot = ClaudeSessionSnapshot {
        session_id,
        cwd: cwd.clone(),
        project_name,
        git_branch,
        model: Some(model_id.clone()),
        model_display: Some(cost::model_display_name(&model_id).to_string()),
        session_total_tokens: Some(total_tokens),
        last_turn_tokens,
        session_delta_tokens: session_delta,
        input_tokens,
        output_tokens,
        cache_creation_tokens,
        cache_read_tokens,
        max_turn_api_input,
        reasoning_effort: infer_effort(&entry.accumulator),
        reasoning_effort_explicit: entry.accumulator.reasoning_effort_explicitly_set,
        has_thinking_blocks: entry.accumulator.has_thinking_blocks,
        total_cost,
        total_api_duration_ms: 0,
        limits,
        activity: entry.accumulator.activity_tracker.finalize(),
        started_at,
        last_token_event_at,
        last_activity: modified,
        source: DataSource::Jsonl,
        source_file: path_buf,
        subagents: Vec::new(),
        is_subagent: false,
        parent_session_id: None,
    };

    entry.snapshot = Some(snapshot.clone());
    Ok(Some(snapshot))
}

fn process_jsonl_message(acc: &mut SessionAccumulator, msg: &JsonlMessage) {
    // Extract timestamp
    let observed_at = msg
        .timestamp
        .as_deref()
        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
        .map(|dt| dt.with_timezone(&Utc));

    // Track first timestamp as session start
    if acc.started_at.is_none() {
        acc.started_at = observed_at;
    }

    // Extract cwd from any message
    if acc.cwd.is_none()
        && let Some(ref cwd) = msg.cwd
        && !cwd.is_empty()
    {
        acc.cwd = Some(PathBuf::from(cwd));
    }
    if acc.session_id.is_none()
        && let Some(session_id) = msg.session_id.as_deref()
        && is_valid_session_id(session_id)
    {
        acc.session_id = Some(session_id.to_string());
    }

    let msg_type = msg.msg_type.as_deref().unwrap_or("");

    // Extract message content for processing
    let Some(ref message) = msg.message else {
        acc.activity_tracker.observe_timestamp(observed_at);
        return;
    };

    match msg_type {
        "assistant" => {
            // Extract model
            if let Some(ref model) = message.model
                && !model.is_empty()
                && !msg.is_api_error_message
                && !is_synthetic_model_id(model)
            {
                acc.model = Some(model.clone());
            }

            // Extract token usage (including prompt cache tokens)
            if let Some(ref usage) = message.usage {
                let input = usage.input_tokens;
                let output = usage.output_tokens;
                let cache_creation = usage.cache_creation_input_tokens;
                let cache_read = usage.cache_read_input_tokens;

                // All input-side tokens (non-cached + cache write + cache read)
                let all_input = input + cache_creation + cache_read;

                // Track the largest single-turn API input for 1M context detection
                acc.max_turn_api_input = acc.max_turn_api_input.max(all_input);

                acc.total_input_tokens += all_input;
                acc.total_output_tokens += output;
                acc.total_cache_creation_tokens += cache_creation;
                acc.total_cache_read_tokens += cache_read;

                // Calculate cost with cache-aware pricing
                let model_id = acc.model.as_deref().unwrap_or("claude-sonnet-4-20250514");
                acc.total_cost += cost::calculate_cost_with_context(
                    model_id,
                    input,
                    output,
                    cache_creation,
                    cache_read,
                );

                // Track last turn tokens (all tokens for this message)
                acc.last_turn_tokens = Some(all_input + output);
                acc.last_token_event_at = observed_at;

                // Track session total for delta calculation
                acc.previous_session_total_tokens = acc.session_total_tokens;
                acc.session_total_tokens = Some(acc.total_input_tokens + acc.total_output_tokens);
            }

            // Scan message.content[] for tool_use entries (activity tracking).
            // JSONL wraps tool calls inside message.content as an array of objects,
            // each with a "type" field. The top-level "type" stays "assistant".
            if let Some(ref content_val) = message.content {
                process_content_for_activity(acc, content_val, observed_at);
            } else {
                acc.activity_tracker.note_commentary(observed_at);
            }
        }
        "user" => {
            // New turn — reset per-turn state so values only reflect the current turn
            acc.has_thinking_blocks = false;
            // reasoning_effort is sticky: once set by a system-reminder, it persists
            // across turns until explicitly changed by another system-reminder injection.
            extract_reasoning_effort(acc, message);
            // User messages may contain tool_result entries inside message.content[]
            if let Some(ref content_val) = message.content {
                process_content_for_activity(acc, content_val, observed_at);
            } else {
                acc.activity_tracker.observe_timestamp(observed_at);
            }
        }
        _ => {
            acc.activity_tracker.observe_timestamp(observed_at);
        }
    }
}

/// Scan a `message.content` value (array or single object) for tool_use / tool_result entries.
fn process_content_for_activity(
    acc: &mut SessionAccumulator,
    content_val: &Value,
    observed_at: Option<DateTime<Utc>>,
) {
    let items: Vec<&Value> = if let Some(arr) = content_val.as_array() {
        arr.iter().collect()
    } else if content_val.is_object() {
        vec![content_val]
    } else {
        acc.activity_tracker.note_commentary(observed_at);
        return;
    };

    let mut found_activity = false;
    for item in items {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match item_type {
            "tool_use" => {
                let (kind, target) = classify_tool_call(item);
                // Track full file path for workspace detection
                if matches!(kind, ActivityKind::ReadingFile | ActivityKind::EditingFile)
                    && let Some(full_path) = extract_full_file_path(item.get("input"))
                {
                    acc.last_file_target = Some(PathBuf::from(full_path));
                }
                let call_id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                acc.activity_tracker
                    .register_call(&call_id, kind, target, observed_at);
                found_activity = true;
            }
            "thinking" => {
                acc.has_thinking_blocks = true;
            }
            "tool_result" => {
                let call_id = item
                    .get("tool_use_id")
                    .or_else(|| item.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                acc.activity_tracker.resolve_call(&call_id, observed_at);
                found_activity = true;
            }
            _ => {}
        }
    }

    if !found_activity {
        acc.activity_tracker.note_commentary(observed_at);
    }
}

/// Return the effort we are actually confident about.
///
/// Claude Desktop's in-composer effort selector (Low/Medium/High/Extra High/Max)
/// is kept in the Electron app's memory. It is NOT written to the JSONL
/// transcript, to `~/.claude/settings.json`, nor to the Local Storage LevelDB —
/// so if the user picks "Extra High" in the composer we have no filesystem
/// signal to read it from.
///
/// Previously this function silently upgraded `Medium → High` whenever thinking
/// blocks appeared, which meant Pulse would display "High" seconds into a
/// session even if the user had actually chosen "Extra High". Confidently
/// wrong is worse than honest uncertainty, so we now return the accumulator's
/// explicit value (from a `<reasoning_effort>` tag in the JSONL) or the
/// `effortLevel` default loaded from `settings.json`. No more inference.
fn infer_effort(acc: &SessionAccumulator) -> ReasoningEffort {
    acc.reasoning_effort
}

/// Extract reasoning effort level from system-reminder text injected into user messages.
/// Searches for both `"reasoning effort level: X"` and `"antml:reasoning_effort"` XML tags.
fn extract_reasoning_effort(acc: &mut SessionAccumulator, message: &JsonlMessageContent) {
    let Some(ref content_val) = message.content else {
        return;
    };

    let mut set_effort = |text: &str| {
        // Pattern 1: plain text "reasoning effort level: high"
        if let Some(pos) = text.find("reasoning effort level: ") {
            let after = &text[pos + 24..];
            let level = after
                .split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap_or("");
            if let Some(effort) = parse_effort_level(level) {
                acc.reasoning_effort = effort;
                acc.reasoning_effort_explicitly_set = true;
            }
        }
        // Pattern 2: XML tag <reasoning_effort>99</reasoning_effort>
        if let Some(pos) = text.find("antml:reasoning_effort>") {
            let after = &text[pos + 23..];
            let value = after.split('<').next().unwrap_or("").trim();
            if let Ok(n) = value.parse::<u32>() {
                // 5-tier numeric mapping aligned with API effort levels.
                let effort = match n {
                    0..=25 => ReasoningEffort::Low,
                    26..=55 => ReasoningEffort::Medium,
                    56..=80 => ReasoningEffort::High,
                    81..=99 => ReasoningEffort::ExtraHigh,
                    _ => ReasoningEffort::Max,
                };
                acc.reasoning_effort = effort;
                acc.reasoning_effort_explicitly_set = true;
            } else if let Some(effort) = parse_effort_level(value) {
                acc.reasoning_effort = effort;
                acc.reasoning_effort_explicitly_set = true;
            }
        }
    };

    if let Some(s) = content_val.as_str() {
        set_effort(s);
    } else if let Some(arr) = content_val.as_array() {
        for item in arr {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                set_effort(text);
            }
            if let Some(content) = item.get("content").and_then(|v| v.as_str()) {
                set_effort(content);
            }
        }
    }
}

fn parse_effort_level(s: &str) -> Option<ReasoningEffort> {
    ReasoningEffort::from_api(s)
}

fn classify_tool_call(content: &Value) -> (ActivityKind, Option<String>) {
    let name = content.get("name").and_then(|v| v.as_str()).unwrap_or("");

    let input = content.get("input");

    match name {
        "Read" | "read" | "file_read" | "NotebookRead" => {
            let target = extract_filename(input);
            (ActivityKind::ReadingFile, target)
        }
        "Write" | "write" | "Edit" | "edit" | "file_write" | "NotebookEdit" => {
            let target = extract_filename(input);
            (ActivityKind::EditingFile, target)
        }
        "Glob" | "glob" | "Grep" | "grep" | "Search" | "search" => {
            let target = input
                .and_then(|v| v.get("pattern"))
                .and_then(|v| v.as_str())
                .map(|s| truncate_target(s, 40));
            (ActivityKind::ReadingFile, target)
        }
        "Bash" | "bash" | "command_execute" | "terminal" => {
            let target = input
                .and_then(|v| v.get("command"))
                .and_then(|v| v.as_str())
                .map(|s| truncate_target(s, 50));
            (ActivityKind::RunningCommand, target)
        }
        "AskUserQuestion" | "ask_user" => (ActivityKind::WaitingInput, None),
        "WebSearch" | "WebFetch" | "web_search" | "web_fetch" => (ActivityKind::Thinking, None),
        _ => (ActivityKind::Thinking, None),
    }
}

/// Extract the full file path from a tool_use input field (for workspace detection).
fn extract_full_file_path(input: Option<&Value>) -> Option<String> {
    input
        .and_then(|v| {
            v.get("file_path")
                .or(v.get("path"))
                .or(v.get("notebook_path"))
        })
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Extract just the filename from a file_path/path input field.
fn extract_filename(input: Option<&Value>) -> Option<String> {
    input
        .and_then(|v| {
            v.get("file_path")
                .or(v.get("path"))
                .or(v.get("notebook_path"))
        })
        .and_then(|v| v.as_str())
        .map(|s| {
            Path::new(s)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| truncate_target(s, 40))
        })
}

/// Truncate a target string, respecting UTF-8 char boundaries.
fn truncate_target(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let suffix_len = 3; // "..."
        let mut end = max_len.saturating_sub(suffix_len);
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

fn is_valid_session_id(session_id: &str) -> bool {
    let trimmed = session_id.trim();
    if trimmed.len() != 36 {
        return false;
    }
    let bytes = trimmed.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        let is_hyphen_slot = matches!(index, 8 | 13 | 18 | 23);
        if is_hyphen_slot {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn is_synthetic_model_id(model_id: &str) -> bool {
    let trimmed = model_id.trim();
    trimmed.is_empty() || trimmed.eq_ignore_ascii_case("<synthetic>")
}

fn non_synthetic_model(model_id: &str) -> Option<String> {
    (!is_synthetic_model_id(model_id)).then(|| model_id.trim().to_string())
}

fn non_synthetic_display_name(display_name: &str) -> Option<String> {
    let trimmed = display_name.trim();
    (!trimmed.is_empty() && !trimmed.starts_with('<') && trimmed != "Test")
        .then(|| trimmed.to_string())
}

/// Smart project name derivation with fallback chain:
/// 1. Path::file_name() — works for normal paths
/// 2. Nearest project metadata (package.json, Cargo.toml, pyproject.toml, go.mod)
/// 3. Git repo name — parent folder of nearest .git
/// 4. Drive label — "C: Drive" for drive roots
/// 5. "Unknown Project" fallback
fn derive_project_name(cwd: &Path) -> String {
    // Strategy 1: simple file_name (works for most paths)
    if let Some(name) = cwd.file_name() {
        let s = name.to_string_lossy();
        if !s.is_empty() {
            return s.to_string();
        }
    }

    // Strategy 2: project metadata (walk up max 3 levels)
    if let Some(name) = read_project_name_from_ancestors(cwd, 3) {
        return name;
    }

    // Strategy 3: git repo name — folder containing .git
    if let Some(name) = find_git_repo_name(cwd, 3) {
        return name;
    }

    // Strategy 4: drive label for root paths like C:\
    let path_str = cwd.to_string_lossy();
    if path_str.len() <= 3 {
        // Looks like a drive root: "C:\", "D:\", "C:/", etc.
        let drive = path_str
            .chars()
            .next()
            .filter(|c| c.is_ascii_alphabetic())
            .map(|c| c.to_ascii_uppercase());
        if let Some(letter) = drive {
            return format!("{letter}: Drive");
        }
    }

    "Unknown Project".to_string()
}

/// Walk up from `dir` (inclusive) looking for project metadata files.
/// Returns the `name` field from the first match found.
fn read_project_name_from_ancestors(dir: &Path, max_levels: usize) -> Option<String> {
    let mut current = Some(dir);
    for _ in 0..=max_levels {
        let d = current?;
        if let Some(name) = read_project_name_from_metadata(d) {
            return Some(name);
        }
        current = d.parent();
    }
    None
}

/// Try to extract a project name from metadata files in a single directory.
fn read_project_name_from_metadata(dir: &Path) -> Option<String> {
    // package.json — {"name": "..."}
    let pkg = dir.join("package.json");
    if let Some(name) = read_json_name_field(&pkg) {
        return Some(name);
    }

    // Cargo.toml — name = "..."
    if let Some(name) = read_toml_name_field(&dir.join("Cargo.toml")) {
        return Some(name);
    }

    // pyproject.toml — name = "..."
    if let Some(name) = read_toml_name_field(&dir.join("pyproject.toml")) {
        return Some(name);
    }

    // go.mod — module xxx
    let gomod = dir.join("go.mod");
    if gomod.is_file()
        && let Ok(data) = fs::read_to_string(&gomod)
        && let Some(first_line) = data.lines().next()
        && let Some(module) = first_line.strip_prefix("module ")
    {
        let name = module.trim();
        let short = name.rsplit('/').next().unwrap_or(name);
        if !short.is_empty() {
            return Some(short.to_string());
        }
    }

    None
}

fn read_json_name_field(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let data = fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&data).ok()?;
    let name = json.get("name")?.as_str()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn read_toml_name_field(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let data = fs::read_to_string(path).ok()?;
    for line in data.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name")
            && let Some(val) = trimmed.split('=').nth(1)
        {
            let name = val.trim().trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Walk up from `dir` looking for a `.git` directory, return its parent folder name.
fn find_git_repo_name(dir: &Path, max_levels: usize) -> Option<String> {
    let mut current = Some(dir);
    for _ in 0..=max_levels {
        let d = current?;
        if d.join(".git").exists() {
            return d
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .filter(|s| !s.is_empty());
        }
        current = d.parent();
    }
    None
}

/// Decode encoded project path from Claude Code's directory encoding.
/// Claude Code encodes paths: `/` becomes `-`, and literal `-` becomes `--`.
fn decode_project_path(encoded: &str) -> PathBuf {
    if encoded.is_empty() {
        return PathBuf::from(".");
    }

    // Use placeholder for escaped literal dashes
    let decoded = encoded.replace("--", "\x00");
    let decoded = decoded.replace('-', std::path::MAIN_SEPARATOR_STR);
    let decoded = decoded.replace('\x00', "-");

    PathBuf::from(decoded)
}

// ── Session Selection ─────────────────────────────────────────────────────

pub fn preferred_active_session(
    sessions: &[ClaudeSessionSnapshot],
) -> Option<&ClaudeSessionSnapshot> {
    sessions
        .iter()
        .max_by_key(|session| session_rank_key(session))
}

pub fn latest_limits_source(sessions: &[ClaudeSessionSnapshot]) -> Option<&ClaudeSessionSnapshot> {
    sessions
        .iter()
        .filter(|session| limits_present(&session.limits))
        .max_by_key(|session| {
            let observed = session
                .last_token_event_at
                .map(|ts| ts.timestamp())
                .unwrap_or(i64::MIN);
            let activity = session
                .last_activity
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            (observed, activity)
        })
}

pub fn limits_present(limits: &RateLimits) -> bool {
    limits.primary.is_some() || limits.secondary.is_some()
}

/// Merge subagent token/cost data into parent sessions, and populate parent's `subagents` vec.
/// Subagent sessions remain in the list (for Recent Sessions display) but inherit parent's project name.
#[allow(clippy::too_many_arguments)]
fn merge_subagents_into_parents(sessions: &mut [ClaudeSessionSnapshot]) {
    // Collect subagent info indexed by parent session ID
    let mut subagent_data: HashMap<String, Vec<(usize, SubagentInfo)>> = HashMap::new();
    for (idx, session) in sessions.iter().enumerate() {
        if !session.is_subagent {
            continue;
        }
        let Some(ref parent_id) = session.parent_session_id else {
            continue;
        };
        let agent_type = session
            .project_name
            .strip_prefix("subagent:")
            .unwrap_or("unknown")
            .to_string();
        let info = SubagentInfo {
            agent_type,
            model: session.model.clone(),
            model_display: session.model_display.clone(),
            activity: session.activity.clone(),
            tokens: session.input_tokens + session.output_tokens,
            cost: session.total_cost,
        };
        subagent_data
            .entry(parent_id.clone())
            .or_default()
            .push((idx, info));
    }

    // Apply subagent data to parent sessions
    for session in sessions.iter_mut() {
        if session.is_subagent {
            continue;
        }
        let Some(subagents) = subagent_data.remove(&session.session_id) else {
            continue;
        };
        for (_, info) in &subagents {
            session.input_tokens += info.tokens.saturating_sub(info.cost as u64); // approx — tokens already counted
            session.total_cost += info.cost;
            session.subagents.push(info.clone());
        }
    }

    // Inherit parent project name for subagent sessions
    let parent_names: HashMap<String, String> = sessions
        .iter()
        .filter(|s| !s.is_subagent)
        .map(|s| (s.session_id.clone(), s.project_name.clone()))
        .collect();
    for session in sessions.iter_mut() {
        if session.is_subagent
            && let Some(ref parent_id) = session.parent_session_id
            && let Some(parent_name) = parent_names.get(parent_id)
        {
            let agent_type = session
                .project_name
                .strip_prefix("subagent:")
                .unwrap_or("agent")
                .to_string();
            session.project_name = format!("↳ {} ({})", agent_type, parent_name);
        }
    }
}

fn dedupe_sessions_by_id(sessions: Vec<ClaudeSessionSnapshot>) -> Vec<ClaudeSessionSnapshot> {
    let mut deduped: Vec<ClaudeSessionSnapshot> = Vec::new();
    let mut index_by_id: HashMap<String, usize> = HashMap::new();

    for session in sessions {
        let session_id = session.session_id.clone();
        if let Some(existing_index) = index_by_id.get(&session_id).copied() {
            if session_rank_key(&session) > session_rank_key(&deduped[existing_index]) {
                deduped[existing_index] = session;
            }
            continue;
        }

        let next_index = deduped.len();
        index_by_id.insert(session_id, next_index);
        deduped.push(session);
    }

    deduped
}

fn should_include_session(
    snapshot: &ClaudeSessionSnapshot,
    recency: SystemTime,
    stale_cutoff: SystemTime,
    sticky_cutoff: SystemTime,
) -> bool {
    if recency >= stale_cutoff {
        return true;
    }
    if recency < sticky_cutoff {
        return false;
    }
    snapshot
        .activity
        .as_ref()
        .is_some_and(session_activity_is_sticky_active)
}

fn session_activity_is_sticky_active(activity: &ActivitySnapshot) -> bool {
    if activity.pending_calls > 0 {
        return true;
    }
    matches!(activity.kind, ActivityKind::WaitingInput) || is_working_activity_kind(&activity.kind)
}

fn session_rank_key(snapshot: &ClaudeSessionSnapshot) -> (SystemTime, usize, u8, String) {
    let (pending_calls, activity_priority) =
        snapshot
            .activity
            .as_ref()
            .map_or((0usize, 0u8), |activity| {
                (
                    activity.pending_calls,
                    session_activity_priority(&activity.kind),
                )
            });
    (
        snapshot.last_activity,
        pending_calls,
        activity_priority,
        snapshot.session_id.clone(),
    )
}

fn session_activity_priority(kind: &ActivityKind) -> u8 {
    match kind {
        ActivityKind::Thinking
        | ActivityKind::ReadingFile
        | ActivityKind::EditingFile
        | ActivityKind::RunningCommand => 3,
        ActivityKind::WaitingInput => 2,
        ActivityKind::Idle => 1,
    }
}

fn is_working_activity_kind(kind: &ActivityKind) -> bool {
    matches!(
        kind,
        ActivityKind::Thinking
            | ActivityKind::ReadingFile
            | ActivityKind::EditingFile
            | ActivityKind::RunningCommand
    )
}

fn session_recency(snapshot: &ClaudeSessionSnapshot, file_modified: SystemTime) -> SystemTime {
    let mut newest = SystemTime::UNIX_EPOCH;

    if let Some(activity) = &snapshot.activity {
        for candidate in [
            activity.last_effective_signal_at,
            activity.last_active_at,
            activity.observed_at,
        ] {
            if let Some(ts) = candidate.and_then(datetime_to_system_time)
                && ts > newest
            {
                newest = ts;
            }
        }
    }

    if let Some(ts) = snapshot
        .last_token_event_at
        .and_then(datetime_to_system_time)
        && ts > newest
    {
        newest = ts;
    }

    if newest > SystemTime::UNIX_EPOCH {
        newest
    } else {
        file_modified
    }
}

fn datetime_to_system_time(ts: DateTime<Utc>) -> Option<SystemTime> {
    if ts.timestamp() < 0 {
        return None;
    }
    let secs = ts.timestamp() as u64;
    let nanos = ts.timestamp_subsec_nanos() as u64;
    SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(secs))?
        .checked_add(Duration::from_nanos(nanos))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ReasoningEffort (5-tier Opus 4.7) ──

    #[test]
    fn effort_label_all_variants() {
        assert_eq!(ReasoningEffort::Low.label(), "Low");
        assert_eq!(ReasoningEffort::Medium.label(), "Medium");
        assert_eq!(ReasoningEffort::High.label(), "High");
        assert_eq!(ReasoningEffort::ExtraHigh.label(), "Extra High");
        assert_eq!(ReasoningEffort::Max.label(), "Max");
    }

    #[test]
    fn effort_short_label_all_variants() {
        assert_eq!(ReasoningEffort::Low.short_label(), "Low");
        assert_eq!(ReasoningEffort::Medium.short_label(), "Med");
        assert_eq!(ReasoningEffort::High.short_label(), "High");
        assert_eq!(ReasoningEffort::ExtraHigh.short_label(), "X-High");
        assert_eq!(ReasoningEffort::Max.short_label(), "Max");
    }

    #[test]
    fn effort_api_value_matches_anthropic_spec() {
        assert_eq!(ReasoningEffort::Low.api_value(), "low");
        assert_eq!(ReasoningEffort::Medium.api_value(), "medium");
        assert_eq!(ReasoningEffort::High.api_value(), "high");
        assert_eq!(ReasoningEffort::ExtraHigh.api_value(), "xhigh");
        assert_eq!(ReasoningEffort::Max.api_value(), "max");
    }

    #[test]
    fn effort_from_api_roundtrip() {
        for e in [
            ReasoningEffort::Low,
            ReasoningEffort::Medium,
            ReasoningEffort::High,
            ReasoningEffort::ExtraHigh,
            ReasoningEffort::Max,
        ] {
            assert_eq!(ReasoningEffort::from_api(e.api_value()), Some(e));
        }
    }

    #[test]
    fn effort_from_api_aliases() {
        assert_eq!(
            ReasoningEffort::from_api("xhigh"),
            Some(ReasoningEffort::ExtraHigh)
        );
        assert_eq!(
            ReasoningEffort::from_api("x-high"),
            Some(ReasoningEffort::ExtraHigh)
        );
        assert_eq!(
            ReasoningEffort::from_api("extra_high"),
            Some(ReasoningEffort::ExtraHigh)
        );
        assert_eq!(
            ReasoningEffort::from_api("extra high"),
            Some(ReasoningEffort::ExtraHigh)
        );
        assert_eq!(
            ReasoningEffort::from_api("EXTRAHIGH"),
            Some(ReasoningEffort::ExtraHigh)
        );
        assert_eq!(
            ReasoningEffort::from_api("med"),
            Some(ReasoningEffort::Medium)
        );
        assert_eq!(
            ReasoningEffort::from_api("maximum"),
            Some(ReasoningEffort::Max)
        );
        assert_eq!(ReasoningEffort::from_api("bogus"), None);
    }

    #[test]
    fn effort_is_high_includes_extra_high() {
        assert!(!ReasoningEffort::Low.is_high());
        assert!(!ReasoningEffort::Medium.is_high());
        assert!(ReasoningEffort::High.is_high());
        assert!(ReasoningEffort::ExtraHigh.is_high());
        assert!(ReasoningEffort::Max.is_high());
    }

    #[test]
    fn effort_serde_snake_case_preserves_extra_high() {
        let json = serde_json::to_string(&ReasoningEffort::ExtraHigh).unwrap();
        assert_eq!(json, "\"extra_high\"");
        let parsed: ReasoningEffort = serde_json::from_str("\"extra_high\"").unwrap();
        assert_eq!(parsed, ReasoningEffort::ExtraHigh);
    }

    #[test]
    fn decode_project_paths() {
        // On any platform, test the logic
        let decoded = decode_project_path("home-user-projects-my--app");
        let decoded_str = decoded.to_string_lossy();
        // Should have decoded dashes to separators and double-dashes to literal dashes
        assert!(decoded_str.contains("my-app") || decoded_str.contains("my-app"));
    }

    #[test]
    fn git_branch_cache_ttl() {
        let mut cache = GitBranchCache::new(Duration::from_secs(30));
        // Just verify it doesn't crash on non-existent path
        assert_eq!(cache.get(Path::new("/nonexistent/path")), None);
    }

    #[test]
    fn activity_kind_text() {
        let snap = ActivitySnapshot {
            kind: ActivityKind::EditingFile,
            target: Some("src/main.rs".to_string()),
            ..Default::default()
        };
        assert_eq!(snap.action_text(), "Editing");
        // to_text shortens the path to just the filename
        assert_eq!(snap.to_text(true), "Editing main.rs");
        assert_eq!(snap.to_text(false), "Editing");
    }

    #[test]
    fn classify_read_tool() {
        let content = serde_json::json!({
            "name": "Read",
            "input": {"file_path": "/foo/bar.rs"}
        });
        let (kind, target) = classify_tool_call(&content);
        assert_eq!(kind, ActivityKind::ReadingFile);
        assert_eq!(target, Some("bar.rs".to_string()));
    }

    #[test]
    fn classify_bash_tool() {
        let content = serde_json::json!({
            "name": "Bash",
            "input": {"command": "cargo build"}
        });
        let (kind, target) = classify_tool_call(&content);
        assert_eq!(kind, ActivityKind::RunningCommand);
        assert_eq!(target, Some("cargo build".to_string()));
    }

    #[test]
    fn classify_write_tool() {
        let content = serde_json::json!({
            "name": "Write",
            "input": {"file_path": "/home/user/project/src/lib.rs"}
        });
        let (kind, target) = classify_tool_call(&content);
        assert_eq!(kind, ActivityKind::EditingFile);
        assert_eq!(target, Some("lib.rs".to_string()));
    }

    #[test]
    fn classify_edit_tool() {
        let content = serde_json::json!({
            "name": "Edit",
            "input": {"file_path": "C:\\Users\\dev\\project\\main.rs"}
        });
        let (kind, target) = classify_tool_call(&content);
        assert_eq!(kind, ActivityKind::EditingFile);
        assert_eq!(target, Some("main.rs".to_string()));
    }

    #[test]
    fn classify_glob_tool() {
        let content = serde_json::json!({
            "name": "Glob",
            "input": {"pattern": "**/*.rs"}
        });
        let (kind, target) = classify_tool_call(&content);
        assert_eq!(kind, ActivityKind::ReadingFile);
        assert_eq!(target, Some("**/*.rs".to_string()));
    }

    #[test]
    fn classify_ask_user() {
        let content = serde_json::json!({"name": "AskUserQuestion"});
        let (kind, target) = classify_tool_call(&content);
        assert_eq!(kind, ActivityKind::WaitingInput);
        assert_eq!(target, None);
    }

    #[test]
    fn classify_web_search() {
        let content = serde_json::json!({"name": "WebSearch"});
        let (kind, _) = classify_tool_call(&content);
        assert_eq!(kind, ActivityKind::Thinking);
    }

    #[test]
    fn classify_unknown_tool() {
        let content = serde_json::json!({"name": "CustomTool"});
        let (kind, _) = classify_tool_call(&content);
        assert_eq!(kind, ActivityKind::Thinking);
    }

    #[test]
    fn extract_filename_from_path_variants() {
        // file_path field
        let input = serde_json::json!({"file_path": "/a/b/c.rs"});
        assert_eq!(extract_filename(Some(&input)), Some("c.rs".to_string()));

        // path field
        let input = serde_json::json!({"path": "/a/b/d.ts"});
        assert_eq!(extract_filename(Some(&input)), Some("d.ts".to_string()));

        // notebook_path field
        let input = serde_json::json!({"notebook_path": "/a/b/notebook.ipynb"});
        assert_eq!(
            extract_filename(Some(&input)),
            Some("notebook.ipynb".to_string())
        );

        // No path field
        let input = serde_json::json!({"other": "value"});
        assert_eq!(extract_filename(Some(&input)), None);

        // None input
        assert_eq!(extract_filename(None), None);
    }

    #[test]
    fn truncate_target_short() {
        assert_eq!(truncate_target("hello", 40), "hello");
    }

    #[test]
    fn truncate_target_long() {
        let long = "a".repeat(60);
        let result = truncate_target(&long, 40);
        assert!(result.len() <= 40);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn activity_snapshot_default() {
        let snap = ActivitySnapshot::default();
        assert_eq!(snap.kind, ActivityKind::Idle);
        assert_eq!(snap.target, None);
        assert_eq!(snap.action_text(), "Idle");
    }

    #[test]
    fn activity_kind_all_variants() {
        assert_eq!(
            ActivitySnapshot {
                kind: ActivityKind::Thinking,
                ..Default::default()
            }
            .action_text(),
            "Thinking"
        );
        assert_eq!(
            ActivitySnapshot {
                kind: ActivityKind::ReadingFile,
                ..Default::default()
            }
            .action_text(),
            "Reading"
        );
        assert_eq!(
            ActivitySnapshot {
                kind: ActivityKind::RunningCommand,
                ..Default::default()
            }
            .action_text(),
            "Running command"
        );
        assert_eq!(
            ActivitySnapshot {
                kind: ActivityKind::WaitingInput,
                ..Default::default()
            }
            .action_text(),
            "Waiting for input"
        );
    }

    #[test]
    fn synthetic_model_is_ignored() {
        let mut acc = SessionAccumulator::with_default_effort(ReasoningEffort::Medium);
        acc.model = Some("claude-opus-4-6".to_string());

        let msg: JsonlMessage = serde_json::from_value(serde_json::json!({
            "type": "assistant",
            "is_api_error_message": true,
            "message": {
                "model": "<synthetic>",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "API Error"}]
            }
        }))
        .unwrap();

        process_jsonl_message(&mut acc, &msg);
        assert_eq!(acc.model.as_deref(), Some("claude-opus-4-6"));
    }

    #[test]
    fn session_id_validation_accepts_uuid_shape() {
        assert!(is_valid_session_id("4ccf0482-61c0-4611-9d22-becaf1781231"));
        assert!(!is_valid_session_id("encoded_c--foo"));
        assert!(is_synthetic_model_id("<synthetic>"));
    }

    #[test]
    fn derive_name_normal_path() {
        assert_eq!(
            derive_project_name(Path::new("/home/user/my-project")),
            "my-project"
        );
        assert_eq!(
            derive_project_name(Path::new("D:\\X\\Work\\Property Alpha")),
            "Property Alpha"
        );
    }

    #[test]
    fn derive_name_drive_root() {
        assert_eq!(derive_project_name(Path::new("C:\\")), "C: Drive");
        assert_eq!(derive_project_name(Path::new("D:\\")), "D: Drive");
        assert_eq!(derive_project_name(Path::new("C:/")), "C: Drive");
    }

    #[test]
    fn derive_name_from_package_json() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name": "my-cool-app", "version": "1.0.0"}"#,
        )
        .unwrap();
        // Create a subdirectory with no file_name fallback scenario
        // For normal dirs, file_name() works — test metadata fallback via helper directly
        assert_eq!(
            read_project_name_from_metadata(dir.path()),
            Some("my-cool-app".to_string())
        );
    }

    #[test]
    fn derive_name_from_cargo_toml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"discord-presence\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();
        assert_eq!(
            read_project_name_from_metadata(dir.path()),
            Some("discord-presence".to_string())
        );
    }

    #[test]
    fn derive_name_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        let name = find_git_repo_name(dir.path(), 3);
        assert!(name.is_some());
    }
}
