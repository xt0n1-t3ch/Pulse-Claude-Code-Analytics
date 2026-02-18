use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor::MoveTo;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, QueueableCommand};

use crate::config::{PresenceConfig, TerminalLogoMode};
use crate::metrics::MetricsSnapshot;
use crate::session::{ClaudeSessionSnapshot, RateLimits};
use crate::usage::UsageData;
use crate::util::{
    format_cost, format_time_until, format_time_until_reset, format_token_triplet, format_tokens,
    human_duration, now_local, truncate,
};

// ── Brand Color ───────────────────────────────────────────────────────────
// Claude mascot orange — matches the salmon/terracotta of the pixel art pig
const MASCOT: Color = Color::Rgb {
    r: 210,
    g: 120,
    b: 80,
};

// ── ASCII Art Banners ─────────────────────────────────────────────────────

const CLAUDE_ASCII: [&str; 8] = [
    r"    ██████╗██╗      █████╗ ██╗   ██╗██████╗ ███████╗",
    r"   ██╔════╝██║     ██╔══██╗██║   ██║██╔══██╗██╔════╝",
    r"   ██║     ██║     ███████║██║   ██║██║  ██║█████╗  ",
    r"   ██║     ██║     ██╔══██║██║   ██║██║  ██║██╔══╝  ",
    r"   ╚██████╗███████╗██║  ██║╚██████╔╝██████╔╝███████╗",
    r"    ╚═════╝╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚═════╝ ╚══════╝",
    r"     Discord Presence for Claude Code              ",
    r"     Live activity + usage telemetry               ",
];

const CC_ASCII: [&str; 6] = [
    r"    ___ _      _   _   _ ___  ___    ___ ___  ___  ___ ",
    r"   / __| |    /_\ | | | |   \| __|  / __/ _ \|   \| __|",
    r"  | (__| |__ / _ \| |_| | |) | _|  | (_| (_) | |) | _| ",
    r"   \___|____/_/ \_\\___/|___/|___|  \___\___/|___/|___|",
    r"    Discord Presence for Claude Code                    ",
    r"    Live activity + usage telemetry                     ",
];

#[allow(dead_code)]
const COMPACT_BANNER: [&str; 2] = [
    "CLAUDE CODE DISCORD PRESENCE",
    "Live activity + usage telemetry",
];

const MINIMAL_BANNER: &str = "CC Presence";

// ── Layout Modes ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutMode {
    Full,
    Compact,
    Minimal,
}

fn select_layout(width: u16, height: u16) -> LayoutMode {
    if width >= 100 && height >= 30 {
        LayoutMode::Full
    } else if width >= 60 && height >= 18 {
        LayoutMode::Compact
    } else {
        LayoutMode::Minimal
    }
}

// ── Render Data ───────────────────────────────────────────────────────────

pub struct RenderData<'a> {
    pub running_for: Duration,
    pub mode_label: &'a str,
    pub discord_status: &'a str,
    pub client_id_configured: bool,
    pub poll_interval_secs: u64,
    pub stale_secs: u64,
    pub show_activity: bool,
    pub show_activity_target: bool,
    pub logo_mode: TerminalLogoMode,
    pub logo_path: Option<&'a str>,
    pub active: Option<&'a ClaudeSessionSnapshot>,
    pub effective_limits: Option<&'a RateLimits>,
    pub api_usage: Option<&'a UsageData>,
    pub plan_name: Option<&'a str>,
    pub metrics: Option<&'a MetricsSnapshot>,
    pub sessions: &'a [ClaudeSessionSnapshot],
    pub config: &'a PresenceConfig,
    pub setup_active: bool,
    pub setup_step: u8,
    /// True for ~5 seconds after an Extra Usage charge is detected.
    pub extra_usage_alert: bool,
}

// ── Terminal Management ───────────────────────────────────────────────────

pub fn enter_terminal() -> Result<()> {
    terminal::enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, crossterm::cursor::Hide)?;
    Ok(())
}

