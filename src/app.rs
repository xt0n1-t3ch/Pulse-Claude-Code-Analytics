use std::env;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use std::{io, io::IsTerminal};

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use tracing::debug;

use crate::config::{self, PresenceConfig, RuntimeSettings};
use crate::discord::DiscordPresence;
use crate::metrics::MetricsTracker;
use crate::process_guard::{self, RunningState};
use crate::session::{
    collect_active_sessions_multi, latest_limits_source, preferred_active_session,
    read_statusline_data, ClaudeSessionSnapshot, GitBranchCache, RateLimits, SessionParseCache,
};
use crate::ui::{self, RenderData};
use crate::usage::UsageManager;
use crate::util::{format_time_until, format_token_triplet};

const RELAUNCH_GUARD_ENV: &str = "CC_PRESENCE_TERMINAL_RELAUNCHED";

#[derive(Debug, Clone)]
pub enum AppMode {
    SmartForeground,
    ClaudeChild { args: Vec<String> },
}

pub fn run(config: PresenceConfig, mode: AppMode, runtime: RuntimeSettings) -> Result<()> {
    match mode {
        AppMode::SmartForeground => run_foreground_tui(config, runtime),
        AppMode::ClaudeChild { args } => run_claude_wrapper(config, runtime, args),
    }
}

pub fn print_status(config: &PresenceConfig) -> Result<()> {
    let runtime = config::runtime_settings();
    let projects_roots = config::projects_paths();
    let mut cache = GitBranchCache::new(Duration::from_secs(30));
    let mut parse_cache = SessionParseCache::default();
    let sessions = collect_active_sessions_multi(
        &projects_roots,
        runtime.stale_threshold,
        runtime.active_sticky_window,
        &mut cache,
        &mut parse_cache,
    )?;
    let running = process_guard::inspect_running_instance()?;
    let (is_running, running_pid) = match running {
        RunningState::NotRunning => (false, None),
        RunningState::Running { pid } => (true, pid),
    };

    let mut usage_mgr = UsageManager::new();
    let usage = usage_mgr.get_usage();

    println!("cc-discord-presence status");
    println!("running: {is_running}");
    if let Some(pid) = running_pid {
        println!("pid: {pid}");
    }
    println!("config: {}", config::config_path().display());
    print_path_list("projects_dirs", &projects_roots);
    println!(
        "discord_client_id: {}",
        if config.effective_client_id().is_some() {
            "configured"
        } else {
            "missing"
        }
    );
    if let Some(plan_name) = config.plan_display_name() {
        println!("plan: {plan_name}");
    }
    println!("active_sessions: {}", sessions.len());

    // Check statusline data source
    let statusline = read_statusline_data(&mut cache);
    if let Some(ref sl) = statusline {
        println!("statusline_source: active (session {})", sl.session_id);
    }

    if let Some(ref usage) = usage {
        println!(
            "api_usage: 5h {:.0}% used | 7d {:.0}% used",
            usage.five_hour.utilization, usage.seven_day.utilization
        );
    }

    if let Some(active) = preferred_active_session(&sessions).or(statusline.as_ref()) {
        let limits_source = latest_limits_source(&sessions);
        if let Some(source) = limits_source {
            println!("limits_source_session: {}", source.session_id);
        }
        print_active_summary(
            active,
            limits_source.map(|source| &source.limits),
            config.privacy.show_activity,
            config.privacy.show_activity_target,
        );
    }
    Ok(())
}

pub fn doctor(config: &PresenceConfig) -> Result<u8> {
    let mut issues = 0u8;
    let projects_roots = config::projects_paths();
    let existing_roots: Vec<&PathBuf> =
        projects_roots.iter().filter(|path| path.exists()).collect();

    println!("cc-discord-presence doctor");
    println!("config_path: {}", config::config_path().display());
    println!(
        "statusline_path: {}",
        config::statusline_data_path().display()
    );
    println!("credentials_path: {}", config::credentials_path().display());
    print_path_list("projects_paths", &projects_roots);

    if existing_roots.is_empty() {
        issues += 1;
        println!("[WARN] No discovered Claude Code sessions directory is currently accessible.");
    } else {
        println!(
            "[OK] Discovered {} accessible projects root(s).",
            existing_roots.len()
        );
    }

    if config.effective_client_id().is_none() {
        issues += 1;
        println!("[WARN] Discord client id not configured.");
    } else {
        println!("[OK] Discord client id configured.");
    }

    if config::statusline_data_path().exists() {
        println!("[OK] Statusline data file exists.");
    } else {
        println!("[INFO] Statusline data file not found (will fall back to JSONL parsing).");
    }

    if config::credentials_path().exists() {
        println!("[OK] Anthropic credentials file exists.");
    } else {
        println!("[INFO] Credentials file not found (API usage tracking unavailable).");
    }

    if command_available("claude") {
        println!("[OK] claude command available.");
    } else if !existing_roots.is_empty() {
        println!(
            "[INFO] claude command not found in PATH (session-file monitoring can still work)."
        );
    } else {
        issues += 1;
        println!("[WARN] claude command not found in PATH.");
    }

    if command_available("git") {
        println!("[OK] git command available.");
    } else {
        issues += 1;
        println!("[WARN] git command not found in PATH.");
    }

    if let Some(plan) = config.plan_display_name() {
        println!("[OK] Plan: {plan}");
    } else {
        println!("[INFO] No plan configured (run setup wizard to set one).");
    }

    if issues == 0 {
        println!("Doctor: healthy");
        Ok(0)
    } else {
        println!("Doctor: {issues} issue(s) found");
        Ok(1)
    }
}

