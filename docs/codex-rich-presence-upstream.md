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
| Sync strategy | source sync with a small Pulse compatibility overlay |

The local mirror copies the Rust modules Pulse consumes for Codex session parsing,
pricing, telemetry, display labels, and Rich Presence composition. `UPSTREAM.json`
records the upstream branch and commit so CI can prove whether Pulse is current.

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
- `src/codex/cost.rs` appends Pulse-only fast-mode helpers for GPT-5.5 and GPT-5.4.
- `src/codex/session/parser.rs` rewrites upstream `git` probes through Pulse's no-window command helper so Windows polling does not open console windows.
- `tests/codex_upstream_contract.rs` checks the Pulse-facing module boundary stays
  present after every sync.

All other mirrored code should come from upstream through the sync script.

## Check freshness

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-codex-rich-presence-upstream.ps1
```

The check reads `src/codex/UPSTREAM.json`, resolves upstream `main`, and fails if
the pinned commit is stale. CI runs the same check.

## Pull latest upstream code

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/update-codex-rich-presence.ps1
```

The update script clones the upstream repo, refreshes `src/codex/`, reapplies the
compatibility overlay, updates `UPSTREAM.json`, formats Rust, checks freshness, and
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
