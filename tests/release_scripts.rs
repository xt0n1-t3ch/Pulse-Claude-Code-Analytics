use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const VENDORED_FILES: [&str; 11] = [
    "config.rs",
    "cost.rs",
    "discord.rs",
    "model.rs",
    "session.rs",
    "util.rs",
    "session/activity.rs",
    "session/parser.rs",
    "telemetry/limits.rs",
    "telemetry/plan.rs",
    "telemetry/service_tier.rs",
];

#[test]
fn vendoring_integrity_is_offline_and_accepts_matching_hashes() {
    let fixture = manifest_fixture(EMPTY_SHA256);

    let mut command = script_command("check-codex-rich-presence-upstream.ps1");
    command.arg("-Root").arg(fixture.path()).env("PATH", "");
    let output = command.output().expect("run integrity checker");

    assert_success(&output);
    assert!(stdout(&output).contains("integrity verified"));
}

#[test]
fn vendoring_integrity_rejects_malformed_manifest() {
    let fixture = TempDir::new().expect("fixture");
    write(fixture.path().join("src/codex/UPSTREAM.json"), "{oops");

    let output = script_command("check-codex-rich-presence-upstream.ps1")
        .arg("-Root")
        .arg(fixture.path())
        .output()
        .expect("run integrity checker");

    assert_failure_contains(&output, "not valid JSON");
}

#[test]
fn vendoring_integrity_rejects_legacy_manifest_without_hash_contract() {
    let fixture = TempDir::new().expect("fixture");
    write(
        fixture.path().join("src/codex/UPSTREAM.json"),
        r#"{"repository":"https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence","branch":"main","commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
    );

    let output = script_command("check-codex-rich-presence-upstream.ps1")
        .arg("-Root")
        .arg(fixture.path())
        .output()
        .expect("run integrity checker");

    assert_failure_contains(&output, "schema_version 2 and sync_version 2");
}

#[test]
fn vendoring_integrity_rejects_mismatched_file_hash() {
    let fixture = manifest_fixture(&"0".repeat(64));

    let output = script_command("check-codex-rich-presence-upstream.ps1")
        .arg("-Root")
        .arg(fixture.path())
        .output()
        .expect("run integrity checker");

    assert_failure_contains(&output, "hash mismatch");
}

#[test]
fn updater_syncs_explicit_tag_and_commit_without_touching_pulse_adapters() {
    let canonical = canonical_fixture();
    let commit = git_output(canonical.path(), ["rev-parse", "HEAD"]);
    let pulse = pulse_fixture(true);
    let process_before = fs::read(pulse.path().join("src/codex/process.rs")).expect("process");
    let module_before = fs::read(pulse.path().join("src/codex/mod.rs")).expect("module");

    let output = update_command(canonical.path(), pulse.path(), "v1.7.2", &commit)
        .output()
        .expect("run updater");

    assert_success(&output);
    assert_eq!(
        fs::read(pulse.path().join("src/codex/process.rs")).expect("process"),
        process_before
    );
    assert_eq!(
        fs::read(pulse.path().join("src/codex/mod.rs")).expect("module"),
        module_before
    );

    let cost = read(pulse.path().join("src/codex/cost.rs"));
    assert_eq!(cost.matches("pub fn speed_multiplier").count(), 1);
    assert!(cost.contains("crate::codex::config"));

    let manifest = read(pulse.path().join("src/codex/UPSTREAM.json"));
    assert!(manifest.contains("\"schema_version\": 2"));
    assert!(manifest.contains("\"ref\": \"v1.7.2\""));
    assert!(manifest.contains(&commit));
    assert_eq!(manifest.matches("\"source_sha256\"").count(), 11);
    assert_eq!(manifest.matches("\"sha256\"").count(), 11);
    assert!(manifest.contains("\"local_adapters\""));

    let integrity = script_command("check-codex-rich-presence-upstream.ps1")
        .arg("-Root")
        .arg(pulse.path())
        .output()
        .expect("run integrity checker");
    assert_success(&integrity);
}

#[test]
fn updater_rejects_missing_process_compatibility_before_writing() {
    let canonical = canonical_fixture();
    let commit = git_output(canonical.path(), ["rev-parse", "HEAD"]);
    let pulse = pulse_fixture(false);
    let sentinel = pulse.path().join("src/codex/config.rs");
    write(&sentinel, "keep me\n");

    let output = update_command(canonical.path(), pulse.path(), "v1.7.2", &commit)
        .output()
        .expect("run updater");

    assert_failure_contains(&output, "is_desktop_surface_running");
    assert_eq!(read(sentinel), "keep me\n");
}

#[test]
fn updater_rejects_mutable_or_mismatched_refs() {
    let canonical = canonical_fixture();
    let commit = git_output(canonical.path(), ["rev-parse", "HEAD"]);
    let pulse = pulse_fixture(true);

    let malformed = update_command(canonical.path(), pulse.path(), "main", &commit)
        .output()
        .expect("run updater");
    assert_failure_contains(&malformed, "semantic version tag");

    let mismatch = update_command(canonical.path(), pulse.path(), "v1.7.2", &"0".repeat(40))
        .output()
        .expect("run updater");
    assert_failure_contains(&mismatch, "does not resolve to commit");
}

