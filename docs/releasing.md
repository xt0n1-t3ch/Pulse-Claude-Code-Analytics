# Release contract

Pulse releases are annotated-tag, exact-commit, and immutable. The local v1.6 worktree is not permission to publish.

## Version surfaces

`scripts/release-contract.json` records the product/core/config/database contract. A release tag must agree with every Cargo, npm, lockfile, Tauri, README, docs-index, and changelog version surface.

Pulse additionally refuses release when `codex-presence-core` is a path dependency. Promotion requires the canonical Git URL, a full 40-character `rev`, core 1.0.0, and a canonical manifest carrying the same SHA.

## Required proof

1. Rust fmt, clippy, workspace tests, frontend check/tests/build, and the Tauri release bundle pass without warnings.
2. Schema 12 to 13 config migration and database schema 5 migration pass from fixtures.
3. Dark and Light evidence exists for Dashboard, Discord, Sessions, Costs, Reports, and Settings at 1280×860, 900×600, and 720×560.
4. Windows runtime proves Fast, semantic weekly-only usage, Credits, field persistence, preview/live Discord equivalence, native theme, and narrow resize.
5. Before/after startup, idle CPU/memory, Tauri traffic, JSONL reads, SQLite writes/query plans, and initial gzip are recorded.
6. `pulse-windows-x64.spdx.json` validates against the Windows executable and is covered by `SHA256SUMS.txt`.

## Promotion

Canonical v1.8.0 is promoted first. Pulse then replaces the local core path with the exact released Git revision and reruns every gate. Only explicit approval permits the annotated `v1.6.0` tag and tag-only release workflow.