fn run_foreground_tui(mut config: PresenceConfig, runtime: RuntimeSettings) -> Result<()> {
    let stop = install_stop_signal()?;
    if !io::stdout().is_terminal() {
        if maybe_relaunch_in_terminal()? {
            return Ok(());
        }
        return run_headless_foreground(config, runtime, stop);
    }

    let mut git_cache = GitBranchCache::new(Duration::from_secs(30));
    let mut parse_cache = SessionParseCache::default();
    let mut discord = DiscordPresence::new(config.effective_client_id());
    let mut usage_mgr = UsageManager::new();
    let mut metrics_tracker = MetricsTracker::new();
    let projects_roots = config::projects_paths();
    let started = Instant::now();
    let mut last_tick = Instant::now() - runtime.poll_interval;
    let mut sessions: Vec<ClaudeSessionSnapshot> = Vec::new();
    let mut last_render_signature = String::new();
    let mut last_render_at = Instant::now() - Duration::from_secs(31);
    let mut force_redraw = true;
    let mut cached_usage = None;
    let mut last_extra_spent: Option<f64> = None;
    let mut extra_usage_alert_until: Option<Instant> = None;
    let mut setup_active = !config.initialized;
    let mut setup_step: u8 = 0;

    ui::enter_terminal()?;

    let mut run = || -> Result<()> {
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }

            if last_tick.elapsed() >= runtime.poll_interval {
                // Collect sessions from JSONL
                sessions = collect_active_sessions_multi(
                    &projects_roots,
                    runtime.stale_threshold,
                    runtime.active_sticky_window,
                    &mut git_cache,
                    &mut parse_cache,
                )?;

                // Check statusline data (priority source, merged with JSONL granularity)
                if let Some(statusline_session) = read_statusline_data(&mut git_cache) {
                    merge_statusline_into_sessions(&mut sessions, statusline_session);
                }

                // Update metrics tracker with latest session data
                metrics_tracker.update(&sessions);
                metrics_tracker.persist_if_due();

                let active = preferred_active_session(&sessions);
                let effective_limits = latest_limits_source(&sessions).map(|source| &source.limits);

                // Fetch API usage
                cached_usage = usage_mgr.get_usage();

                // Detect Extra Usage charge — play alert + trigger auto-toggle cycle
                if let Some(ref usage) = cached_usage {
                    if let Some(ref extra) = usage.extra_usage {
                        if let Some(spent) = extra.used_credits {
                            if let Some(prev) = last_extra_spent {
                                if (spent - prev).abs() > 0.001 {
                                    crate::sound::play_extra_usage_alert();
                                    extra_usage_alert_until =
                                        Some(Instant::now() + Duration::from_secs(5));
                                    if let Some(token) = usage_mgr.get_access_token() {
                                        crate::usage::spawn_extra_usage_toggle_cycle(token);
                                    }
                                }
                            }
                            last_extra_spent = Some(spent);
                        }
                    }
                }

                if let Err(err) =
                    discord.update(active, effective_limits, cached_usage.as_ref(), &config)
                {
                    debug!(error = %err, "discord presence update failed");
                }

                let plan_name = config.plan_display_name().map(|s| s.to_string());
                let render = RenderData {
                    running_for: started.elapsed(),
                    mode_label: "Smart Foreground",
                    discord_status: discord.status(),
                    client_id_configured: config.effective_client_id().is_some(),
                    poll_interval_secs: runtime.poll_interval.as_secs(),
                    stale_secs: runtime.stale_threshold.as_secs(),
                    show_activity: config.privacy.show_activity,
                    show_activity_target: config.privacy.show_activity_target,
                    logo_mode: config.display.terminal_logo_mode.clone(),
                    logo_path: config.display.terminal_logo_path.as_deref(),
                    active,
                    effective_limits,
                    api_usage: cached_usage.as_ref(),
                    plan_name: plan_name.as_deref(),
                    metrics: metrics_tracker.snapshot(),
                    sessions: &sessions,
                    config: &config,
                    setup_active,
                    setup_step,
                    extra_usage_alert: extra_usage_alert_until
                        .map_or(false, |t| t > Instant::now()),
                };
                let signature = ui::frame_signature(&render);
                let should_draw = force_redraw
                    || signature != last_render_signature
                    || last_render_at.elapsed() >= Duration::from_secs(30);
                if should_draw {
                    ui::draw(&render)?;
                    last_render_signature = signature;
                    last_render_at = Instant::now();
                    force_redraw = false;
                }
                last_tick = Instant::now();
            }

            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => {
                        if setup_active {
                            match handle_setup_key(
                                key.code,
                                &mut setup_step,
                                &mut config,
                                &mut setup_active,
                            ) {
                                SetupAction::Continue => {
                                    force_redraw = true;
                                }
                                SetupAction::Quit => break,
                                SetupAction::Noop => {}
                            }
                        } else {
                            match key.code {
                                KeyCode::Char('q') => break,
                                KeyCode::Char('c')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    break
                                }
                                KeyCode::Char('p') | KeyCode::Char('P') => {
                                    let enabled = config.toggle_privacy();
                                    let _ = config.save();
                                    debug!(privacy_enabled = enabled, "privacy toggled");
                                    force_redraw = true;
                                }
                                KeyCode::Char('s') | KeyCode::Char('S') => {
                                    // Setup wizard only via first-launch; no hotkey
                                }
                                KeyCode::Char('r') | KeyCode::Char('R') => {
                                    usage_mgr.invalidate_cache();
                                    last_tick = Instant::now() - runtime.poll_interval;
                                    force_redraw = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    Event::Resize(_, _) => {
                        force_redraw = true;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    };

    let run_result = run();
    discord.shutdown();
    let _ = ui::leave_terminal();
    run_result
}

fn run_headless_foreground(
    config: PresenceConfig,
    runtime: RuntimeSettings,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let mut git_cache = GitBranchCache::new(Duration::from_secs(30));
    let mut parse_cache = SessionParseCache::default();
    let mut discord = DiscordPresence::new(config.effective_client_id());
    let mut usage_mgr = UsageManager::new();
    let mut metrics_tracker = MetricsTracker::new();
    let projects_roots = config::projects_paths();
    println!("No interactive terminal detected; running in headless foreground mode.");
    println!("Press Ctrl+C to stop.");

    while !stop.load(Ordering::Relaxed) {
        let mut sessions = collect_active_sessions_multi(
            &projects_roots,
            runtime.stale_threshold,
            runtime.active_sticky_window,
            &mut git_cache,
            &mut parse_cache,
        )?;

        if let Some(statusline_session) = read_statusline_data(&mut git_cache) {
            merge_statusline_into_sessions(&mut sessions, statusline_session);
        }

        metrics_tracker.update(&sessions);
        metrics_tracker.persist_if_due();

        let active = preferred_active_session(&sessions);
        let effective_limits = latest_limits_source(&sessions).map(|source| &source.limits);
        let usage = usage_mgr.get_usage();
        if let Err(err) = discord.update(active, effective_limits, usage.as_ref(), &config) {
            debug!(error = %err, "discord presence update failed");
        }
        thread::sleep(runtime.poll_interval);
    }

    discord.shutdown();
    Ok(())
}

// ── Setup Wizard Key Handler ─────────────────────────────────────────────

#[allow(dead_code)]
enum SetupAction {
    Continue,
    Quit,
    Noop,
}

fn handle_setup_key(
    key: KeyCode,
    step: &mut u8,
    config: &mut PresenceConfig,
    setup_active: &mut bool,
) -> SetupAction {
    match *step {
        // Step 0: Plan selection
        0 => match key {
            KeyCode::Char('1') => {
                config.plan = Some("free".to_string());
                *step = 1;
                SetupAction::Continue
            }
            KeyCode::Char('2') => {
                config.plan = Some("pro".to_string());
                *step = 1;
                SetupAction::Continue
            }
            KeyCode::Char('3') => {
                config.plan = Some("max_5x".to_string());
                *step = 1;
                SetupAction::Continue
            }
            KeyCode::Char('4') => {
                config.plan = Some("max_20x".to_string());
                *step = 1;
                SetupAction::Continue
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                *setup_active = false;
                SetupAction::Continue
            }
            _ => SetupAction::Noop,
        },
        // Step 1: Privacy preferences
        1 => match key {
            KeyCode::Char('1') => {
                // Full visibility (default)
                config.privacy.enabled = false;
                config.initialized = true;
                let _ = config.save();
                *setup_active = false;
                SetupAction::Continue
            }
            KeyCode::Char('2') => {
                // Privacy mode
                config.privacy.enabled = true;
                config.initialized = true;
                let _ = config.save();
                *setup_active = false;
                SetupAction::Continue
            }
            KeyCode::Esc => {
                *step = 0;
                SetupAction::Continue
            }
            KeyCode::Char('q') => {
                *setup_active = false;
                SetupAction::Continue
            }
            _ => SetupAction::Noop,
        },
        _ => {
            *setup_active = false;
            SetupAction::Continue
        }
    }
}

// ── Claude Child Wrapper ─────────────────────────────────────────────────

fn run_claude_wrapper(
    config: PresenceConfig,
    runtime: RuntimeSettings,
    args: Vec<String>,
) -> Result<()> {
    let stop = install_stop_signal()?;
    let mut child = spawn_claude_child(args)?;
    let mut git_cache = GitBranchCache::new(Duration::from_secs(30));
    let mut parse_cache = SessionParseCache::default();
    let mut discord = DiscordPresence::new(config.effective_client_id());
    let mut usage_mgr = UsageManager::new();
    let mut metrics_tracker = MetricsTracker::new();
    let projects_roots = config::projects_paths();

    println!("claude child started; Discord presence tracking is active.");

    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            break;
        }

        let mut sessions = collect_active_sessions_multi(
            &projects_roots,
            runtime.stale_threshold,
            runtime.active_sticky_window,
            &mut git_cache,
            &mut parse_cache,
        )?;

        if let Some(statusline_session) = read_statusline_data(&mut git_cache) {
            merge_statusline_into_sessions(&mut sessions, statusline_session);
        }

        metrics_tracker.update(&sessions);
        metrics_tracker.persist_if_due();

        let active = preferred_active_session(&sessions);
        let effective_limits = latest_limits_source(&sessions).map(|source| &source.limits);
        let usage = usage_mgr.get_usage();
        if let Err(err) = discord.update(active, effective_limits, usage.as_ref(), &config) {
            debug!(error = %err, "discord presence update failed");
        }

        if let Some(status) = child
            .try_wait()
            .context("failed to query claude child status")?
        {
            println!("claude exited with status: {status}");
            break;
        }

        thread::sleep(runtime.poll_interval);
    }

    discord.shutdown();
    Ok(())
}

fn spawn_claude_child(args: Vec<String>) -> Result<Child> {
    let mut command = Command::new("claude");
    command
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
        .spawn()
        .context("failed to spawn `claude` child process")
}

// ── Terminal Relaunch ────────────────────────────────────────────────────

fn maybe_relaunch_in_terminal() -> Result<bool> {
    if env::var_os(RELAUNCH_GUARD_ENV).is_some() {
        return Ok(false);
    }

    let exe = env::current_exe().context("failed to resolve current executable path")?;
    let args: Vec<String> = env::args().skip(1).collect();

    #[cfg(windows)]
    {
        return relaunch_windows(&exe.display().to_string(), &args);
    }

    #[cfg(target_os = "macos")]
    {
        return relaunch_macos(&exe.display().to_string(), &args);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return Ok(relaunch_linux_like(&exe.display().to_string(), &args));
    }

    #[allow(unreachable_code)]
    Ok(false)
}

#[cfg(windows)]
fn relaunch_windows(exe: &str, args: &[String]) -> Result<bool> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x00000010;

    let mut cmd = Command::new(exe);
    cmd.args(args);
    cmd.env(RELAUNCH_GUARD_ENV, "1");
    cmd.creation_flags(CREATE_NEW_CONSOLE);

    match cmd.spawn() {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

#[cfg(target_os = "macos")]
fn relaunch_macos(exe: &str, args: &[String]) -> Result<bool> {
    let command = build_unix_shell_command(exe, args);
    let mut apple_script_command = String::new();
    for ch in command.chars() {
        match ch {
            '\\' => apple_script_command.push_str("\\\\"),
            '"' => apple_script_command.push_str("\\\""),
            _ => apple_script_command.push(ch),
        }
    }

    let status = Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "tell application \"Terminal\" to do script \"{apple_script_command}\""
        ))
        .arg("-e")
        .arg("tell application \"Terminal\" to activate")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(status.map(|s| s.success()).unwrap_or(false))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn relaunch_linux_like(exe: &str, args: &[String]) -> bool {
    let command = build_unix_shell_command(exe, args);
    let terminal_candidates: [(&str, &[&str]); 7] = [
        ("x-terminal-emulator", &["--", "bash", "-lc"]),
        ("gnome-terminal", &["--", "bash", "-lc"]),
        ("konsole", &["-e", "bash", "-lc"]),
        ("xfce4-terminal", &["--command", "bash -lc"]),
        ("alacritty", &["-e", "bash", "-lc"]),
        ("kitty", &["-e", "bash", "-lc"]),
        ("wezterm", &["start", "--", "bash", "-lc"]),
    ];

    for (terminal, prefix) in terminal_candidates {
        let spawned = if terminal == "xfce4-terminal" {
            Command::new(terminal)
                .arg(prefix[0])
                .arg(format!("bash -lc {}", shell_escape_single(&command)))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        } else {
            let mut cmd = Command::new(terminal);
            for part in prefix {
                cmd.arg(part);
            }
            cmd.arg(&command)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        };

        if spawned.is_ok() {
            return true;
        }
    }

    false
}

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
fn build_unix_shell_command(exe: &str, args: &[String]) -> String {
    use std::fmt::Write as _;

    let mut command = String::new();
    let _ = write!(
        command,
        "{RELAUNCH_GUARD_ENV}=1 {}",
        shell_escape_single(exe)
    );
    for arg in args {
        let _ = write!(command, " {}", shell_escape_single(arg));
    }
    command
}

#[cfg(any(target_os = "macos", all(unix, not(target_os = "macos"))))]
fn shell_escape_single(input: &str) -> String {
    format!("'{}'", input.replace('\'', "'\\''"))
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn print_active_summary(
    active: &ClaudeSessionSnapshot,
    effective_limits: Option<&RateLimits>,
    show_activity: bool,
    show_activity_target: bool,
) {
    println!("active_session:");
    println!("  project: {}", active.project_name);
    println!("  path: {}", active.cwd.display());
    println!("  model: {}", active.model.as_deref().unwrap_or("unknown"));
    if let Some(display) = &active.model_display {
        println!("  model_display: {display}");
    }
    println!(
        "  branch: {}",
        active.git_branch.as_deref().unwrap_or("n/a")
    );
    if show_activity {
        if let Some(activity) = &active.activity {
            println!("  activity: {}", activity.to_text(show_activity_target));
        }
    }
    if active.total_cost > 0.0 {
        println!("  cost: ${:.4}", active.total_cost);
    }
    println!(
        "  {}",
        format_token_triplet(
            active.session_delta_tokens,
            active.last_turn_tokens,
            active.session_total_tokens,
        )
    );

    let limits = effective_limits.unwrap_or(&active.limits);
    if let Some(primary) = &limits.primary {
        println!(
            "  5h remaining: {:.0}% (reset {})",
            primary.remaining_percent,
            format_time_until(primary.resets_at)
        );
    }
    if let Some(secondary) = &limits.secondary {
        println!(
            "  7d remaining: {:.0}% (reset {})",
            secondary.remaining_percent,
            format_time_until(secondary.resets_at)
        );
    }
}

fn command_available(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn merge_statusline_into_sessions(
    sessions: &mut Vec<ClaudeSessionSnapshot>,
    statusline: ClaudeSessionSnapshot,
) {
    let existing_idx = sessions
        .iter()
        .position(|s| s.session_id == statusline.session_id);
    if let Some(idx) = existing_idx {
        let jsonl = &sessions[idx];
        // Merge: statusline wins for cost/model/total, JSONL wins for granular token data + activity
        let merged = ClaudeSessionSnapshot {
            last_turn_tokens: statusline.last_turn_tokens.or(jsonl.last_turn_tokens),
            session_delta_tokens: statusline
                .session_delta_tokens
                .or(jsonl.session_delta_tokens),
            activity: statusline.activity.clone().or(jsonl.activity.clone()),
            last_token_event_at: statusline.last_token_event_at.or(jsonl.last_token_event_at),
            ..statusline
        };
        sessions[idx] = merged;
    } else {
        sessions.insert(0, statusline);
    }
}

fn print_path_list(label: &str, paths: &[PathBuf]) {
    println!("{label}:");
    for path in paths {
        println!("  - {}", path.display());
    }
}

fn install_stop_signal() -> Result<Arc<AtomicBool>> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    ctrlc::set_handler(move || {
        flag.store(true, Ordering::Relaxed);
    })
    .context("failed to install Ctrl+C handler")?;
    Ok(stop)
}
