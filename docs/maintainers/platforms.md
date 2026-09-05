# Platform support and verification

Pulse targets Windows, macOS and Linux on x64 and ARM64. A configured target is not proof of a tested application or a published download.

## Current release status

On 2026-09-05, the immutable [v1.8.1 release](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.8.1) contains Windows x64 NSIS/MSI installers, a Windows SPDX file and checksums. It has no macOS, Linux, Windows ARM64 or updater assets. Do not replace that release or reuse its tag to add files.

The next complete release must pass the manual Release workflow for all targets:

| Platform | Native runner | Rust target | Required public packages |
| --- | --- | --- | --- |
| Windows x64 | windows-2022 | x86_64-pc-windows-msvc | NSIS, MSI, updater signature, SPDX |
| Windows ARM64 | windows-11-arm | aarch64-pc-windows-msvc | NSIS, MSI, updater signature, SPDX |
| macOS Intel | macos-15-intel | x86_64-apple-darwin | DMG, app archive, updater signature |
| macOS Apple Silicon | macos-latest | aarch64-apple-darwin | DMG, app archive, updater signature |
| Linux x64 | ubuntu-22.04 | x86_64-unknown-linux-gnu | DEB, RPM, AppImage, updater signature |
| Linux ARM64 | ubuntu-22.04-arm | aarch64-unknown-linux-gnu | DEB, RPM, AppImage, updater signature |

The workflow verifies the Rust host target before executing tests. It builds the frontend once and uses that artifact in every native bundle. Native jobs run warning-denying Clippy and workspace tests before packaging.

## Verify without publishing

After an explicitly authorized annotated tag exists on the remote, dispatch Release with `publish_release=false`, the default. It runs the native matrix and retains verification artifacts for seven days. It does not create a GitHub release, and its unsigned packages are not updater candidates.

Local regression checks run with `cargo test --locked --test release_scripts`. They are also part of `npm run verify`. These fixture and workflow checks do not execute Linux or macOS binaries on Windows.

## Public release gate

Publishing requires an explicit dispatch with `publish_release=true`, all six successful jobs, complete artifacts, checksums and six signed updater entries. Missing updater signatures fail assembly. Apple credentials are not required for GitHub packages.

Configure `TAURI_SIGNING_PRIVATE_KEY` without printing its value; add `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` if the key has a password.

The macOS artifacts are GitHub downloads, not App Store submissions. They have no Apple Developer ID signature or notarization. The workflow checks the packaged executable architecture. macOS may block first launch. Do not claim Gatekeeper acceptance or notarization. Updater signatures prove the update payload, not an Apple-approved publisher. Windows downloads may also trigger SmartScreen when publisher signing or reputation is unavailable.

Only the complete release can become latest. `scripts/release-local.ps1 -WindowsOnlyRecovery` is an explicitly limited Windows x64 recovery path. It always uses `--latest=false` and cannot repair an immutable release.

## Native runtime acceptance

Before claiming support for a release, retain evidence from the installed package on each target:

1. Install the architecture-matched package and launch it with development ports 1420 and 1421 stopped.
2. Check the embedded UI, version, light/dark themes, resize and all primary views.
3. Check single-instance behavior, close-to-tray, restore, notifications and persisted settings.
4. Test Claude, Codex and OpenCode with available local sessions. Separate live activity, history and account limits; verify unavailable sources remain unavailable.
5. Connect Discord and compare published fields with the Pulse preview. Verify idle and completed sessions do not retain stale live data.
6. Test an upgrade from the previous supported version, database migration, rollback backup and the signed updater path.

Record the OS version, architecture, tag commit, installer SHA-256, result and known limits. A missing host or credential is a gap, not a pass. Do not enable fixture-backed behavior in production to obtain a screenshot.

## Known platform differences

- Windows uses WebView2. EcoQoS and Efficiency mode apply only to Windows.
- macOS has a configured minimum of 11.0. The current Claude subscription reader reads `.credentials.json`; it does not read Keychain-only credentials. Local transcript analytics do not require that quota credential.
- Linux uses GTK 3 and WebKitGTK 4.1. Tray availability depends on AppIndicator support and the desktop environment. AppImage does not guarantee compatibility with every Linux distribution.
- Provider sign-in, quota access and cost coverage are separate from operating-system support. An installed client or configured credential is not proof of authenticated quota access.

## Sources

- [GitHub-hosted runner labels](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [Tauri macOS signing and notarization](https://v2.tauri.app/distribute/sign/macos/)
- [Release workflow](../../.github/workflows/release.yml), [release contract](../../scripts/release-contract.json), [Tauri configuration](../../src-tauri/tauri.conf.json)
