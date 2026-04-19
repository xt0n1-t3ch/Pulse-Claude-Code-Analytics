use crate::db;

/// Render the analytics report as Markdown — suitable for pasting into a
/// GitHub issue, a Slack message, or a CC session. Sections mirror the HTML
/// report: cache grade, stats, top sessions, project + model breakdowns.
/// Render the analytics report as Markdown — suitable for pasting into a
/// GitHub issue, a Slack message, or a CC session. Sections mirror the HTML
/// report: cache grade, stats, top sessions, project + model breakdowns.
pub fn generate_markdown_report(days: Option<i64>, project: Option<&str>) -> String {
    use super::analyzers::{
        cache_health, inflection, model_routing, prompt_complexity, session_trace, tool_frequency,
    };
    use std::fmt::Write as _;

    let d = days.unwrap_or(30);
    let sessions = db::get_session_history(Some(d), project, Some(5000));
    let projects = db::get_project_stats(Some(d));
    let models = db::get_model_distribution(Some(d));
    let forecast = db::get_cost_forecast();
    let summary = db::get_analytics_summary();

    let total_sessions = sessions.len();
    let total_cost: f64 = sessions.iter().map(|s| s.total_cost).sum();
    let total_tokens: i64 = sessions.iter().map(|s| s.total_tokens).sum();

    let cache = cache_health::analyze(&sessions);
    let routing = model_routing::analyze(&sessions);
    let inflections = inflection::detect(&sessions);
    let traces = session_trace::load_session_traces(&sessions);
    let tool_frequency = tool_frequency::analyze(&sessions, &traces);
    let prompt_complexity = prompt_complexity::analyze(&sessions, &traces);

    let mut top_sessions: Vec<_> = sessions.iter().collect();
    top_sessions.sort_by(|a, b| {
        b.total_cost
            .partial_cmp(&a.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let period = project
        .map(|p| format!("{p} — last {d} days"))
        .unwrap_or_else(|| format!("All projects — last {d} days"));
    let generated = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");
    let md_escape = |value: &str| {
        value
            .replace('|', r"\|")
            .replace('\r', " ")
            .replace('\n', " ")
    };
    let truncate = |value: &str, max_chars: usize| {
        let mut out = String::new();
        for (idx, ch) in value.chars().enumerate() {
            if idx >= max_chars {
                out.push('…');
                break;
            }
            out.push(ch);
        }
        out
    };

    let mut md = String::new();
    writeln!(md, "# Pulse Analytics Report\n").unwrap();
    writeln!(md, "{period}\n").unwrap();
    writeln!(md, "Generated {generated}\n").unwrap();

    writeln!(md, "## Executive Summary\n").unwrap();
    writeln!(md, "| Metric | Value |\n|---|---|").unwrap();
    writeln!(md, "| Total cost | {} |", format_cost(total_cost)).unwrap();
    writeln!(md, "| Sessions | {} |", total_sessions).unwrap();
    writeln!(md, "| Tokens | {} |", format_tokens_short(total_tokens)).unwrap();
    writeln!(
        md,
        "| Cache grade | {} ({:.1}% weighted hit ratio) |",
        cache.grade, cache.trend_weighted_ratio
    )
    .unwrap();
    writeln!(
        md,
        "| Daily average | {} |",
        format_cost(forecast.daily_average)
    )
    .unwrap();
    writeln!(
        md,
        "| All-time tracked | {} sessions over {} days |\n",
        summary.total_sessions, summary.days_tracked
    )
    .unwrap();

    writeln!(md, "## Cache\n").unwrap();
    writeln!(md, "- Grade: {} ({})", cache.grade, cache.grade_label).unwrap();
    writeln!(
        md,
        "- Weighted hit ratio: {:.1}%",
        cache.trend_weighted_ratio
    )
    .unwrap();
    writeln!(md, "- Overall hit ratio: {:.1}%", cache.hit_ratio).unwrap();
    writeln!(
        md,
        "- Cache read tokens: {}",
        format_tokens_short(cache.total_cache_read)
    )
    .unwrap();
    writeln!(
        md,
        "- Cache write tokens: {}",
        format_tokens_short(cache.total_cache_write)
    )
    .unwrap();
    writeln!(
        md,
        "- Net input tokens: {}",
        format_tokens_short(cache.total_input)
    )
    .unwrap();
    writeln!(md, "- Diagnosis: {}\n", cache.diagnosis).unwrap();

    writeln!(md, "## Routing\n").unwrap();
    writeln!(md, "| Family | Sessions | Cost share | Avg cost/session | Total cost |\n|---|---:|---:|---:|---:|").unwrap();
    for (label, stats) in [
        ("Opus", &routing.opus),
        ("Sonnet", &routing.sonnet),
        ("Haiku", &routing.haiku),
        ("Other", &routing.other),
    ] {
        writeln!(
            md,
            "| {} | {} | {:.1}% | {} | {} |",
            label,
            stats.sessions,
            stats.cost_share_pct,
            format_cost(stats.avg_cost_per_session),
            format_cost(stats.cost)
        )
        .unwrap();
    }
    writeln!(md, "\nRouting diagnosis: {}", routing.diagnosis).unwrap();
    writeln!(
        md,
        "Estimated reroute savings: {}\n",
        format_cost(routing.estimated_savings_if_rerouted)
    )
    .unwrap();

    if !models.is_empty() {
        writeln!(md, "### Model Distribution\n").unwrap();
        writeln!(md, "| Model | Sessions | Cost |\n|---|---:|---:|").unwrap();
        for (name, count, cost) in &models {
            writeln!(
                md,
                "| {} | {} | {} |",
                md_escape(name),
                count,
                format_cost(*cost)
            )
            .unwrap();
        }
        writeln!(md).unwrap();
    }

    writeln!(md, "## Inflections\n").unwrap();
    if inflections.is_empty() {
        writeln!(
            md,
            "No major cost-per-session inflections detected in this window.\n"
        )
        .unwrap();
    } else {
        writeln!(md, "| Date | Direction | Delta | Sessions | Cost | Baseline | Note |\n|---|---|---:|---:|---:|---:|---|").unwrap();
        for point in inflections.iter().take(10) {
            let direction = if point.direction == "spike" {
                "Spike"
            } else {
                "Drop"
            };
            writeln!(
                md,
                "| {} | {} | {:.2}x | {} | {} | {} | {} |",
                point.date,
                direction,
                point.multiplier,
                point.sessions_on_day,
                format_cost(point.cost_on_day),
                format_cost(point.baseline_cost),
                md_escape(&point.note)
            )
            .unwrap();
        }
        writeln!(md).unwrap();
    }

    writeln!(md, "## Sessions\n").unwrap();
    if top_sessions.is_empty() {
        writeln!(md, "No sessions found in this window.\n").unwrap();
    } else {
        writeln!(
            md,
            "| # | Project | Model | Tokens | Duration | Cost |\n|---:|---|---|---:|---:|---:|"
        )
        .unwrap();
        for (idx, session) in top_sessions.iter().take(10).enumerate() {
            writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} |",
                idx + 1,
                md_escape(&session.project),
                md_escape(&session.model),
                format_tokens_short(session.total_tokens),
                format_duration(session.duration_secs),
                format_cost(session.total_cost)
            )
            .unwrap();
        }
        writeln!(md).unwrap();
    }
    if !projects.is_empty() {
        writeln!(md, "### Projects\n").unwrap();
        writeln!(md, "| Project | Sessions | Tokens | Avg Cost | Total Cost | Top Model |\n|---|---:|---:|---:|---:|---|").unwrap();
        for project in projects.iter().take(20) {
            writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} |",
                md_escape(&project.project),
                project.session_count,
                format_tokens_short(project.total_tokens),
                format_cost(project.avg_session_cost),
                format_cost(project.total_cost),
                md_escape(&project.top_model)
            )
            .unwrap();
        }
        writeln!(md).unwrap();
    }

    writeln!(md, "## Tools\n").unwrap();
    writeln!(
        md,
        "- Traced sessions: {} of {}",
        tool_frequency.traced_sessions, tool_frequency.sessions_analyzed
    )
    .unwrap();
    writeln!(
        md,
        "- Total tool calls: {}",
        tool_frequency.total_tool_calls
    )
    .unwrap();
    writeln!(
        md,
        "- Average tools/session: {:.1}",
        tool_frequency.avg_tools_per_session
    )
    .unwrap();
    writeln!(md, "- MCP share: {:.1}%", tool_frequency.mcp_share_pct).unwrap();
    writeln!(md, "- Diagnosis: {}\n", tool_frequency.diagnosis).unwrap();
    if tool_frequency.available && !tool_frequency.top_tools.is_empty() {
        writeln!(md, "| Tool | Calls | Share |\n|---|---:|---:|").unwrap();
        for tool in &tool_frequency.top_tools {
            writeln!(
                md,
                "| {} | {} | {:.1}% |",
                md_escape(&tool.name),
                tool.count,
                tool.share_pct
            )
            .unwrap();
        }
        writeln!(md).unwrap();
    }

    writeln!(md, "## Prompts\n").unwrap();
    writeln!(
        md,
        "- Prompts analyzed: {}",
        prompt_complexity.prompts_analyzed
    )
    .unwrap();
    writeln!(
        md,
        "- Average complexity: {:.1}",
        prompt_complexity.avg_complexity_score
    )
    .unwrap();
    writeln!(
        md,
        "- Average specificity: {:.1}",
        prompt_complexity.avg_specificity_score
    )
    .unwrap();
    writeln!(
        md,
        "- High-complexity sessions: {}",
        prompt_complexity.high_complexity_sessions
    )
    .unwrap();
    writeln!(
        md,
        "- Low-specificity sessions: {}",
        prompt_complexity.low_specificity_sessions
    )
    .unwrap();
    writeln!(md, "- Diagnosis: {}\n", prompt_complexity.diagnosis).unwrap();
    if prompt_complexity.available && !prompt_complexity.top_sessions.is_empty() {
        writeln!(
            md,
            "| Project | Complexity | Specificity | Label | Preview |\n|---|---:|---:|---|---|"
        )
        .unwrap();
        for session in prompt_complexity.top_sessions.iter().take(8) {
            writeln!(
                md,
                "| {} | {} | {} | {} | {} |",
                md_escape(&session.project),
                session.complexity_score,
                session.specificity_score,
                md_escape(session.label),
                md_escape(&truncate(&session.preview, 96))
            )
            .unwrap();
        }
        writeln!(md).unwrap();
    }

    writeln!(md, "---\n").unwrap();
    writeln!(md, "Generated by Pulse (cc-discord-presence)").unwrap();
    md
}

