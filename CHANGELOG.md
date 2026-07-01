# Changelog

All notable changes to **Pulse** are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning is [SemVer](https://semver.org/).

## [1.4.2] — 2026-07-01

v1.4.2 is the release-hygiene patch for the v1.4.1 lane. It keeps the published v1.4.1 tag immutable, then ships the final Discord Rich Presence contract fixes, Sonnet 5 copy corrections, docs cleanup, and dependency sweep as a new patch release. No public API was removed.

### Fixed

- Discord display preferences now cross the Tauri IPC boundary with the correct camelCase argument contract. Turning off Git branch persists to the Claude and Codex config files and removes the branch from both the real Discord broadcaster and the Pulse Live Preview. (#46)
- Pulse Live Preview now renders the backend Rich Presence payload instead of reconstructing Discord details/state in Svelte, so preview and live Discord use the same source of truth. (#46)
- Plain Claude thinking blocks no longer render as a fake workflow systems label. `ULTRACODE` is shown only for a live workflow record, and stale workflow records expire instead of keeping Discord stuck after the workflow ends. (#46)
- Codex systems copy uses `Tool active` for pending tool calls and avoids workflow wording for normal thinking activity. (#46)

### Changed

- README and docs now introduce Sonnet 5 with Anthropic's published introductory cache prices: 5-minute writes at $2.50 / MTok, 1-hour writes at $4.00 / MTok, and cache reads at $0.20 / MTok during the introductory window. (#46)
- The release notes template remains CHANGELOG-driven, so GitHub Releases use the curated section for the tag instead of stale generated wording. (#46)
- Local runtime proof artifacts under `target/evidence` were treated as scratch output and removed from the workspace cleanup path. (#46)

### Validated

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --jobs 1 -- -D warnings`
- `cargo test --workspace --jobs 1`
- `npm --prefix frontend run check`
- `npm --prefix frontend run test`
- `npm --prefix frontend run build`
- `cd src-tauri && cargo tauri build`

## [1.4.1] — 2026-06-30

v1.4.1 fixes a real, live-confirmed data-staleness bug in every "how full is my context right now" UI surface, and adds explicit aggregation-scope labels to the Dashboard and Costs cost totals. No public API was removed.

### Fixed

- Claude Discord Rich Presence uses the square `large` Developer Portal asset again instead of the letterboxed `claude-code` upload, so the activity card art fills the square slot instead of rendering as a tiny wide mascot inside black padding.
- Discord field toggles now apply through one shared contract for Claude and Codex. Turning off Git branch removes the branch from both the live broadcaster and the Pulse preview; the new Systems toggle exposes only safe generic signals such as `ULTRACODE`, `Tool active`, or `1 agent`.
- Claude and Codex activity labels suppress decorative/noisy shell commands such as `echo =====` banners and temp-path launchers. Live Claude transcript writes also reactivate stale `Idle` activity, so active sessions update back to `Thinking` instead of staying stuck.
- The Context Window header card, the "Per-session utilization" panel, and the Sessions/Dashboard "ctx-1m" badge all read a field (`max_turn_api_input`) that is a monotonically-increasing, never-resetting all-time peak across a session's entire lifetime -- including across compactions. A session that hit a high-water mark before an auto-compaction kept showing that historical peak, and the resulting false "Context is 100% full -- CRITICAL" recommendation, indefinitely after the compaction had actually emptied the context back out. Confirmed live: `get_context_breakdown` returned `used_tokens: 999486` for a real, currently-running session whose own JSONL transcript recorded a `compact_boundary` event with `compactMetadata.postTokens: 25500` 2.5 hours earlier. (#44)
- `src/session.rs` now detects `{"type":"system","subtype":"compact_boundary"}` events (which Claude Code writes on every compaction, with the authoritative post-compaction size in `compactMetadata.postTokens`) and tracks a new field, `current_context_tokens`, separately from `max_turn_api_input`. The new field resets to the real post-compaction size at each boundary and otherwise tracks the most recent turn's total -- the field every "current state" UI surface now reads. `max_turn_api_input` is untouched and keeps its correct, separate role: detecting whether a session has ever required the 1M context tier (a lifetime question that should never decrease). See [docs/context-tracking.md](docs/context-tracking.md). (#44)

### Changed

- Dashboard's "Total Cost" KPI tile is now labeled "Total Cost (Live)" (it sums only currently-live sessions); the Costs view's "Total Spent" tile is now labeled "Total Spent (30d)" (it sums the persisted 30-day historical database). Both totals were already real and correctly computed -- they answer different, legitimate questions that weren't previously distinguished by their labels. (#44)

## [1.4.0] — 2026-06-30

v1.4.0 adds native Claude Sonnet 5 support, including a generic, date-driven introductory-pricing system that automatically reverts to standard pricing with no manual flag, and fixes a pre-existing 1M-context pricing bug discovered while building it. No public API was removed.

### Added

- Claude Sonnet 5 (`claude-sonnet-5`) native support: introductory pricing of $2.00 input / $10.00 output per MTok through August 31, 2026, then $3.00 / $15.00 standard — automatically, evaluated against the real clock on every poll cycle. (#43)
- A reusable introductory-pricing mechanism in `src/cost.rs`: clock-injected `cost::model_pricing_at(model_id, now)` (the real source of truth; `cost::model_pricing(model_id)` stays the existing real-clock entry point) and `cost::active_intro_pricing(model_id, now)`, which returns the active promo only while genuinely inside its window — `None` both for models with no promo and once a promo's window has closed. Adding the next time-boxed launch is a registry addition, not new branching logic. (#43)
- Sessions and Dashboard live-session cards show a new "Intro Pricing" badge sourced entirely from the backend (`SessionInfo.intro_pricing`) — exact discounted rate, human end date, and the rate it reverts to, with zero date math or hardcoded pricing in the frontend. (#43)
- `cost::has_inflated_tokenizer()` now also covers Sonnet 5 (Anthropic-confirmed new tokenizer, ~1.0-1.35x more tokens than Sonnet 4.6 for the same input, permanent and independent of the promo window) — the existing Sessions/Dashboard `⚠` marker now triggers for it too. (#43)
- [docs/sonnet-5.md](docs/sonnet-5.md): official specs, the introductory-pricing mechanism, Anthropic's published cache multipliers, and the 1M-context bug fix below.

### Fixed

- `cost::is_ga_1m_context("claude-sonnet-5")` previously returned `false` — the generic Sonnet/Opus version parser expects a two-segment id like `"4-6"` and Sonnet 5's id has only one numeric segment (`"5"`), so it fell through and would have applied the beta long-context 2x/1.5x surcharge above 200K tokens. A dedicated `is_sonnet_5_class()` classifier (mirroring the existing `is_mythos_class()` pattern for Fable 5 / Mythos 5) now short-circuits `is_ga_1m_context`, `supports_1m_context`, `has_inflated_tokenizer`, and the pricing lookup, so all four agree that Sonnet 5 is GA at 1M context. (#43)

### Notes

- Anthropic publishes prompt-caching rates as multipliers of the input price: 5-minute writes at 1.25x, 1-hour writes at 2x, and cache hits at 0.10x. Pulse applies those official multipliers to Sonnet 5's introductory input rate and uses the 5-minute write rate for Claude Code JSONL because transcripts do not expose cache TTL. See [docs/sonnet-5.md](docs/sonnet-5.md).
- This environment's `pulse` crate build (Tauri's full dependency tree) needs `cargo --jobs 2` under tight available memory, or the build can hit `STATUS_COMMIT_LIMIT_EXCEEDED` and cascade into unrelated-looking errors in transitive dependencies (`icu_properties`, `idna`). Documented in [tests/index.md](tests/index.md) — environment characteristic, not a code defect.

## [1.3.0] — 2026-06-16

v1.3.0 makes the Codex and Claude Rich Presence accurate again, makes the plan override actually stick, gives the Discord Live Preview the real Rich Presence artwork, and hardens the analytics core. No public API was removed.

### Added

- Discord Live Preview renders the real Rich Presence artwork — the Claude Code mascot and the Codex mark — bundled in-app and mapped by provider/surface, with a Fast-tier (⚡) indicator on the state line. The activity card now mirrors Discord's layout (large image, optional small badge, name, details, state, elapsed). (#32)
- Canonical Claude plan mapping module `cc_discord_presence::plan` (key, name, display name, badge, tolerant override parser), shared by the core library and the Tauri command layer. (#33)
- Coverage for the new behavior: plan round-trip/tolerance, bounded session-scan depth/entry limits, report-trace depth cap, canonical plan-key round-trip in Settings, and a credentials-refresh plan-detection test. (#31, #33)

### Changed

- Cost analysis reports **cost per 1M tokens** instead of per 1K, which rounded to `$0.00` at any realistic usage. (#34)
- Dashboard cache-health shows a neutral `—` instead of a red `F` when there is no token data yet. (#34)
- The duplicated Claude plan key/label/badge mapping is centralized; `config.rs` and the command layer delegate to the canonical module. (#33)
- Claude and Codex session-file walks and the report trace scan are now depth/entry/dir bounded so a pathological tree cannot walk unbounded. (#33)
- Previously-swallowed failures (Discord presence update, config/provider and display-pref saves, usage-cache write/remove, Codex plan-cache save) are logged via `tracing` instead of being discarded. (#33)

### Fixed

- Codex service tier (Fast mode) is read from `~/.codex/config.toml` `service_tier` (where current Codex versions persist it) with the legacy `default-service-tier` global-state key kept as a fallback, so Fast is detected and shown as `⚡ … · Fast`. (#30)
- The manual plan override persists to the config file, reaches the live Discord broadcast, and stays selected instead of snapping back to Auto-detect; the Settings select now uses a canonical plan-key contract. (#31)
- Claude plan auto-detect reads the credentials plan fields fresh from disk, so a plan upgrade (e.g. Max 5x → Max 20x) is reflected without restarting Pulse. (#31)

### Notes

- The live Discord broadcast still requires the Rich Presence images to be uploaded to each Discord application's Developer Portal (`codex-logo` / `codex-app` are not yet uploaded); the in-app Live Preview bundles its own art and is unaffected. Tracked in #36. See [docs/discord-assets.md](docs/discord-assets.md).

## [1.2.0] — 2026-06-10

v1.2.0 is a minor release for Anthropic's Fable/Mythos 5 launch, the Context Window view's stale one-session bias, and Pulse's first in-app release-awareness flow. The release adds new model economics, multi-session UI, and update-check UX without removing any public API.

### Added

- Claude Fable 5 and Claude Mythos 5 support: `claude-fable-5` / `claude-mythos-5`, $10 input / $50 output per MTok, $12.50 5-minute cache writes, $1 cache reads, 1M GA context, and 128K max output metadata.
- Rich Presence labels for the new family: `Fable 5 (1M)` and `Mythos 5 (1M)` render cleanly instead of falling through to raw parenthetical model IDs.
- `get_context_breakdowns(session_ids?: string[] | null)` Tauri command for returning context breakdowns across active sessions.
- Context Window active-session cards: every live session is visible at the top of the view, with utilization, activity, and click-to-select detail routing.
- Frontend API type `SessionContextBreakdown` and Vitest coverage for multi-session Context cards, active Discord preview selection, and Fable/Mythos session badges.
- Backend update-check command `check_app_update()` that compares the packaged Pulse version against the latest stable GitHub Release, plus `open_app_release_page()` with a Pulse-release URL allowlist.
- Global update popup with current/latest version, release title, release notes toggle, Later, Skip version, Open release, 6-hour polling, and a `?fakeUpdate=` development lane.
- Documentation page [docs/fable-5.md](docs/fable-5.md) with official specs, pricing, context window, cache TTL, and validator notes.
- Documentation page [docs/update-checks.md](docs/update-checks.md) covering the release-check flow and why v1.2.0 does not fake a signed auto-installer without updater metadata.
- Documentation page [docs/codex-rich-presence-upstream.md](docs/codex-rich-presence-upstream.md) covering the Codex Rich Presence source-of-truth repo, sync scripts, and CI freshness gate.
- `src/codex/UPSTREAM.json`, `scripts/check-codex-rich-presence-upstream.*`, and `scripts/update-codex-rich-presence.*` so Pulse can prove and refresh its mirrored Codex Rich Presence core from `xt0n1-t3ch/Codex-Discord-Rich-Presence`.
- `tests/codex_upstream_contract.rs` to lock the Pulse-facing boundary around the upstream Codex presence modules.

### Changed

- `get_context_breakdown(session_id?)` is now a compatibility wrapper over `get_context_breakdowns`, keeping older callers stable while centralizing context logic.
- Context fallback selection now prefers active sessions before historical/idle rows; `get_sessions_context_usage()` prepends live active sessions, then dedupes historical results by session id.
- Discord preview chooses the first active session before falling back to historical sessions and shows the active-session count when multiple sessions are live.
- Usage-limit labels now use provider-accurate copy such as `5-hour window` instead of calling Anthropic's usage window a current session.
- Settings now exposes a manual **Check for updates** action that triggers the same global update popup.
- Frontend package metadata now matches the v1.2.0 root, Cargo, Tauri, and lockfile versions.
- Codex-specific Rich Presence code now flows from the standalone [Codex Discord Rich Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence) repository through a source-sync mirror plus a small Pulse compatibility overlay, avoiding private drift while keeping Pulse's Windows resources link-safe.

### Fixed

- Fable/Mythos pricing does not apply beta long-context surcharge across the full 1M window.
- Fable/Mythos sessions are not marked as Opus-tokenizer-inflated and are not treated as fast-capable until Anthropic documents those behaviors.
- Context Window no longer shows only one active session when multiple Claude/Codex sessions are live.
- Per-session Context Window utilization no longer reports impossible values above 100% from lifetime session token totals; history stores the last context snapshot and clamps stale rows.
- Windows session polling no longer launches `wsl.exe` by default. WSL transcript scanning is now explicit via `CC_PRESENCE_INCLUDE_WSL=1`, preventing broken WSL installs from throwing crash dialogs in the background.
- Mirrored Codex Rich Presence git-branch probes now use no-window process spawning on Windows.
- Removed stale Markdown issue templates now superseded by the YAML templates.

## [1.1.0] — 2026-05-28

### Added

- Claude Opus 4.8 support: pricing, 1M-context GA, and inflated-tokenizer detection.
- Fast mode: per-turn `usage.speed` detection, 2x speed-aware pricing, and a fast marker in Sessions, Discord presence, and the HTML report.
- OpenAI Codex GPT-5.5 pricing ($5 / $30 per MTok) and Codex Fast mode (`/fast`) cost: GPT-5.5 bills at 2.5x and GPT-5.4 at 2x the standard rate.
- Per-session Context Window: a session selector, per-session token usage, and tiered compaction hints.
- Single-scan Reports: `get_reports_bundle` loads every analyzer from one JSONL scan.
- Centralized test suite: Vitest unit, integration, and component plus Rust integration; see [tests/index.md](../tests/index.md).
- Docs: [architecture.md](architecture.md), [troubleshooting.md](troubleshooting.md), [opus-4-8.md](opus-4-8.md).

### Changed

- Reports and Insights no longer hangs: analyzer and export commands run off the UI thread via `spawn_blocking`, with per-file and file-count scan guards.
- HTML report rebuilt to render offline (no Google Fonts CDN), match the in-app dark theme, draw smooth bezier charts, and show a fast-vs-standard split.
- Repo conventions follow DLSSync: an artifact `.gitignore`, a root npm script hub, and CI running `clippy -D warnings`.

### Fixed

- Per-category cost breakdown reconciles with the speed-correct total; the statusline total stays authoritative.
- Sessions top-table no longer mutates reactive state during render.
- Codex unknown-model pricing keeps the real model id instead of relabeling it.

## [1.0.0] — 2026-04-18

### Added — Initial public release

- **Pulse desktop app** (Tauri 2 + Svelte 5) — native dashboard for Claude Code.
- **Analytics views** — Dashboard, Sessions, Context, Costs, Reports, Discord, Settings.
- **Cache health grading** — A–F letter grade with trend-weighted hit ratio.
- **Recommendations engine** — rule-based findings with "Copy Fix Prompt" for Claude Code.
- **Inflection detector** — ≥2× cost-per-session deviation alerts.
- **Model routing analyzer** — Opus/Sonnet/Haiku mix + savings estimate.
- **Tool-frequency, prompt-complexity, session-health** analyzers.
- **Discord Rich Presence** — five-tier reasoning (Low/Medium/High/Extra High/Max), multi-tier asset resolver, auto-reconnect.
- **Opus 4.7 support** — inflated-tokenizer detection, 1M context GA pricing.
- **Plan usage** — session/weekly/Sonnet/Extra-usage limits with sound alert on spikes.
- **Local-first** — SQLite at `~/.claude/pulse-analytics.db`, zero telemetry.
- **Tri-OS installers** — Windows (NSIS/MSI), macOS (DMG, arm64 + x64), Linux (deb/rpm/AppImage).

[1.4.2]: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.4.2
[1.4.1]: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.4.1
[1.4.0]: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.4.0
[1.3.0]: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.3.0
[1.2.0]: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.2.0
[1.1.0]: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.1.0
[1.0.0]: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.0.0
