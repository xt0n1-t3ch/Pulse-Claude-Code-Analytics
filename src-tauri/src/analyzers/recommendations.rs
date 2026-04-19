//! Recommendations engine — turns the raw analyzer output into a short list
//! of actionable items. Each item carries a `fix_prompt` that the frontend's
//! "Fix with Claude Code" button copies to the clipboard so the user can
//! paste it straight into a CC session.

use serde::Serialize;

use super::Severity;
use super::cache_health::CacheHealthReport;
use super::inflection::InflectionPoint;
use super::model_routing::ModelRoutingReport;
use super::prompt_complexity::PromptComplexityReport;
use super::session_health::SessionHealthReport;
use super::tool_frequency::ToolFrequencyReport;
use crate::db::HistoricalSession;

#[derive(Debug, Clone, Serialize)]
pub struct Recommendation {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub estimated_savings: Option<String>,
    pub action: String,
    pub fix_prompt: String,
    pub color: &'static str,
}

pub struct AnalysisContext<'a> {
    pub sessions: &'a [HistoricalSession],
    pub cache: &'a CacheHealthReport,
    pub routing: &'a ModelRoutingReport,
    pub inflections: &'a [InflectionPoint],
    pub tool_frequency: Option<&'a ToolFrequencyReport>,
    pub prompt_complexity: Option<&'a PromptComplexityReport>,
    pub session_health: Option<&'a SessionHealthReport>,
}