pub fn leave_terminal() -> Result<()> {
    execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

// ── Frame Signature ───────────────────────────────────────────────────────

pub fn frame_signature(data: &RenderData<'_>) -> String {
    let mut sig = String::new();
    sig.push_str(data.discord_status);
    sig.push('|');
    if let Some(active) = data.active {
        sig.push_str(&active.session_id);
        sig.push('|');
        sig.push_str(&active.total_cost.to_string());
        sig.push('|');
        sig.push_str(&active.session_total_tokens.unwrap_or(0).to_string());
        sig.push('|');
        if let Some(ref activity) = active.activity {
            sig.push_str(activity.action_text());
        }
    }
    sig.push('|');
    sig.push_str(&data.sessions.len().to_string());
    if let Some(usage) = data.api_usage {
        sig.push_str(&format!(
            "|u{:.0}|u{:.0}",
            usage.five_hour.utilization, usage.seven_day.utilization
        ));
    }
    if let Some(metrics) = data.metrics {
        sig.push_str(&format!("|m{:.4}", metrics.totals.cost_usd));
    }
    sig.push_str(&format!("|a{}", data.extra_usage_alert as u8));
    sig
}

// ── Draw ──────────────────────────────────────────────────────────────────

pub fn draw(data: &RenderData<'_>) -> Result<()> {
    let (width, height) = terminal::size()?;
    let width = width as usize;
    let height = height as usize;
    let layout = select_layout(width as u16, height as u16);

    let mut out = io::stdout();
    out.queue(Clear(ClearType::All))?;

    let mut row: u16 = 0;
    let footer_rows: u16 = 2;
    let max_body_row = (height as u16).saturating_sub(footer_rows);

    // Setup wizard takes over rendering
    if data.setup_active {
        render_setup(&mut out, data.setup_step, width, height as u16)?;
        out.flush()?;
        return Ok(());
    }

    // Banner
    render_banner(&mut out, &mut row, max_body_row, width, layout, data)?;

    // Runtime section
    render_runtime(&mut out, &mut row, max_body_row, width, layout, data)?;

    // Usage section (from API)
    render_usage(&mut out, &mut row, max_body_row, width, layout, data)?;

    // Metrics (cumulative cost + token breakdown)
    render_metrics(&mut out, &mut row, max_body_row, width, layout, data)?;

    // Active session
    render_active(&mut out, &mut row, max_body_row, width, layout, data)?;

    // Recent sessions
    render_recent(&mut out, &mut row, max_body_row, width, layout, data)?;

    // Footer
    render_footer(&mut out, width, height as u16)?;

    out.flush()?;
    Ok(())
}

// ── Section Renderers ─────────────────────────────────────────────────────

fn render_banner(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    layout: LayoutMode,
    _data: &RenderData<'_>,
) -> Result<()> {
    match layout {
        LayoutMode::Full => {
            for (i, line) in CLAUDE_ASCII.iter().enumerate() {
                if i < 6 {
                    write_line_colored(out, row, max_row, width, line, MASCOT)?;
                } else {
                    write_line_colored(out, row, max_row, width, line, Color::DarkGrey)?;
                }
            }
            write_line(out, row, max_row, width, "")?;
        }
        LayoutMode::Compact => {
            for (i, line) in CC_ASCII.iter().enumerate() {
                if i < 4 {
                    write_line_colored(out, row, max_row, width, line, MASCOT)?;
                } else {
                    write_line_colored(out, row, max_row, width, line, Color::DarkGrey)?;
                }
            }
            write_line(out, row, max_row, width, "")?;
        }
        LayoutMode::Minimal => {
            write_line_colored(out, row, max_row, width, MINIMAL_BANNER, Color::DarkYellow)?;
        }
    }
    Ok(())
}

fn render_runtime(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    layout: LayoutMode,
    data: &RenderData<'_>,
) -> Result<()> {
    if layout == LayoutMode::Minimal {
        // Minimal: single line
        let discord_icon = if data.discord_status.contains("Connected") {
            "\u{25CF}"
        } else {
            "\u{25CB}"
        };
        let plan_str = data.plan_name.unwrap_or("");
        let line = if plan_str.is_empty() {
            format!("{} {}", discord_icon, data.discord_status)
        } else {
            format!("{} {} | {}", discord_icon, data.discord_status, plan_str)
        };
        write_line(out, row, max_row, width, &line)?;
        return Ok(());
    }

    write_hr(out, row, max_row, width, "Runtime")?;
    write_kv(out, row, max_row, width, "Mode", data.mode_label)?;
    write_kv(out, row, max_row, width, "Now", &now_local())?;
    write_kv(
        out,
        row,
        max_row,
        width,
        "Uptime",
        &human_duration(data.running_for),
    )?;
    {
        let (dot, dot_color) = discord_status_indicator(data.discord_status);
        write_kv_icon(
            out,
            row,
            max_row,
            width,
            "Discord",
            dot,
            dot_color,
            data.discord_status,
        )?;
    }

    if layout == LayoutMode::Full {
        write_kv(
            out,
            row,
            max_row,
            width,
            "Client ID",
            if data.client_id_configured {
                "configured"
            } else {
                "missing"
            },
        )?;
        if let Some(plan) = data.plan_name {
            write_kv(out, row, max_row, width, "Plan", plan)?;
        }
        write_kv(
            out,
            row,
            max_row,
            width,
            "Polling",
            &format!("{}s | Stale: {}s", data.poll_interval_secs, data.stale_secs),
        )?;
    } else if let Some(plan) = data.plan_name {
        write_kv(out, row, max_row, width, "Plan", plan)?;
    }

    Ok(())
}

fn render_usage(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    layout: LayoutMode,
    data: &RenderData<'_>,
) -> Result<()> {
    let Some(usage) = data.api_usage else {
        return Ok(());
    };

    if layout == LayoutMode::Minimal {
        // "5h 14% │ 7d 66% │ Sonnet 5%"
        let mut parts = vec![
            format!("5h {:.0}%", usage.five_hour.utilization),
            format!("7d {:.0}%", usage.seven_day.utilization),
        ];
        if let Some(ref s) = usage.sonnet_free {
            parts.push(format!("Sonnet {:.0}%", s.utilization));
        }
        write_line(out, row, max_row, width, &parts.join(" │ "))?;
        return Ok(());
    }

    write_hr(out, row, max_row, width, "Usage")?;

    // Adaptive bar: fills available width after label(12) + percent(4) + reset text(~16) + gaps
    let bar_width = (width.saturating_sub(40)).clamp(6, 30);

    let five_hr_reset = format_time_until_reset(usage.five_hour.resets_at);
    write_colored_bar(
        out,
        row,
        max_row,
        width,
        "5h Session",
        usage.five_hour.utilization,
        bar_width,
        &five_hr_reset,
        usage_color(usage.five_hour.utilization),
    )?;

    let seven_day_reset = format_time_until_reset(usage.seven_day.resets_at);
    write_colored_bar(
        out,
        row,
        max_row,
        width,
        "7d Weekly ",
        usage.seven_day.utilization,
        bar_width,
        &seven_day_reset,
        usage_color(usage.seven_day.utilization),
    )?;

    // Sonnet-only window (Max plans) — field is "seven_day_sonnet" in the API
    if let Some(ref sonnet) = usage.sonnet_free {
        let sonnet_reset = format_time_until_reset(sonnet.resets_at);
        write_colored_bar(
            out,
            row,
            max_row,
            width,
            "Sonnet    ",
            sonnet.utilization,
            bar_width,
            &sonnet_reset,
            usage_color(sonnet.utilization),
        )?;
    }

    // Extra (pay-per-use) usage — only when enabled and data is available
    if let Some(ref extra) = usage.extra_usage {
        if extra.is_enabled {
            if let (Some(spent), Some(limit)) = (extra.used_credits, extra.monthly_limit) {
                // API returns values in cents — divide by 100 to get USD
                let spent_usd = spent / 100.0;
                let limit_usd = limit / 100.0;
                let pct = extra.utilization.unwrap_or(0.0);
                write_hr(out, row, max_row, width, "Extra Usage")?;
                write_kv(
                    out,
                    row,
                    max_row,
                    width,
                    "Spent",
                    &format!("${:.2} / ${:.2} ({:.0}% used)", spent_usd, limit_usd, pct),
                )?;
                // Flash alert for ~5 s when a new charge is detected
                if data.extra_usage_alert && *row < max_row {
                    out.queue(MoveTo(0, *row))?;
                    out.queue(SetForegroundColor(Color::Yellow))?;
                    out.queue(Print(format!("  {:<12}", "Alert")))?;
                    out.queue(SetForegroundColor(Color::DarkGrey))?;
                    out.queue(Print(": "))?;
                    out.queue(SetForegroundColor(Color::Yellow))?;
                    out.queue(Print("! charge detected \u{2014} toggling off/on"))?;
                    out.queue(ResetColor)?;
                    *row += 1;
                }
            }
        }
    }

    Ok(())
}

fn render_metrics(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    layout: LayoutMode,
    data: &RenderData<'_>,
) -> Result<()> {
    let Some(metrics) = data.metrics else {
        return Ok(());
    };

    if metrics.totals.cost_usd == 0.0 && metrics.totals.total_tokens == 0 {
        return Ok(());
    }

    if layout == LayoutMode::Minimal {
        let line = format!(
            "Cost: {} | {} tok",
            format_cost(metrics.totals.cost_usd),
            format_tokens(metrics.totals.total_tokens),
        );
        write_line(out, row, max_row, width, &line)?;
        return Ok(());
    }

    write_hr(out, row, max_row, width, "Metrics")?;

    write_kv(
        out,
        row,
        max_row,
        width,
        "Total Cost",
        &format_cost(metrics.totals.cost_usd),
    )?;
    write_kv(
        out,
        row,
        max_row,
        width,
        "Tokens",
        &format!(
            "{} (in: {} | out: {})",
            format_tokens(metrics.totals.total_tokens),
            format_tokens(metrics.totals.input_tokens),
            format_tokens(metrics.totals.output_tokens),
        ),
    )?;

    if layout == LayoutMode::Full {
        // Cache token counts
        if metrics.totals.cache_write_tokens > 0 || metrics.totals.cache_read_tokens > 0 {
            write_kv(
                out,
                row,
                max_row,
                width,
                "Cache",
                &format!(
                    "write: {} | read: {}",
                    format_tokens(metrics.totals.cache_write_tokens),
                    format_tokens(metrics.totals.cache_read_tokens),
                ),
            )?;
        }

        // Cost breakdown by token type with proportional bars
        let total = metrics.totals.cost_usd.max(0.0001);
        let bar_width = if width > 60 { 15 } else { 8 };

        for (label, cost) in [
            ("Input    ", metrics.cost_breakdown.input_cost),
            ("Output   ", metrics.cost_breakdown.output_cost),
            ("Cache Wr ", metrics.cost_breakdown.cache_write_cost),
            ("Cache Rd ", metrics.cost_breakdown.cache_read_cost),
        ] {
            if cost > 0.0 {
                let pct = (cost / total) * 100.0;
                write_cost_bar(out, row, max_row, label, cost, pct, bar_width)?;
            }
        }

        // Per-model breakdown
        if !metrics.by_model.is_empty() {
            write_line(out, row, max_row, width, "")?;
            for model in &metrics.by_model {
                let pct = (model.cost_usd / total) * 100.0;
                let line = format!(
                    "  {} {} ({:.0}%) | {} tok | {} sess",
                    format_cost(model.cost_usd),
                    model.display_name,
                    pct,
                    format_tokens(model.input_tokens + model.output_tokens),
                    model.session_count,
                );
                write_line(out, row, max_row, width, &line)?;
            }
        }
    } else {
        // Compact: show top model if available
        if let Some(top) = metrics.by_model.first() {
            write_kv(
                out,
                row,
                max_row,
                width,
                "Top Model",
                &format!("{} ({})", top.display_name, format_cost(top.cost_usd)),
            )?;
        }
    }

    Ok(())
}

fn render_active(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    layout: LayoutMode,
    data: &RenderData<'_>,
) -> Result<()> {
    let Some(active) = data.active else {
        if layout != LayoutMode::Minimal {
            write_hr(out, row, max_row, width, "Active Session")?;
            write_line(out, row, max_row, width, "  No active Claude Code sessions")?;
        }
        return Ok(());
    };

    if layout == LayoutMode::Minimal {
        let model = active.model_display.as_deref().unwrap_or("Claude");
        let activity_text = active
            .activity
            .as_ref()
            .map(|a| a.action_text())
            .unwrap_or("Idle");
        let line = format!("{} | {} | {}", active.project_name, model, activity_text);
        write_line(out, row, max_row, width, &line)?;

        if active.total_cost > 0.0 {
            let tokens = active.session_total_tokens.unwrap_or(0);
            let line = format!(
                "Cost: {} | {} tokens",
                format_cost(active.total_cost),
                crate::util::format_tokens(tokens)
            );
            write_line(out, row, max_row, width, &line)?;
        }
        return Ok(());
    }

    write_hr(out, row, max_row, width, "Active Session")?;
    write_kv(out, row, max_row, width, "Project", &active.project_name)?;

    if let Some(model) = &active.model_display {
        let model_id = active.model.as_deref().unwrap_or("");
        let tokens = active.session_total_tokens.unwrap_or(0);
        let model_str = crate::cost::model_display_with_context(model_id, model, tokens);
        write_kv(out, row, max_row, width, "Model", &model_str)?;
    }

    if let Some(activity) = &active.activity {
        let text = activity.to_text(data.show_activity_target);
        write_kv(out, row, max_row, width, "Activity", &text)?;
    }

    let token_line = format_token_triplet(
        active.session_delta_tokens,
        active.last_turn_tokens,
        active.session_total_tokens,
    );
    write_kv(
        out,
        row,
        max_row,
        width,
        "Tokens",
        &token_line.replace("Tokens: ", ""),
    )?;

    if active.total_cost > 0.0 {
        write_kv(
            out,
            row,
            max_row,
            width,
            "Cost",
            &format_cost(active.total_cost),
        )?;
    }

    // Limits bars for active session
    if let Some(limits) = data.effective_limits {
        if let Some(primary) = &limits.primary {
            let bar_width = if width > 60 { 18 } else { 10 };
            let reset_str = format_time_until(primary.resets_at);
            write_colored_bar(
                out,
                row,
                max_row,
                width,
                "5h left   ",
                primary.remaining_percent,
                bar_width,
                &reset_str,
                remaining_color(primary.remaining_percent),
            )?;
        }
        if let Some(secondary) = &limits.secondary {
            let bar_width = if width > 60 { 18 } else { 10 };
            let reset_str = format_time_until(secondary.resets_at);
            write_colored_bar(
                out,
                row,
                max_row,
                width,
                "7d left   ",
                secondary.remaining_percent,
                bar_width,
                &reset_str,
                remaining_color(secondary.remaining_percent),
            )?;
        }
    }

    if layout == LayoutMode::Full {
        if let Some(branch) = &active.git_branch {
            write_kv(out, row, max_row, width, "Branch", branch)?;
        }
        write_kv(
            out,
            row,
            max_row,
            width,
            "Path",
            &truncate(&active.cwd.display().to_string(), width.saturating_sub(20)),
        )?;
    }

    Ok(())
}

fn render_recent(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    layout: LayoutMode,
    data: &RenderData<'_>,
) -> Result<()> {
    if data.sessions.len() <= 1 {
        return Ok(());
    }

    let max_items = match layout {
        LayoutMode::Full => 5,
        LayoutMode::Compact => 3,
        LayoutMode::Minimal => 1,
    };

    if layout != LayoutMode::Minimal {
        write_hr(out, row, max_row, width, "Recent Sessions")?;
    }

    let active_id = data.active.map(|a| a.session_id.as_str()).unwrap_or("");

    let mut count = 0;
    for session in data.sessions.iter() {
        if count >= max_items {
            break;
        }
        if *row >= max_row {
            break;
        }

        let is_active = session.session_id == active_id;
        let marker = if is_active { ">" } else { "-" };
        let model_id = session.model.as_deref().unwrap_or("");
        let tokens = session.session_total_tokens.unwrap_or(0);
        let model_str = session
            .model_display
            .as_ref()
            .map(|d| crate::cost::model_display_with_context(model_id, d, tokens))
            .unwrap_or_else(|| "Claude".to_string());
        let model = model_str.as_str();

        if layout == LayoutMode::Minimal {
            let line = format!("{} {} | {}", marker, session.project_name, model);
            write_line(out, row, max_row, width, &line)?;
        } else {
            let branch_str = session
                .git_branch
                .as_deref()
                .map(|b| format!(" | {}", b))
                .unwrap_or_default();
            let line = format!(
                "  {} {} {} | {}",
                marker, session.project_name, branch_str, model
            );
            write_line(out, row, max_row, width, &line)?;

            if layout == LayoutMode::Full {
                let activity_text = session
                    .activity
                    .as_ref()
                    .map(|a| a.action_text())
                    .unwrap_or("Idle");
                let tokens = session.session_total_tokens.unwrap_or(0);
                let detail = format!(
                    "    {} | Session total {} | {}",
                    activity_text,
                    crate::util::format_tokens(tokens),
                    if session.total_cost > 0.0 {
                        format!("Cost: {}", format_cost(session.total_cost))
                    } else {
                        String::new()
                    }
                );
                write_line(out, row, max_row, width, &detail)?;
            }
        }

        count += 1;
    }

    Ok(())
}

fn render_footer(out: &mut impl Write, width: usize, height: u16) -> Result<()> {
    let controls = "[P] Privacy  [R] Refresh  [Q] Quit";
    let credits = "XT0N1.T3CH";

    let row = height.saturating_sub(2);
    out.queue(MoveTo(0, row))?;

    // Controls line
    let padding = width.saturating_sub(controls.len() + credits.len());
    out.queue(SetForegroundColor(Color::DarkYellow))?;
    out.queue(Print(controls))?;
    out.queue(ResetColor)?;
    out.queue(Print(" ".repeat(padding)))?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print(credits))?;
    out.queue(ResetColor)?;

    Ok(())
}

