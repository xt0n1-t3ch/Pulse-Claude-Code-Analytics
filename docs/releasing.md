# Release contract

Pulse releases are annotated-tag, exact-commit, and immutable. The manually
dispatched release workflow builds and publishes the complete Windows, macOS,
and Linux x64/ARM64 matrix. The local script remains a Windows-x64 recovery
lane and cannot satisfy the complete release contract.

## Version surfaces

`scripts/release-contract.json` records the product/core/config/database
contract. A release tag must agree with every Cargo, npm, lockfile, Tauri,
README, docs-index, and changelog version surface, plus
`src/codex/UPSTREAM.json`'s `compatibility.pulse` pin.

Pulse additionally refuses release when `codex-presence-core` is a path
dependency. Pulse v1.7.5 consumes core 2.0.0 from the immutable upstream
`v1.10.2` release at its full 40-character Git revision; the canonical Git
dependency and `src/codex/UPSTREAM.json` must carry the same SHA.

## Local verification (replaces CI)

There is no per-push / per-PR GitHub Actions CI. The gates below run locally:

- `npm run verify` — `fmt:rust:check` + frontend `check` + `clippy -D warnings`
  + `test` (frontend + `cargo test --workspace`).
- Frontend production bundle: `npm --prefix frontend run build`.
- Tauri release bundle: produced by the release script below.

`.github/workflows/` keeps only `release.yml` and `upstream-freshness.yml`, both
manual-only (`workflow_dispatch`). They never run automatically on a push, PR,
tag, or schedule; `release.yml` is the cross-platform build owner.

## Required proof

1. `npm run verify` passes without warnings, and the Tauri release bundle builds.
2. Config schema 13 and database schema 5 migrations pass from fixtures.
3. Dark and Light evidence exists for Dashboard, Discord, Sessions, Costs,
   Reports, and Settings at 1280×860, 900×600, and 720×560.
4. Windows runtime proves single-instance, close-to-tray + Settings toggle,
   semantic weekly-only usage, Credits, field persistence, preview/live Discord
   equivalence, native theme, and narrow resize.
5. `SHA256SUMS.txt` covers every exact-version installer, updater payload,
   updater manifest, and validated Windows x64/ARM64 SPDX SBOM.

## Build and publish (cross-platform)

After the annotated release tag exists and its commit passes the contract,
dispatch `release.yml` with that tag. It builds Windows x64/ARM64, macOS
x64/ARM64, and Linux x64/ARM64; assembles all signed updater targets; verifies
24 published files and 23 checksum entries; then finalizes an immutable release.

## Windows-x64 recovery lane

Use the local release script only when a Windows-x64 recovery artifact is
needed. It is not a complete multi-platform release:

```powershell
# Bump every version surface + CHANGELOG first, then:
pwsh -File scripts/release-local.ps1              # build installers + gh release
pwsh -File scripts/release-local.ps1 -SkipBuild   # reuse an existing build
```

The script builds the frontend + Tauri NSIS/MSI installers, selects only names
matching the tag version, generates and validates `pulse-windows-x64.spdx.json`,
and writes `SHA256SUMS.txt` under `target/release/local-release/vX.Y.Z/`. It
uploads a draft, downloads and hash-checks all four assets, then makes the
release public. The updater's signed `latest.json` is workflow-owned because
the minisign key is a repository secret and is not produced locally.

Never move a published tag or replace a published asset — publish a new patch
version when a correction is required.
