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
| Codex upstream freshness | `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-codex-rich-presence-upstream.ps1` | Verifies `src/codex/UPSTREAM.json` matches the latest `main` commit in `xt0n1-t3ch/Codex-Discord-Rich-Presence` |

Vitest config: [frontend/vitest.config.ts](../frontend/vitest.config.ts) (alias `@` → `frontend/src`,
`fs.allow` widened to the repo root so the root-level spec tree resolves). Tauri IPC, the event/app/
window/dialog/fs plugins, and Chart.js are all mocked in [tests/setup.ts](setup.ts) so store/view
modules import cleanly outside a WebView; a WAAPI `Element.prototype.animate` stub plus a `matchMedia`
shim let Svelte transitions and media queries run headless. The fake `__TAURI_INTERNALS__.invoke`
routes known list commands, including `get_context_breakdowns`, to `[]`, scalars to stub values, and `get_reports_bundle` to an empty bundle.

## Frontend — unit (`tests/unit/`)

| File | Module under test | Coverage |
|:---|:---|:---|
| [utils.test.ts](unit/utils.test.ts) | `lib/utils` | `fmtTokens` (M/K/unit tiers), `fmtCost` ($ two-decimal), `fmtDuration` (s/m/h+m), `fmtTps` (/s→K/s), `fmtPct` (whole-percent rounding), `usageColor` (normal/warning/danger), `classifyActivity` (thinking/editing/reading/running/waiting/idle), `fmtClock` (HH:MM passthrough + em-dash fallback on null·undefined) |

## Frontend — integration (`tests/integration/`)

| File | Surface | Coverage |
|:---|:---|:---|
| [tauri-mock.test.ts](integration/tauri-mock.test.ts) | `lib/api` over the mocked Tauri IPC | list commands resolve to `[]`, mapped scalar (`get_active_provider`→`"claude"`), unmapped command resolves to `undefined` without throwing |
| [poll-flow.test.ts](integration/poll-flow.test.ts) | `stores.poll()` → global stores → `Dashboard.svelte` | one `poll()` pass calls each loader exactly once and hydrates `health`/`metrics`/`sessions`/`rateLimits`/`planInfo` + derived `activeSessions`; Dashboard then renders KPI values ($8.00) and two live session cards end to end |
| [phase5-flow.test.ts](integration/phase5-flow.test.ts) | `Context.svelte` + `Reports.svelte` data flow | multi-session context payloads hydrate active context cards; selecting a different session pill re-queries `getContextBreakdown` with the new session id (`"s2"`); Reports renders the bundle through a single `getReportsBundle` call |

## Frontend — component render (`tests/components/`)

DOM-render tests for every view/component via `@testing-library/svelte` on happy-dom. Tauri is
satisfied through the injected internals in [setup.ts](setup.ts); the Chart.js view (`Costs`) swaps in
the [fixtures/ChartStub.svelte](fixtures/ChartStub.svelte) stub so canvas-bound charts render headless.

| File | Component | Coverage |
|:---|:---|:---|
| [PulseMark.test.ts](components/PulseMark.test.ts) | `PulseMark` | svg sized to the `size` prop, P-glyph-only when `showPulse` is false (1 path), P glyph + pulse line when true (2 paths) |
| [SessionCard.test.ts](components/SessionCard.test.ts) | `SessionCard` | fast badge present/absent on the `fast` flag, inflated-tokenizer marker shown for opus 4.7+ and Sonnet 5 (sourced from the backend `has_inflated_tokenizer` flag, not a local regex) and omitted for 4.6, Opus 4.8 model display name, Fable/Mythos badges without tokenizer warnings, Sonnet 5 "Intro Pricing" badge presence/absence driven by `session.intro_pricing` |
| [Dashboard.test.ts](components/Dashboard.test.ts) | `Dashboard` (view) | four KPI tiles + values ($12.50 / 1.1M / 4), cost breakdown reconciles to the per-component estimated total, plan usage limits + model distribution (Opus 4.8 + Sonnet 4.6) |
| [Sessions.test.ts](components/Sessions.test.ts) | `Sessions` (view) | KPI tile labels (Total Tokens / Total Cost / Avg Duration / Avg Cost/Session), live session rows + "2 active", history table loaded from the api layer |
| [Costs.test.ts](components/Costs.test.ts) | `Costs` (view) | four cost KPI tiles, Cost-by-Type legend reconciles to the per-component total, budget tracking from the budget-status fixture ($30.00 / $100.00) |
| [Context.test.ts](components/Context.test.ts) | `Context` (view) | all active context-window cards render at once, click selection swaps the detailed breakdown, session-pill strip remains wired, per-session usage row is visible |
| [UpdateBanner.test.ts](components/UpdateBanner.test.ts) | `UpdateBanner` | update popup renders current -> latest version, hides when no update exists, Later/Skip/Open release actions work, skipped versions stay hidden until manual force, `?fakeUpdate=` creates a local dev update without backend IPC |
| [Reports.test.ts](components/Reports.test.ts) | `Reports` (view) | sections populated from a single bundle call (deferred-resolver harness), loading/reload banner shown on a filter-triggered re-fetch then cleared |
| [Discord.test.ts](components/Discord.test.ts) | `Discord` (view) | live-preview profile prefers active sessions over historical fallback, shows active-session count, renders Discord username, six field toggles + three presets + master Rich Presence toggle, `setDiscordEnabled(false)` called when the master toggle is flipped off |
| [Settings.test.ts](components/Settings.test.ts) | `Settings` (view) | identity masthead + config controls (3 rail controls), db size + session total loaded from the api layer (5.0 MB / 42), two-step confirm before `clearHistory`, theme toggle via the appearance control |