fn render_setup(out: &mut impl Write, step: u8, width: usize, height: u16) -> Result<()> {
    out.queue(Clear(ClearType::All))?;
    let mut row: u16 = 2;
    let max_row = height.saturating_sub(2);

    write_line_colored(
        out,
        &mut row,
        max_row,
        width,
        "Setup Wizard",
        Color::DarkYellow,
    )?;
    write_line(out, &mut row, max_row, width, "")?;

    match step {
        0 => {
            write_line(
                out,
                &mut row,
                max_row,
                width,
                "Select your Claude Code plan:",
            )?;
            write_line(out, &mut row, max_row, width, "")?;
            write_line(out, &mut row, max_row, width, "  [1] Free")?;
            write_line(out, &mut row, max_row, width, "  [2] Pro ($20/mo)")?;
            write_line(out, &mut row, max_row, width, "  [3] Max 5x ($100/mo)")?;
            write_line(out, &mut row, max_row, width, "  [4] Max 20x ($200/mo)")?;
            write_line(out, &mut row, max_row, width, "")?;
            write_line(out, &mut row, max_row, width, "  [Esc] Skip")?;
        }
        1 => {
            write_line(out, &mut row, max_row, width, "Privacy preferences:")?;
            write_line(out, &mut row, max_row, width, "")?;
            write_line(
                out,
                &mut row,
                max_row,
                width,
                "  [1] Show everything (project, model, tokens, cost)",
            )?;
            write_line(
                out,
                &mut row,
                max_row,
                width,
                "  [2] Privacy mode (hide details)",
            )?;
            write_line(out, &mut row, max_row, width, "")?;
            write_line(out, &mut row, max_row, width, "  [Esc] Back")?;
        }
        _ => {
            write_line(out, &mut row, max_row, width, "Setup complete!")?;
        }
    }

    out.flush()?;
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn write_line(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    text: &str,
) -> Result<()> {
    if *row >= max_row {
        return Ok(());
    }
    out.queue(MoveTo(0, *row))?;
    let display = truncate(text, width);
    out.queue(Print(&display))?;
    *row += 1;
    Ok(())
}

fn write_line_colored(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    text: &str,
    color: Color,
) -> Result<()> {
    if *row >= max_row {
        return Ok(());
    }
    out.queue(MoveTo(0, *row))?;
    out.queue(SetForegroundColor(color))?;
    out.queue(SetAttribute(Attribute::Bold))?;
    let display = truncate(text, width);
    out.queue(Print(&display))?;
    out.queue(ResetColor)?;
    out.queue(SetAttribute(Attribute::Reset))?;
    *row += 1;
    Ok(())
}

fn write_hr(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    title: &str,
) -> Result<()> {
    if *row >= max_row {
        return Ok(());
    }
    let dashes_left = 2usize;
    let title_len = title.chars().count();
    // "── " + title + " ──────────..."
    let dashes_right = width.saturating_sub(dashes_left + title_len + 2);
    out.queue(MoveTo(0, *row))?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print("─".repeat(dashes_left)))?;
    out.queue(Print(" "))?;
    out.queue(SetForegroundColor(MASCOT))?;
    out.queue(SetAttribute(Attribute::Bold))?;
    out.queue(Print(title))?;
    out.queue(SetAttribute(Attribute::Reset))?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print(" "))?;
    out.queue(Print("─".repeat(dashes_right)))?;
    out.queue(ResetColor)?;
    *row += 1;
    Ok(())
}

