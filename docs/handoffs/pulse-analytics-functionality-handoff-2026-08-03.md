# Pulse analytics functionality and design completion handoff

## Objective and acceptance

Continue the in-progress Pulse v1.7.0 overhaul in
`D:\X\2-Dev\MCP-Servers\cc-discord-presence` and its canonical Codex Rich
Presence owner at
`D:\X\2-Dev\MCP-Servers\Codex-Discord-Rich-Presence`.

The literal accepted outcome is:

1. Every existing Pulse route — Home, Sessions, Context, Costs, Reports,
   Discord, and Settings — uses real local/provider-backed data, distinguishes
   loading, empty, unavailable, partial, stale, and error states, and contains
   no fabricated quota, plan, cost, session, or provider values.
2. Provider selection is functional rather than decorative. Codex, Claude,
   OpenAI API, and Anthropic API lanes appear only when discovered, and an
   unavailable lane explains its real failure instead of rendering fake data.
3. Codex and Claude allowance windows are rendered from backend-reported
   capabilities. Do not invent `5h`, weekly, Spark, Fable-only, plan names, or
   reset values. Codex remaining values count down from 100%; Claude usage
   values count up from 0%.
4. The 7-day Sessions and Reports windows contain sessions that actually fall
   in the last seven days. A backend failure must not look like a genuine empty
   dataset.
5. Costs are exact or explicitly provenance-qualified. Unknown cost is
   `Unavailable`, never `$0.00`, `>=`, a guessed token rate, or a guessed
   monthly total. The same contract applies to Pulse, exports, reports, and
   Discord Rich Presence.
6. Reset notifications are generated exactly once for a genuine quota reset:
   Codex returns to 100% remaining or Claude returns to 0% used. Ordinary
   polling, a changed future `reset_at`, threshold crossings, provider-health
   changes, and duplicate app instances must not create reset notifications.
7. Codex reset credits are read from the Codex backend and presented only when
   reported. The canonical Codex Discord implementation remains
   `Codex-Discord-Rich-Presence`; Pulse bundles or consumes that contract rather
   than diverging from it.
8. No PowerShell window flashes during provider probes. Windows children use
   hidden/no-window process creation. Pulse enforces one producer/window
   instance and does not spawn duplicate pollers.
9. Native notifications and tray state work through Tauri-supported Windows,
   macOS, and Linux primitives. No platform shell popup is used.
10. The approved visual direction is the dense, edge-to-edge workspace in
    `C:\Users\xt0n1\AppData\Local\Temp\codex-clipboard-dc43c1a6-527a-49cb-953d-23223540c697.png`.
    All seven pages must be responsive, coherent, non-repetitive, and verified
    against that source at the same viewport/state.
11. Completion requires fresh backend/browser proof, full Bun/Vitest/Playwright
    and Rust validation, one `open-code-review`, a production Tauri `.exe`,
    installation/readback of exactly one new Pulse process, and task-owned
    resource cleanup.

Do not commit, stage, push, publish, release, reset, stash, clean, or discard
either dirty worktree until every requested validation gate is green and Tony
explicitly approves the Git effect.

## Current state

### Completed and freshly confirmed

- **CONFIRMED — branch and dirty tree preserved.**
  - Pulse branch: `improvements/analytics-functionality`
  - Pulse HEAD: `47d492b980348805474236d0bcb433d5185093cb`
  - Pulse status: 114 modified/untracked entries; tracked diff currently
    reports 80 files, 8,186 insertions, and 2,801 deletions.
  - Canonical Codex branch: `improvements/analytics-functionality`
  - Canonical Codex HEAD: `1ed354b9cee86ba5029323693cae47e41ff9c3d4`
  - Canonical Codex tracked diff currently reports 22 files, 1,825 insertions,
    and 198 deletions, plus two untracked branding images.
