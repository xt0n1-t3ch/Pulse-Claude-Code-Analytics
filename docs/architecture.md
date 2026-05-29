# Architecture

Pulse is two artifacts built from one Cargo workspace: a headless Rust daemon
(`cc-discord-presence`) that pushes Discord Rich Presence, and the Pulse desktop
GUI (`pulse`, Tauri 2.0 + Svelte 5) that reads the same session data for
analytics. Both crates live under [`../Cargo.toml`](../Cargo.toml) as workspace
members, and the GUI backend depends on the daemon crate by path so the two
share one parsing, pricing, and provider model.

The data flows in one direction through four stages: Claude Code (or Codex)
writes session transcripts to disk → the daemon crate parses them into
snapshots and computes cost → the daemon pushes a Rich Presence frame over
Discord IPC while the Tauri GUI upserts the same snapshots into SQLite → the
Svelte frontend invokes Tauri commands that run analyzers over that SQLite
history. See [`index.md`](index.md) for the full doc map.

## Component map

```
Claude Code / Codex                      ~/.claude (or ~/.codex)
  └─ writes JSONL transcripts ─────────▶  projects/<encoded>/*.jsonl
  └─ writes statusline snapshot ───────▶  discord-presence-data.json
                                                │
                  ┌─────────────────────────────┴─────────────────────────────┐
                  ▼                                                             ▼
  daemon crate: cc-discord-presence (src/)                  Pulse GUI backend (src-tauri/)
    session.rs   JSONL → SessionAccumulator                   commands.rs  Tauri IPC + 5s poller
    cost.rs      speed-aware pricing                          db.rs        SQLite upsert + queries
    discord.rs   Rich Presence over Discord IPC               analyzers/   cchubber-style reports
    usage.rs     Anthropic usage API + Extra Usage            report.rs    HTML + Markdown export
    codex/       parallel Codex implementation                      │
                  │                                                  ▼
                  ▼                                       Svelte 5 frontend (frontend/src/)
            Discord profile                                 lib/api.ts  typed invoke() wrappers
            (Rich Presence)                                 lib/stores  reactive stores + poll
                                                            views/      Dashboard … Reports
```

## Daemon crate — `cc-discord-presence` (`src/`)

The daemon's module surface is declared in [`../src/lib.rs`](../src/lib.rs), and
its CLI entrypoint [`../src/main.rs`](../src/main.rs) routes `status`, `doctor`,
`claude <args>`, and the no-arg foreground mode through a single-instance lock in
[`../src/process_guard.rs`](../src/process_guard.rs). The no-arg path lands in
`run_daemon()` in [`../src/app.rs`](../src/app.rs), which loops on a poll
interval and calls `tick_session_cycle()` once per iteration; the GUI was made
authoritative for analytics in v3.0, so the daemon only updates Discord and
prints a one-liner pointing users at the Pulse window.

Session ingestion centers on `SessionAccumulator` in
[`../src/session.rs`](../src/session.rs), a struct that folds a stream of
`JsonlMessage` events into one running snapshot. Each `process_jsonl_message`
call updates token totals (input, output, cache-creation, cache-read), the
`max_turn_api_input` high-water mark used for 1M-context detection, the
five-tier `ReasoningEffort`, an `ActivityTracker` for idle debounce, and the
per-turn `Speed` and `service_tier` that drive cost. Reads are cursor-based and
incremental through a `SessionParseCache`, so re-reading a growing transcript
only processes the new tail rather than the whole file each tick.

Cost is computed per turn in [`../src/cost.rs`](../src/cost.rs) and is
speed-aware: `calculate_cost_with_context_and_speed()` first applies the beta
1M-context surcharge (2× input, 1.5× output, 2× cache for beta models whose
total API input exceeds 200K tokens) and then scales the whole turn by
`speed_multiplier()`, which returns `FAST_RATE_MULTIPLIER` for a fast turn on a
fast-capable model and `1.0` otherwise. The four billable categories returned by
`calculate_category_costs()` are defined to always sum to that same total, so
per-turn accumulation reconciles with the accumulated session cost rather than
drifting. The pricing table and GA-vs-beta surcharge rules are documented in
detail in [`cost-calculation.md`](cost-calculation.md).

Discord output lives in [`../src/discord.rs`](../src/discord.rs), which holds a
`DiscordPresence` IPC client (default application ID `1466664856261230716`,
overridable via `CC_DISCORD_CLIENT_ID`) and the multi-tier asset resolver: a
`mp:` reference passes through, a portal-confirmed asset key is used directly, a
plain `https://` URL is wrapped as `mp:external/https/<rest>` for the Discord
Media Proxy, and anything else falls back to passing the key as-is. Uploading
the `claude-code` large-image key so the logo actually renders is covered in
[`discord-assets.md`](discord-assets.md). Plan-level limits and Extra Usage come
from [`../src/usage.rs`](../src/usage.rs), which calls the Anthropic usage API on
a tiered cache (30s for the first few fetches, then 300s, backed by an on-disk
cache file) and, on an Extra Usage spike, fires the Win32 beep in
[`../src/sound.rs`](../src/sound.rs).

Codex support is a parallel implementation under
[`../src/codex/mod.rs`](../src/codex/mod.rs) rather than a generalization of the
Claude path: it carries its own `config`, `cost`, `discord`, `process`,
`session`, and `telemetry` submodules, including a Codex-specific
`SessionAccumulator` and a `telemetry` tree (`limits`, `plan`, `service_tier`)
for Codex's distinct rate-limit and plan model. The provider in effect is
resolved through [`../src/provider.rs`](../src/provider.rs), so the GUI can
analyze either Claude or Codex history without conflating the two.