/// Returns the indicator dot and its color for a Discord connection status string.
fn discord_status_indicator(status: &str) -> (&'static str, Color) {
    if status.starts_with("Connected") {
        ("●", Color::Green)
    } else if status.starts_with("Reconnecting") {
        ("◌", Color::Yellow)
    } else {
        ("○", Color::DarkGrey)
    }
}

/// Like `write_kv` but renders a colored icon before the value text.
#[allow(clippy::too_many_arguments)]
fn write_kv_icon(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    key: &str,
    icon: &str,
    icon_color: Color,
    value: &str,
) -> Result<()> {
    if *row >= max_row {
        return Ok(());
    }
    out.queue(MoveTo(0, *row))?;
    out.queue(SetForegroundColor(Color::White))?;
    out.queue(Print(format!("  {:<12}", key)))?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print(": "))?;
    out.queue(SetForegroundColor(icon_color))?;
    out.queue(Print(icon))?;
    out.queue(Print(" "))?;
    out.queue(ResetColor)?;
    let icon_len = icon.chars().count();
    let max_val = width.saturating_sub(16 + icon_len + 1);
    let val_display = truncate(value, max_val);
    out.queue(Print(&val_display))?;
    *row += 1;
    Ok(())
}

/// Color for usage percentage (how much you've USED): green=low, yellow=medium, red=high
fn usage_color(utilization: f64) -> Color {
    if utilization <= 40.0 {
        Color::Green
    } else if utilization <= 70.0 {
        Color::Yellow
    } else {
        Color::Red
    }
}

