# Release contract

Pulse releases use annotated tags, exact commits and immutable assets. The normal release path is the manual six-platform Release workflow. The local Windows script is a limited recovery path, not the cross-platform release process.

## Version surfaces

`scripts/release-contract.json` owns the product, core, configuration and database contract. Tags must agree with Cargo, npm, lockfiles, Tauri, README, the docs index, changelog and `src/codex/UPSTREAM.json`.

Pulse v1.8.2 consumes `codex-presence-core` 2.0.0 through the full Git revision recorded in `src/codex/UPSTREAM.json`. Path dependencies and mismatched pins fail the release contract.

## Commit and pull request checks

Before an authorized commit, run the focused tests for the change. Before an authorized push or release, run `npm run verify` and `npm --prefix frontend run build`.

Changes to OS-specific code, dependencies, installers, signing or release tooling also require the platform contract tests:

```powershell
cargo test --locked --test release_scripts
```

These tests run in the regular workspace suite. They check native runner coverage, publication opt-in, required packages, updater signatures and the Windows-only recovery boundary. They do not replace native runtime tests.

Keep changes under `Unreleased` until a release is requested. Record compatibility and release impact. Keep `AGENTS.md`, this guide, the [platform support guide](platforms.md) and the [test map](../../tests/index.md) aligned. Do not claim a platform passed if no native evidence exists.

There is no automatic push, pull request, tag or scheduled CI. `release.yml` and `upstream-freshness.yml` remain manual-only.

## Cross-platform verification

After the release tag is explicitly authorized and pushed, dispatch Release with that tag and `publish_release=false`. This is the default. It verifies the exact annotated tag, builds the frontend once, runs Clippy/tests natively on all six targets and retains unsigned verification packages without publishing.

The installed GUI must also pass the [native runtime checklist](platforms.md#native-runtime-acceptance). Complete release claims require evidence, not only a configured matrix.

## Publish a complete release

Only after explicit publication authorization:

1. Bump every version surface and prepare the reviewed changelog section.
2. Run local verification. Record completed native runtime checks and any unavailable hosts; do not infer installed-runtime acceptance from a compiled package.
3. Create and push the authorized annotated tag from the reviewed commit.
4. Configure the required updater signing secret. GitHub macOS packages do not require Apple credentials.
5. Dispatch Release with the tag and `publish_release=true`.
6. Require all six native jobs, the complete package set, six updater entries and checksums.
7. Verify the draft's downloaded bytes and GitHub digests before making it public and immutable.

The workflow blocks publication if any required target, updater signature or checksum check fails. macOS packages are distributed without Apple Developer ID signing or notarization; state this limit in the release notes. It never updates an existing immutable release. A correction requires a new patch version.

## Windows-only recovery

Use this path only when a Windows x64-only recovery release is explicitly requested:

```powershell
pwsh -File scripts/release-local.ps1 -WindowsOnlyRecovery
```

`-SkipBuild` can reuse exact-version installers; `-Draft` retains the verified draft. The script requires a Windows x64 host and an explicit recovery flag before remote operations. It publishes four Windows assets without an updater manifest and always sets `--latest=false`.

A recovery release does not satisfy the six-platform contract. The already-published v1.8.1 Windows-only release remains unchanged.

## Local portable validation

Run `npm run build:portable` to embed the frontend through Tauri's custom protocol. A raw Cargo GUI build can still point at the development URL. Validate the actual application window with development listeners stopped, not only the process or Discord connection.
