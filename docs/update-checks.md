# Pulse update checks

Pulse checks GitHub Releases from the Tauri backend and shows a small in-app popup when a newer stable release is available.

## Runtime flow

1. `check_app_update()` calls the GitHub latest-release API for `xt0n1-t3ch/Pulse-Claude-Code-Analytics`.
2. The backend compares the release tag against `env!("CARGO_PKG_VERSION")`.
3. Drafts and prereleases are ignored.
4. `UpdateBanner.svelte` renders `New Update Available` with current version, latest version, release title, release notes, and actions.
5. The explicit **Update** action hands the release to Tauri's signed updater. After `downloadAndInstall()` resolves, the process plugin relaunches Pulse automatically.
6. `open_app_release_page()` remains an allowlisted fallback for inspecting the GitHub release.

The popup checks at startup and then every 6 hours. Settings exposes a manual **Check for updates** action by dispatching `pulse:check-updates`.

## User controls

- **Later** hides the current popup until the next check.
- **Skip version** stores the latest version in `localStorage` and suppresses that release during automatic checks.
- **Update** is the single approval checkpoint for signed download, install, and relaunch.
- **Open release** opens the allowlisted GitHub release page as a fallback.

## Signed updater boundary

Release discovery is informational; the Tauri updater configuration and signature verification remain authoritative for installation. The UI never treats a GitHub release response as install proof. Relaunch occurs only after the user clicks **Update** and the signed updater reports a successful install. Download, install, or relaunch failures stay in the popup as retryable errors.

## Validators

```bash
cargo test -p pulse update_check --lib
npm --prefix frontend run test -- tests/components/UpdateBanner.test.ts
npm --prefix frontend run check
```
