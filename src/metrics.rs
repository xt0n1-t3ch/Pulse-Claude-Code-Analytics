use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::config;
use crate::cost;
use crate::session::ClaudeSessionSnapshot;
use crate::util::{format_cost, format_tokens, human_duration};

const PERSIST_INTERVAL: Duration = Duration::from_secs(10);

// ── Public Snapshot Types (serializable) ─────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub daemon_started_at: DateTime<Utc>,
    pub snapshot_at: DateTime<Utc>,
    pub uptime_seconds: u64,
    pub totals: TokenTotals,
    pub cost_breakdown: CostBreakdown,
    pub by_model: Vec<ModelMetrics>,
    pub active_sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenTotals {
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CostBreakdown {
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetrics {
    pub model_id: String,
    pub display_name: String,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    pub session_count: u32,
}

// ── Tracker ──────────────────────────────────────────────────────────────

/// Internal per-session record (latest values, NOT cumulative deltas).
#[derive(Debug, Clone, Default)]
struct SessionRecord {
    model_id: String,
    display_name: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    total_cost: f64,
}

pub struct MetricsTracker {
    daemon_started_at: DateTime<Utc>,
    started_instant: Instant,
    /// Latest snapshot per session_id — replace-on-update, not additive.
    sessions: HashMap<String, SessionRecord>,
    last_persist_at: Option<Instant>,
    /// Cached snapshot for TUI rendering (avoids recomputing every frame).
    cached_snapshot: Option<MetricsSnapshot>,
}

impl MetricsTracker {
    pub fn new() -> Self {
        Self {
            daemon_started_at: Utc::now(),
            started_instant: Instant::now(),
            sessions: HashMap::new(),
            last_persist_at: None,
            cached_snapshot: None,
        }
    }

    /// Update internal records from the latest session snapshots.
    pub fn update(&mut self, sessions: &[ClaudeSessionSnapshot]) {
        for session in sessions {
            let model_id = session.model.clone().unwrap_or_default();
            let display_name = session
                .model_display
                .clone()
                .unwrap_or_else(|| cost::model_display_name(&model_id));
            let record = SessionRecord {
                model_id,
                display_name,
                input_tokens: session.input_tokens,
                output_tokens: session.output_tokens,
                cache_creation_tokens: session.cache_creation_tokens,
                cache_read_tokens: session.cache_read_tokens,
                total_cost: session.total_cost,
            };
            self.sessions.insert(session.session_id.clone(), record);
        }
        self.cached_snapshot = Some(self.compute_snapshot(sessions.len()));
    }

    /// Get the latest computed snapshot for TUI rendering.
    pub fn snapshot(&self) -> Option<&MetricsSnapshot> {
        self.cached_snapshot.as_ref()
    }

    /// Persist to disk if enough time has elapsed since last write.
    pub fn persist_if_due(&mut self) {
        if let Some(last) = self.last_persist_at
            && last.elapsed() < PERSIST_INTERVAL
        {
            return;
        }
        let Some(ref snap) = self.cached_snapshot else {
            return;
        };
        self.last_persist_at = Some(Instant::now());
        persist_json(snap);
        persist_markdown(snap);
    }

    fn compute_snapshot(&self, active_count: usize) -> MetricsSnapshot {
        let mut totals = TokenTotals::default();
        let mut cost_bd = CostBreakdown::default();
        let mut by_model_map: HashMap<String, ModelMetrics> = HashMap::new();

        for record in self.sessions.values() {
            totals.input_tokens += record.input_tokens;
            totals.output_tokens += record.output_tokens;
            totals.cache_write_tokens += record.cache_creation_tokens;
            totals.cache_read_tokens += record.cache_read_tokens;
            totals.cost_usd += record.total_cost;

            // Cost breakdown by token type (re-compute from tokens + pricing)
            let pricing = cost::model_pricing(&record.model_id);
            // input_tokens in the snapshot already includes cache tokens,
            // so for the breakdown we compute: pure_input = input - cache_creation - cache_read
            let pure_input = record
                .input_tokens
                .saturating_sub(record.cache_creation_tokens)
                .saturating_sub(record.cache_read_tokens);
            cost_bd.input_cost += (pure_input as f64 / 1_000_000.0) * pricing.input_per_million;
            cost_bd.output_cost +=
                (record.output_tokens as f64 / 1_000_000.0) * pricing.output_per_million;
            cost_bd.cache_write_cost += (record.cache_creation_tokens as f64 / 1_000_000.0)
                * pricing.cache_write_per_million;
            cost_bd.cache_read_cost +=
                (record.cache_read_tokens as f64 / 1_000_000.0) * pricing.cache_read_per_million;

            // Per-model aggregation
            let key = record.display_name.clone();
            let entry = by_model_map.entry(key).or_insert_with(|| ModelMetrics {
                model_id: record.model_id.clone(),
                display_name: record.display_name.clone(),
                cost_usd: 0.0,
                input_tokens: 0,
                output_tokens: 0,
                cache_write_tokens: 0,
                cache_read_tokens: 0,
                session_count: 0,
            });
            entry.cost_usd += record.total_cost;
            entry.input_tokens += record.input_tokens;
            entry.output_tokens += record.output_tokens;
            entry.cache_write_tokens += record.cache_creation_tokens;
            entry.cache_read_tokens += record.cache_read_tokens;
            entry.session_count += 1;
        }

        totals.total_tokens = totals.input_tokens + totals.output_tokens;

        let mut by_model: Vec<ModelMetrics> = by_model_map.into_values().collect();
        by_model.sort_by(|a, b| {
            b.cost_usd
                .partial_cmp(&a.cost_usd)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        MetricsSnapshot {
            daemon_started_at: self.daemon_started_at,
            snapshot_at: Utc::now(),
            uptime_seconds: self.started_instant.elapsed().as_secs(),
            totals,
            cost_breakdown: cost_bd,
            by_model,
            active_sessions: active_count,
        }
    }
}

impl Default for MetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ── Persistence ──────────────────────────────────────────────────────────

fn persist_json(snap: &MetricsSnapshot) {
    let path = config::claude_home().join("discord-presence-metrics.json");
    let tmp = config::claude_home().join("discord-presence-metrics.json.tmp");
    match serde_json::to_string_pretty(snap) {
        Ok(data) => {
            if let Err(e) = std::fs::write(&tmp, &data) {
                warn!("Failed to write metrics JSON: {e}");
                return;
            }
            if let Err(e) = std::fs::rename(&tmp, &path) {
                warn!("Failed to rename metrics JSON: {e}");
            }
        }
        Err(e) => warn!("Failed to serialize metrics: {e}"),
    }
}

fn persist_markdown(snap: &MetricsSnapshot) {
    let path = config::claude_home().join("discord-presence-metrics.md");
    let tmp = config::claude_home().join("discord-presence-metrics.md.tmp");
    let md = generate_markdown(snap);
    if let Err(e) = std::fs::write(&tmp, &md) {
        warn!("Failed to write metrics markdown: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        warn!("Failed to rename metrics markdown: {e}");
    }
}

fn generate_markdown(snap: &MetricsSnapshot) -> String {
    let now_local = Local::now().format("%b %d, %Y %I:%M %p");
    let uptime = human_duration(Duration::from_secs(snap.uptime_seconds));

    let mut md = String::new();
    md.push_str("# Claude Code Metrics Report\n\n");
    md.push_str(&format!(
        "*Generated: {} | Uptime: {}*\n\n",
        now_local, uptime
    ));

    // Total spend
    md.push_str("## Total Spend\n\n");
    md.push_str("| Metric | Value |\n");
    md.push_str("|--------|-------|\n");
    md.push_str(&format!(
        "| Total Cost | {} |\n",
        format_cost(snap.totals.cost_usd)
    ));
    md.push_str(&format!(
        "| Total Tokens | {} |\n",
        format_tokens(snap.totals.total_tokens)
    ));
    md.push_str(&format!(
        "| Input Tokens | {} |\n",
        format_tokens(snap.totals.input_tokens)
    ));
    md.push_str(&format!(
        "| Output Tokens | {} |\n",
        format_tokens(snap.totals.output_tokens)
    ));
    md.push_str(&format!(
        "| Cache Write | {} |\n",
        format_tokens(snap.totals.cache_write_tokens)
    ));
    md.push_str(&format!(
        "| Cache Read | {} |\n",
        format_tokens(snap.totals.cache_read_tokens)
    ));
    md.push('\n');

    // Cost breakdown by type
    let total = snap.totals.cost_usd.max(0.0001); // avoid div by zero
    md.push_str("## Cost Breakdown by Type\n\n");
    md.push_str("| Type | Cost | % of Total |\n");
    md.push_str("|------|------|------------|\n");
    for (label, cost) in [
        ("Input", snap.cost_breakdown.input_cost),
        ("Output", snap.cost_breakdown.output_cost),
        ("Cache Write", snap.cost_breakdown.cache_write_cost),
        ("Cache Read", snap.cost_breakdown.cache_read_cost),
    ] {
        let pct = (cost / total) * 100.0;
        md.push_str(&format!(
            "| {} | {} | {:.1}% |\n",
            label,
            format_cost(cost),
            pct
        ));
    }
    md.push('\n');

    // By model
    if !snap.by_model.is_empty() {
        md.push_str("## By Model\n\n");
        md.push_str("| Model | Sessions | Tokens | Cost | % |\n");
        md.push_str("|-------|----------|--------|------|---|\n");
        for model in &snap.by_model {
            let model_tokens = model.input_tokens + model.output_tokens;
            let pct = (model.cost_usd / total) * 100.0;
            md.push_str(&format!(
                "| {} | {} | {} | {} | {:.1}% |\n",
                model.display_name,
                model.session_count,
                format_tokens(model_tokens),
                format_cost(model.cost_usd),
                pct
            ));
        }
        md.push('\n');
    }

    md.push_str(&format!("*Active sessions: {}*\n", snap.active_sessions));
    md
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{DataSource, RateLimits};
    use std::path::PathBuf;
    use std::time::SystemTime;

    fn make_session(
        id: &str,
        model: &str,
        input: u64,
        output: u64,
        cache_write: u64,
        cache_read: u64,
        cost: f64,
    ) -> ClaudeSessionSnapshot {
        ClaudeSessionSnapshot {
            session_id: id.to_string(),
            cwd: PathBuf::from("/test"),
            project_name: "test".to_string(),
            git_branch: None,
            model: Some(model.to_string()),
            model_display: Some(cost::model_display_name(model)),
            session_total_tokens: Some(input + output),
            last_turn_tokens: None,
            session_delta_tokens: None,
            input_tokens: input,
            output_tokens: output,
            cache_creation_tokens: cache_write,
            cache_read_tokens: cache_read,
            total_cost: cost,
            limits: RateLimits::default(),
            activity: None,
            started_at: None,
            last_token_event_at: None,
            last_activity: SystemTime::now(),
            source: DataSource::Jsonl,
            source_file: PathBuf::from("/test.jsonl"),
        }
    }

    #[test]
    fn accumulates_sessions_without_double_counting() {
        let mut tracker = MetricsTracker::new();

        // First update: session with 1000 input, 500 output
        let sessions = vec![make_session(
            "s1",
            "claude-opus-4-6",
            1000,
            500,
            200,
            300,
            0.05,
        )];
        tracker.update(&sessions);
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.totals.input_tokens, 1000);
        assert_eq!(snap.totals.output_tokens, 500);
        assert!((snap.totals.cost_usd - 0.05).abs() < 0.001);

        // Second update: same session with increased values (NOT additive)
        let sessions = vec![make_session(
            "s1",
            "claude-opus-4-6",
            2000,
            1000,
            400,
            600,
            0.10,
        )];
        tracker.update(&sessions);
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.totals.input_tokens, 2000); // replaced, not 3000
        assert_eq!(snap.totals.output_tokens, 1000);
        assert!((snap.totals.cost_usd - 0.10).abs() < 0.001);
    }

    #[test]
    fn aggregates_multiple_sessions() {
        let mut tracker = MetricsTracker::new();
        let sessions = vec![
            make_session("s1", "claude-opus-4-6", 1000, 500, 100, 200, 0.05),
            make_session("s2", "claude-sonnet-4-5", 2000, 800, 0, 0, 0.03),
        ];
        tracker.update(&sessions);
        let snap = tracker.snapshot().unwrap();
        assert_eq!(snap.totals.input_tokens, 3000);
        assert_eq!(snap.totals.output_tokens, 1300);
        assert!((snap.totals.cost_usd - 0.08).abs() < 0.001);
        assert_eq!(snap.by_model.len(), 2);
        assert_eq!(snap.active_sessions, 2);
    }

    #[test]
    fn per_model_breakdown() {
        let mut tracker = MetricsTracker::new();
        let sessions = vec![
            make_session("s1", "claude-opus-4-6", 1000, 500, 0, 0, 0.05),
            make_session("s2", "claude-opus-4-6", 2000, 800, 0, 0, 0.08),
            make_session("s3", "claude-sonnet-4-5", 500, 200, 0, 0, 0.01),
        ];
        tracker.update(&sessions);
        let snap = tracker.snapshot().unwrap();

        // Opus should be first (sorted by cost desc)
        assert_eq!(snap.by_model[0].display_name, "Claude Opus 4.6");
        assert_eq!(snap.by_model[0].session_count, 2);
        assert!((snap.by_model[0].cost_usd - 0.13).abs() < 0.001);

        assert_eq!(snap.by_model[1].display_name, "Claude Sonnet 4.5");
        assert_eq!(snap.by_model[1].session_count, 1);
    }

    #[test]
    fn markdown_generation() {
        let snap = MetricsSnapshot {
            daemon_started_at: Utc::now(),
            snapshot_at: Utc::now(),
            uptime_seconds: 3600,
            totals: TokenTotals {
                cost_usd: 1.23,
                input_tokens: 450_000,
                output_tokens: 125_000,
                cache_write_tokens: 50_000,
                cache_read_tokens: 300_000,
                total_tokens: 575_000,
            },
            cost_breakdown: CostBreakdown {
                input_cost: 0.45,
                output_cost: 0.63,
                cache_write_cost: 0.06,
                cache_read_cost: 0.02,
            },
            by_model: vec![ModelMetrics {
                model_id: "claude-opus-4-6".to_string(),
                display_name: "Claude Opus 4.6".to_string(),
                cost_usd: 1.23,
                input_tokens: 450_000,
                output_tokens: 125_000,
                cache_write_tokens: 50_000,
                cache_read_tokens: 300_000,
                session_count: 2,
            }],
            active_sessions: 1,
        };
        let md = generate_markdown(&snap);
        assert!(md.contains("# Claude Code Metrics Report"));
        assert!(md.contains("Total Cost"));
        assert!(md.contains("$1.23"));
        assert!(md.contains("Claude Opus 4.6"));
        assert!(md.contains("By Model"));
    }

    #[test]
    fn json_roundtrip() {
        let snap = MetricsSnapshot {
            daemon_started_at: Utc::now(),
            snapshot_at: Utc::now(),
            uptime_seconds: 60,
            totals: TokenTotals {
                cost_usd: 0.05,
                input_tokens: 1000,
                output_tokens: 500,
                cache_write_tokens: 0,
                cache_read_tokens: 0,
                total_tokens: 1500,
            },
            cost_breakdown: CostBreakdown::default(),
            by_model: vec![],
            active_sessions: 1,
        };
        let json = serde_json::to_string_pretty(&snap).unwrap();
        let parsed: MetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert!((parsed.totals.cost_usd - 0.05).abs() < 0.001);
        assert_eq!(parsed.totals.input_tokens, 1000);
    }
}