- **CONFIRMED — frontend cost surfaces fail closed.**
  - Costs, monthly cockpit, live session cards, historical session rows,
    Reports, and Discord fallback paths no longer convert unknown cost into an
    exact `$0.00`.
  - The live Costs view currently says `Cost unavailable`; 0 of 42 stored
    sessions is priced.
- **CONFIRMED — frontend async states exist on the current source.**
  - Sessions, Context, Costs, Reports, and Settings have loading/error/retry or
    unavailable paths.
  - Sessions project options combine live and historical projects, and changing
    the project refreshes history.
- **CONFIRMED — source diagnostics expose unavailable routes.**
  - The current browser view shows Anthropic API unavailable because no API key
    is configured, Claude subscription unavailable because its token expired,
    Codex subscription authenticated, and OpenAI API unavailable because no key
    is configured.
  - The missing Claude card is therefore an authentication-state result, not a
    license to fabricate Claude metrics.
- **CONFIRMED — current frontend static/unit validation.**
  - `bun run check` from `frontend`: 0 errors and 0 warnings.
  - `bun run test` from `frontend`: 26 files, 199 tests passed.
  - `cargo fmt --all -- --check`: passed.
  - `git diff --check`: passed; output contains only line-ending warnings.
- **CONFIRMED — fresh browser capture exists for all seven routes at
  1488 × 1058.**
  - `D:\X\2-Dev\MCP-Servers\cc-discord-presence\design-qa\current-audit\01-home.png`
  - `D:\X\2-Dev\MCP-Servers\cc-discord-presence\design-qa\current-audit\02-sessions.png`
  - `D:\X\2-Dev\MCP-Servers\cc-discord-presence\design-qa\current-audit\03-context.png`
  - `D:\X\2-Dev\MCP-Servers\cc-discord-presence\design-qa\current-audit\04-costs.png`
  - `D:\X\2-Dev\MCP-Servers\cc-discord-presence\design-qa\current-audit\05-reports.png`
  - `D:\X\2-Dev\MCP-Servers\cc-discord-presence\design-qa\current-audit\06-discord.png`
  - `D:\X\2-Dev\MCP-Servers\cc-discord-presence\design-qa\current-audit\07-settings.png`
  - Contact sheet:
    `D:\X\2-Dev\MCP-Servers\cc-discord-presence\design-qa\current-audit\00-contact-sheet.png`

### Implemented but not yet accepted

- **UNVERIFIED CURRENT HEAD — backend cost contract.**
  The completed cost slice changed totals, averages, forecasts, timelines,
  budgets, analyzers, reports, and exports to carry exact/partial/unavailable
  provenance. Its worker reported `cargo test -p pulse --jobs 2`, Clippy,
  format, and diff-check green before later shared-file edits. Re-run all Rust
  gates on the final merged tree.
- **UNVERIFIED CURRENT HEAD — genuine reset state machine, legacy reset-row
  migration, Codex reset credits, hidden Windows probes, and single-instance
  changes.**
  The backend worker was interrupted at Tony's handoff request. Its edits are
  present in the shared worktree but were not integrated or proven at the live
  seam.
- **UNVERIFIED CURRENT HEAD — Discord quota direction copy.**
  Backend work was requested to emit `7d 38% remaining` for Codex and
  `5h 24% used` for Claude. The currently running old bridge was not rebuilt, so
  the deployed behavior was not re-read.

### Observed blockers and open defects

- **CONFIRMED P0 — 7-day persistence is wrong.**
  - The real browser shows two active Codex sessions dated inside the current
    seven-day window.
  - `get_reports_bundle(days=7)` returns `total_sessions=0`,
    `priced_sessions=0`, seven zero-filled daily points, and
    `cost_basis=unavailable`.
  - `get_reports_bundle(days=30)` returns `total_sessions=42`,
    `priced_sessions=0`, 30 daily points, and 42 summed daily sessions.
  - Sessions → Last 7 days therefore renders an empty history table even though
    recent sessions exist.
  - Root cause has not been freshly proven. Inspect the SQLite timestamp
    normalization and the poller's session upsert path before editing.