## Backend — Rust (`cargo test --workspace`)

Two workspace crates. `cc-discord-presence` (repo root) is the daemon + analytics core;
`pulse` (`src-tauri/`) is the Tauri host that depends on it. `--workspace` runs both crates' inline
`#[cfg(test)]` modules plus the cargo integration tests below. All green.

### Integration tests (`tests/*.rs`, `src-tauri/tests/*.rs`)

| File | Crate | Coverage |
|:---|:---|:---|
| [daemon_e2e.rs](daemon_e2e.rs) | `cc-discord-presence` | end-to-end daemon pipeline over temp JSONL fixtures: Claude session collect accumulates speed-aware per-category cost and reconciles categories to the headline total; tracks last-turn speed/effort/service-tier and builds presence lines (project, "Opus 4.8 (1M)", fast ⚡ marker, effort label); Codex session parses meta/turn-context/token-count, resolves effort/window/totals, and builds presence state (model display, "(Extra High)", fast marker, "Pro ($200/month)") |
| [codex_upstream_contract.rs](codex_upstream_contract.rs) | `cc-discord-presence` | Pulse-facing contract for the mirrored Codex Rich Presence modules: config, cost, display labels, telemetry limits, active-session selection, and the OpenCode process compatibility probe |
| [reports_e2e.rs](../src-tauri/tests/reports_e2e.rs) | `pulse` | `build_reports_bundle_from_roots` aggregates fixture traces (user/assistant/tool/mcp/compaction counts, cache health); regression guard that the JSONL tree is scanned exactly once per bundle (no double/8x scan); oversized JSONL over `MAX_JSONL_BYTES` is skipped while small files still trace |
| [report_html.rs](../src-tauri/tests/report_html.rs) | `pulse` | `generate_html_report` / `generate_markdown_report`: writes a sample HTML for `/browser` visual review; offline-safe (no Google Fonts / gstatic / `@import` / `http://`, https only in w3.org+github namespaces); well-formed doctype + single `<html>`, inline `<style>`, inline SVG charts (token-composition aria-label); brand kicker + KPI strip + all eight analyzer section anchors + Speed Split; offline system/monospace font stacks; markdown is non-empty GFM with every section heading + the speed-split table header |

### Inline `#[cfg(test)]` modules

Per-crate, per-module unit tests compiled with each crate. Representative coverage:

