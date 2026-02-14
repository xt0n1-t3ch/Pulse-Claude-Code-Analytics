use std::io::{self, Write};
use std::time::Duration;

use anyhow::Result;
use crossterm::cursor::MoveTo;
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, QueueableCommand};

use crate::config::{PresenceConfig, TerminalLogoMode};
use crate::session::{ClaudeSessionSnapshot, RateLimits};
use crate::usage::UsageData;
use crate::util::{
    format_cost, format_time_until, format_time_until_reset, format_token_triplet, human_duration,
    now_local, truncate,
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
    pub sessions: &'a [ClaudeSessionSnapshot],
    pub config: &'a PresenceConfig,
    pub setup_active: bool,
    pub setup_step: u8,
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
            for line in &CLAUDE_ASCII {
                write_line(out, row, max_row, width, line)?;
            }
            write_line(out, row, max_row, width, "")?;
        }
        LayoutMode::Compact => {
            for line in &CC_ASCII {
                write_line(out, row, max_row, width, line)?;
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
    write_kv(out, row, max_row, width, "Discord", data.discord_status)?;

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
        let line = format!(
            "5h {:.0}% | 7d {:.0}%",
            usage.five_hour.utilization, usage.seven_day.utilization
        );
        write_line(out, row, max_row, width, &line)?;
        return Ok(());
    }

    write_hr(out, row, max_row, width, "Usage")?;

    let bar_width = if width > 40 { 20 } else { 10 };
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
        write_kv(out, row, max_row, width, "Model", model)?;
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
        let model = session.model_display.as_deref().unwrap_or("Claude");

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
    let dashes_left = 5;
    let dashes_right = width.saturating_sub(dashes_left + title.len() + 4);
    let line = format!(
        "{} {} {}",
        "-".repeat(dashes_left),
        title,
        "-".repeat(dashes_right)
    );
    out.queue(MoveTo(0, *row))?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print(&line))?;
    out.queue(ResetColor)?;
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
    let filled_str = "#".repeat(filled);
    let empty_str = "-".repeat(empty);

    out.queue(MoveTo(0, *row))?;
    out.queue(Print(format!("  {} [", label)))?;
    out.queue(SetForegroundColor(color))?;
    out.queue(SetAttribute(Attribute::Bold))?;
    out.queue(Print(format!("{:>3.0}%", percent)))?;
    out.queue(SetAttribute(Attribute::Reset))?;
    out.queue(Print("] "))?;
    out.queue(SetForegroundColor(color))?;
    out.queue(Print(&filled_str))?;
    out.queue(ResetColor)?;
    out.queue(SetForegroundColor(Color::DarkGrey))?;
    out.queue(Print(&empty_str))?;
    out.queue(ResetColor)?;
    out.queue(Print(format!(" reset {}", reset_text)))?;
    *row += 1;
    Ok(())
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
    let line = format!("  {:<12}: {}", key, value);
    write_line(out, row, max_row, width, &line)?;
    Ok(())
}