- **CONFIRMED P0 — the running bridge still exposes legacy reset spam.**
  - Current unread count: 174.
  - Latest row: ID 306, key `gpt_5.3_codex_spark`, created
    `2026-08-03T07:41:57.001387200Z`.
  - Body: `codex gpt_5.3_codex_spark quota reset at
    2026-08-10T07:41:51+00:00`.
  - The count remained stable during the final short read, but the bridge
    process predates the latest state-machine edits. Restarting/rebuilding is
    required before deciding whether the source fix works.
  - Existing false reset rows must be migrated/dismissed idempotently; do not
    simply mark all user notifications read.
- **CONFIRMED P0 — Rust current-tree proof is blocked by the running bridge.**
  - `cargo test -p pulse --jobs 2` failed before compiling tests because Cargo
    could not replace
    `D:\X\2-Dev\MCP-Servers\cc-discord-presence\target\debug\pulse-dev-bridge.exe`
    while PID 14516 held it open.
  - This is a resource lock, not a test assertion. Re-run after the cleanup
    receipt below.
- **CONFIRMED P1 — provider selection is not yet an end-to-end contract.**
  The global source selector filters Home live sessions and part of Discord,
  but Sessions, Context, Costs, Reports, and some Home historical aggregates
  do not pass a source/provider identity to their backend queries. Either add
  source-scoped backend APIs and propagate the selection everywhere, or hide
  the selector where the backend cannot honor it. Do not display a global
  control that silently mixes providers.
- **CONFIRMED P1 — final Design QA is not valid yet.**
  `D:\X\2-Dev\MCP-Servers\cc-discord-presence\design-qa.md` is stale. It claims
  180 tests, zero proofed routes, old screenshot paths, and
  `Final result: VISUAL QA PASSED`; the required exact result is `passed` or
  `blocked`. Replace it only after a same-state, same-viewport source/runtime
  comparison and a post-fix comparison pass.
- **PENDING — one requested `open-code-review`.**
  It has not been run against the frozen final diff.
- **PENDING — Playwright, full Rust, production build, native install, and
  process proof.**
  No current-tree claim exists for these gates.

## Artifact ledger