| Area | Module | Coverage |
|:---|:---|:---|
| cost / pricing | `src/cost.rs` | per-tier pricing, Fable/Mythos rates, Sonnet 5 introductory/standard pricing across the clock-injected cutoff boundary, digit-boundary-safe Sonnet 5 id classification, cache math, speed-aware totals, 1M-context surcharge, GA no-surcharge table, fast-capable model table |
| presence lines | `src/discord.rs` | Claude presence details/state/tooltip composition across model/effort/speed/marker permutations, including Fable 5 (1M) and Mythos 5 (1M) labels |
| session collect | `src/session.rs` | JSONL parse, token/cost accumulation, reasoning-effort + speed + service-tier extraction, git-branch + parse caching; compaction-boundary detection resets `current_context_tokens` to the real post-compaction size while `max_turn_api_input` (the 1M-tier lifetime peak) survives unchanged, including the missing-`compactMetadata`, compaction-as-first-line, and zero-compaction edge cases |
| metrics / usage | `src/metrics.rs`, `src/usage.rs` | aggregate metrics rollups, plan/usage-window derivation |
| config | `src/config.rs`, `src/codex/config.rs` | presence/pricing config defaults + round-trip + Windows WSL opt-in flag parsing |
| util / process | `src/util.rs`, `src/process_guard.rs` | path/format helpers, single-instance process guard |
| codex core | `src/codex/{session.rs,session/parser.rs,cost.rs,discord.rs,util.rs,process.rs}` | Codex JSONL parse, cost, presence lines, helpers |
| codex telemetry | `src/codex/telemetry/{plan.rs,service_tier.rs,limits.rs}` | plan-tier + service-tier resolution, rate-limit window parsing |
| db | `src-tauri/src/db.rs` | SQLite historical-session insert/query/round-trip + context snapshot storage clamped to the model window |
| analyzers | `src-tauri/src/analyzers/{session_trace.rs,cache_health.rs,model_routing.rs,prompt_complexity.rs,inflection.rs}` | trace scan + scan-pass counting, cache-health grading, model-routing split, prompt-complexity scoring, inflection-point detection |
| commands | `src-tauri/src/commands.rs` | reports-bundle assembly from roots; `SessionInfo.intro_pricing`/`has_inflated_tokenizer` wiring for Claude sessions (real-clock, matched against a fresh `cost::active_intro_pricing` call so the test never goes stale across the real cutoff date) and confirmed absent for Codex sessions; `SessionInfo.context_used_tokens` and `build_claude_context_breakdown`'s `used_tokens` reflect current fill (`current_context_tokens`) rather than the historical peak (`max_turn_api_input`), while `context_window_tokens`/the 1M-vs-200K decision still correctly keys off the peak |
| update checks | `src-tauri/src/update_check.rs` | SemVer tag comparison, newer-release detection, prerelease/draft suppression, release URL allowlist |

## v1.4.1 targeted validators

Run these before cutting the context-tracking-fix release:

```bash
cargo test --workspace --jobs 2 compact_boundary
cargo test --workspace --jobs 2 current_context_tokens
cargo test -p pulse --lib --jobs 2 reflects_current_fill
npm --prefix frontend run test -- tests/components/Dashboard.test.ts tests/components/Costs.test.ts
npm --prefix frontend run check
```

Live re-proof (not part of the automated suite -- run manually before release): rebuild, relaunch
with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>`, and re-issue the
exact original repro (`window.__TAURI_INTERNALS__.invoke("get_context_breakdown", {sessionId})`
via CDP `Runtime.evaluate`) against the real session that first surfaced the bug; confirm
`used_tokens` is no longer pinned at the stale historical peak.

## v1.4.0 targeted validators

Run these before cutting the Sonnet 5 release:

```bash
cargo test --workspace --jobs 2 sonnet_5
cargo test -p pulse --lib --jobs 2 intro_pricing
npm --prefix frontend run test -- tests/unit/utils.test.ts tests/components/SessionCard.test.ts
npm --prefix frontend run check
```

Full pre-ship gate (see below) applies as usual. Note: this environment compiles the `pulse`
crate's full dependency tree (Tauri + the `icu_properties`/`idna` chain it pulls in) under
tight available memory — pass `--jobs 2` to `cargo` invocations that touch the `pulse` package
or the build can hit `STATUS_COMMIT_LIMIT_EXCEEDED` and cascade into unrelated-looking
compile errors in transitive dependencies. This is an environment/parallelism characteristic,
not a code defect — see the Sonnet 5 handoff for the diagnosis.

## v1.2.0 targeted validators

Run these before cutting the Fable/Mythos + multi-session Context release:

```bash
cargo test --workspace fable mythos
cargo test --workspace presence
npm --prefix frontend run test -- tests/components/Context.test.ts tests/integration/phase5-flow.test.ts tests/components/Discord.test.ts tests/components/SessionCard.test.ts
cargo test -p pulse update_check --lib
cargo test --workspace --test codex_upstream_contract
cargo test --workspace session_used_tokens_uses_context_snapshot_not_lifetime_total
cargo test --workspace wsl_roots_are_explicit_opt_in
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-codex-rich-presence-upstream.ps1
npm --prefix frontend run test -- tests/components/UpdateBanner.test.ts
npm --prefix frontend run check
```

Full pre-ship remains:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-codex-rich-presence-upstream.ps1
npm --prefix frontend run test
npm --prefix frontend run build
```

## Project cross-reference

| Doc | Where | Purpose |
|:---|:---|:---|
| [README.md](../README.md) | repo root | Install, feature overview, daemon + GUI quick start |
| [CHANGELOG.md](../CHANGELOG.md) | repo root | Release history (config + DB schema v3) |
| [docs/index.md](../docs/index.md) | `docs/` | Documentation hub: architecture, Discord assets, reasoning-effort variants, analyzers, cost calculation |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | repo root | Contribution + local-dev workflow |
