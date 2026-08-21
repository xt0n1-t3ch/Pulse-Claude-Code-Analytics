//! Snapshot degradation contract, proven against the real IPC payload.
//!
//! These tests run as their own binary so fault injection cannot contaminate
//! other suites: one fixture poisons nothing but still fails the Discord
//! settings read, and every assertion below inspects exactly what the
//! frontend receives from the `get_app_snapshot` command.
//!
//! Contract under test: a failure of an ancillary subsystem (Discord presence
//! configuration) must degrade only its own payload. Health, metrics,
//! sessions, access, rate limits, preview, and plan telemetry stay intact,
//! and the degraded field is identified as degraded instead of being replaced
//! with fabricated defaults.

use serde_json::Value;
use std::sync::Mutex;

/// `CLAUDE_HOME` / `CODEX_HOME` are process-global, so tests in this binary
/// must serialize their home redirections.
static HOME_ENV_LOCK: Mutex<()> = Mutex::new(());

fn isolated_homes(tag: &str) -> (std::sync::MutexGuard<'static, ()>, std::path::PathBuf) {
    let guard = HOME_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let temp = std::env::temp_dir().join(format!(
        "pulse-snapshot-degradation-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&temp);
    let claude_home = temp.join("claude");
    let codex_home = temp.join("codex");
    std::fs::create_dir_all(&claude_home).expect("claude home");
    std::fs::create_dir_all(&codex_home).expect("codex home");
    unsafe {
        std::env::set_var("CLAUDE_HOME", &claude_home);
        std::env::set_var("CODEX_HOME", &codex_home);
    }
    (guard, claude_home)
}

fn core_payload_is_intact(snapshot: &Value) {
    assert_eq!(snapshot["revision"], 1, "snapshot revision");
    assert_eq!(snapshot["sync_state"], "live", "sync state");
    assert!(
        snapshot["health"].is_object() && snapshot["health"]["version"].is_string(),
        "health telemetry must survive: {}",
        snapshot["health"]
    );
    assert!(
        snapshot["metrics"].is_object(),
        "metrics telemetry must survive"
    );
    assert!(
        snapshot["sessions"].is_array(),
        "session telemetry must survive"
    );
    assert!(
        snapshot["access"].is_object(),
        "access telemetry must survive"
    );
    assert!(
        snapshot["discord_preview"].is_object(),
        "presence preview must survive"
    );
    assert!(snapshot["plan"].is_object(), "plan info must survive");
}

#[test]
fn malformed_presence_config_degrades_only_the_discord_settings_payload() {
    let (_guard, claude_home) = isolated_homes("malformed");
    pulse::commands::set_active_provider("claude".to_string()).expect("save Claude provider");

    let config_path = claude_home.join("discord-presence-config.json");
    std::fs::write(&config_path, b"{ not valid json !!").expect("write malformed config");

    // Direct settings read reports the truth about itself...
    let read_error = pulse::commands::get_discord_settings()
        .expect_err("malformed config must fail the dedicated settings read");
    assert!(
        read_error.contains("invalid JSON"),
        "unexpected error shape: {read_error}"
    );

    // ...but the application snapshot used to propagate that same failure and
    // destroy otherwise-valid analytics telemetry with it.
    let snapshot = pulse::commands::get_app_snapshot().expect("core snapshot must survive");

    assert!(
        snapshot["discord_settings"].is_null(),
        "unreadable config must not be reported as readable settings: {}",
        snapshot["discord_settings"]
    );
    let diagnostic = snapshot["discord_settings_error"]
        .as_str()
        .expect("degraded payload must carry the read error as a diagnostic");
    assert!(
        diagnostic.contains("invalid JSON"),
        "diagnostic must identify the real failure: {diagnostic}"
    );

    core_payload_is_intact(&snapshot);
}

#[test]
fn unreadable_presence_config_degrades_and_recovers_after_repair() {
    let (_guard, claude_home) = isolated_homes("unreadable");
    pulse::commands::set_active_provider("claude".to_string()).expect("save Claude provider");

    let config_path = claude_home.join("discord-presence-config.json");
    std::fs::write(&config_path, "{}").expect("seed valid config");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o000))
            .expect("strip permissions");
    }

    if pulse::commands::get_discord_settings().is_ok() {
        // Root (or a filesystem ignoring permissions) can still read the file,
        // so the permission fault cannot be injected on this machine.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o644));
        }
        return;
    }

    let snapshot = pulse::commands::get_app_snapshot()
        .expect("core snapshot must survive an unreadable config");
    assert!(snapshot["discord_settings"].is_null());
    assert!(
        snapshot["discord_settings_error"].is_string(),
        "the unreadable-config fault must be diagnosable: {}",
        snapshot["discord_settings_error"]
    );
    core_payload_is_intact(&snapshot);

    // Recovery: repairing the underlying condition restores the full payload.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o644))
            .expect("restore permissions");
    }
    let recovered = pulse::commands::get_app_snapshot().expect("recovered snapshot");
    assert!(
        recovered["discord_settings"].is_object(),
        "repairing the config must restore readable settings: {}",
        recovered["discord_settings"]
    );
    assert!(
        recovered["discord_settings_error"].is_null(),
        "a healthy read must not carry a stale diagnostic: {}",
        recovered["discord_settings_error"]
    );
}

/// Provider quota authentication absence is independent of transport health:
/// no authenticated usage source must leave the snapshot build successful,
/// with no fabricated provider state, even while every route is unproofed.
#[test]
fn missing_authenticated_usage_source_does_not_fail_the_snapshot() {
    let (_guard, _claude_home) = isolated_homes("no-auth");
    pulse::commands::set_active_provider("claude".to_string()).expect("save Claude provider");

    let snapshot = pulse::commands::get_app_snapshot().expect("no-auth snapshot");
    let routes = snapshot["access"]["routes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        routes
            .iter()
            .all(|route| route["source"]["proof"] != "quota_response"),
        "fixture must not carry authenticated quota proof: {routes:?}"
    );
    assert!(
        snapshot["discord_settings"].is_object() && snapshot["discord_settings_error"].is_null(),
        "an absent usage source says nothing about Discord settings health"
    );
}