/// Color for remaining percentage (how much is LEFT): green=plenty, yellow=getting low, red=critical
fn remaining_color(remaining: f64) -> Color {
    if remaining >= 60.0 {
        Color::Green
    } else if remaining >= 30.0 {
        Color::Yellow
    } else {
        Color::Red
    }
}

fn write_colored_bar(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    _width: usize,
    label: &str,
    percent: f64,
    bar_width: usize,
    reset_text: &str,
    color: Color,
) -> Result<()> {
    if *row >= max_row {
        return Ok(());
    }
    let filled = ((percent.clamp(0.0, 100.0) / 100.0) * bar_width as f64).round() as usize;
    let empty = bar_width.saturating_sub(filled);

    out.queue(MoveTo(0, *row))?;
    out.queue(SetForegroundColor(Color::White))?;
    out.queue(Print(format!("  {} ", label)))?;
    out.queue(SetForegroundColor(color))?;
    out.queue(SetAttribute(Attribute::Bold))?;
    out.queue(Print(format!("{:>3.0}%", percent)))?;
    out.queue(SetAttribute(Attribute::Reset))?;
    out.queue(Print(" "))?;
    out.queue(SetForegroundColor(color))?;
    out.queue(Print("█".repeat(filled)))?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print("░".repeat(empty)))?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print(format!(" reset {}", reset_text)))?;
    out.queue(ResetColor)?;
    *row += 1;
    Ok(())
}