| Artifact | State | Evidence |
| --- | --- | --- |
| `D:\X\2-Dev\MCP-Servers\cc-discord-presence` | DIRTY, preserved | Branch/HEAD/status above; no stage/commit/push |
| `D:\X\2-Dev\MCP-Servers\Codex-Discord-Rich-Presence` | DIRTY, preserved | Branch/HEAD/status above; no stage/commit/push |
| `frontend/src/App.svelte` | Modified | Shared full-width/flex shell |
| `frontend/src/styles/global.css` | Modified | Existing Pulse tokens and responsive contracts |
| `frontend/src/views/Dashboard.svelte` | Modified | Dense Home rail/work layout and fail-closed metrics |
| `frontend/src/views/Sessions.svelte` | Modified | Real history controls, cost provenance, loading/error states |
| `frontend/src/views/Context.svelte` | Modified | Active-session context and retry/error states |
| `frontend/src/views/Costs.svelte` | Modified | Exact/partial/unavailable cost presentation |
| `frontend/src/views/Reports.svelte` | Modified | Period-specific bundle load and fail-closed cost/report states |
| `frontend/src/views/Discord.svelte` | Modified | Backend-authoritative preview and selected-route allowances |
| `frontend/src/views/Settings.svelte` | Modified | Adaptive provider/plan/data/source diagnostics |
| `frontend/src/components/AccessSourceBar.svelte` | New/untracked | Authenticated source selector |
| `frontend/src/components/AllowanceRail.svelte` | New/untracked | Provider allowance and reset-credit presentation |
| `frontend/src/components/NotificationCenter.svelte` | New/untracked | Durable notification center |
| `frontend/src/components/DataSourceInspector.svelte` | New/untracked | Route health/provenance diagnostics |
| `frontend/src/lib/access.ts` | New/untracked | Source/allowance frontend helpers |
| `frontend/src/lib/plans.ts` | New/untracked | Provider-specific manual plan options, not detected plan proof |
| `src-tauri/src/access.rs` | New/untracked | Adaptive provider access DTOs and capability mapping |
| `src-tauri/src/notifications.rs` | New/untracked | Durable notification store/state machine/migrations |
| `src-tauri/src/commands.rs` | Modified hot file | Polling, access, analytics, Discord preview, notifications |
| `src-tauri/src/db.rs` | Modified hot file | Analytics schema/query/upsert and cost provenance |
| `src-tauri/src/report.rs` | Modified | Provenance-aware exports/reports |
| `src-tauri/src/dev_bridge.rs` | Modified | Real authenticated dev transport; no live fixtures |
| `src-tauri/src/main.rs` | Modified | Tray/native notification/single-instance startup |
| `src/codex/account_usage.rs` | Modified | Codex backend quota/reset-credit probe, hidden Windows process |
| `src/usage.rs` | Modified hot file | Claude subscription usage/capability parsing |
| `src/discord.rs` and `src/codex/discord.rs` | Modified hot files | Canonical Rich Presence composition |
| `tests/**`, `src-tauri/tests/**`, `frontend/tests/**` | Modified/new | 199 frontend tests currently green; Rust final rerun pending |
| `frontend/package-lock.json`, `frontend/bun.lock` | Both present/changed | Reconcile intentionally before release; do not delete blindly |
| `design-qa/current-audit/**` | Fresh evidence | Seven 1488 × 1058 captures and contact sheet |
| `design-qa.md` | Stale/invalid | Must be rewritten from final comparison evidence |

Use `git status --short` in each repository for the complete current path list.
Do not infer ownership from this summarized table and do not reset unrelated
dirty files.

## Decisions and constraints

1. **Provider truth is authoritative.** A configured credential, a manual plan
   override, a transcript, or a cached response is diagnostic state, not fresh
   provider proof.
2. **No hardcoded capability windows.** Render the ordered windows the backend
   actually returns. UI label helpers may format reported durations/scopes, but
   must not create missing buckets.
3. **No guessed plan.** Supported manual override options are provider-specific,
   but auto-detected plan text is only shown when a real signal reports it.
4. **Cost must fail closed.** `unavailable` and `partial` are first-class states.
   A partial total must name its coverage; unknown is not zero.
5. **One notification owner.** The durable backend reset-transition state
   machine owns native, tray, and in-app notification creation. Frontend polling
   may display records; it must not independently synthesize reset events.
6. **One process owner.** Pulse must have one analytics/notification producer.
   Never mass-kill `node.exe`, `bun.exe`, Rust binaries, shells, or MCP hosts.
7. **Frontend ownership.** The primary orchestrator owns frontend structure,
   visual fidelity, integration, browser QA, and final proof. Backend slices may
   be delegated, but shared hot files require one writer at a time.
8. **Canonical Codex owner.** Changes to Codex quota parsing and Rich Presence
   behavior must land first in
   `D:\X\2-Dev\MCP-Servers\Codex-Discord-Rich-Presence`; Pulse must consume or
   synchronize that owner through the existing upstream contract.
9. **Dirty worktrees are protected.** No reset/stash/clean, drive-by formatting,
   lockfile deletion, broad refactor, or Git effect.
10. **No model or reasoning override until Codex restarts.** The active runtime
    reported a changed model catalog. A future operator must use the harness
    model/route it actually exposes instead of trusting self-identification.

## Implementation state

### Frontend contracts already present

- `HistoricalSession.known_cost`, `CostTotals.cost_basis`,
  `priced_sessions`, and related report/forecast DTOs support fail-closed
  rendering.
