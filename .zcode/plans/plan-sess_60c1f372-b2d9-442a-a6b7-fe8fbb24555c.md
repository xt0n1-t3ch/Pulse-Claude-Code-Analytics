# Pulse v1.7.x — Discord identity end-to-end + Home/Settings polish

Scope decisions (Tony): UI intensity = **consolidate + polish** (keep current visual identity); Discord identity fix = **robust end-to-end** (Rust scanner hardened + real display name + periodic refresh). Delegation: two `general-purpose` agents (backend Rust, frontend Svelte) working in parallel on disjoint file sets; Amy orchestrates, reviews diffs against this contract, and owns final verification. No commits, PRs, tags or releases (per org rules; changes stay in worktree + CHANGELOG `Unreleased`).

## Contract (single source of truth for both agents)

`DiscordUserInfo` gains ONE additive field, serialized snake_case like its siblings:
`global_name: Option<String>` (None = account has no global/display name). No other DTO shape changes. `get_discord_user()` keeps returning `Option<DiscordUserInfo>` — never breaking bridge/frontend consumers. Dev-bridge allow-list already routes `get_discord_user`; nothing to change there. AppSnapshot/persisted-snapshot contracts stay untouched.

## Phase A — Backend hardening (`src-tauri/src/commands.rs`) [agent: backend]

1. **Refactor scan core into a testable pure helper**, e.g. `scan_discord_leveldb_dirs(dirs: &[PathBuf]) -> Option<DiscordUserInfo>`, called by `get_discord_user()` after building the platform candidate list.
2. **Fix abort-on-first-bad-file**: replace `std::fs::read(entry.path()).ok()?` (line ~2912) with skip-and-continue; a failed metadata/read/sort entry never ends the search.
3. **Multi-variant fallback**: try EVERY existing LevelDB dir (Stable → Canary → PTB, Flatpak/Snap variants included) until a valid record is found, instead of first-existing-dir-only.
4. **Extract `global_name`** in `extract_discord_user`'s 600-byte chunk parser (`"global_name":"..."` substring pattern, same style as username/avatar).
5. **TTL cache ~60s**: process-local `Mutex<Option<(Instant, Option<DiscordUserInfo>)>>` so view refreshes don't rescan the filesystem each time. Clone on hit; recompute on miss/expiry.
6. **Rust tests** (commands.rs test module): global_name parsed when present / None when absent; unreadable file is skipped and later file wins; second variant dir used when first yields nothing. Helper must be testable without touching the real `%APPDATA%`.

## Phase B — Discord view + identity freshness (`frontend/src/lib/api.ts`, `lib/stores.ts`, `views/Discord.svelte`) [agent: frontend]

1. `api.ts`: extend TS `DiscordUserInfo` with `global_name: string | null` (lines ~213-224).
2. Refresh loop: call `loadDiscordUser()` every ~60s (timer owned by `App.svelte` onMount with cleanup, mirroring existing poll patterns); store already exposes `discordUser` — no new store shape.
3. `Discord.svelte` preview header (line ~631):
   - Display name = `$discordUser.global_name ?? $discordUser.username`; fallback text unchanged ("Discord user unavailable").
   - DELETE the literal `<span class="dp-tag">ツ</span>`.
   - Subtitle shows real handle `@username`; hide line gracefully when unavailable.
   - Avatar/banner chains stay as-is (they're correct; freshness comes from the store refresh).
4. Update `tests/components/Discord.test.ts` fixture minimally: keep passing assertions; add one case asserting global_name wins over username, and one asserting @handle rendering.

## Phase C — Settings polish (`frontend/src/views/Settings.svelte` + shared components)

1. **Layout defect fix**: `.rail` declares 3 grid tracks but holds 4 controls (Provider, Plan override, Appearance, Window close; lines ~317-379, CSS ~640-649). Rebuild rail as proper 4-column responsive grid collapsing cleanly to 2/1 columns.
2. **Standardize segmented controls**: replace hand-rolled `.theme-toggle/.theme-opt` blocks (Appearance + Window close) with the existing `SegmentedControl` component — kills the broken `role="radiogroup"` + `aria-pressed` mismatch too.
3. **Export pending state**: disable button + label swap while `export_all_data` runs; guard double-clicks.
4. **Close-to-tray silent failure**: initial `get_app_settings().catch(() => {})` (lines ~40-47) becomes a non-blocking warning toast; optimistic default retained.
5. **Footer truth**: Platform cell derives Windows/macOS/Linux from `navigator.userAgentData?.platform ?? navigator.platform` mapping (no deprecated raw string); Engine/Runtime labels stay static build facts.

## Phase D — Dashboard polish (`frontend/src/views/Dashboard.svelte`)

1. **Initial loading state**: use existing `analyticsLoading` flag + global `.skeleton` utility to show placeholder blocks during first bundle load (today the area renders empty/false "No active session" while loading).
2. **Stat cells consolidation**: replace the three hand-built stat cells (cost / burn rate / tokens, lines ~369-385) with `MetricStrip`+`StatCard` for visual parity; keep the bespoke token-composition meter and context meters (they're unique visuals, not duplication).
3. **Tab semantics**: instance tray tabs get proper linkage — focus panel declared `role="tabpanel"` with `aria-labelledby`/`aria-controls` wired to selected tab id.
4. **Provenance kickers**: label sections with their data window ("session", "30 days", "month to date", "all-time") since summary is all-history but forecast is month-scoped — small text kickers only, no layout redesign.

## Phase E — Verification gate (Amy, sequential after agents land)

1. `cargo fmt --all` then `cargo clippy --workspace --all-targets -- -D warnings` then `cargo test --workspace`.
2. Frontend: install deps if needed, then repo gate (`npm run verify` = fmt:rust:check + frontend check + clippy + tests; vitest component suite including updated Discord.test.ts).
3. `docs/architecture/dev-bridge.md` NOT edited (contract unchanged); CHANGELOG.md gets an `Unreleased` section entry (Added/Fixed) per Keep a Changelog. AGENTS.md needs no structural updates for this change.
4. One bounded `open-code-review` pass over the full diff after focused tests pass; findings validated against code before any fix-up.
5. Manual visual check path (optional, for Tony): `bun --cwd frontend run dev` with `PULSE_DEV_BRIDGE_TOKEN` set uses the REAL bridge whose allow-list serves fresh `get_discord_user` — preview will show his actual avatar/name with 60s freshness.

## Risks / guards

- Serializer shape: Rust struct uses plain snake_case serde (matches TS mirror) — additive field with `#[serde(default)]`-equivalent tolerance not needed for responses, but Option serialization keeps old frontend code path safe regardless.
- Cache mutex poisoning: use `.lock().unwrap_or_else(|p| p.into_inner())` pattern consistent with file conventions.
- No new dependencies; std `Instant`/`Mutex` only.
- View race protections (generation counters, provider scoping) preserved verbatim — no logic moved across them.