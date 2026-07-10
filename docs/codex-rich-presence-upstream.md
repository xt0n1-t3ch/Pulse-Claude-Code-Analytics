# Codex Rich Presence upstream sync

Pulse keeps the Codex-specific Discord Rich Presence core aligned with the standalone
[Codex Discord Rich Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence)
repository.

## Source of truth

| Field | Value |
| --- | --- |
| Upstream repo | <https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence> |
| Local mirror | `src/codex/` |
| Pin file | `src/codex/UPSTREAM.json` |
| Pinned release | `v1.7.5` |
| Pinned commit | `2b3c7f51cf320c9a0c0beced963254348202c8c1` |
| Shared config | schema 12 with persisted `presence_enabled` |
| Sync strategy | immutable tag/commit source sync with a small Pulse compatibility overlay |

The local mirror copies the Rust modules Pulse consumes for Codex session parsing,
pricing, telemetry, display labels, and Rich Presence composition. `UPSTREAM.json`
records the upstream tag, full commit, sync-script version, file inventory, and SHA-256 hashes so CI can prove the mirror is immutable and exact.

## Why source sync instead of a Cargo Git dependency

The upstream crate ships its own Windows resource build. Linking that crate directly
inside Pulse produced a duplicate Windows `VERSION` resource at build time because
Pulse also embeds app resources through Tauri. Source sync keeps the same logic
available to Pulse without linking a second Windows application resource.

## Compatibility overlay

Pulse owns only the glue the upstream crate does not expose as a stable library
contract:

- `src/codex/mod.rs` declares the mirrored modules under Pulse's namespace.
- `src/codex/process.rs` exposes the OpenCode process probe needed by the Pulse UI.
- `src/codex/session/parser.rs` rewrites upstream `git` probes through Pulse's no-window command helper so Windows polling does not open console windows.
- `tests/codex_upstream_contract.rs` checks the Pulse-facing module boundary stays
  present after every sync.

Pulse does not append pricing tables or Fast multipliers to vendored files. Model identity, pricing, context, and presentation stay owned by the canonical mirror; Pulse adapters translate the public DTOs into Tauri responses. Inventory `model-catalog-v3` also mirrors `app.rs` and `process_guard.rs` as uncompiled source-contract inputs so canonical cross-file regression tests remain buildable without declaring a second Pulse runtime owner.

All other mirrored code should come from upstream through the sync script.

## Check freshness

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-codex-rich-presence-upstream.ps1
```

The release check validates every file hash against `src/codex/UPSTREAM.json`. A separate non-required scheduled drift check compares the pin with upstream and opens or updates one issue; normal CI does not silently replace a released pin with moving `main`.

## Pull latest upstream code

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/update-codex-rich-presence.ps1 -Tag v1.7.5 -Commit 2b3c7f51cf320c9a0c0beced963254348202c8c1
```

The update script checks out the explicit tag and commit, refreshes `src/codex/`, reapplies the
compatibility overlay, updates `UPSTREAM.json`, formats Rust, validates hashes, and
runs:

```powershell
cargo test --workspace codex_upstream
```

There is also a POSIX shell variant at `scripts/update-codex-rich-presence.sh` for
Linux/macOS contributors.

## Release gate

Before a Pulse release, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-codex-rich-presence-upstream.ps1
cargo test --workspace codex_upstream
```

If upstream changed, sync first, inspect the diff, then run the full Pulse pre-ship
validators from `tests/index.md`.