- Reports has period-specific loading/error handling and seven-day tests, but
  the real backend currently returns an incorrect empty 7-day dataset.
- Sessions has a seven-day selector, project refresh, historical-project
  options, loading/error/retry, exact-cost availability, compare, filter,
  search, and export controls.
- Context guards missing/zero context and loads breakdowns by active IDs.
- Settings uses explicit unavailable/loading states for DB size/session count
  and shows all discovered source diagnostics.
- The Home shell is edge-to-edge and fills the viewport. Current evidence is in
  `design-qa/current-audit`.

### Backend contracts present but not fully integrated

- `src-tauri/src/access.rs` models source identity, provider, capability,
  availability, freshness, provenance, allowance windows, and reset credits.
- `src-tauri/src/notifications.rs` contains durable tables, state, legacy-row
  migration, reset-transition logic, tray presentation, and native-delivery
  helpers.
- `src-tauri/src/db.rs` and `src-tauri/src/commands.rs` carry the analytics and
  reporting persistence/query seams that must be debugged for 7-day history.
- The authenticated bridge at `/__pulse_api` uses the real Rust command
  dispatcher; fixture routes are test-only.

### Shared-file conflict boundaries

- Only one worker may edit each of these at a time:
  - `src-tauri/src/commands.rs`
  - `src-tauri/src/db.rs`
  - `src-tauri/src/notifications.rs`
  - `src/usage.rs`
  - `src/discord.rs`
  - `src/codex/discord.rs`
  - companion `crates/codex-presence-core/src/usage.rs`
  - companion `src/discord.rs`
- The primary orchestrator integrates all frontend/API signatures in
  `frontend/src/lib/api.ts` and `frontend/src/lib/stores.ts`.
- Do not run a formatter across either repository while another worker edits a
  shared hot file.

## Validation

### Fresh commands and results on 2026-08-03

From `D:\X\2-Dev\MCP-Servers\cc-discord-presence\frontend`:

```powershell
bun run check
```

Result: **PASS** — `svelte-check found 0 errors and 0 warnings`.

```powershell
bun run test
```

Result: **PASS** — 26 files, 199 tests.

From `D:\X\2-Dev\MCP-Servers\cc-discord-presence`:

```powershell
cargo fmt --all -- --check
git diff --check
```

Result: **PASS**. Diff check printed only line-ending conversion warnings.

```powershell
cargo test -p pulse --jobs 2
```

Result: **BLOCKED BEFORE TEST EXECUTION**:

```text
error: failed to remove file
D:\X\2-Dev\MCP-Servers\cc-discord-presence\target\debug\pulse-dev-bridge.exe
Caused by: Access is denied. (os error 5)
```

The exact owner was PID 14516, the task-owned bridge. Re-run after cleanup.

### Required final gate; not yet executed on the final tree

```powershell
cd D:\X\2-Dev\MCP-Servers\cc-discord-presence\frontend
bun run check
bun run test
bun run build
bun run test:e2e
bun run test:e2e:tauri

cd D:\X\2-Dev\MCP-Servers\cc-discord-presence
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --jobs 2
git diff --check
```

Run the companion repository's documented format, Clippy, test, and upstream
contract suites separately. A green Pulse suite does not prove the canonical
repo.

After implementation and the suites above, run `open-code-review` exactly once
against the frozen diff, resolve every in-scope P0/P1/P2, and rerun affected
validators. Do not run it early as an iterative linter.

## Remaining work

Execute in this dependency order. Stop if an assumption fails; do not switch
design or data contracts silently.

### 1. Re-enter and reproduce with a fresh bridge

1. Confirm the cleanup receipt below: no listener on 1420/1421 and no exact
   task-owned dev process remains.
2. Run the narrowest Rust tests for notifications, access, DB history, and
   reports before starting the bridge.
