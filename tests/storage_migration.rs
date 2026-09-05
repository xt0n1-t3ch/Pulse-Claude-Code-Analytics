use std::fs;
use std::process::Command;

#[test]
fn daemon_startup_uses_pulse_home_and_keeps_provider_roots_separate() {
    let root = tempfile::tempdir().unwrap();
    let claude = root.path().join("claude-source");
    let codex = root.path().join("codex-source");
    let pulse = root.path().join("pulse-state");
    fs::create_dir(&claude).unwrap();
    fs::create_dir(&codex).unwrap();
    fs::write(
        claude.join("pulse-provider.json"),
        r#"{"active_provider":"opencode"}"#,
    )
    .unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_cc-discord-presence"))
            .arg("status")
            .env("PULSE_HOME", &pulse)
            .env("CLAUDE_HOME", &claude)
            .env("CODEX_HOME", &codex)
            .env("CC_PRESENCE_INCLUDE_WSL", "0")
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(pulse.join("legacy-migration-v1.json").exists());
    assert!(pulse.join("claude/discord-presence-config.json").exists());
    assert!(!claude.join("discord-presence-config.json").exists());
    assert_eq!(
        fs::read(pulse.join("pulse-provider.json")).unwrap(),
        fs::read(claude.join("pulse-provider.json")).unwrap()
    );
    fs::write(
        claude.join("pulse-provider.json"),
        r#"{"active_provider":"codex"}"#,
    )
    .unwrap();
    assert!(run().status.success());
    assert!(
        fs::read_to_string(pulse.join("pulse-provider.json"))
            .unwrap()
            .contains("opencode")
    );
}

#[test]
fn relative_pulse_home_fails_before_writing_state() {
    let root = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_cc-discord-presence"))
        .arg("status")
        .current_dir(root.path())
        .env("PULSE_HOME", "relative-state")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("PULSE_HOME must be an absolute path")
    );
    assert!(!root.path().join("relative-state").exists());
}