#[test]
fn release_contract_accepts_stable_and_prerelease_tags() {
    let stable = release_fixture("1.5.2");
    let stable_output = release_contract_command(stable.path(), "v1.5.2")
        .output()
        .expect("run release check");
    assert_success(&stable_output);

    let prerelease = release_fixture("1.5.3-rc.1");
    let prerelease_output = release_contract_command(prerelease.path(), "v1.5.3-rc.1")
        .output()
        .expect("run release check");
    assert_success(&prerelease_output);
}

#[test]
fn release_contract_rejects_malformed_or_mismatched_tags() {
    let fixture = release_fixture("1.5.2");

    let malformed = release_contract_command(fixture.path(), "release-1.5.2")
        .output()
        .expect("run release check");
    assert_failure_contains(&malformed, "semantic version tag");

    for tag in ["v01.5.2", "v1.5.2-rc..1"] {
        let malformed = release_contract_command(fixture.path(), tag)
            .output()
            .expect("run release check");
        assert_failure_contains(&malformed, "semantic version tag");
    }

    let mismatch = release_contract_command(fixture.path(), "v1.5.3")
        .output()
        .expect("run release check");
    assert_failure_contains(&mismatch, "version mismatch");
}

#[test]
fn release_assets_reject_empty_input_and_checksum_nonempty_input() {
    let fixture = TempDir::new().expect("fixture");
    let artifacts = fixture.path().join("artifacts");
    let output = fixture.path().join("release-assets");
    fs::create_dir_all(&artifacts).expect("artifacts");

    let empty = release_assets_command(&artifacts, &output)
        .output()
        .expect("run asset collector");
    assert_failure_contains(&empty, "No release assets");

    write(artifacts.join("pulse-windows/Pulse.exe"), "binary");
    let collected = release_assets_command(&artifacts, &output)
        .output()
        .expect("run asset collector");
    assert_success(&collected);
    assert!(output.join("Pulse.exe").is_file());
    let sums = read(output.join("SHA256SUMS.txt"));
    assert!(sums.ends_with("  Pulse.exe\n"));
}

#[test]
fn workflows_pin_actions_and_gate_tag_only_publication_on_preflight() {
    let root = repository_root();
    let ci = read(root.join(".github/workflows/ci.yml"));
    let release = read(root.join(".github/workflows/release.yml"));
    let freshness = read(root.join(".github/workflows/upstream-freshness.yml"));
    let combined = format!("{ci}\n{release}\n{freshness}");

    assert!(!release.contains("workflow_dispatch:"));
    assert!(release.contains("tags: [\"v*.*.*\"]"));
    assert!(release.contains("needs: [preflight, build]"));
    assert!(release.contains("contents: write"));
    assert!(release.contains("check-release-contract.ps1"));
    assert!(release.contains("check-codex-rich-presence-upstream.ps1"));
    assert!(release.contains("NPM_VERSION: \"11.10.1\""));
    assert!(release.contains("No release assets"));
    assert!(freshness.contains("workflow_dispatch:"));
    assert!(freshness.contains("issues: write"));
    assert!(!ci.contains("git ls-remote"));

    for line in combined.lines().map(str::trim) {
        if let Some(action) = line.strip_prefix("- uses: ") {
            let revision = action.rsplit_once('@').expect("action revision").1;
            assert_eq!(revision.len(), 40, "action is not pinned: {action}");
            assert!(
                revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "action is not pinned: {action}"
            );
        }
    }
}

fn manifest_fixture(hash: &str) -> TempDir {
    let fixture = TempDir::new().expect("fixture");
    write(fixture.path().join("src/codex/cost.rs"), "");
    write(
        fixture.path().join("src/codex/UPSTREAM.json"),
        &format!(
            r#"{{
  "schema_version": 2,
  "sync_version": 2,
  "repository": "https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence",
  "ref": "v1.7.1",
  "commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "upstream_committed_at": "2026-07-06T00:00:00Z",
  "strategy": "namespace-rebase-v1",
  "local_adapters": ["src/codex/mod.rs", "src/codex/process.rs"],
  "files": [
    {{
      "source": "src/cost.rs",
      "target": "src/codex/cost.rs",
      "source_sha256": "{EMPTY_SHA256}",
      "sha256": "{hash}"
    }}
  ]
}}
"#
        ),
    );
    fixture
}

