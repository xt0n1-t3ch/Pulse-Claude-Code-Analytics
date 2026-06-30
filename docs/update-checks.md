# Pulse update checks

Pulse checks GitHub Releases from the Tauri backend and shows a small in-app popup when a newer stable release is available.

## Runtime flow

1. `check_app_update()` calls the GitHub latest-release API for `xt0n1-t3ch/Pulse-Claude-Code-Analytics`.
2. The backend compares the release tag against `env!("CARGO_PKG_VERSION")`.
3. Drafts and prereleases are ignored.
4. `UpdateBanner.svelte` renders the available update with current version, latest version, release title, release notes, and actions.
5. `open_app_release_page()` opens only allowlisted Pulse GitHub release URLs.

The popup checks at startup and then every 6 hours. Settings exposes a manual **Check for updates** action by dispatching `pulse:check-updates`.

## User controls

- **Later** hides the current popup until the next check.
- **Skip version** stores the latest version in `localStorage` and suppresses that release during automatic checks.
- **Open release** opens the GitHub release page so the user can download the installer or portable asset.

## Signed updater note

DLSSync uses Tauri's signed updater lane with `latest.json` and a public signing key. Pulse does not publish signed updater metadata yet, so v1.2.0 intentionally uses a backend release checker plus release-page handoff instead of inventing updater keys or pretending auto-install is available.

To move to signed in-app installs later:

1. Add a Tauri updater signing key to the release secret store.
2. Generate and upload `latest.json` during `.github/workflows/release.yml`.
3. Add `tauri-plugin-updater` and process relaunch wiring.
4. Keep the current popup as the user-facing shell around the signed install action.

## Validators

```bash
cargo test -p pulse update_check --lib
npm --prefix frontend run test -- tests/components/UpdateBanner.test.ts
npm --prefix frontend run check
```