fn write_cost_bar(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    label: &str,
    cost: f64,
    pct: f64,
    bar_width: usize,
) -> Result<()> {
    if *row >= max_row {
        return Ok(());
    }
    let filled = ((pct.clamp(0.0, 100.0) / 100.0) * bar_width as f64).round() as usize;
    let empty = bar_width.saturating_sub(filled);
    let color = cost_proportion_color(pct);

    out.queue(MoveTo(0, *row))?;
    out.queue(SetForegroundColor(Color::White))?;
    out.queue(Print(format!("  {:<10}", label)))?;
    out.queue(SetForegroundColor(color))?;
    out.queue(Print(format!("{} ", format_cost(cost))))?;
    out.queue(Print("█".repeat(filled)))?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print("░".repeat(empty)))?;
    out.queue(ResetColor)?;
    out.queue(Print(format!(" {:.0}%", pct)))?;
    *row += 1;
    Ok(())
}

/// Color for cost proportion: biggest cost contributors are red/yellow
fn cost_proportion_color(pct: f64) -> Color {
    if pct >= 50.0 {
        Color::Red
    } else if pct >= 25.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn write_kv(
    out: &mut impl Write,
    row: &mut u16,
    max_row: u16,
    width: usize,
    key: &str,
    value: &str,
) -> Result<()> {
    if *row >= max_row {
        return Ok(());
    }
    out.queue(MoveTo(0, *row))?;
    out.queue(SetForegroundColor(Color::White))?;
    out.queue(Print(format!("  {:<12}", key)))?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print(": "))?;
    out.queue(ResetColor)?;
    let max_val = width.saturating_sub(16);
    let val_display = truncate(value, max_val);
    out.queue(Print(&val_display))?;
    *row += 1;
    Ok(())
}