pub fn generate(ctx: &AnalysisContext) -> Vec<Recommendation> {
    let mut recs: Vec<Recommendation> = Vec::new();

    if ctx.cache.trend_weighted_ratio < 50.0 && ctx.cache.sessions_analyzed > 3 {
        let sev = if ctx.cache.trend_weighted_ratio < 30.0 {
            Severity::Critical
        } else {
            Severity::Warning
        };
        recs.push(Recommendation {
            id: "cache-hit-low".into(),
            severity: sev,
            title: "Prompt cache is underperforming".into(),
            description: format!(
                "Recent cache hit ratio is {:.0}% — every turn is re-paying for the stable part \
                of your context. Usually this means CLAUDE.md changed recently or volatile content \
                moved to the top of the prompt.",
                ctx.cache.trend_weighted_ratio
            ),
            estimated_savings: Some("up to 40–60% fewer input tokens".into()),
            action: "Audit CLAUDE.md for volatile content. Keep stable text (rules, project \
                overview) at the top; put anything that changes per-session at the bottom."
                .into(),
            fix_prompt: format!(
                "Analyze my Claude Code cache health. Read the CLAUDE.md files in this repo and \
                in ~/.claude/. Identify sections that are likely changing between sessions and \
                invalidating the prompt cache (current hit ratio: {:.0}%). Suggest a reordering \
                that pins stable rules at the top and pushes volatile content to the bottom.",
                ctx.cache.trend_weighted_ratio
            ),
            color: sev.color_hint(),
        });
    } else if ctx.cache.trend_weighted_ratio >= 70.0 && ctx.cache.sessions_analyzed > 5 {
        recs.push(Recommendation {
            id: "cache-healthy".into(),
            severity: Severity::Positive,
            title: "Cache health is strong".into(),
            description: format!(
                "{:.0}% of your input is served from cache. Keep your stable prompts stable — \
                small reshuffles can drop this grade quickly.",
                ctx.cache.trend_weighted_ratio
            ),
            estimated_savings: None,
            action: "No action — maintain current prompt stability.".into(),
            fix_prompt: "No fix needed — my cache hit ratio is healthy. Just verify nothing has \
                changed that would regress it."
                .into(),
            color: Severity::Positive.color_hint(),
        });
    }

    if ctx.routing.opus.cost_share_pct >= 90.0 && ctx.routing.total_cost > 10.0 {
        recs.push(Recommendation {
            id: "opus-dominance".into(),
            severity: Severity::Warning,
            title: "Opus is carrying almost all your spend".into(),
            description: format!(
                "{:.0}% of cost is Opus ({}). Simple lookups, commit messages, and quick \
                refactors run fine on Sonnet or Haiku at 5–25× lower cost.",
                ctx.routing.opus.cost_share_pct, ctx.routing.opus.sessions
            ),
            estimated_savings: Some(format!(
                "~${:.2} if ~30% of Opus work moves to Sonnet",
                ctx.routing.estimated_savings_if_rerouted
            )),
            action: "For the next few sessions, try Sonnet-first on reads / small edits and \
                only escalate to Opus when reasoning depth is needed."
                .into(),
            fix_prompt: format!(
                "Looking at my last 30 days of Claude Code usage, {:.0}% of cost is Opus. \
                Suggest a concrete workflow: which classes of tasks should I route to Sonnet or \
                Haiku to cut cost without hurting quality?",
                ctx.routing.opus.cost_share_pct
            ),
            color: Severity::Warning.color_hint(),
        });
    }

    if let Some(spike) = ctx.inflections.iter().find(|p| p.direction == "spike") {
        recs.push(Recommendation {
            id: format!("inflection-spike-{}", spike.date),
            severity: Severity::Warning,
            title: format!("Cost spike on {}", spike.date),
            description: format!(
                "Cost per session ran {:.1}× the prior 3-day baseline. ${:.2} across {} \
                session(s). Likely cause: CLAUDE.md change, longer contexts, or a CC version update.",
                spike.multiplier, spike.cost_on_day, spike.sessions_on_day
            ),
            estimated_savings: None,
            action: format!(
                "Compare your setup on {} to the days before. `git log --since=\"{} 1 day ago\" \
                --until=\"{} 1 day\"` often reveals the trigger.",
                spike.date, spike.date, spike.date
            ),
            fix_prompt: format!(
                "On {} my Claude Code cost/session jumped {:.1}× versus the baseline. Check \
                ~/.claude/projects/**/statusline.jsonl for sessions on that date and figure out \
                what changed — CLAUDE.md edits, new skills/MCP servers, or CC version bump.",
                spike.date, spike.multiplier
            ),
            color: Severity::Warning.color_hint(),
        });
    }

    if let Some(tool_frequency) = ctx.tool_frequency {
        if tool_frequency.avg_tools_per_session >= 40.0 {
            recs.push(Recommendation {
                id: "tool-density-high".into(),
                severity: Severity::Warning,
                title: "Tool-call density is high".into(),
                description: format!(
                    "Average session fires {:.0} tools. Long read/search chains usually mean context drift and a missing `/compact` checkpoint.",
                    tool_frequency.avg_tools_per_session
                ),
                estimated_savings: Some("~10–25% fewer cache-write tokens".into()),
                action: "Compact every 30–40 tool calls. Split recon from implementation so searches do not ride along with edits."
                    .into(),
                fix_prompt: "Review my Claude Code workflow for tool overuse. I average 40+ tool calls per session. Suggest a tighter cadence for search, `/compact`, and fresh-session boundaries."
                    .into(),
                color: Severity::Warning.color_hint(),
            });
        }

        if tool_frequency.mcp_share_pct >= 20.0 {
            recs.push(Recommendation {
                id: "mcp-heavy".into(),
                severity: Severity::Info,
                title: "MCP traffic is unusually heavy".into(),
                description: format!(
                    "{:.0}% of tool calls are MCP-backed. Extra schemas and server churn can invalidate prompt prefixes.",
                    tool_frequency.mcp_share_pct
                ),
                estimated_savings: Some("~5–15% per avoidable cache break".into()),
                action: "Disconnect idle MCP servers between tasks and keep only the ones needed for the current repo.".into(),
                fix_prompt: "Audit my active Claude Code MCP servers. Which ones are adding tool-schema bloat without helping the current task, and what should I disable by default?"
                    .into(),
                color: Severity::Info.color_hint(),
            });
        }
    }

    if let Some(prompt_complexity) = ctx.prompt_complexity
        && prompt_complexity.available
        && prompt_complexity.avg_specificity_score < 45.0
    {
        recs.push(Recommendation {
            id: "prompt-specificity-low".into(),
            severity: Severity::Info,
            title: "Prompts are broad; specificity can improve".into(),
            description: format!(
                "Average specificity score is {:.0}/100. Missing file paths, exact errors, or line references forces wider repo scans.",
                prompt_complexity.avg_specificity_score
            ),
            estimated_savings: Some("~20–40% fewer exploratory tokens".into()),
            action: "When possible, include file paths, failing command output, and desired end-state in the first prompt.".into(),
            fix_prompt: "Show me how to rewrite my recent Claude Code prompts so they are more specific. Use file paths, failing commands, exact errors, and concrete acceptance criteria."
                .into(),
            color: Severity::Info.color_hint(),
        });
    }

    if let Some(session_health) = ctx.session_health
        && session_health.available
        && session_health.peak_overlap_pct > 40
    {
        recs.push(Recommendation {
            id: "peak-hour-overlap".into(),
            severity: Severity::Info,
            title: "A lot of work lands in throttled hours".into(),
            description: format!(
                "{}% of traced session traffic overlaps Anthropic peak hours.",
                session_health.peak_overlap_pct
            ),
            estimated_savings: Some("~30% longer session limits off-peak".into()),
            action: "Shift big refactors, test generation, or long debugging runs outside 5am–11am PT when possible.".into(),
            fix_prompt: "Help me reschedule the most expensive Claude Code work to off-peak hours. Which task types should I batch for later to preserve 5-hour limits?"
                .into(),
            color: Severity::Info.color_hint(),
        });
    }

    let long_sessions: usize = ctx
        .sessions
        .iter()
        .filter(|s| s.duration_secs > 7200)
        .count();
    if long_sessions >= 5 {
        recs.push(Recommendation {
            id: "long-sessions".into(),
            severity: Severity::Info,
            title: format!("{long_sessions} sessions exceed 2 hours"),
            description: "Very long sessions drift context and eat cache. Shorter, focused \
                sessions tend to use tokens more efficiently and compact less."
                .into(),
            estimated_savings: Some("10–25% fewer cache-write tokens".into()),
            action: "Break long pairings into feature-sized sessions. Use /compact or start fresh \
                when you shift topics."
                .into(),
            fix_prompt: "I keep hitting multi-hour Claude Code sessions. Suggest a workflow for \
                chunking my work into shorter focused sessions without losing thread between them \
                (e.g. summary files, per-feature branches, /compact cadence)."
                .into(),
            color: Severity::Info.color_hint(),
        });
    }

    let expensive: Vec<&HistoricalSession> = ctx
        .sessions
        .iter()
        .filter(|s| s.total_cost > 20.0)
        .collect();
    if !expensive.is_empty() {
        recs.push(Recommendation {
            id: "high-cost-sessions".into(),
            severity: Severity::Info,
            title: format!("{} high-cost session(s) (>$20)", expensive.len()),
            description: "A handful of sessions carry outsized cost. Worth knowing which ones and \
                why — they're usually either rabbit-hole debugging or repeated re-reads."
                .into(),
            estimated_savings: None,
            action: "Open the Sessions tab, sort by cost, and review the top 3 — the pattern is \
                usually obvious once you see them lined up."
                .into(),
            fix_prompt: "Analyze my 3 most expensive Claude Code sessions (you can find them in \
                ~/.claude/pulse-analytics.db or by sorting my JSONL sessions by total cost). \
                What made each one expensive, and what would have kept it cheaper?"
                .into(),
            color: Severity::Info.color_hint(),
        });
    }

    if recs.is_empty() {
        recs.push(Recommendation {
            id: "all-good".into(),
            severity: Severity::Positive,
            title: "No action items right now".into(),
            description: "Your cache, model routing, and session shape all look healthy.".into(),
            estimated_savings: None,
            action: "Keep going.".into(),
            fix_prompt: String::new(),
            color: Severity::Positive.color_hint(),
        });
    }

    recs
}