fn canonical_fixture() -> TempDir {
    let fixture = TempDir::new().expect("canonical fixture");
    for relative in VENDORED_FILES {
        let body = match relative {
            "cost.rs" => {
                "use crate::config::PricingConfig;\npub fn is_fast_capable(_: &str) -> bool { true }\npub fn speed_multiplier(_: &str, _: bool) -> f64 { 1.0 }\n"
            }
            "discord.rs" => {
                "use crate::session::CodexSessionSnapshot;\npub fn presence_lines() {}\n"
            }
            _ => "use crate::config::PricingConfig;\n",
        };
        write(fixture.path().join("src").join(relative), body);
    }
    git(fixture.path(), ["init", "-q"]);
    git(fixture.path(), ["config", "user.name", "Pulse Tests"]);
    git(
        fixture.path(),
        ["config", "user.email", "pulse-tests@example.invalid"],
    );
    git(fixture.path(), ["add", "src"]);
    git(fixture.path(), ["commit", "-qm", "fixture"]);
    git(fixture.path(), ["tag", "-a", "v1.7.2", "-m", "fixture"]);
    fixture
}

fn pulse_fixture(complete_process_adapter: bool) -> TempDir {
    let fixture = TempDir::new().expect("pulse fixture");
    let desktop_probe = if complete_process_adapter {
        "pub fn is_desktop_surface_running() -> bool { false }\n"
    } else {
        ""
    };
    write(
        fixture.path().join("src/codex/process.rs"),
        &format!(
            "const CODEX_APP_PROCESS_NAME: &str = \"Codex.exe\";\npub fn is_opencode_running() -> bool {{ false }}\npub fn is_codex_app_running() -> bool {{ false }}\n{desktop_probe}"
        ),
    );
    write(
        fixture.path().join("src/codex/mod.rs"),
        "pub mod process;\n",
    );
    fixture
}

fn release_fixture(version: &str) -> TempDir {
    let fixture = TempDir::new().expect("release fixture");
    write(
        fixture.path().join("Cargo.toml"),
        &format!("[package]\nname = \"cc-discord-presence\"\nversion = \"{version}\"\n"),
    );
    write(
        fixture.path().join("src-tauri/Cargo.toml"),
        &format!("[package]\nname = \"pulse\"\nversion = \"{version}\"\n"),
    );
    write(
        fixture.path().join("src-tauri/tauri.conf.json"),
        &format!(r#"{{"version":"{version}"}}"#),
    );
    write(
        fixture.path().join("package.json"),
        &format!(r#"{{"name":"pulse","version":"{version}"}}"#),
    );
    write(
        fixture.path().join("frontend/package.json"),
        &format!(r#"{{"name":"pulse-frontend","version":"{version}"}}"#),
    );
    write(
        fixture.path().join("frontend/package-lock.json"),
        &format!(
            r#"{{"name":"pulse-frontend","version":"{version}","packages":{{"":{{"name":"pulse-frontend","version":"{version}"}}}}}}"#
        ),
    );
    write(
        fixture.path().join("Cargo.lock"),
        &format!(
            "[[package]]\nname = \"cc-discord-presence\"\nversion = \"{version}\"\n\n[[package]]\nname = \"pulse\"\nversion = \"{version}\"\n"
        ),
    );
    write(
        fixture.path().join("CHANGELOG.md"),
        &format!(
            "# Changelog\n\n## [{version}] - 2026-07-09\n\n### Fixed\n\n- Validate immutable release inputs before any platform build starts.\n- Publish only artifacts produced after every required quality check succeeds.\n"
        ),
    );
    fixture
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn script_command(name: &str) -> Command {
    let mut command = Command::new("pwsh");
    command
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-File")
        .arg(repository_root().join("scripts").join(name));
    command
}

fn update_command(repository: &Path, root: &Path, tag: &str, commit: &str) -> Command {
    let mut command = script_command("update-codex-rich-presence.ps1");
    command
        .arg("-Repository")
        .arg(repository)
        .arg("-Tag")
        .arg(tag)
        .arg("-Commit")
        .arg(commit)
        .arg("-Root")
        .arg(root);
    command
}

fn release_contract_command(root: &Path, tag: &str) -> Command {
    let mut command = script_command("check-release-contract.ps1");
    command.arg("-Root").arg(root).arg("-Tag").arg(tag);
    command
}

fn release_assets_command(artifacts: &Path, output: &Path) -> Command {
    let mut command = script_command("release-assets.ps1");
    command
        .arg("-ArtifactsDirectory")
        .arg(artifacts)
        .arg("-OutputDirectory")
        .arg(output);
    command
}

fn git<const N: usize>(directory: &Path, arguments: [&str; N]) {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("run git");
    assert_success(&output);
}

fn git_output<const N: usize>(directory: &Path, arguments: [&str; N]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("run git");
    assert_success(&output);
    stdout(&output).trim().to_string()
}

fn write(path: impl AsRef<Path>, contents: &str) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(path, contents).expect("write fixture");
}

fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).expect("read file")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn assert_success(output: &Output) {
    assert!(output.status.success(), "{}", output_text(output));
}

fn assert_failure_contains(output: &Output, expected: &str) {
    assert!(!output.status.success(), "command unexpectedly passed");
    assert!(
        output_text(output).contains(expected),
        "expected {expected:?}\n{}",
        output_text(output)
    );
}
