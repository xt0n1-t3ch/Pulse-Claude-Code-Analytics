use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
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
        if show_target {
            if let Some(ref target) = self.target {
                let trimmed = target.trim();
                if !trimmed.is_empty() {
                    let short = shorten_activity_target(&self.kind, trimmed);
                    return format!("{} {}", self.action_text(), short);
                }
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
    pub total_cost: f64,
    pub limits: RateLimits,
    pub activity: Option<ActivitySnapshot>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_token_event_at: Option<DateTime<Utc>>,
    pub last_activity: SystemTime,
    pub source: DataSource,
    pub source_file: PathBuf,
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
        if let Some(cached) = self.entries.get(&key) {
            if Instant::now() < cached.expires_at {
                return cached.value.clone();
            }
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
    let output = Command::new("git")
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
        let sha_output = Command::new("git")
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
    fn new(modified: SystemTime) -> Self {
        Self {
            cursor: 0,
            file_len: 0,
            modified,
            accumulator: SessionAccumulator::default(),
            snapshot: None,
        }
    }

    fn reset(&mut self, modified: SystemTime) {
        self.cursor = 0;
        self.file_len = 0;
        self.modified = modified;
        self.accumulator = SessionAccumulator::default();
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
    total_cost: f64,
    session_total_tokens: Option<u64>,
    previous_session_total_tokens: Option<u64>,
    last_turn_tokens: Option<u64>,
    limits: RateLimits,
    last_token_event_at: Option<DateTime<Utc>>,
    activity_tracker: ActivityTracker,
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
    #[allow(dead_code)]
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

    // Validate model
    if status.model.id.is_empty() && status.model.display_name.is_empty() {
        return None;
    }
    if status.model.display_name == "Test" {
        return None;
    }

    let project_name = Path::new(&project_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown Project".to_string());

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
        model: Some(status.model.id.clone()),
        model_display: Some(if !status.model.display_name.is_empty() {
            status.model.display_name
        } else {
            cost::model_display_name(&status.model.id).to_string()
        }),
        session_total_tokens: Some(total_tokens),
        last_turn_tokens: None,
        session_delta_tokens: None,
        input_tokens: status.context_window.total_input_tokens,
        output_tokens: status.context_window.total_output_tokens,
        cache_creation_tokens: 0, // statusline doesn't expose cache breakdown
        cache_read_tokens: 0,
        total_cost: status.cost.total_cost_usd,
        limits: RateLimits::default(),
        activity: None,
        started_at,
        last_token_event_at: None,
        last_activity: modified,
        source: DataSource::Statusline,
        source_file: data_path,
    })
}

// ── JSONL Session Parsing ─────────────────────────────────────────────────

/// Claude Code JSONL message format
#[derive(Debug, Deserialize)]
struct JsonlMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    message: Option<JsonlMessageContent>,
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
    collect_active_sessions_multi(
        &projects_roots,
        stale_threshold,
        active_sticky_window,
        git_cache,
        parse_cache,
    )
}

pub fn collect_active_sessions_multi(
    projects_roots: &[PathBuf],
    stale_threshold: Duration,
    active_sticky_window: Duration,
    git_cache: &mut GitBranchCache,
    parse_cache: &mut SessionParseCache,
) -> Result<Vec<ClaudeSessionSnapshot>> {
    let now = SystemTime::now();
    let stale_cutoff = now
        .checked_sub(stale_threshold)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let sticky_cutoff = now
        .checked_sub(active_sticky_window)
        .unwrap_or(SystemTime::UNIX_EPOCH);

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
            )? {
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
    sessions.sort_by_key(|session| Reverse(session_rank_key(session)));
    Ok(sessions)
}

fn parse_session_file_cached(
    path: &Path,
    metadata: &std::fs::Metadata,
    modified: SystemTime,
    encoded_dir: &str,
    git_cache: &mut GitBranchCache,
    parse_cache: &mut SessionParseCache,
) -> Result<Option<ClaudeSessionSnapshot>> {
    let file_len = metadata.len();
    let path_buf = path.to_path_buf();

    let entry = parse_cache
        .entries
        .entry(path_buf.clone())
        .or_insert_with(|| CachedSessionEntry::new(modified));

    // Reset if file was truncated or modified time changed significantly
    if file_len < entry.file_len || modified != entry.modified {
        entry.reset(modified);
    }

    // No new data
    if file_len == entry.cursor && entry.snapshot.is_some() {
        return Ok(entry.snapshot.clone());
    }

    // Read new lines from cursor
    let mut file =
        std::fs::File::open(path).with_context(|| format!("cannot open {}", path.display()))?;

    if entry.cursor > 0 {
        file.seek(SeekFrom::Start(entry.cursor))?;
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

    let project_name = cwd
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown Project".to_string());

    let git_branch = git_cache.get(&cwd);
    let total_tokens = entry.accumulator.total_input_tokens + entry.accumulator.total_output_tokens;

    let session_delta = entry.accumulator.session_total_tokens.and_then(|current| {
        entry.accumulator.previous_session_total_tokens.map(|prev| {
            if current > prev {
                current - prev
            } else {
                0
            }
        })
    });

    let session_id = if encoded_dir.is_empty() {
        cwd.to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "_")
    } else {
        format!("encoded_{encoded_dir}")
    };

    let last_turn_tokens = entry.accumulator.last_turn_tokens;
    let input_tokens = entry.accumulator.total_input_tokens;
    let output_tokens = entry.accumulator.total_output_tokens;
    let cache_creation_tokens = entry.accumulator.total_cache_creation_tokens;
    let cache_read_tokens = entry.accumulator.total_cache_read_tokens;
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
        total_cost,
        limits,
        activity: entry.accumulator.activity_tracker.finalize(),
        started_at,
        last_token_event_at,
        last_activity: modified,
        source: DataSource::Jsonl,
        source_file: path_buf,
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
    if acc.cwd.is_none() {
        if let Some(ref cwd) = msg.cwd {
            if !cwd.is_empty() {
                acc.cwd = Some(PathBuf::from(cwd));
            }
        }
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
            if let Some(ref model) = message.model {
                if !model.is_empty() {
                    acc.model = Some(model.clone());
                }
            }

            // Extract token usage (including prompt cache tokens)
            if let Some(ref usage) = message.usage {
                let input = usage.input_tokens;
                let output = usage.output_tokens;
                let cache_creation = usage.cache_creation_input_tokens;
                let cache_read = usage.cache_read_input_tokens;

                // All input-side tokens (non-cached + cache write + cache read)
                let all_input = input + cache_creation + cache_read;

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
                let call_id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                acc.activity_tracker
                    .register_call(&call_id, kind, target, observed_at);
                found_activity = true;
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

/// Decode encoded project path from Claude Code's directory encoding.
/// Claude Code encodes paths: `/` becomes `-`, and literal `-` becomes `--`.
fn decode_project_path(encoded: &str) -> PathBuf {
    if encoded.is_empty() {
        return PathBuf::from(".");
    }

    // Use placeholder for escaped literal dashes
    let decoded = encoded.replace("--", "\x00");
    let decoded = decoded.replace('-', &std::path::MAIN_SEPARATOR.to_string());
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
            if let Some(ts) = candidate.and_then(datetime_to_system_time) {
                if ts > newest {
                    newest = ts;
                }
            }
        }
    }

    if let Some(ts) = snapshot
        .last_token_event_at
        .and_then(datetime_to_system_time)
    {
        if ts > newest {
            newest = ts;
        }
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
}
