use crate::db;

const REPO_URL: &str = "https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics";

struct ReportData {
    sessions: Vec<db::HistoricalSession>,
    daily: Vec<db::DailyStat>,
    summary: db::AnalyticsSummary,
    projects: Vec<db::ProjectStat>,
    forecast: db::CostForecast,
    hourly: Vec<db::HourlyActivity>,
    models: Vec<(String, i64, f64)>,
}

fn load_report_data(days: i64, project: Option<&str>) -> ReportData {
    let sessions = db::get_session_history(Some(days), project, Some(5000));
    if project.is_none() {
        return ReportData {
            daily: db::get_daily_stats(Some(days)),
            summary: db::get_analytics_summary(),
            projects: db::get_project_stats(Some(days)),
            forecast: db::get_cost_forecast(),
            hourly: db::get_hourly_activity(Some(days)),
            models: db::get_model_distribution(Some(days)),
            sessions,
        };
    }

    use chrono::{Datelike, Timelike, Utc};
    use std::collections::{BTreeMap, HashMap, HashSet};

    let now = Utc::now();
    let days_elapsed = now.day() as i64;
    let days_in_month = {
        let (y, m) = (now.year(), now.month());
        if m == 12 {
            chrono::NaiveDate::from_ymd_opt(y + 1, 1, 1)
        } else {
            chrono::NaiveDate::from_ymd_opt(y, m + 1, 1)
        }
        .and_then(|d| d.pred_opt())
        .map(|d| d.day() as i64)
        .unwrap_or(30)
    };
    let month_start = now.format("%Y-%m-01T00:00:00+00:00").to_string();

    let mut daily_map: BTreeMap<(String, String, String), db::DailyStat> = BTreeMap::new();
    let mut hourly_map: BTreeMap<i64, db::HourlyActivity> = BTreeMap::new();
    let mut project_map: HashMap<String, db::ProjectStat> = HashMap::new();
    let mut model_map: HashMap<String, (i64, f64)> = HashMap::new();
    let mut tracked_days: HashSet<String> = HashSet::new();
    let mut top_model_counts: HashMap<String, i64> = HashMap::new();

    for session in &sessions {
        let ts = session
            .started_at
            .as_deref()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc));
        let date = ts
            .map(|value| value.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| now.format("%Y-%m-%d").to_string());
        tracked_days.insert(date.clone());

        let daily_key = (date.clone(), session.project.clone(), session.model.clone());
        let entry = daily_map.entry(daily_key).or_insert(db::DailyStat {
            date: date.clone(),
            project: session.project.clone(),
            model: session.model.clone(),
            session_count: 0,
            total_cost: 0.0,
            total_tokens: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_write_tokens: 0,
            cache_read_tokens: 0,
        });
        entry.session_count += 1;
        entry.total_cost += session.total_cost;
        entry.total_tokens += session.total_tokens;
        entry.input_tokens += session.input_tokens;
        entry.output_tokens += session.output_tokens;
        entry.cache_write_tokens += session.cache_write_tokens;
        entry.cache_read_tokens += session.cache_read_tokens;

        let project_entry = project_map
            .entry(session.project.clone())
            .or_insert(db::ProjectStat {
                project: session.project.clone(),
                session_count: 0,
                total_cost: 0.0,
                total_tokens: 0,
                avg_session_cost: 0.0,
                avg_duration_secs: 0.0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                top_model: String::new(),
            });
        project_entry.session_count += 1;
        project_entry.total_cost += session.total_cost;
        project_entry.total_tokens += session.total_tokens;
        project_entry.avg_duration_secs += session.duration_secs as f64;
        project_entry.cache_read_tokens += session.cache_read_tokens;
        project_entry.cache_write_tokens += session.cache_write_tokens;

        *model_map.entry(session.model.clone()).or_insert((0, 0.0)) = {
            let (count, cost) = model_map.get(&session.model).copied().unwrap_or((0, 0.0));
            (count + 1, cost + session.total_cost)
        };
        *top_model_counts.entry(session.model.clone()).or_insert(0) += 1;

        if let Some(ts) = ts {
            let hour = i64::from(ts.hour() as i32);
            let hourly = hourly_map.entry(hour).or_insert(db::HourlyActivity {
                hour,
                session_count: 0,
                total_cost: 0.0,
            });
            hourly.session_count += 1;
            hourly.total_cost += session.total_cost;
        }
    }

    let mut projects: Vec<db::ProjectStat> = project_map
        .into_values()
        .map(|mut stat| {
            if stat.session_count > 0 {
                stat.avg_session_cost = stat.total_cost / stat.session_count as f64;
                stat.avg_duration_secs /= stat.session_count as f64;
            }
            stat.top_model = top_model_counts
                .iter()
                .max_by_key(|(_, count)| *count)
                .map(|(model, _)| model.clone())
                .unwrap_or_default();
            stat
        })
        .collect();
    projects.sort_by(|a, b| {
        b.total_cost
            .partial_cmp(&a.total_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut models: Vec<(String, i64, f64)> = model_map
        .into_iter()
        .map(|(model, (count, cost))| (model, count, cost))
        .collect();
    models.sort_by_key(|m| std::cmp::Reverse(m.1));

    let total_sessions = sessions.len() as i64;
    let total_cost: f64 = sessions.iter().map(|s| s.total_cost).sum();
    let total_tokens: i64 = sessions.iter().map(|s| s.total_tokens).sum();
    let total_cache_read: i64 = sessions.iter().map(|s| s.cache_read_tokens).sum();
    let total_cache_write: i64 = sessions.iter().map(|s| s.cache_write_tokens).sum();
    let avg_duration_secs = if total_sessions > 0 {
        sessions.iter().map(|s| s.duration_secs).sum::<i64>() as f64 / total_sessions as f64
    } else {
        0.0
    };
    let avg_tokens_per_session = if total_sessions > 0 {
        total_tokens as f64 / total_sessions as f64
    } else {
        0.0
    };
    let avg_cost_per_session = if total_sessions > 0 {
        total_cost / total_sessions as f64
    } else {
        0.0
    };
    let top_project = projects
        .first()
        .map(|p| p.project.clone())
        .unwrap_or_else(|| "—".into());
    let top_model = models
        .first()
        .map(|m| m.0.clone())
        .unwrap_or_else(|| "—".into());
    let spent_this_month: f64 = sessions
        .iter()
        .filter(|s| {
            s.started_at
                .as_deref()
                .is_some_and(|ts| ts >= month_start.as_str())
        })
        .map(|s| s.total_cost)
        .sum();
    let daily_average = if days_elapsed > 0 {
        spent_this_month / days_elapsed as f64
    } else {
        0.0
    };

    ReportData {
        sessions,
        daily: daily_map.into_values().collect(),
        summary: db::AnalyticsSummary {
            total_sessions,
            total_cost,
            total_tokens,
            total_cache_read,
            total_cache_write,
            avg_duration_secs,
            avg_tokens_per_session,
            avg_cost_per_session,
            top_project,
            top_model,
            days_tracked: tracked_days.len() as i64,
        },
        projects,
        forecast: db::CostForecast {
            spent_this_month,
            days_elapsed,
            days_in_month,
            projected_monthly: daily_average * days_in_month as f64,
            daily_average,
        },
        hourly: hourly_map.into_values().collect(),
        models,
    }
}

/// Render the analytics report as Markdown — suitable for pasting into a
/// GitHub issue, a Slack message, or a CC session. Sections mirror the HTML
/// report: cache grade, stats, top sessions, project + model breakdowns.
/// Render the analytics report as Markdown — suitable for pasting into a
/// GitHub issue, a Slack message, or a CC session. Sections mirror the HTML
/// report: cache grade, stats, top sessions, project + model breakdowns.
pub fn generate_markdown_report(days: Option<i64>, project: Option<&str>) -> String {
    use super::analyzers::{
        cache_health, inflection, model_routing, prompt_complexity, session_trace, tool_frequency,
        trace_overview,
    };
    use std::fmt::Write as _;

    let d = days.unwrap_or(30);
    let provider = cc_discord_presence::provider::load_active_provider();
    let data = load_report_data(d, project);
    let sessions = data.sessions;
    let projects = data.projects;
    let models = data.models;
    let forecast = data.forecast;
    let summary = data.summary;

    let total_sessions = sessions.len();
    let total_cost: f64 = sessions.iter().map(|s| s.total_cost).sum();
    let total_tokens: i64 = sessions.iter().map(|s| s.total_tokens).sum();

    let cache = cache_health::analyze_for_provider(provider, &sessions);
    let routing = model_routing::analyze(&sessions);
    let inflections = inflection::detect_for_provider(provider, &sessions);
    let traces = session_trace::load_session_traces(&sessions);
    let tool_frequency = tool_frequency::analyze(&sessions, &traces);
    let prompt_complexity = prompt_complexity::analyze(&sessions, &traces);
    let trace_overview =
        trace_overview::build(provider, &sessions, &traces, cache.trend_weighted_ratio);

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
    let md_escape = |value: &str| value.replace('|', r"\|").replace(['\r', '\n'], " ");
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

    writeln!(md, "## Telemetry Topology\n").unwrap();
    writeln!(md, "- Provider: {}", provider.display_name()).unwrap();
    writeln!(
        md,
        "- Instruction file: {}",
        provider.instruction_file_name()
    )
    .unwrap();
    writeln!(md, "- Session store: {}", provider.sessions_glob_label()).unwrap();
    writeln!(md, "- Global state: {}", provider.global_state_label()).unwrap();
    writeln!(
        md,
        "- Trace coverage: {} of {} sessions",
        trace_overview.traced_sessions, trace_overview.total_sessions
    )
    .unwrap();
    writeln!(
        md,
        "- Messages: {} user · {} assistant",
        trace_overview.user_messages, trace_overview.assistant_messages
    )
    .unwrap();
    writeln!(
        md,
        "- Tool calls: {} total · {} MCP · {} compact checkpoints\n",
        trace_overview.total_tool_calls,
        trace_overview.mcp_tool_calls,
        trace_overview.total_compactions
    )
    .unwrap();
    if !trace_overview.top_tools.is_empty() {
        writeln!(md, "### Top traced tools\n").unwrap();
        writeln!(md, "| Tool | Calls | Share |\n|---|---:|---:|").unwrap();
        for tool in &trace_overview.top_tools {
            writeln!(
                md,
                "| {} | {} | {:.1}% |",
                md_escape(&tool.name),
                tool.calls,
                tool.share_pct
            )
            .unwrap();
        }
        writeln!(md).unwrap();
    }
    writeln!(
        md,
        "```mermaid\n{}\n```\n",
        trace_overview.telemetry_mermaid
    )
    .unwrap();
    writeln!(md, "```mermaid\n{}\n```\n", trace_overview.cache_mermaid).unwrap();

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
        trace_overview,
    };
    use std::collections::BTreeMap;
    use std::fmt::Write as _;

    let d = days.unwrap_or(30);
    let provider = cc_discord_presence::provider::load_active_provider();
    let data = load_report_data(d, project);
    let sessions = data.sessions;
    let daily = data.daily;
    let summary = data.summary;
    let projects = data.projects;
    let forecast = data.forecast;
    let hourly = data.hourly;
    let models = data.models;

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

    let cache = cache_health::analyze_for_provider(provider, &sessions);
    let routing = model_routing::analyze(&sessions);
    let inflections = inflection::detect_for_provider(provider, &sessions);
    let traces = session_trace::load_session_traces(&sessions);
    let tool_frequency = tool_frequency::analyze(&sessions, &traces);
    let prompt_complexity = prompt_complexity::analyze(&sessions, &traces);
    let trace_overview =
        trace_overview::build(provider, &sessions, &traces, cache.trend_weighted_ratio);

    let grade_color = match cache.grade {
        'A' | 'B' => "#22c55e",
        'C' => "#fbbf24",
        _ => "#ef4444",
    };
    let project_table_html = build_project_table(&projects);
    let model_table_html = build_model_table(&models, total_sessions);
    let top_sessions_html = build_top_sessions(&sessions);
    let hourly_heatmap_html = build_hourly_heatmap(&hourly);
    let recommendations = build_recommendations(provider, &sessions, &traces);
    let mut by_date: BTreeMap<String, f64> = BTreeMap::new();
    for day in &daily {
        *by_date.entry(day.date.clone()).or_default() += day.total_cost;
    }
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
    let daily_chart_html = build_daily_cost_svg(&by_date);
    let token_chart_html =
        build_token_composition_svg(total_input, total_output, total_cache_w, total_cache_r);

    let mut html = String::new();
    html.push_str(r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Pulse Analytics Report</title>
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

.kicker,.summary-label,.info-label,.metric .label,.heat-label,.routing-share {
  font-family: var(--font-mono); font-size: 10px; font-weight: 600;
  letter-spacing: 0.12em; text-transform: uppercase; color: var(--text-muted);
}

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

.report-svg { width: 100%; height: 240px; display: block; }
.token-legend { list-style: none; padding: 0; margin: 14px 0 0 0; display: grid; grid-template-columns: repeat(2, minmax(0,1fr)); gap: 8px 18px; font-family: var(--font-mono); font-size: 11px; color: var(--text-secondary); letter-spacing: 0.02em; }
.token-legend li { display: flex; align-items: center; gap: 8px; }
.token-legend li b { margin-left: auto; color: var(--text-primary); font-weight: 600; }
.token-legend .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

.routing-row + .routing-row { margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border); }
.routing-label-row { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; }
.routing-name {
  font-family: var(--font-sans); font-weight: 600;
  font-size: 15px; color: var(--text-primary); letter-spacing: -0.01em;
}
.routing-meta { margin-top: 3px; color: var(--text-muted); font-size: 11px; font-family: var(--font-mono); }
.routing-track { height: 3px; margin-top: 10px; background: var(--border); border-radius: var(--radius-sm); overflow: hidden; }
.routing-fill { height: 100%; background: var(--text-primary); transition: width 1.2s cubic-bezier(.2,.9,.3,1); }

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

.info-value {
  font-family: var(--font-sans); font-weight: 700;
  font-size: clamp(20px, 2vw, 26px); color: var(--text-primary);
  letter-spacing: -0.02em; margin-top: 6px;
  font-variant-numeric: tabular-nums;
}
.info-card p { color: var(--text-secondary); font-size: 12px; margin-top: 4px; }

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

.empty-state {
  padding: 28px; background: var(--bg-card);
  border: 1px dashed var(--border-hover);
  border-radius: var(--radius-lg);
  color: var(--text-secondary); font-size: 13px; text-align: center;
  font-family: var(--font-sans);
}

.diagram-code {
  margin: 0;
  padding: 16px 18px;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  color: var(--text-secondary);
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.65;
  white-space: pre-wrap;
  word-break: break-word;
}

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

.footer {
  margin-top: 48px; padding: 22px 0; border-top: 1px solid var(--border);
  font-family: var(--font-mono); font-size: 11px; color: var(--text-muted);
  letter-spacing: 0.04em;
  display: flex; flex-wrap: wrap; gap: 18px;
  align-items: baseline; justify-content: space-between;
}
.footer-brand {
  text-transform: uppercase; letter-spacing: 0.14em; font-weight: 600;
  color: var(--text-secondary);
}
.footer-meta b { color: var(--text-primary); font-weight: 600; }
.footer-links { opacity: .85; }
.footer a { color: var(--text-primary); text-decoration: none; border-bottom: 1px solid var(--border-hover); transition: border-color .15s var(--ease); }
.footer a:hover { border-bottom-color: var(--text-primary); }

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
    write!(html, r##"<header class="hero"><div class="hero-top"><div><div class="kicker">Pulse · {provider_name} Analytics</div><h1>Analytics Report</h1><div class="hero-meta">{period_label}</div></div><div class="generated-at">Generated {generated_at}</div></div><div class="hero-divider"></div><div class="summary-grid"><div class="summary-card"><div class="summary-label">Total Cost</div><div class="summary-value">{total_cost}</div><div class="summary-meta">{period_label}</div></div><div class="summary-card"><div class="summary-label">Sessions</div><div class="summary-value">{total_sessions}</div><div class="summary-meta">Tracked in current window</div></div><div class="summary-card"><div class="summary-label">Tokens</div><div class="summary-value">{total_tokens}</div><div class="summary-meta">Input + output + cache</div></div><div class="summary-card"><div class="summary-label">Cache Grade</div><div class="summary-value" style="color:{grade_color}">{cache_grade}</div><div class="summary-meta">{cache_ratio:.1}% weighted hit ratio</div></div><div class="summary-card"><div class="summary-label">Daily Average</div><div class="summary-value">{daily_avg}</div><div class="summary-meta">Projected month {projected_monthly}</div></div></div></header>"##, provider_name = html_escape(cc_discord_presence::provider::load_active_provider().display_name()), period_label = html_escape(&period_label), generated_at = html_escape(&generated_at), total_cost = html_escape(&format_cost(total_cost)), total_sessions = total_sessions, total_tokens = html_escape(&format_tokens_short(total_tokens)), grade_color = grade_color, cache_grade = cache.grade, cache_ratio = cache.trend_weighted_ratio, daily_avg = html_escape(&format_cost(forecast.daily_average)), projected_monthly = html_escape(&format_cost(forecast.projected_monthly))).unwrap();
    let topology_tools_html = if trace_overview.top_tools.is_empty() {
        r#"<div class="empty-state">No traced tool mix yet.</div>"#.to_string()
    } else {
        let mut html = String::from(
            r#"<div class="card"><h2>Top Traced Tools</h2><table><tr><th>Tool</th><th>Calls</th><th>Share</th></tr>"#,
        );
        for tool in &trace_overview.top_tools {
            write!(
                html,
                r#"<tr><td>{}</td><td class="num">{}</td><td class="num">{:.1}%</td></tr>"#,
                html_escape(&tool.name),
                tool.calls,
                tool.share_pct
            )
            .unwrap();
        }
        html.push_str("</table></div>");
        html
    };

    html.push_str(r##"<nav class="anchor-nav screen-only"><a href="#cache">Cache</a><a href="#routing">Routing</a><a href="#inflections">Inflections</a><a href="#sessions">Sessions</a><a href="#tools">Tools</a><a href="#topology">Topology</a><a href="#prompts">Prompts</a><a href="#recommendations">Fixes</a></nav>"##);
    write!(html, r##"<section id="cache" class="section"><div class="section-header"><div><h2>Cache</h2><p>Weighted cache health drives grade color. Token mix stays visible for fast copy-paste review.</p></div></div><div class="section-grid"><div class="card"><div class="cache-grade"><div class="cache-letter" style="color:{grade_color}">{cache_grade}</div><div class="cache-copy"><h3>Cache Health</h3><div class="ratio">{cache_ratio:.1}%</div><p>{cache_diagnosis}</p></div></div><div class="metric-strip"><div class="metric"><div class="label">Overall Hit Ratio</div><div class="value">{overall_ratio:.1}%</div></div><div class="metric"><div class="label">Cache Read</div><div class="value">{cache_read}</div></div><div class="metric"><div class="label">Cache Write</div><div class="value">{cache_write}</div></div></div></div><div class="card chart-card"><h2>Token Composition</h2>{token_chart_html}<ul class="token-legend"><li><span class="dot" style="background:#f5f5f5"></span>Pure Input<b>{pure_input_short}</b></li><li><span class="dot" style="background:#7cb9e8"></span>Output<b>{output_short}</b></li><li><span class="dot" style="background:#fbbf24"></span>Cache Write<b>{cache_w_short}</b></li><li><span class="dot" style="background:#22c55e"></span>Cache Read<b>{cache_r_short}</b></li></ul></div></div></section>"##, grade_color = grade_color, cache_grade = cache.grade, cache_ratio = cache.trend_weighted_ratio, cache_diagnosis = html_escape(&cache.diagnosis), overall_ratio = cache.hit_ratio, cache_read = html_escape(&format_tokens_short(cache.total_cache_read)), cache_write = html_escape(&format_tokens_short(cache.total_cache_write)), token_chart_html = token_chart_html, pure_input_short = html_escape(&format_tokens_short(total_input)), output_short = html_escape(&format_tokens_short(total_output)), cache_w_short = html_escape(&format_tokens_short(total_cache_w)), cache_r_short = html_escape(&format_tokens_short(total_cache_r))).unwrap();
    write!(html, r##"<section id="routing" class="section"><div class="section-header"><div><h2>Routing</h2><p>Family-level spend split. Bars stay monochrome. Diagnosis stays textual for export parity.</p></div></div><div class="section-grid"><div class="card"><h2>Family Spend</h2>{routing_rows}<div class="metric-strip"><div class="metric"><div class="label">Sessions</div><div class="value">{routing_sessions}</div></div><div class="metric"><div class="label">Spend</div><div class="value">{routing_cost}</div></div><div class="metric"><div class="label">Potential Savings</div><div class="value">{routing_savings}</div></div></div><p style="margin-top:18px;">{routing_diagnosis}</p></div>{model_table_html}</div></section>"##, routing_rows = routing_rows_html, routing_sessions = routing.total_sessions, routing_cost = html_escape(&format_cost(routing.total_cost)), routing_savings = html_escape(&format_cost(routing.estimated_savings_if_rerouted)), routing_diagnosis = html_escape(&routing.diagnosis), model_table_html = model_table_html).unwrap();
    write!(html, r##"<section id="inflections" class="section"><div class="section-header"><div><h2>Inflections</h2><p>Spike cards use red rail. Efficiency drops use green rail. Sorted by absolute signal strength.</p></div></div>{inflections_html}</section>"##, inflections_html = inflections_html).unwrap();
    write!(html, r##"<section id="sessions" class="section"><div class="section-header"><div><h2>Sessions</h2><p>Daily cost trend, hourly activity, top sessions, project mix. Same data sources. Cleaner export.</p></div></div><div class="section-grid"><div class="card chart-card"><h2>Daily Cost Trend</h2>{daily_chart_html}</div><div class="card"><h2>Hourly Activity</h2>{hourly_heatmap_html}</div></div><div class="section-grid" style="margin-top:18px;">{top_sessions_html}{project_table_html}</div></section>"##, daily_chart_html = daily_chart_html, hourly_heatmap_html = hourly_heatmap_html, top_sessions_html = top_sessions_html, project_table_html = project_table_html).unwrap();
    write!(html, r##"<section id="tools" class="section"><div class="section-header"><div><h2>Tools</h2><p>Tool intensity, MCP share, compact gaps, top tool mix.</p></div></div><div class="info-grid"><div class="info-card"><div class="info-label">Traced Sessions</div><div class="info-value">{traced_sessions}</div><p>{sessions_analyzed} sessions analyzed</p></div><div class="info-card"><div class="info-label">Total Tool Calls</div><div class="info-value">{tool_calls}</div><p>{tools_per_session:.1} avg per session</p></div><div class="info-card"><div class="info-label">Calls / Hour</div><div class="info-value">{calls_per_hour:.1}</div><p>{mcp_share:.1}% MCP share</p></div><div class="info-card"><div class="info-label">Compact Gaps</div><div class="info-value">{compact_gaps}</div><p>{tool_diagnosis}</p></div></div><div style="margin-top:18px;">{tools_table_html}</div></section>"##, traced_sessions = tool_frequency.traced_sessions, sessions_analyzed = tool_frequency.sessions_analyzed, tool_calls = tool_frequency.total_tool_calls, tools_per_session = tool_frequency.avg_tools_per_session, calls_per_hour = tool_frequency.avg_tool_calls_per_hour, mcp_share = tool_frequency.mcp_share_pct, compact_gaps = tool_frequency.compact_gap_sessions, tool_diagnosis = html_escape(&tool_frequency.diagnosis), tools_table_html = tools_table_html).unwrap();
    write!(html, r##"<section id="topology" class="section"><div class="section-header"><div><h2>Telemetry Topology</h2><p>Provider-aware wiring from instruction files to cache reuse, session telemetry, analytics storage, exports, and rich presence.</p></div></div><div class="info-grid"><div class="info-card"><div class="info-label">Provider</div><div class="info-value">{provider_display}</div><p>{instruction_file} · {fix_label}</p></div><div class="info-card"><div class="info-label">Session Store</div><div class="info-value">{traced_sessions}/{total_sessions}</div><p>{session_store}</p></div><div class="info-card"><div class="info-label">Message Flow</div><div class="info-value">{user_messages}/{assistant_messages}</div><p>User / assistant traced messages</p></div><div class="info-card"><div class="info-label">Cache + Tools</div><div class="info-value">{tool_calls}</div><p>{cache_ratio:.1}% cache hit · {mcp_calls} MCP · {compactions} compactions</p></div></div><div class="section-grid" style="margin-top:18px;"><div class="card"><h2>Telemetry Flow</h2><pre class="diagram-code">{telemetry_mermaid}</pre></div><div class="card"><h2>Cache &amp; Tool Flow</h2><pre class="diagram-code">{cache_mermaid}</pre></div></div><div style="margin-top:18px;">{topology_tools_html}</div></section>"##,
        provider_display = html_escape(&trace_overview.provider_display),
        instruction_file = html_escape(&trace_overview.instruction_file),
        fix_label = html_escape(&trace_overview.fix_button_label),
        traced_sessions = trace_overview.traced_sessions,
        total_sessions = trace_overview.total_sessions,
        session_store = html_escape(&trace_overview.session_store),
        user_messages = trace_overview.user_messages,
        assistant_messages = trace_overview.assistant_messages,
        tool_calls = trace_overview.total_tool_calls,
        cache_ratio = trace_overview.cache_hit_ratio,
        mcp_calls = trace_overview.mcp_tool_calls,
        compactions = trace_overview.total_compactions,
        telemetry_mermaid = html_escape(&trace_overview.telemetry_mermaid),
        cache_mermaid = html_escape(&trace_overview.cache_mermaid),
        topology_tools_html = topology_tools_html,
    ).unwrap();
    write!(html, r##"<section id="prompts" class="section"><div class="section-header"><div><h2>Prompts</h2><p>Prompt complexity stays copyable. Preview column trims long prompts without hiding signal.</p></div></div><div class="info-grid"><div class="info-card"><div class="info-label">Prompts Analyzed</div><div class="info-value">{prompts_analyzed}</div><p>{prompt_sessions} sessions scanned</p></div><div class="info-card"><div class="info-label">Avg Complexity</div><div class="info-value">{avg_complexity:.1}</div><p>{high_complexity} high-complexity sessions</p></div><div class="info-card"><div class="info-label">Avg Specificity</div><div class="info-value">{avg_specificity:.1}</div><p>{low_specificity} low-specificity sessions</p></div><div class="info-card"><div class="info-label">Diagnosis</div><div class="info-value">{prompt_label}</div><p>{prompt_diagnosis}</p></div></div><div style="margin-top:18px;">{prompt_table_html}</div></section>"##, prompts_analyzed = prompt_complexity.prompts_analyzed, prompt_sessions = prompt_complexity.sessions_analyzed, avg_complexity = prompt_complexity.avg_complexity_score, high_complexity = prompt_complexity.high_complexity_sessions, avg_specificity = prompt_complexity.avg_specificity_score, low_specificity = prompt_complexity.low_specificity_sessions, prompt_label = if prompt_complexity.available { "Live" } else { "Pending" }, prompt_diagnosis = html_escape(&prompt_complexity.diagnosis), prompt_table_html = prompt_table_html).unwrap();
    write!(
        html,
        r##"<section id="recommendations" class="section"><div class="section-header"><div><h2>Recommendations</h2><p>Rule-based fixes from the Pulse recommendations engine. Each card ships with a "{fix_label}" button — paste into {provider_name} to remediate.</p></div></div><ul class="rec-list">{recommendations}</ul></section><footer class="footer"><div class="footer-brand">Pulse · {provider_name} Analytics</div><div class="footer-meta">All-time <b>{all_time_sessions}</b> sessions · <b>{all_time_cost}</b> · {all_time_days} days tracked</div><div class="footer-links"><a href="{repo_url}">Source</a> · v{version}</div></footer></div>"##,
        provider_name = html_escape(provider.display_name()),
        fix_label = html_escape(provider.fix_action_label()),
        recommendations = recommendations,
        repo_url = REPO_URL,
        version = env!("CARGO_PKG_VERSION"),
        all_time_sessions = summary.total_sessions,
        all_time_cost = html_escape(&format_cost(summary.total_cost)),
        all_time_days = summary.days_tracked
    )
    .unwrap();
    html.push_str("<script>function pulseCopy(text){if(navigator.clipboard&&window.isSecureContext){return navigator.clipboard.writeText(text);}return new Promise((resolve,reject)=>{try{const ta=document.createElement('textarea');ta.value=text;ta.setAttribute('readonly','');ta.style.position='fixed';ta.style.top='-1000px';ta.style.opacity='0';document.body.appendChild(ta);ta.select();ta.setSelectionRange(0,ta.value.length);const ok=document.execCommand('copy');document.body.removeChild(ta);ok?resolve():reject(new Error('execCommand copy failed'));}catch(e){reject(e);}});}document.querySelectorAll('.rec-fix').forEach((btn)=>{btn.addEventListener('click',async()=>{const prompt=btn.getAttribute('data-prompt')||'';const original=btn.textContent;try{await pulseCopy(prompt);btn.classList.add('copied');btn.textContent='Copied prompt';}catch(err){btn.classList.add('copy-failed');btn.textContent='Copy failed - select manually';console.error('clipboard copy failed',err);}setTimeout(()=>{btn.classList.remove('copied','copy-failed');btn.textContent=original;},2000);});});</script></body></html>");
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

fn build_daily_cost_svg(by_date: &std::collections::BTreeMap<String, f64>) -> String {
    if by_date.is_empty() {
        return r#"<div class="empty-state">No daily cost data available.</div>"#.to_string();
    }
    let width = 760.0;
    let height = 220.0;
    let padding_x = 18.0;
    let padding_y = 20.0;
    let values: Vec<f64> = by_date.values().copied().collect();
    let max = values.iter().copied().fold(0.0_f64, f64::max).max(1.0);
    let step_x = if values.len() > 1 {
        (width - padding_x * 2.0) / (values.len() - 1) as f64
    } else {
        0.0
    };
    let points: Vec<String> = values
        .iter()
        .enumerate()
        .map(|(idx, value)| {
            let x = padding_x + step_x * idx as f64;
            let y = height - padding_y - ((value / max) * (height - padding_y * 2.0));
            format!("{x:.2},{y:.2}")
        })
        .collect();
    let mut area_points = Vec::with_capacity(points.len() + 2);
    area_points.push(format!("{padding_x:.2},{:.2}", height - padding_y));
    area_points.extend(points.iter().cloned());
    let end_x = padding_x + step_x * (values.len().saturating_sub(1)) as f64;
    area_points.push(format!("{end_x:.2},{:.2}", height - padding_y));
    let labels = by_date
        .iter()
        .enumerate()
        .filter(|(idx, _)| values.len() <= 6 || *idx == 0 || *idx == values.len() - 1 || idx % 2 == 0)
        .map(|(idx, (date, _))| {
            let x = padding_x + step_x * idx as f64;
            format!(
                r##"<text x="{x:.2}" y="{y}" text-anchor="middle" fill="#6b6b6b" font-size="10">{label}</text>"##,
                y = height - 4.0,
                label = html_escape(date),
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        r##"<svg viewBox="0 0 {width} {height}" class="report-svg" role="img" aria-label="Daily cost trend">
<rect x="0" y="0" width="{width}" height="{height}" fill="transparent"/>
<line x1="{padding_x}" y1="{baseline}" x2="{end_x}" y2="{baseline}" stroke="#1f1f1f" stroke-width="1"/>
<polygon points="{area}" fill="rgba(245,245,245,0.08)"/>
<polyline points="{points}" fill="none" stroke="#f5f5f5" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
{labels}
</svg>"##,
        baseline = height - padding_y,
        end_x = width - padding_x,
        area = area_points.join(" "),
        points = points.join(" "),
        labels = labels,
    )
}

fn build_token_composition_svg(input: i64, output: i64, cache_w: i64, cache_r: i64) -> String {
    let segments = [
        ("#f5f5f5", input.max(0) as f64),
        ("#7cb9e8", output.max(0) as f64),
        ("#fbbf24", cache_w.max(0) as f64),
        ("#22c55e", cache_r.max(0) as f64),
    ];
    let total = segments
        .iter()
        .map(|(_, value)| *value)
        .sum::<f64>()
        .max(1.0);
    let width = 760.0;
    let bar_x = 22.0;
    let bar_y = 74.0;
    let bar_w = width - 44.0;
    let bar_h = 18.0;
    let mut cursor = bar_x;
    let mut bars = String::new();
    for (color, value) in segments {
        let segment_w = (value / total) * bar_w;
        if segment_w > 0.0 {
            bars.push_str(&format!(
                r##"<rect x="{x:.2}" y="{bar_y}" width="{w:.2}" height="{bar_h}" rx="9" fill="{color}"/>"##,
                x = cursor,
                w = segment_w.max(2.0),
            ));
        }
        cursor += segment_w;
    }
    format!(
        r##"<svg viewBox="0 0 {width} 140" class="report-svg" role="img" aria-label="Token composition">
<rect x="{bar_x}" y="{bar_y}" width="{bar_w}" height="{bar_h}" rx="9" fill="#121212" stroke="#1f1f1f" stroke-width="1"/>
{bars}
<text x="{bar_x}" y="40" fill="#fafafa" font-size="26" font-weight="700">{total_label}</text>
<text x="{bar_x}" y="58" fill="#6b6b6b" font-size="11" font-family="JetBrains Mono, monospace">total token mix</text>
</svg>"##,
        bars = bars,
        total_label = html_escape(&format_tokens_short(
            (input + output + cache_w + cache_r).max(0)
        )),
    )
}

fn build_hourly_heatmap(hourly: &[db::HourlyActivity]) -> String {
    let mut counts = [0i64; 24];
    let mut costs = [0.0f64; 24];
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

fn build_recommendations(
    provider: cc_discord_presence::provider::Provider,
    sessions: &[db::HistoricalSession],
    traces: &std::collections::HashMap<String, crate::analyzers::session_trace::SessionTrace>,
) -> String {
    use super::analyzers::{
        cache_health, inflection, model_routing, prompt_complexity, recommendations,
        session_health, tool_frequency,
    };
    let provider_name = provider.display_name();

    let cache = cache_health::analyze_for_provider(provider, sessions);
    let routing = model_routing::analyze(sessions);
    let inflections = inflection::detect_for_provider(provider, sessions);
    let tool_frequency = tool_frequency::analyze(sessions, traces);
    let prompt_complexity = prompt_complexity::analyze(sessions, traces);
    let session_health =
        session_health::analyze(sessions, traces, &tool_frequency, &prompt_complexity);
    let ctx = recommendations::AnalysisContext {
        provider,
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
                    r#"<button class="rec-fix" data-prompt="{}">Fix with {}</button>"#,
                    html_escape(&r.fix_prompt),
                    html_escape(provider_name)
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