3. Start `bun --cwd frontend run dev` with a fresh
   `PULSE_DEV_BRIDGE_TOKEN`; record exact PIDs, parent chain, ports, scratch,
   logs, expiry, and close mechanism.
4. Query `get_access_snapshot`, `get_snapshot`, `get_session_history(7)`, and
   `get_reports_bundle(7)` directly through `/__pulse_api`.

**Stop condition:** the fresh bridge must be running current bytes. Do not
interpret the old PID 14516 behavior as evidence for the new source.

### 2. Fix 7-day persistence at the owner

1. Compare active session IDs/timestamps with the corresponding rows in
   `pulse-analytics.db`.
2. Verify timestamp units, UTC conversion, and the
   `COALESCE(started_at, created_at, updated_at)` filter used by 7-day queries.
3. Verify the background poller upserts the current session into the same DB
   file queried by history/reports.
4. Add a failing Rust integration test containing:
   - one session inside seven days;
   - one older than seven days;
   - nullable legacy timestamps;
   - the same session refreshed by the poller.
5. Apply the smallest DB/poller fix.
6. Prove `get_session_history(7)` and `get_reports_bundle(7)` return the recent
   live session and exactly seven daily points.

**Stop condition:** real bridge 7d data reconciles to DB rows and browser
Sessions/Reports, not merely a fixture.

### 3. Finish reset-notification semantics and migration

1. Run the notification state-machine tests on current source.
2. Test repeated identical snapshots, changing future `reset_at`, stale cached
   snapshots, process restart, and concurrent poll/native display.
3. Verify Codex emits once only on `<100 → 100 remaining`; Claude emits once
   only on `>0 → 0 used`.
4. Verify Spark's 100% initial observation creates no event.
5. Run the idempotent legacy migration against a copied DB first, then the real
   local DB through the application lifecycle.
6. Poll at least three cycles and prove notification count/IDs do not increase.
7. Verify tray unread count and in-app unread count agree.

**Stop condition:** zero new records across repeated ordinary polls and exactly
one record for a synthetic/fixture transition test. Do not trigger a real
provider reset to prove the fixture seam.

### 4. Complete provider-scoped analytics

1. Decide and document the canonical query key: stable source ID when source
   identity matters; provider slug only when intentionally aggregated.
2. Add optional source/provider scope to summary, history, cost totals,
   forecast, hourly activity, reports bundle, and context APIs.
3. Propagate the selected source from `AccessSourceBar` through Sessions,
   Context, Costs, Reports, Discord, and Home historical aggregates.
4. Keep `all` as explicit cross-provider aggregation.
5. Add cross-provider fixtures proving a Claude selection cannot show Codex
   sessions/cost/quotas and vice versa.

**Stop condition:** every visible global source selection changes all
source-sensitive consumers or the selector is hidden on unsupported views.

### 5. Reconcile provider/plan/quota truth and Discord

1. Refresh Claude authentication only through the user's legitimate local
   provider flow. Until then, show `token expired`; do not fabricate Claude
   limits.
2. Verify Codex and Claude plan labels against live provider responses. Manual
   override remains visibly manual.
3. Render every reported allowance window generically and in provider order.
4. Validate Codex reset credits and expiry readback.
5. In the canonical Codex repo, make Discord quota text directional:
   `remaining` for Codex and `used` for Claude.
6. Prove backend and Discord preview omit cost when unavailable and never emit
   comparison prefixes such as `>=`.
7. Run the canonical upstream synchronization/contract check before accepting
   Pulse.

**Stop condition:** Discord, Pulse cards, and provider backend agree on the
same plan, windows, values, direction, freshness, and cost availability.

### 6. Finish the seven-page product sweep

Use the approved screenshot, existing Pulse tokens, and current audit captures.
Do not invent a new direction.

1. Home: preserve the edge-to-edge rail/work hierarchy; explain unavailable
   sources without repeating diagnostics.