pub fn generate_html_report(days: Option<i64>, project: Option<&str>) -> String {
    use super::analyzers::{
        cache_health, inflection, model_routing, prompt_complexity, session_trace, tool_frequency,
    };
    use std::collections::BTreeMap;
    use std::fmt::Write as _;

    let d = days.unwrap_or(30);
    let sessions = db::get_session_history(Some(d), project, Some(5000));
    let daily = db::get_daily_stats(Some(d));
    let summary = db::get_analytics_summary();
    let projects = db::get_project_stats(Some(d));
    let forecast = db::get_cost_forecast();
    let hourly = db::get_hourly_activity(Some(d));
    let models = db::get_model_distribution(Some(d));

    let total_sessions = sessions.len();
    let total_cost: f64 = sessions.iter().map(|s| s.total_cost).sum();
    let total_tokens: i64 = sessions.iter().map(|s| s.total_tokens).sum();
    let total_input: i64 = sessions
        .iter()
        .map(|s| (s.input_tokens - s.cache_write_tokens - s.cache_read_tokens).max(0))
        .sum();
    let total_output: i64 = sessions.iter().map(|s| s.output_tokens).sum();
    let total_cache_w: i64 = sessions.iter().map(|s| s.cache_write_tokens).sum();
    let total_cache_r: i64 = sessions.iter().map(|s| s.cache_read_tokens).sum();

    let cache = cache_health::analyze(&sessions);
    let routing = model_routing::analyze(&sessions);
    let inflections = inflection::detect(&sessions);
    let traces = session_trace::load_session_traces(&sessions);
    let tool_frequency = tool_frequency::analyze(&sessions, &traces);
    let prompt_complexity = prompt_complexity::analyze(&sessions, &traces);

    let grade_color = match cache.grade {
        'A' | 'B' => "#22c55e",
        'C' => "#fbbf24",
        _ => "#ef4444",
    };
    let project_table_html = build_project_table(&projects);
    let model_table_html = build_model_table(&models, total_sessions);
    let top_sessions_html = build_top_sessions(&sessions);
    let hourly_heatmap_html = build_hourly_heatmap(&hourly);
    let recommendations = build_recommendations(&sessions);
    let _legacy_daily_chart = build_daily_chart_data(&daily);
    let _legacy_token_chart =
        build_token_chart_data(total_input, total_output, total_cache_w, total_cache_r);

    let mut by_date: BTreeMap<String, f64> = BTreeMap::new();
    for day in &daily {
        *by_date.entry(day.date.clone()).or_default() += day.total_cost;
    }
    let daily_labels = by_date
        .keys()
        .map(|d| format!("'{d}'"))
        .collect::<Vec<_>>()
        .join(",");
    let daily_values = by_date
        .values()
        .map(|v| format!("{v:.2}"))
        .collect::<Vec<_>>()
        .join(",");

    let mut routing_rows_html = String::new();
    for (label, stats) in [
        ("Opus", &routing.opus),
        ("Sonnet", &routing.sonnet),
        ("Haiku", &routing.haiku),
        ("Other", &routing.other),
    ] {
        let width = stats.cost_share_pct.clamp(0.0, 100.0);
        write!(routing_rows_html, r##"<div class="routing-row"><div class="routing-label-row"><span class="routing-name">{label}</span><span class="routing-share">{share:.1}%</span></div><div class="routing-meta">{sessions} sessions · {avg_cost} avg · {cost}</div><div class="routing-track"><div class="routing-fill" style="width:{width:.1}%"></div></div></div>"##, label = html_escape(label), share = stats.cost_share_pct, sessions = stats.sessions, avg_cost = format_cost(stats.avg_cost_per_session), cost = html_escape(&format_cost(stats.cost)), width = width).unwrap();
    }

    let mut inflections_html = String::new();
    if inflections.is_empty() {
        inflections_html.push_str(r#"<div class="empty-state">No major cost-per-session inflections detected in this window.</div>"#);
    } else {
        inflections_html.push_str(r#"<div class="inflection-grid">"#);
        for point in inflections.iter().take(6) {
            let direction_class = if point.direction == "spike" {
                "inflection-up"
            } else {
                "inflection-down"
            };
            let direction_label = if point.direction == "spike" {
                "Up"
            } else {
                "Down"
            };
            write!(inflections_html, r##"<article class="inflection-card {direction_class}"><div class="inflection-head"><span class="inflection-date">{date}</span><span class="inflection-direction">{direction_label}</span></div><div class="inflection-metric">{multiplier:.2}×</div><div class="inflection-support">{sessions} sessions · {cost}</div><p>{note}</p></article>"##, direction_class = direction_class, date = html_escape(&point.date), direction_label = direction_label, multiplier = point.multiplier, sessions = point.sessions_on_day, cost = html_escape(&format_cost(point.cost_on_day)), note = html_escape(&point.note)).unwrap();
        }
        inflections_html.push_str("</div>");
    }
    let mut tools_table_html = String::new();
    if tool_frequency.available && !tool_frequency.top_tools.is_empty() {
        tools_table_html.push_str(r#"<div class="card"><h2>Top Tools</h2><table><tr><th>Tool</th><th>Calls</th><th>Share</th></tr>"#);
        for tool in &tool_frequency.top_tools {
            write!(
                tools_table_html,
                r#"<tr><td>{}</td><td class="num">{}</td><td class="num">{:.1}%</td></tr>"#,
                html_escape(&tool.name),
                tool.count,
                tool.share_pct
            )
            .unwrap();
        }
        tools_table_html.push_str("</table></div>");
    } else {
        tools_table_html.push_str(r#"<div class="card"><h2>Top Tools</h2><div class="empty-state">No JSONL tool traces available yet.</div></div>"#);
    }

    let truncate = |value: &str, max_chars: usize| {
        let mut out = String::new();
        for (idx, ch) in value.chars().enumerate() {
            if idx >= max_chars {
                out.push('…');
                break;
            }
            out.push(ch);
        }
        out
    };
    let mut prompt_table_html = String::new();
    if prompt_complexity.available && !prompt_complexity.top_sessions.is_empty() {
        prompt_table_html.push_str(r#"<div class="card"><h2>Most Complex Prompts</h2><table><tr><th>Project</th><th>Complexity</th><th>Specificity</th><th>Label</th><th>Preview</th></tr>"#);
        for session in prompt_complexity.top_sessions.iter().take(8) {
            write!(prompt_table_html, r#"<tr><td>{}</td><td class="num">{}</td><td class="num">{}</td><td>{}</td><td class="preview-cell">{}</td></tr>"#, html_escape(&session.project), session.complexity_score, session.specificity_score, html_escape(session.label), html_escape(&truncate(&session.preview, 140))).unwrap();
        }
        prompt_table_html.push_str("</table></div>");
    } else {
        prompt_table_html.push_str(r#"<div class="card"><h2>Most Complex Prompts</h2><div class="empty-state">No prompt previews available yet.</div></div>"#);
    }

    let generated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let period_label = if let Some(p) = project {
        format!("{p} — Last {d} days")
    } else {
        format!("All Projects — Last {d} days")
    };
    let daily_chart_script = format!(
        r##"const dailyLabels=[{labels}];const dailyValues=[{values}];const dailyPointColors=dailyValues.map((value,index)=>{{if(index===0)return '#7cb9e8';const prev=dailyValues[index-1];if(value>prev)return '#ef4444';if(value<prev)return '#22c55e';return '#7cb9e8';}});new Chart(document.getElementById('dailyCostChart'),{{type:'line',data:{{labels:dailyLabels,datasets:[{{data:dailyValues,borderColor:'#f5f5f5',backgroundColor:'rgba(245,245,245,0.08)',fill:true,tension:0.32,borderWidth:2,pointRadius:3,pointHoverRadius:5,pointBackgroundColor:dailyPointColors,pointBorderColor:dailyPointColors,pointBorderWidth:0}}]}},options:{{responsive:true,maintainAspectRatio:false,plugins:{{legend:{{display:false}}}},scales:{{x:{{grid:{{color:'#1f1f1f',drawBorder:false}},ticks:{{color:'#6b6b6b'}}}},y:{{grid:{{color:'#1f1f1f',drawBorder:false}},ticks:{{color:'#6b6b6b',callback:(value)=>'$'+Number(value).toFixed(2)}}}}}}}}}});"##,
        labels = daily_labels,
        values = daily_values
    );
    let token_chart_script = format!(
        r##"new Chart(document.getElementById('tokenChart'),{{type:'doughnut',data:{{labels:['Pure Input','Output','Cache Write','Cache Read'],datasets:[{{data:[{input},{output},{cache_w},{cache_r}],backgroundColor:['#f5f5f5','#7cb9e8','#fbbf24','#22c55e'],borderColor:'#0b0b0b',borderWidth:2,hoverOffset:8}}]}},options:{{responsive:true,maintainAspectRatio:false,cutout:'62%',animation:{{animateRotate:true,animateScale:true,duration:900,easing:'easeOutQuart'}},plugins:{{legend:{{display:false}},tooltip:{{backgroundColor:'rgba(11,11,11,0.95)',borderColor:'#1f1f1f',borderWidth:1,titleColor:'#fafafa',bodyColor:'#a0a0a0',padding:12,callbacks:{{label:function(ctx){{var v=ctx.parsed;var s=(v>=1e6)?(v/1e6).toFixed(1)+'M':(v>=1e3)?(v/1e3).toFixed(1)+'K':v.toString();var t=ctx.dataset.data.reduce(function(a,b){{return a+b;}},0);var p=t>0?(v/t*100).toFixed(1):'0.0';return ' '+ctx.label+': '+s+' ('+p+'%)';}}}}}}}}}}}});"##,
        input = total_input,
        output = total_output,
        cache_w = total_cache_w,
        cache_r = total_cache_r
    );

    let mut html = String::new();
    html.push_str(r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Pulse Analytics Report</title>
<script src="https://cdn.jsdelivr.net/npm/chart.js@4"></script>
<style>
@import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800&family=JetBrains+Mono:wght@400;500;600&display=swap');
/* Pulse Report — matches the GUI design system: ultra-black Inter + JetBrains Mono.
   Auto-follows prefers-color-scheme; user can also click the theme toggle. */
:root {
  color-scheme: dark;
  --bg: #000000;
  --bg-secondary: #050505;
  --bg-card: #0b0b0b;
  --bg-card-hover: #121212;
  --bg-elevated: #141414;
  --border: #1f1f1f;
  --border-hover: #2a2a2a;
  --border-strong: #333333;
  --text-primary: #fafafa;
  --text-secondary: #a0a0a0;
  --text-muted: #6b6b6b;
  --success: #22c55e;
  --success-dim: rgba(34,197,94,0.10);
  --warning: #f59e0b;
  --warning-dim: rgba(245,158,11,0.12);
  --danger: #ef4444;
  --danger-dim: rgba(239,68,68,0.12);
  --info: #7cb9e8;
  --info-dim: rgba(124,185,232,0.12);
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 10px;
  --radius-full: 9999px;
  --font-sans: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
  --font-mono: 'JetBrains Mono', 'SF Mono', Menlo, Consolas, monospace;
  --ease: cubic-bezier(0.4, 0, 0.2, 1);
}
[data-theme="light"] {
  color-scheme: light;
  --bg: #ffffff;
  --bg-secondary: #fafafa;
  --bg-card: #ffffff;
  --bg-card-hover: #f7f7f7;
  --bg-elevated: #f1f1f1;
  --border: #eaeaea;
  --border-hover: #d4d4d4;
  --border-strong: #b8b8b8;
  --text-primary: #0a0a0a;
  --text-secondary: #4a4a4a;
  --text-muted: #8a8a8a;
  --success: #15803d;
  --warning: #b45309;
  --danger: #b91c1c;
  --info: #1d4ed8;
}
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) {
    color-scheme: light;
    --bg: #ffffff; --bg-secondary: #fafafa; --bg-card: #ffffff;
    --bg-card-hover: #f7f7f7; --bg-elevated: #f1f1f1;
    --border: #eaeaea; --border-hover: #d4d4d4; --border-strong: #b8b8b8;
    --text-primary: #0a0a0a; --text-secondary: #4a4a4a; --text-muted: #8a8a8a;
    --success: #15803d; --warning: #b45309; --danger: #b91c1c; --info: #1d4ed8;
  }
}
*, *::before, *::after { margin: 0; padding: 0; box-sizing: border-box; }
html { scroll-behavior: smooth; }
body {
  background: var(--bg); color: var(--text-primary);
  font-family: var(--font-sans); font-size: 14px; line-height: 1.5;
  padding: 40px 24px 64px;
  -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale;
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'cv02','cv03','cv04','cv11';
}
.report-shell { max-width: 1240px; margin: 0 auto; }
a { color: inherit; }

/* theme toggle */
.theme-toggle {
  position: fixed; top: 16px; right: 16px; z-index: 100;
  width: 36px; height: 36px;
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius-md);
  display: inline-flex; align-items: center; justify-content: center;
  cursor: pointer; color: var(--text-secondary);
  transition: background .15s var(--ease), border-color .15s var(--ease), color .15s var(--ease);
}
.theme-toggle:hover { background: var(--bg-card-hover); border-color: var(--border-hover); color: var(--text-primary); }
.theme-toggle svg { width: 16px; height: 16px; display: block; }
.theme-toggle .icon-sun { display: none; }
[data-theme="light"] .theme-toggle .icon-sun { display: block; }
[data-theme="light"] .theme-toggle .icon-moon { display: none; }
@media (prefers-color-scheme: light) {
  :root:not([data-theme]) .theme-toggle .icon-sun { display: block; }
  :root:not([data-theme]) .theme-toggle .icon-moon { display: none; }
}

/* kicker label */
.kicker,.summary-label,.info-label,.metric .label,.heat-label,.routing-share {
  font-family: var(--font-mono); font-size: 10px; font-weight: 600;
  letter-spacing: 0.12em; text-transform: uppercase; color: var(--text-muted);
}

/* hero */
.hero { padding: 8px 0 20px; }
.hero-top { display: flex; justify-content: space-between; align-items: flex-start; gap: 24px; margin-bottom: 4px; }
.hero h1 {
  font-family: var(--font-sans); font-weight: 700;
  font-size: clamp(32px, 4.2vw, 44px); line-height: 1.05;
  letter-spacing: -0.025em; color: var(--text-primary); margin: 6px 0 4px;
}
.hero-meta { color: var(--text-secondary); font-size: 14px; margin-top: 2px; }
.generated-at { font-family: var(--font-mono); font-size: 10px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--text-muted); }
.hero-divider { height: 1px; background: var(--border); margin: 20px 0 24px; }

/* summary grid */
.summary-grid {
  display: grid; grid-template-columns: repeat(5, minmax(0,1fr));
  gap: 10px;
}
.summary-card {
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius-lg); padding: 18px 18px 16px;
  transition: border-color .15s var(--ease), background .15s var(--ease);
}
.summary-card:hover { border-color: var(--border-hover); background: var(--bg-card-hover); }
.summary-value {
  font-family: var(--font-sans); font-weight: 700;
  font-size: clamp(22px, 2.2vw, 28px); letter-spacing: -0.02em;
  color: var(--text-primary); margin: 10px 0 4px;
  font-variant-numeric: tabular-nums; line-height: 1.1;
}
.summary-meta { color: var(--text-muted); font-size: 11px; line-height: 1.4; font-family: var(--font-mono); }

/* anchor nav (sticky) */
.anchor-nav {
  position: sticky; top: 0; z-index: 20;
  display: flex; flex-wrap: wrap; gap: 0;
  margin: 28px 0 24px; padding: 0;
  background: color-mix(in srgb, var(--bg) 88%, transparent);
  backdrop-filter: blur(10px); -webkit-backdrop-filter: blur(10px);
  border-top: 1px solid var(--border); border-bottom: 1px solid var(--border);
}
.anchor-nav a {
  padding: 12px 16px; color: var(--text-muted);
  font-family: var(--font-mono); font-size: 10px; font-weight: 600;
  text-transform: uppercase; letter-spacing: 0.12em;
  text-decoration: none;
  border-right: 1px solid var(--border);
  transition: color .15s var(--ease), background .15s var(--ease);
}
.anchor-nav a:hover { color: var(--text-primary); background: var(--bg-card); }

/* sections */
.section { margin-bottom: 48px; }
.section-header {
  display: flex; justify-content: space-between; gap: 20px;
  align-items: flex-end; margin-bottom: 16px;
  padding-bottom: 12px; border-bottom: 1px solid var(--border);
}
.section-header h2 {
  font-family: var(--font-sans); font-weight: 700;
  font-size: clamp(20px, 2.2vw, 26px); letter-spacing: -0.02em;
  color: var(--text-primary); margin: 0;
}
.section-header p { margin: 6px 0 0; color: var(--text-secondary); font-size: 13px; max-width: 64ch; }
.section-grid { display: grid; grid-template-columns: repeat(2, minmax(0,1fr)); gap: 10px; }
.info-grid { display: grid; grid-template-columns: repeat(4, minmax(0,1fr)); gap: 10px; }
.metric-strip { display: grid; grid-template-columns: repeat(3, minmax(0,1fr)); gap: 10px; margin-top: 16px; }

/* cards */
.card,.info-card {
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius-lg); padding: 22px;
  transition: border-color .15s var(--ease), background .15s var(--ease);
}
.card:hover,.info-card:hover { border-color: var(--border-hover); }
.card > h2, .card > h3, .info-card > h2, .info-card > h3 {
  font-family: var(--font-mono); font-size: 10px; font-weight: 600;
  letter-spacing: 0.12em; text-transform: uppercase;
  color: var(--text-muted); margin: 0 0 14px;
}

/* metric block (inside metric-strip) */
.metric {
  background: var(--bg-secondary); border: 1px solid var(--border);
  border-radius: var(--radius-md); padding: 14px 16px;
}
.metric .label { display: block; margin-bottom: 6px; }
.metric .value {
  font-family: var(--font-sans); font-weight: 700;
  font-size: 18px; color: var(--text-primary);
  font-variant-numeric: tabular-nums; letter-spacing: -0.01em;
}

/* cache grade */
.cache-grade { display: flex; gap: 22px; align-items: center; margin-bottom: 16px; }
.cache-letter {
  font-family: var(--font-sans); font-weight: 800;
  font-size: clamp(72px, 9vw, 108px); line-height: 0.9;
  letter-spacing: -0.06em;
}
.cache-copy h3 {
  font-family: var(--font-mono); font-size: 10px; font-weight: 600;
  letter-spacing: 0.12em; text-transform: uppercase;
  color: var(--text-muted); margin-bottom: 4px;
}
.cache-copy .ratio {
  font-family: var(--font-sans); font-weight: 700;
  font-size: 26px; color: var(--text-primary); letter-spacing: -0.02em;
}
.cache-copy p { color: var(--text-secondary); font-size: 13px; margin-top: 4px; max-width: 48ch; }

/* chart */
.chart-card canvas { width: 100% !important; height: 240px !important; }
.token-legend { list-style: none; padding: 0; margin: 14px 0 0 0; display: grid; grid-template-columns: repeat(2, minmax(0,1fr)); gap: 8px 18px; font-family: var(--font-mono); font-size: 11px; color: var(--text-secondary); letter-spacing: 0.02em; }
.token-legend li { display: flex; align-items: center; gap: 8px; }
.token-legend li b { margin-left: auto; color: var(--text-primary); font-weight: 600; }
.token-legend .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

/* routing rows */
.routing-row + .routing-row { margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border); }
.routing-label-row { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; }
.routing-name {
  font-family: var(--font-sans); font-weight: 600;
  font-size: 15px; color: var(--text-primary); letter-spacing: -0.01em;
}
.routing-meta { margin-top: 3px; color: var(--text-muted); font-size: 11px; font-family: var(--font-mono); }
.routing-track { height: 3px; margin-top: 10px; background: var(--border); border-radius: var(--radius-sm); overflow: hidden; }
.routing-fill { height: 100%; background: var(--text-primary); transition: width 1.2s cubic-bezier(.2,.9,.3,1); }

/* inflections */
.inflection-grid { display: grid; grid-template-columns: repeat(3, minmax(0,1fr)); gap: 10px; }
.inflection-card {
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius-lg); padding: 18px; position: relative;
  transition: border-color .15s var(--ease);
}
.inflection-card:hover { border-color: var(--border-hover); }
.inflection-card::before {
  content: ''; position: absolute; top: 0; left: 18px;
  width: 32px; height: 2px; border-radius: 2px;
}
.inflection-up::before { background: var(--danger); }
.inflection-down::before { background: var(--success); }
.inflection-head { display: flex; justify-content: space-between; gap: 10px; align-items: baseline; }
.inflection-date {
  font-family: var(--font-mono); font-size: 10px; color: var(--text-muted);
  letter-spacing: 0.12em; text-transform: uppercase;
}
.inflection-direction {
  font-family: var(--font-mono); font-size: 9px; font-weight: 600;
  letter-spacing: 0.14em; text-transform: uppercase;
  padding: 3px 8px; border-radius: var(--radius-full);
}
.inflection-up .inflection-direction { background: var(--danger-dim); color: var(--danger); }
.inflection-down .inflection-direction { background: var(--success-dim); color: var(--success); }
.inflection-metric {
  font-family: var(--font-sans); font-weight: 700; font-size: 26px;
  color: var(--text-primary); letter-spacing: -0.02em; margin-top: 8px;
  font-variant-numeric: tabular-nums;
}
.inflection-support { font-family: var(--font-mono); font-size: 11px; color: var(--text-muted); margin-top: 2px; }
.inflection-card p { color: var(--text-secondary); font-size: 12px; margin-top: 8px; line-height: 1.5; }

/* info card value */
.info-value {
  font-family: var(--font-sans); font-weight: 700;
  font-size: clamp(20px, 2vw, 26px); color: var(--text-primary);
  letter-spacing: -0.02em; margin-top: 6px;
  font-variant-numeric: tabular-nums;
}
.info-card p { color: var(--text-secondary); font-size: 12px; margin-top: 4px; }

/* tables */
table { width: 100%; border-collapse: collapse; }
th, td {
  padding: 10px 12px; border-bottom: 1px solid var(--border);
  text-align: left; vertical-align: top; font-size: 13px;
  font-family: var(--font-sans);
}
th {
  font-family: var(--font-mono); font-size: 10px; font-weight: 600;
  letter-spacing: 0.12em; text-transform: uppercase;
  color: var(--text-muted);
  border-bottom: 1px solid var(--border-hover);
}
td { color: var(--text-secondary); font-variant-numeric: tabular-nums; }
tr:hover td { color: var(--text-primary); }
tr:last-child td { border-bottom: none; }
.num, .cost { text-align: right; font-family: var(--font-mono); }
.cost { color: var(--text-primary); font-weight: 600; }
.preview-cell {
  max-width: 420px; color: var(--text-secondary);
  font-size: 12px; line-height: 1.5;
  font-family: var(--font-sans);
}

/* heatmap */
.heatmap { display: grid; grid-template-columns: repeat(6, minmax(0,1fr)); gap: 4px; }
.heat-cell {
  background: var(--bg-card); border: 1px solid var(--border);
  border-radius: var(--radius-md); padding: 12px;
  background-image: linear-gradient(180deg, rgba(34,197,94, calc(var(--heat,0) * .35)) 0%, transparent 100%);
}
.heat-label { display: block; }
.heat-value {
  font-family: var(--font-sans); font-weight: 700;
  font-size: 16px; color: var(--text-primary); margin-top: 4px;
  font-variant-numeric: tabular-nums; letter-spacing: -0.01em;
}
.heat-meta { color: var(--text-muted); font-size: 10px; font-family: var(--font-mono); }

/* empty state */
.empty-state {
  padding: 28px; background: var(--bg-card);
  border: 1px dashed var(--border-hover);
  border-radius: var(--radius-lg);
  color: var(--text-secondary); font-size: 13px; text-align: center;
  font-family: var(--font-sans);
}

/* recommendations */
.rec-list { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 10px; }
.rec-item {
  background: var(--bg-card); border: 1px solid var(--border);
  border-left: 3px solid var(--text-muted);
  border-radius: var(--radius-lg); padding: 18px 22px;
  transition: border-color .15s var(--ease), background .15s var(--ease);
}
.rec-item:hover { border-color: var(--border-hover); }
.rec-item[data-sev="critical"] { border-left-color: var(--danger); }
.rec-item[data-sev="warning"]  { border-left-color: var(--warning); }
.rec-item[data-sev="info"]     { border-left-color: var(--info); }
.rec-item[data-sev="positive"] { border-left-color: var(--success); }
.rec-head { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; margin-bottom: 6px; }
.rec-pill {
  padding: 3px 8px; border-radius: var(--radius-full);
  font-family: var(--font-mono); font-size: 9px; font-weight: 600;
  letter-spacing: 0.14em; text-transform: uppercase;
}
.rec-pill.critical { background: var(--danger-dim); color: var(--danger); }
.rec-pill.warning  { background: var(--warning-dim); color: var(--warning); }
.rec-pill.info     { background: var(--info-dim); color: var(--info); }
.rec-pill.positive { background: var(--success-dim); color: var(--success); }
.rec-title {
  font-family: var(--font-sans); font-weight: 600;
  font-size: 15px; color: var(--text-primary); letter-spacing: -0.01em;
}
.rec-desc { color: var(--text-secondary); font-size: 13px; line-height: 1.55; margin-top: 4px; }
.rec-meta { margin-top: 10px; display: flex; gap: 14px; flex-wrap: wrap; font-size: 11px; font-family: var(--font-mono); }
.meta-k { color: var(--text-muted); letter-spacing: 0.10em; text-transform: uppercase; }
.meta-v { color: var(--text-secondary); }
.meta-v.accent { color: var(--text-primary); font-weight: 600; }
.rec-fix {
  margin-top: 12px; padding: 7px 14px;
  background: var(--bg-elevated); border: 1px solid var(--border-hover);
  border-radius: var(--radius-sm);
  color: var(--text-primary);
  font-family: var(--font-mono); font-size: 10px; font-weight: 600;
  letter-spacing: 0.12em; text-transform: uppercase;
  cursor: pointer; transition: all .15s var(--ease);
}
.rec-fix:hover { background: var(--text-primary); color: var(--bg); border-color: var(--text-primary); }
.rec-fix.copied { background: var(--success); color: #ffffff; border-color: var(--success); }

/* footer */
.footer {
  margin-top: 40px; padding: 22px 0; border-top: 1px solid var(--border);
  font-family: var(--font-mono); font-size: 11px; color: var(--text-muted);
  letter-spacing: 0.04em;
}
.footer a { color: var(--text-primary); text-decoration: none; border-bottom: 1px solid var(--border-hover); transition: border-color .15s var(--ease); }
.footer a:hover { border-bottom-color: var(--text-primary); }

/* responsive */
@media (max-width: 1100px) {
  .summary-grid, .info-grid { grid-template-columns: repeat(2, minmax(0,1fr)); }
  .section-grid, .inflection-grid { grid-template-columns: 1fr; }
  .heatmap { grid-template-columns: repeat(3, minmax(0,1fr)); }
}
@media (max-width: 760px) {
  body { padding: 24px 16px 40px; }
  .hero-top, .section-header, .routing-label-row { flex-direction: column; align-items: flex-start; gap: 8px; }
  .summary-grid, .metric-strip { grid-template-columns: 1fr; }
  .cache-grade { flex-direction: column; gap: 14px; align-items: flex-start; }
  table { display: block; overflow-x: auto; white-space: nowrap; }
}

/* print */
@media print {
  :root {
    color-scheme: light;
    --bg: #ffffff; --bg-secondary: #fafafa; --bg-card: #ffffff; --bg-card-hover: #f5f5f5;
    --bg-elevated: #f1f1f1; --border: #d4d4d4; --border-hover: #b0b0b0;
    --text-primary: #0a0a0a; --text-secondary: #333; --text-muted: #666;
  }
  body { padding: 0; }
  .anchor-nav, .rec-fix, .theme-toggle, .screen-only { display: none !important; }
  .section { break-inside: avoid; }
  .hero { border-top: 2px solid #000; padding-top: 16px; }
}
</style>
<script>(function(){try{var t=localStorage.getItem('pulse-report-theme');if(t==='light'||t==='dark'){document.documentElement.setAttribute('data-theme',t);}}catch(e){}})();</script>
</head>
<body>
<button class="theme-toggle screen-only" onclick="(function(){var r=document.documentElement;var next=(r.getAttribute('data-theme')==='light')?'dark':(r.getAttribute('data-theme')==='dark')?'light':(matchMedia('(prefers-color-scheme: light)').matches?'dark':'light');r.setAttribute('data-theme',next);try{localStorage.setItem('pulse-report-theme',next);}catch(e){}})()" aria-label="Toggle theme" title="Toggle dark/light theme">
  <svg class="icon-moon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
  <svg class="icon-sun" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/></svg>
</button>
<div class="report-shell">"##);
    write!(html, r##"<header class="hero"><div class="hero-top"><div><div class="kicker">Pulse · Claude Code Analytics</div><h1>Analytics Report</h1><div class="hero-meta">{period_label}</div></div><div class="generated-at">Generated {generated_at}</div></div><div class="hero-divider"></div><div class="summary-grid"><div class="summary-card"><div class="summary-label">Total Cost</div><div class="summary-value">{total_cost}</div><div class="summary-meta">{period_label}</div></div><div class="summary-card"><div class="summary-label">Sessions</div><div class="summary-value">{total_sessions}</div><div class="summary-meta">Tracked in current window</div></div><div class="summary-card"><div class="summary-label">Tokens</div><div class="summary-value">{total_tokens}</div><div class="summary-meta">Input + output + cache</div></div><div class="summary-card"><div class="summary-label">Cache Grade</div><div class="summary-value" style="color:{grade_color}">{cache_grade}</div><div class="summary-meta">{cache_ratio:.1}% weighted hit ratio</div></div><div class="summary-card"><div class="summary-label">Daily Average</div><div class="summary-value">{daily_avg}</div><div class="summary-meta">Projected month {projected_monthly}</div></div></div></header>"##, period_label = html_escape(&period_label), generated_at = html_escape(&generated_at), total_cost = html_escape(&format_cost(total_cost)), total_sessions = total_sessions, total_tokens = html_escape(&format_tokens_short(total_tokens)), grade_color = grade_color, cache_grade = cache.grade, cache_ratio = cache.trend_weighted_ratio, daily_avg = html_escape(&format_cost(forecast.daily_average)), projected_monthly = html_escape(&format_cost(forecast.projected_monthly))).unwrap();
    html.push_str(r##"<nav class="anchor-nav screen-only"><a href="#cache">Cache</a><a href="#routing">Routing</a><a href="#inflections">Inflections</a><a href="#sessions">Sessions</a><a href="#tools">Tools</a><a href="#prompts">Prompts</a></nav>"##);
    write!(html, r##"<section id="cache" class="section"><div class="section-header"><div><h2>Cache</h2><p>Weighted cache health drives grade color. Token mix stays visible for fast copy-paste review.</p></div></div><div class="section-grid"><div class="card"><div class="cache-grade"><div class="cache-letter" style="color:{grade_color}">{cache_grade}</div><div class="cache-copy"><h3>Cache Health</h3><div class="ratio">{cache_ratio:.1}%</div><p>{cache_diagnosis}</p></div></div><div class="metric-strip"><div class="metric"><div class="label">Overall Hit Ratio</div><div class="value">{overall_ratio:.1}%</div></div><div class="metric"><div class="label">Cache Read</div><div class="value">{cache_read}</div></div><div class="metric"><div class="label">Cache Write</div><div class="value">{cache_write}</div></div></div></div><div class="card chart-card"><h2>Token Composition</h2><canvas id="tokenChart"></canvas><ul class="token-legend"><li><span class="dot" style="background:#f5f5f5"></span>Pure Input<b>{pure_input_short}</b></li><li><span class="dot" style="background:#7cb9e8"></span>Output<b>{output_short}</b></li><li><span class="dot" style="background:#fbbf24"></span>Cache Write<b>{cache_w_short}</b></li><li><span class="dot" style="background:#22c55e"></span>Cache Read<b>{cache_r_short}</b></li></ul></div></div></section>"##, grade_color = grade_color, cache_grade = cache.grade, cache_ratio = cache.trend_weighted_ratio, cache_diagnosis = html_escape(&cache.diagnosis), overall_ratio = cache.hit_ratio, cache_read = html_escape(&format_tokens_short(cache.total_cache_read)), cache_write = html_escape(&format_tokens_short(cache.total_cache_write)), pure_input_short = html_escape(&format_tokens_short(total_input)), output_short = html_escape(&format_tokens_short(total_output)), cache_w_short = html_escape(&format_tokens_short(total_cache_w)), cache_r_short = html_escape(&format_tokens_short(total_cache_r))).unwrap();
    write!(html, r##"<section id="routing" class="section"><div class="section-header"><div><h2>Routing</h2><p>Family-level spend split. Bars stay monochrome. Diagnosis stays textual for export parity.</p></div></div><div class="section-grid"><div class="card"><h2>Family Spend</h2>{routing_rows}<div class="metric-strip"><div class="metric"><div class="label">Sessions</div><div class="value">{routing_sessions}</div></div><div class="metric"><div class="label">Spend</div><div class="value">{routing_cost}</div></div><div class="metric"><div class="label">Potential Savings</div><div class="value">{routing_savings}</div></div></div><p style="margin-top:18px;">{routing_diagnosis}</p></div>{model_table_html}</div></section>"##, routing_rows = routing_rows_html, routing_sessions = routing.total_sessions, routing_cost = html_escape(&format_cost(routing.total_cost)), routing_savings = html_escape(&format_cost(routing.estimated_savings_if_rerouted)), routing_diagnosis = html_escape(&routing.diagnosis), model_table_html = model_table_html).unwrap();
    write!(html, r##"<section id="inflections" class="section"><div class="section-header"><div><h2>Inflections</h2><p>Spike cards use red rail. Efficiency drops use green rail. Sorted by absolute signal strength.</p></div></div>{inflections_html}</section>"##, inflections_html = inflections_html).unwrap();
    write!(html, r##"<section id="sessions" class="section"><div class="section-header"><div><h2>Sessions</h2><p>Daily cost trend, hourly activity, top sessions, project mix. Same data sources. Cleaner export.</p></div></div><div class="section-grid"><div class="card chart-card"><h2>Daily Cost Trend</h2><canvas id="dailyCostChart"></canvas></div><div class="card"><h2>Hourly Activity</h2>{hourly_heatmap_html}</div></div><div class="section-grid" style="margin-top:18px;">{top_sessions_html}{project_table_html}</div></section>"##, hourly_heatmap_html = hourly_heatmap_html, top_sessions_html = top_sessions_html, project_table_html = project_table_html).unwrap();
    write!(html, r##"<section id="tools" class="section"><div class="section-header"><div><h2>Tools</h2><p>Tool intensity, MCP share, compact gaps, top tool mix.</p></div></div><div class="info-grid"><div class="info-card"><div class="info-label">Traced Sessions</div><div class="info-value">{traced_sessions}</div><p>{sessions_analyzed} sessions analyzed</p></div><div class="info-card"><div class="info-label">Total Tool Calls</div><div class="info-value">{tool_calls}</div><p>{tools_per_session:.1} avg per session</p></div><div class="info-card"><div class="info-label">Calls / Hour</div><div class="info-value">{calls_per_hour:.1}</div><p>{mcp_share:.1}% MCP share</p></div><div class="info-card"><div class="info-label">Compact Gaps</div><div class="info-value">{compact_gaps}</div><p>{tool_diagnosis}</p></div></div><div style="margin-top:18px;">{tools_table_html}</div></section>"##, traced_sessions = tool_frequency.traced_sessions, sessions_analyzed = tool_frequency.sessions_analyzed, tool_calls = tool_frequency.total_tool_calls, tools_per_session = tool_frequency.avg_tools_per_session, calls_per_hour = tool_frequency.avg_tool_calls_per_hour, mcp_share = tool_frequency.mcp_share_pct, compact_gaps = tool_frequency.compact_gap_sessions, tool_diagnosis = html_escape(&tool_frequency.diagnosis), tools_table_html = tools_table_html).unwrap();
    write!(html, r##"<section id="prompts" class="section"><div class="section-header"><div><h2>Prompts</h2><p>Prompt complexity stays copyable. Preview column trims long prompts without hiding signal.</p></div></div><div class="info-grid"><div class="info-card"><div class="info-label">Prompts Analyzed</div><div class="info-value">{prompts_analyzed}</div><p>{prompt_sessions} sessions scanned</p></div><div class="info-card"><div class="info-label">Avg Complexity</div><div class="info-value">{avg_complexity:.1}</div><p>{high_complexity} high-complexity sessions</p></div><div class="info-card"><div class="info-label">Avg Specificity</div><div class="info-value">{avg_specificity:.1}</div><p>{low_specificity} low-specificity sessions</p></div><div class="info-card"><div class="info-label">Diagnosis</div><div class="info-value">{prompt_label}</div><p>{prompt_diagnosis}</p></div></div><div style="margin-top:18px;">{prompt_table_html}</div></section>"##, prompts_analyzed = prompt_complexity.prompts_analyzed, prompt_sessions = prompt_complexity.sessions_analyzed, avg_complexity = prompt_complexity.avg_complexity_score, high_complexity = prompt_complexity.high_complexity_sessions, avg_specificity = prompt_complexity.avg_specificity_score, low_specificity = prompt_complexity.low_specificity_sessions, prompt_label = if prompt_complexity.available { "Live" } else { "Pending" }, prompt_diagnosis = html_escape(&prompt_complexity.diagnosis), prompt_table_html = prompt_table_html).unwrap();
    write!(
        html,
        r##"<section class="section"><div class="section-header"><div><h2>Recommendations</h2><p>Rule-based fixes from the Pulse recommendations engine. Every item has a "Copy Fix Prompt" button — paste the prompt into Claude Code to remediate.</p></div></div><div class="card"><ul class="rec-list">{recommendations}</ul></div></section><footer class="footer">Generated by <a href="https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics">Pulse</a> v{version} · All-time {all_time_sessions} sessions · {all_time_cost} · {all_time_days} days tracked</footer></div>"##,
        recommendations = recommendations,
        version = env!("CARGO_PKG_VERSION"),
        all_time_sessions = summary.total_sessions,
        all_time_cost = html_escape(&format_cost(summary.total_cost)),
        all_time_days = summary.days_tracked
    )
    .unwrap();
    html.push_str("<script>if(typeof Chart!=='undefined'){Chart.defaults.color='#a0a0a0';Chart.defaults.borderColor='#1f1f1f';Chart.defaults.font.family=\"Inter, -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif\";Chart.defaults.font.size=11;try{");
    html.push_str(&daily_chart_script);
    html.push_str("}catch(e){console.error('daily chart failed',e);}try{");
    html.push_str(&token_chart_script);
    html.push_str("}catch(e){console.error('token chart failed',e);}}else{document.querySelectorAll('.chart-card canvas').forEach(c=>{c.style.display='none';});}");
    html.push_str("function pulseCopy(text){if(navigator.clipboard&&window.isSecureContext){return navigator.clipboard.writeText(text);}return new Promise((resolve,reject)=>{try{const ta=document.createElement('textarea');ta.value=text;ta.setAttribute('readonly','');ta.style.position='fixed';ta.style.top='-1000px';ta.style.opacity='0';document.body.appendChild(ta);ta.select();ta.setSelectionRange(0,ta.value.length);const ok=document.execCommand('copy');document.body.removeChild(ta);ok?resolve():reject(new Error('execCommand copy failed'));}catch(e){reject(e);}});}document.querySelectorAll('.rec-fix').forEach((btn)=>{btn.addEventListener('click',async()=>{const prompt=btn.getAttribute('data-prompt')||'';const original=btn.textContent;try{await pulseCopy(prompt);btn.classList.add('copied');btn.textContent='Copied prompt';}catch(err){btn.classList.add('copy-failed');btn.textContent='Copy failed - select manually';console.error('clipboard copy failed',err);}setTimeout(()=>{btn.classList.remove('copied','copy-failed');btn.textContent=original;},2000);});});</script></body></html>");
    html
}
fn format_tokens_short(t: i64) -> String {
    if t >= 1_000_000 {
        format!("{:.1}M", t as f64 / 1_000_000.0)
    } else if t >= 1_000 {
        format!("{:.1}K", t as f64 / 1_000.0)
    } else {
        t.to_string()
    }
}

fn format_cost(c: f64) -> String {
    if c >= 1.0 {
        format!("${c:.2}")
    } else {
        format!("${c:.4}")
    }
}

fn format_duration(secs: i64) -> String {
    if secs <= 0 {
        return "—".to_string();
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

fn build_top_sessions(sessions: &[db::HistoricalSession]) -> String {
    let mut sorted: Vec<_> = sessions.iter().collect();
    sorted.sort_by(|a, b| b.total_cost.partial_cmp(&a.total_cost).unwrap());
    let top = &sorted[..sorted.len().min(25)];
    if top.is_empty() {
        return String::new();
    }
    let mut html = String::from(
        r#"<div class="card"><h2>Most Costly Sessions</h2><table><tr><th>#</th><th>Project</th><th>Model</th><th>Tokens</th><th>Duration</th><th style="text-align:right">Cost</th></tr>"#,
    );
    for (i, s) in top.iter().enumerate() {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"cost\">{}</td></tr>",
            i + 1, s.project, s.model, format_tokens_short(s.total_tokens),
            format_duration(s.duration_secs), format_cost(s.total_cost)
        ));
    }
    html.push_str("</table></div>");
    html
}

fn build_project_table(projects: &[db::ProjectStat]) -> String {
    if projects.is_empty() {
        return String::new();
    }
    let mut html = String::from(
        r#"<div class="card"><h2>Project Breakdown</h2><table><tr><th>Project</th><th>Sessions</th><th>Tokens</th><th>Avg Cost</th><th style="text-align:right">Total Cost</th></tr>"#,
    );
    for p in projects {
        html.push_str(&format!(
            "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"cost\">{}</td></tr>",
            p.project, p.session_count, format_tokens_short(p.total_tokens),
            format_cost(p.avg_session_cost), format_cost(p.total_cost)
        ));
    }
    html.push_str("</table></div>");
    html
}

fn build_model_table(models: &[(String, i64, f64)], total: usize) -> String {
    if models.is_empty() {
        return String::new();
    }
    let mut html = String::from(
        r#"<div class="card"><h2>Model Routing Analysis</h2><table><tr><th>Model</th><th>Sessions</th><th>Share</th><th style="text-align:right">Cost</th></tr>"#,
    );
    for (m, count, cost) in models {
        let pct = if total > 0 {
            (*count as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        html.push_str(&format!(
            "<tr><td>{m}</td><td class=\"num\">{count}</td><td class=\"num\">{pct:.0}%</td><td class=\"cost\">{}</td></tr>",
            format_cost(*cost)
        ));
    }
    html.push_str("</table></div>");
    html
}

fn build_daily_chart_data(daily: &[db::DailyStat]) -> String {
    let mut by_date: std::collections::BTreeMap<&str, f64> = std::collections::BTreeMap::new();
    for d in daily {
        *by_date.entry(&d.date).or_default() += d.total_cost;
    }
    let labels: Vec<String> = by_date.keys().map(|d| format!("'{d}'")).collect();
    let values: Vec<String> = by_date.values().map(|v| format!("{v:.2}")).collect();
    format!(
        "new Chart(document.getElementById('dailyCostChart'), {{
  type: 'line',
  data: {{ labels: [{labels}], datasets: [{{ data: [{values}], borderColor: accent, backgroundColor: 'rgba(249,115,22,0.08)', fill: true, tension: 0.35, pointRadius: 0, pointHoverRadius: 4, borderWidth: 2 }}] }},
  options: {{ responsive: true, maintainAspectRatio: false, scales: {{ x: {{ grid: {{ color: border, drawBorder: false }} }}, y: {{ grid: {{ color: border, drawBorder: false }}, ticks: {{ callback: v => '$' + v.toFixed(2) }} }} }}, plugins: {{ legend: {{ display: false }} }} }}
}});",
        labels = labels.join(","),
        values = values.join(",")
    )
}

fn build_token_chart_data(input: i64, output: i64, cache_w: i64, cache_r: i64) -> String {
    format!(
        "new Chart(document.getElementById('tokenChart'), {{
  type: 'doughnut',
  data: {{ labels: ['Input','Output','Cache Write','Cache Read'], datasets: [{{ data: [{input},{output},{cache_w},{cache_r}], backgroundColor: [accent,'rgba(255,255,255,0.78)','rgba(163,163,163,0.58)','rgba(115,115,115,0.72)'], borderColor: border, borderWidth: 2 }}] }},
  options: {{ responsive: true, maintainAspectRatio: false, cutout: '62%', plugins: {{ legend: {{ position: 'bottom', labels: {{ usePointStyle: true, padding: 14, boxWidth: 8, font: {{ size: 11 }} }} }} }} }}
}});"
    )
}

fn build_hourly_heatmap(hourly: &[db::HourlyActivity]) -> String {
    let mut counts = vec![0i64; 24];
    let mut costs = vec![0.0f64; 24];
    for h in hourly {
        if h.hour >= 0 && h.hour < 24 {
            counts[h.hour as usize] = h.session_count;
            costs[h.hour as usize] = h.total_cost;
        }
    }
    let max_count = counts.iter().copied().max().unwrap_or(0).max(1) as f64;
    let mut html = String::from(r#"<div class="heatmap">"#);
    for hour in 0..24 {
        let label = if hour == 0 {
            "12a".to_string()
        } else if hour < 12 {
            format!("{hour}a")
        } else if hour == 12 {
            "12p".to_string()
        } else {
            format!("{}p", hour - 12)
        };
        let intensity = ((counts[hour] as f64 / max_count) * 0.78 + 0.06).clamp(0.06, 0.92);
        html.push_str(&format!(
            r#"<div class="heat-cell" style="--heat:{intensity:.3}">
  <div class="heat-label">{label}</div>
  <div class="heat-value">{count}</div>
  <div class="heat-meta">{cost}</div>
</div>"#,
            label = html_escape(&label),
            count = counts[hour],
            cost = html_escape(&format_cost(costs[hour])),
        ));
    }
    html.push_str("</div>");
    html
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn build_recommendations(sessions: &[db::HistoricalSession]) -> String {
    use super::analyzers::{
        cache_health, inflection, model_routing, prompt_complexity, recommendations,
        session_health, session_trace, tool_frequency,
    };

    let cache = cache_health::analyze(sessions);
    let routing = model_routing::analyze(sessions);
    let inflections = inflection::detect(sessions);
    let traces = session_trace::load_session_traces(sessions);
    let tool_frequency = tool_frequency::analyze(sessions, &traces);
    let prompt_complexity = prompt_complexity::analyze(sessions, &traces);
    let session_health =
        session_health::analyze(sessions, &traces, &tool_frequency, &prompt_complexity);
    let ctx = recommendations::AnalysisContext {
        sessions,
        cache: &cache,
        routing: &routing,
        inflections: &inflections,
        tool_frequency: Some(&tool_frequency),
        prompt_complexity: Some(&prompt_complexity),
        session_health: Some(&session_health),
    };
    let recs = recommendations::generate(&ctx);

    if recs.is_empty() {
        return r#"<li class="rec-item positive"><div class="rec-title">All clear</div><div class="rec-desc">No specific recommendations at this time.</div></li>"#.to_string();
    }

    recs.iter()
        .map(|r| {
            let severity_key = match r.severity {
                super::analyzers::Severity::Critical => "critical",
                super::analyzers::Severity::Warning => "warning",
                super::analyzers::Severity::Info => "info",
                super::analyzers::Severity::Positive => "positive",
            };
            let savings = r
                .estimated_savings
                .as_ref()
                .map(|s| {
                    format!(
                        r#"<div class="rec-meta"><span class="meta-k">Potential savings</span><span class="meta-v accent">{}</span></div>"#,
                        html_escape(s)
                    )
                })
                .unwrap_or_default();
            let fix_btn = if r.fix_prompt.is_empty()
                || matches!(r.severity, super::analyzers::Severity::Positive)
            {
                String::new()
            } else {
                format!(
                    r#"<button class="rec-fix" data-prompt="{}">Fix with Claude Code</button>"#,
                    html_escape(&r.fix_prompt)
                )
            };
            format!(
                r##"<li class="rec-item rec-{sev}" data-sev="{sev}">
  <div class="rec-head">
    <span class="rec-pill {sev}">{sev_label}</span>
    <div class="rec-title">{title}</div>
  </div>
  <div class="rec-desc">{desc}</div>
  {savings}
  <div class="rec-meta"><span class="meta-k">Action</span><span class="meta-v">{action}</span></div>
  {fix_btn}
</li>"##,
                sev = severity_key,
                sev_label = severity_key.to_uppercase(),
                title = html_escape(&r.title),
                desc = html_escape(&r.description),
                action = html_escape(&r.action),
                savings = savings,
                fix_btn = fix_btn,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