The two data sources are not equal in authority: the JSONL transcripts under
`~/.claude/projects/<encoded>/*.jsonl` are the zero-config fallback and the only
source of granular token breakdowns, while the statusline snapshot at
`~/.claude/discord-presence-data.json` is authoritative for cost. When both are
present, `tick_session_cycle()` merges the statusline session over the
JSONL-derived ones, so the displayed `total_cost_usd` and `total_api_duration_ms`
come from Claude Code's own accounting and the JSONL only supplies the per-token
detail the statusline omits.

## Pulse GUI backend — `pulse` (`src-tauri/`)

The GUI backend is split lib-from-bin: [`../src-tauri/src/lib.rs`](../src-tauri/src/lib.rs)
exposes `commands`, `db`, `report`, `report_template`, and `analyzers` as a
library so they are unit-testable without a window, while
[`../src-tauri/src/main.rs`](../src-tauri/src/main.rs) is the thin binary that
builds the Tauri app, registers the system tray, wires the close-to-tray window
behavior, and registers every IPC command in one `invoke_handler!`. `main()`
calls `start_background_poller()` and then `refresh_usage()` at startup so the
first poll cycle forces fresh usage rather than serving a stale cache.

[`../src-tauri/src/commands.rs`](../src-tauri/src/commands.rs) is the IPC surface
and the home of the 5-second background poller that refreshes live sessions,
rate limits, Discord presence, and SQLite session upserts on a fixed
`REFRESH_INTERVAL`. Every command is a `#[tauri::command]` the frontend can
`invoke()`; the heavy analytics commands offload onto a worker so the UI thread
stays responsive.

Persistence is a single SQLite database at `~/.claude/pulse-analytics.db`,
defined in [`../src-tauri/src/db.rs`](../src-tauri/src/db.rs). The `sessions`
table is the spine — keyed by session `id`, carrying a `provider` column
(`claude` or `codex`), per-category token counts and costs, `effort`,
`context_window`, and active/timestamp bookkeeping — alongside a `daily_stats`
rollup, a single-row `budget_config`, and a `sessions_fts` FTS5 virtual table
(porter/unicode61) kept in sync by insert/update triggers to power
`search_sessions`. SQLite is opened once per process in WAL mode, and historical
queries coalesce `started_at`, `created_at`, and `updated_at` so a null timestamp
never drops a session out of a view, filter, or export.

The analyzers in [`../src-tauri/src/analyzers/`](../src-tauri/src/analyzers/) are
a native Rust reimplementation of the cchubber CLI that run in-process from the
stored SQLite history with no Node subprocess: `cache_health` grades the
trend-weighted prompt-cache hit ratio A–F, `inflection` flags ≥2× cost/session
deviations, `model_routing` splits Opus/Sonnet/Haiku and estimates rerouting
savings, `tool_frequency`, `prompt_complexity`, `session_health`, and
`session_trace` derive session-shape signals from the JSONL traces, and
`recommendations` consumes all of the above to emit rule-based cards each
carrying a paste-ready `fix_prompt`. The grade thresholds, rule list, and the
contract for adding a new recommendation are documented in
[`analyzers.md`](analyzers.md).

`get_reports_bundle` is the single-scan entrypoint that the Reports view uses:
one call pulls the session history from `db.rs`, loads the JSONL traces once via
`session_trace::load_session_traces_from_roots`, then threads that single
`sessions` + `traces` pair through every analyzer and returns one `ReportsBundle`
(cache health, model routing, inflections, tool frequency, prompt complexity,
session health, trace overview, and recommendations). Computing every report
from one load avoids the per-report re-scan the individual commands would
otherwise each pay.

Report export is handled by [`../src-tauri/src/report.rs`](../src-tauri/src/report.rs)
and [`../src-tauri/src/report_template.rs`](../src-tauri/src/report_template.rs),
which render the bundle to branded HTML and Markdown for the Reports view's
one-click export.

## Svelte 5 frontend (`frontend/src/`)

The frontend is a Tauri webview, not a network client: [`../frontend/src/App.svelte`](../frontend/src/App.svelte)
mounts the sidebar, top bar, and a view switch driven by the `currentView`
store, and on mount it starts a 5-second poll and loads the Discord user. The
seven views — Dashboard, Sessions, Context, Costs, Reports, Discord, Settings —
each map to a backend command set and are swapped in place without a router.

All backend access goes through `invoke()` wrappers in
[`../frontend/src/lib/api.ts`](../frontend/src/lib/api.ts), which give every
Tauri command a typed TypeScript signature so the IPC boundary is the only place
raw command names appear. Reactive state lives in
[`../frontend/src/lib/stores.ts`](../frontend/src/lib/stores.ts): a `poll()`
function batches `getHealth`/`getMetrics`/`getLiveSessions`/`getRateLimits`/`getPlanInfo`
through `Promise.all`, pushes the results into writable stores, and raises toast
warnings when session, weekly, or Extra Usage thresholds are crossed — the same
spike the daemon beeps on, surfaced in the UI. Display preferences (the Discord
field toggles) are `localStorage`-persisted stores that mirror their state back
to the backend via `setDiscordDisplayPrefs` so the daemon and the GUI agree on
what Rich Presence shows.
