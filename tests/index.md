# Pulse test suite

Centralized tests for the whole app. Frontend logic/integration/component-render runs on Vitest
(happy-dom); the daemon + analytics core run on `cargo test`. The Vitest config lives in
`frontend/` but the spec tree is rooted here at `tests/`; run both layers before shipping.

## Run

| Layer | Command | What it covers |
|:---|:---|:---|
| Frontend | `npm --prefix frontend run test` | Vitest unit + integration + component render (`tests/unit`, `tests/integration`, `tests/components`) |
| Frontend (watch) | `npm --prefix frontend run test:watch` | Same, watch mode |
| Backend | `cargo test --workspace` | Both Rust crates (`cc-discord-presence` daemon/core + `pulse` Tauri host): inline `#[cfg(test)]` modules + the `tests/*.rs` / `src-tauri/tests/*.rs` integration tests |
| Types | `npm --prefix frontend run check` | svelte-check, 0 errors / 0 warnings |

Vitest config: [frontend/vitest.config.ts](../frontend/vitest.config.ts) (alias `@` → `frontend/src`,
`fs.allow` widened to the repo root so the root-level spec tree resolves). Tauri IPC, the event/app/
window/dialog/fs plugins, and Chart.js are all mocked in [tests/setup.ts](setup.ts) so store/view
modules import cleanly outside a WebView; a WAAPI `Element.prototype.animate` stub plus a `matchMedia`
shim let Svelte transitions and media queries run headless. The fake `__TAURI_INTERNALS__.invoke`
routes known list commands to `[]`, scalars to stub values, and `get_reports_bundle` to an empty bundle.

## Frontend — unit (`tests/unit/`)

| File | Module under test | Coverage |
|:---|:---|:---|
| [utils.test.ts](unit/utils.test.ts) | `lib/utils` | `fmtTokens` (M/K/unit tiers), `fmtCost` ($ two-decimal), `fmtDuration` (s/m/h+m), `fmtTps` (/s→K/s), `fmtPct` (whole-percent rounding), `usageColor` (normal/warning/danger), `classifyActivity` (thinking/editing/reading/running/waiting/idle), `fmtClock` (HH:MM passthrough + em-dash fallback on null·undefined) |

## Frontend — integration (`tests/integration/`)

| File | Surface | Coverage |
|:---|:---|:---|
| [tauri-mock.test.ts](integration/tauri-mock.test.ts) | `lib/api` over the mocked Tauri IPC | list commands resolve to `[]`, mapped scalar (`get_active_provider`→`"claude"`), unmapped command resolves to `undefined` without throwing |
| [poll-flow.test.ts](integration/poll-flow.test.ts) | `stores.poll()` → global stores → `Dashboard.svelte` | one `poll()` pass calls each loader exactly once and hydrates `health`/`metrics`/`sessions`/`rateLimits`/`planInfo` + derived `activeSessions`; Dashboard then renders KPI values ($8.00) and two live session cards end to end |
| [phase5-flow.test.ts](integration/phase5-flow.test.ts) | `Context.svelte` + `Reports.svelte` data flow | selecting a different session pill re-queries `getContextBreakdown` with the new session id (`"s2"`); Reports renders the bundle through a single `getReportsBundle` call |

## Frontend — component render (`tests/components/`)

DOM-render tests for every view/component via `@testing-library/svelte` on happy-dom. Tauri is
satisfied through the injected internals in [setup.ts](setup.ts); the Chart.js view (`Costs`) swaps in
the [fixtures/ChartStub.svelte](fixtures/ChartStub.svelte) stub so canvas-bound charts render headless.

| File | Component | Coverage |
|:---|:---|:---|
| [PulseMark.test.ts](components/PulseMark.test.ts) | `PulseMark` | svg sized to the `size` prop, P-glyph-only when `showPulse` is false (1 path), P glyph + pulse line when true (2 paths) |
| [SessionCard.test.ts](components/SessionCard.test.ts) | `SessionCard` | fast badge present/absent on the `fast` flag, inflated-tokenizer marker shown for opus 4.7+ and omitted for 4.6, Opus 4.8 model display name |
| [Dashboard.test.ts](components/Dashboard.test.ts) | `Dashboard` (view) | four KPI tiles + values ($12.50 / 1.1M / 4), cost breakdown reconciles to the per-component estimated total, plan usage limits + model distribution (Opus 4.8 + Sonnet 4.6) |
| [Sessions.test.ts](components/Sessions.test.ts) | `Sessions` (view) | KPI tile labels (Total Tokens / Total Cost / Avg Duration / Avg Cost/Session), live session rows + "2 active", history table loaded from the api layer |
| [Costs.test.ts](components/Costs.test.ts) | `Costs` (view) | four cost KPI tiles, Cost-by-Type legend reconciles to the per-component total, budget tracking from the budget-status fixture ($30.00 / $100.00) |
| [Context.test.ts](components/Context.test.ts) | `Context` (view) | session-pill strip for seeded sessions (project labels) + per-session usage row, breakdown queried on session select |
| [Reports.test.ts](components/Reports.test.ts) | `Reports` (view) | sections populated from a single bundle call (deferred-resolver harness), loading/reload banner shown on a filter-triggered re-fetch then cleared |
| [Discord.test.ts](components/Discord.test.ts) | `Discord` (view) | live-preview profile with active-session details + Discord username, six field toggles + three presets + master Rich Presence toggle, `setDiscordEnabled(false)` called when the master toggle is flipped off |
| [Settings.test.ts](components/Settings.test.ts) | `Settings` (view) | identity masthead + config controls (3 rail controls), db size + session total loaded from the api layer (5.0 MB / 42), two-step confirm before `clearHistory`, theme toggle via the appearance control |