2. Sessions: verify real 7d/30d/90d/1y differences, filters, search, compare,
   expand, and export.
3. Context: verify source scoping, context switching, inventory accordions,
   zero-window guards, and copy action.
4. Costs: verify unavailable/partial/exact states, project filter, budget save
   success/error, chart/table, and export.
5. Reports: verify each period returns a different real request/dataset,
   error/Retry, severity filter, Markdown, and HTML export.
6. Discord: verify all toggles, ordering, presets, desktop identity, exact
   preview, unavailable Discord-user state, and save persistence.
7. Settings: verify provider/plan/theme mutation serialization, update check,
   export, destructive confirmation, clear-history error/success, and source
   diagnostics.
8. Test desktop, 1280-class, tablet, and 390 × 844 layouts. Persistent controls
   may not overflow the viewport; tables own their horizontal scroll.

**Stop condition:** every primary control changes the intended state and every
route has honest loading, empty, unavailable, partial, error, and success
behavior where applicable.

### 7. Final Design QA

1. Capture Home at exactly 1488 × 1058 with the same state as the approved
   reference.
2. Put the source and implementation into one side-by-side comparison image.
3. Inspect typography, spacing/rhythm, colors/tokens, image/logo quality, copy,
   controls, density, and full-width composition.
4. Fix all P0/P1/P2 findings.
5. Capture the revised same-state viewport and compare again.
6. Capture all seven routes wide plus required mobile states.
7. Rewrite `design-qa.md` with exact paths, pixel dimensions, CSS viewport,
   density, interaction proof, console state, comparison history, and exact
   `final result: passed` or `final result: blocked`.

**Stop condition:** a post-fix comparison contains no actionable P0/P1/P2.

### 8. Final validation, review, build, install, cleanup

1. Run the complete validation matrix above.
2. Run one `open-code-review`; resolve in-scope findings and rerun affected
   tests.
3. Build the production Tauri bundle with the repository's canonical build
   command.
4. Identify the existing global Pulse installation path and process before
   overwrite. Use a recoverable installer/update path.
5. Verify installed executable hash/version and launch exactly one new
   `Pulse.exe`.
6. Verify no Vite/bridge/debug listener or task scratch remains. Preserve all
   unrelated processes.

**Stop condition:** installed consumer shows the final source behavior, exactly
one Pulse process is open, and the task resource ledger is empty.

## Orchestrator brief

Copy/paste this into the continuation task:

> Continue Pulse v1.7.0 from
> `D:\X\2-Dev\MCP-Servers\cc-discord-presence\docs\handoffs\pulse-analytics-functionality-handoff-2026-08-03.md`.
> Read that file and both repositories' AGENTS.md before acting. Preserve both
> dirty worktrees; do not reset, stash, clean, stage, commit, push, publish, or
> release. The two current P0s are real 7-day persistence and duplicate
> quota-reset notifications. Reproduce them with a freshly rebuilt authenticated
> bridge, fix them at the Rust owner with tests, then make provider selection an
> end-to-end query contract. The primary orchestrator owns frontend/design/browser
> QA/integration; backend workers must not edit frontend files. Treat
> `src-tauri/src/commands.rs`, `db.rs`, `notifications.rs`, and the canonical
> Discord usage/composition files as single-writer hot files. Costs must remain
> exact/partial/unavailable; no guessed plans, windows, costs, reset values, or
> provider data. Run full Bun/Vitest/Playwright and Rust suites, one
> open-code-review, same-input Design QA against the approved screenshot, build
> and install the Tauri `.exe`, prove exactly one Pulse process, and clean every
> task-owned resource. Do not set a model or reasoning override until Codex has
> restarted and the active harness exposes the new catalog.

### Suggested cohort order

