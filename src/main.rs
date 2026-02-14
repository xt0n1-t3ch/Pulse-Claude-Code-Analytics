use std::io::IsTerminal;
use std::panic;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use cc_discord_presence::app::{self, AppMode};
use cc_discord_presence::cli::{Cli, Commands};
use cc_discord_presence::config::{self, PresenceConfig};
use cc_discord_presence::process_guard;
use cc_discord_presence::util::setup_tracing;

fn main() -> ExitCode {
    let from_explorer = is_launched_from_explorer();

    // Log diagnostics when launched from Explorer (helps debug close-on-launch issues)
    if from_explorer {
        log_diagnostic("from_explorer=true");
        log_diagnostic(&format!(
            "is_terminal: stdout={} stderr={}",
            std::io::stdout().is_terminal(),
            std::io::stderr().is_terminal()
        ));
    }

    // Catch panics so we can show them with pause() when from Explorer
    let result = panic::catch_unwind(|| run());

    let code = match result {
        Ok(Ok(code)) => {
            if from_explorer {
                log_diagnostic(&format!("exited normally code={code}"));
                eprintln!("[cc-discord-presence] Exited normally (code {code}).");
                pause();
            }
            ExitCode::from(code)
        }
        Ok(Err(err)) => {
            let msg = format!("{err:#}");
            log_diagnostic(&format!("error: {msg}"));
            eprintln!("cc-discord-presence error: {msg}");
            if from_explorer {
                pause();
            }
            ExitCode::from(1)
        }
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            log_diagnostic(&format!("PANIC: {msg}"));
            eprintln!("cc-discord-presence PANIC: {msg}");
            if from_explorer {
                pause();
            }
            ExitCode::from(2)
        }
    };
    code
}

fn run() -> Result<u8> {
    setup_tracing();
    let cli = Cli::parse();
    let config = PresenceConfig::load_or_init()?;

    match cli.command {
        Some(Commands::Status) => {
            app::print_status(&config)?;
            Ok(0)
        }
        Some(Commands::Doctor) => app::doctor(&config),
        Some(Commands::Claude { args }) => {
            let acquired = process_guard::acquire_or_takeover_single_instance()?;
            if let Some(pid) = acquired.takeover_pid {
                println!("Existing instance detected (PID {pid}); takeover completed.");
            }
            let _guard = acquired.guard;
            let runtime = config::runtime_settings();
            app::run(config, AppMode::ClaudeChild { args }, runtime)?;
            Ok(0)
        }
        None => {
            let acquired = process_guard::acquire_or_takeover_single_instance()?;
            if let Some(pid) = acquired.takeover_pid {
                println!("Existing instance detected (PID {pid}); takeover completed.");
            }
            let _guard = acquired.guard;
            let runtime = config::runtime_settings();
            app::run(config, AppMode::SmartForeground, runtime)?;
            Ok(0)
        }
    }
}

/// Detect if we were launched by double-clicking from Windows Explorer.
///
/// Uses `GetConsoleProcessList` to count processes sharing this console.
/// On Windows 10+, the count includes both our process AND `conhost.exe`,
/// so a fresh Explorer-launched console has count=2 (not 1).
/// From an existing terminal (cmd/powershell), count is typically 3+.
#[cfg(windows)]
fn is_launched_from_explorer() -> bool {
    unsafe extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }
    let mut pids = [0u32; 8];
    let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) };
    // 0 = error (treat as Explorer), 1 = just us, 2 = us + conhost.exe
    count <= 2
}

#[cfg(not(windows))]
fn is_launched_from_explorer() -> bool {
    false
}

fn pause() {
    use std::io::{self, BufRead};
    eprintln!();
    eprintln!("Press Enter to close...");
    let _ = io::stdin().lock().read_line(&mut String::new());
}

/// Write a diagnostic line to a log file next to the config.
/// Best-effort — silently ignores write failures.
fn log_diagnostic(msg: &str) {
    use std::io::Write;
    let log_path = config::claude_home().join("cc-discord-presence-debug.log");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let _ = writeln!(file, "[{timestamp}] {msg}");
    }
}