## Backend — Rust (`cargo test --workspace`)

Two workspace crates. `cc-discord-presence` (repo root) is the daemon + analytics core;
`pulse` (`src-tauri/`) is the Tauri host that depends on it. `--workspace` runs both crates' inline
`#[cfg(test)]` modules plus the cargo integration tests below. All green.

### Integration tests (`tests/*.rs`, `src-tauri/tests/*.rs`)

| File | Crate | Coverage |
|:---|:---|:---|
| [daemon_e2e.rs](daemon_e2e.rs) | `cc-discord-presence` | end-to-end daemon pipeline over temp JSONL fixtures: Claude session collect accumulates speed-aware per-category cost and reconciles categories to the headline total; tracks last-turn speed/effort/service-tier and builds presence lines (project, "Opus 4.8 (1M)", fast ⚡ marker, effort label); Codex session parses meta/turn-context/token-count, resolves effort/window/totals, and builds presence state (model display, "(Extra High)", fast marker, "Pro ($200/month)") |
| [reports_e2e.rs](../src-tauri/tests/reports_e2e.rs) | `pulse` | `build_reports_bundle_from_roots` aggregates fixture traces (user/assistant/tool/mcp/compaction counts, cache health); regression guard that the JSONL tree is scanned exactly once per bundle (no double/8x scan); oversized JSONL over `MAX_JSONL_BYTES` is skipped while small files still trace |
| [report_html.rs](../src-tauri/tests/report_html.rs) | `pulse` | `generate_html_report` / `generate_markdown_report`: writes a sample HTML for `/browser` visual review; offline-safe (no Google Fonts / gstatic / `@import` / `http://`, https only in w3.org+github namespaces); well-formed doctype + single `<html>`, inline `<style>`, inline SVG charts (token-composition aria-label); brand kicker + KPI strip + all eight analyzer section anchors + Speed Split; offline system/monospace font stacks; markdown is non-empty GFM with every section heading + the speed-split table header |

### Inline `#[cfg(test)]` modules

Per-crate, per-module unit tests compiled with each crate. Representative coverage:

| Area | Module | Coverage |
|:---|:---|:---|
| cost / pricing | `src/cost.rs` | per-tier pricing, cache math, speed-aware totals, 1M-context surcharge, fast-capable model table |
| presence lines | `src/discord.rs` | Claude presence details/state/tooltip composition across model/effort/speed/marker permutations |
| session collect | `src/session.rs` | JSONL parse, token/cost accumulation, reasoning-effort + speed + service-tier extraction, git-branch + parse caching |
| metrics / usage | `src/metrics.rs`, `src/usage.rs` | aggregate metrics rollups, plan/usage-window derivation |
| config | `src/config.rs`, `src/codex/config.rs` | presence/pricing config defaults + round-trip |
| util / process | `src/util.rs`, `src/process_guard.rs` | path/format helpers, single-instance process guard |
| codex core | `src/codex/{session.rs,session/parser.rs,cost.rs,discord.rs,util.rs,process.rs}` | Codex JSONL parse, cost, presence lines, helpers |
| codex telemetry | `src/codex/telemetry/{plan.rs,service_tier.rs,limits.rs}` | plan-tier + service-tier resolution, rate-limit window parsing |
| db | `src-tauri/src/db.rs` | SQLite historical-session insert/query/round-trip |
| analyzers | `src-tauri/src/analyzers/{session_trace.rs,cache_health.rs,model_routing.rs,prompt_complexity.rs,inflection.rs}` | trace scan + scan-pass counting, cache-health grading, model-routing split, prompt-complexity scoring, inflection-point detection |
| commands | `src-tauri/src/commands.rs` | reports-bundle assembly from roots |

## Project cross-reference

| Doc | Where | Purpose |
|:---|:---|:---|
| [README.md](../README.md) | repo root | Install, feature overview, daemon + GUI quick start |
| [CHANGELOG.md](../CHANGELOG.md) | repo root | Release history (config + DB schema v3) |
| [docs/index.md](../docs/index.md) | `docs/` | Documentation hub: architecture, Discord assets, reasoning-effort variants, analyzers, cost calculation |
| [CLAUDE.md](../CLAUDE.md) | repo root | Full project context |
| [AGENTS.md](../AGENTS.md) | repo root | Agent operating notes |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | repo root | Contribution + local-dev workflow |