```text
Fresh bridge + reproduction
        |
        +--> DB/poller 7d fix ---------+
        |                              |
        +--> notification reset fix ---+--> provider-scoped API integration
        |                              |             |
        +--> canonical Discord truth --+             v
                                             frontend seven-page sweep
                                                       |
                                                       v
                                      full suites -> one review -> Design QA
                                                       |
                                                       v
                                           build/install -> cleaner
```

### Worker return contract

Every worker returns:

1. changed absolute paths and why;
2. exact commands run and relevant output;
3. live/runtime proof versus fixture-only proof;
4. remaining unknowns and failed assumptions;
5. task-owned processes, ports, scratch, or other residue;
6. an explicit statement that it did not stage, commit, push, reset, stash, or
   clean.

## Risks and rollback

- The largest risk is promoting fixture/unit green to provider/runtime truth.
  Always verify the fresh bridge and installed consumer.
- The current 7d result can be caused by wrong DB path, missing upsert,
  timestamp normalization, or filtering. Establish root cause before editing.
- Notification migration touches durable user state. Test on a copied DB and
  use an idempotent migration marker. Do not delete all notifications.
- Two lockfiles are present. Reconcile based on the accepted Bun/npm ownership
  contract; do not remove one just for a clean diff.
- Claude is currently unauthenticated. The correct degraded state is
  unavailable with the real reason; provider metrics cannot be validated until
  authentication is restored.
- No rollback baseline was created because reset/stash/commit were forbidden.
  Recovery is the existing Git HEAD plus the untouched dirty worktrees. Make
  future edits surgical and keep a path ledger; never use `git reset --hard` or
  `git checkout --`.

## Cleanup receipt

Cleaner completed after this handoff was first written.

| Resource | Owner task and identity proof | Close mechanism | Consumer readback | Final state |
| --- | --- | --- | --- | --- |
| Bun dev launcher | PID 11428; parent 26572; created 2026-08-03 03:06:26 local; `C:\Users\xt0n1\.bun\bin\bun.exe run dev` | Exact-PID `Stop-Process` after fresh command-line/executable match | PID absent in all three post-cleanup samples | CLOSED |
| Node Vite supervisor | PID 3052; parent 11428; created 2026-08-03 03:06:26 local; `node.exe scripts/dev.mjs` | Exact-PID `Stop-Process` after fresh identity match | PID absent; port 1420 not listening in all three samples | CLOSED |
| Rust real-data bridge | PID 14516; parent 3052; created 2026-08-03 03:06:33 local; exact workspace `target\debug\pulse-dev-bridge.exe` | Exact-PID `Stop-Process` after fresh identity match | PID absent; port 1421 not listening in all three samples | CLOSED |
| Vite esbuild worker | PID 9484; parent 3052; exact workspace `frontend\node_modules\@esbuild\win32-x64\esbuild.exe --service=0.25.12 --ping` | Exact-PID `Stop-Process` after fresh identity match | PID absent in all three samples | CLOSED |
| Bun console host | PID 30688; parent 11428; task lineage proved before stop | Exact-PID `Stop-Process` | PID absent in all three samples | CLOSED |
| In-app browser preview | The task-owned browser session reported zero remaining tabs | No further close needed | `iab.tabs.list()` returned `[]` | CLOSED |
| Scratch/log directory | `C:\Users\xt0n1\AppData\Local\Temp\pulse-qa-2f0c1402e70f47b58d02e17190c4c502`; contained only `dev.stdout.log` and `dev.stderr.log` | Deleted the two exact files, then removed the verified empty directory | Path absent in all three samples | CLOSED |

Post-cleanup observation window: three samples from
`2026-08-03T04:02:44-05:00` through `2026-08-03T04:02:50-05:00`.
Every sample found zero task processes, zero listeners on 1420/1421, and no
scratch path. No `Pulse.exe` process was running at the end. No process was
targeted by executable name or wildcard.

Preserved intentionally: both dirty worktrees, the handoff, current audit
screenshots/contact sheet, test output artifacts already inside the repository,
and all unrelated workstation processes.
