# CLAUDE.md - Project Context for Claude Code

## Project Overview

**cc-discord-presence** is a Discord Rich Presence plugin for Claude Code plus
**Pulse**, a Tauri 2.0 desktop analytics GUI. Pulse displays real-time session
info (project, branch, model, effort level, cost, tokens) on Discord and
provides a full analytics dashboard with cchubber-style cache health grading,
recommendations engine, and "Fix with Claude Code" actions.

**GUI-only since v3.0** — the legacy TUI (`src/ui/*`) was removed in the
`overhaul/opus-4-7-revamp` branch. Users who want a headless daemon run
`cc-discord-presence` with no args; everyone else uses the Pulse window.

## Tech Stack

- **Language**: Rust
- **GUI**: Tauri 2.0 + Svelte 5 + Vite + TypeScript
- **Key dependencies**:
  - `discord-rich-presence` — Discord IPC Rich Presence client
  - `rusqlite` — SQLite for session history + FTS5 BM25 search
  - `serde` / `serde_json`, `chrono`, `anyhow`, `tracing`, `clap`, `ureq`
  - `chart.js` — frontend charts (Vite bundle)

## Project Structure

```
cc-discord-presence/
├── src/                        # Shared Rust daemon crate (headless)
│   ├── main.rs                 # CLI entrypoint (status | doctor | claude | daemon)
│   ├── lib.rs                  # Module declarations
│   ├── app.rs                  # run_daemon(), tick_session_cycle(), claude wrapper
│   ├── session.rs              # JSONL parsing, ReasoningEffort (5 tiers), merge
│   ├── discord.rs              # Rich Presence + multi-tier asset resolver
│   ├── config.rs               # Schema v3, migration, defaults
│   ├── cost.rs                 # Pricing, 1M context GA, has_inflated_tokenizer
│   ├── usage.rs                # Anthropic API usage + Extra Usage toggle cycle
│   ├── sound.rs                # Win32 Beep alert (Extra Usage spike)
│   ├── metrics.rs              # Session metrics tracker
│   ├── util.rs                 # Formatting helpers
│   ├── cli.rs                  # Clap subcommands
│   ├── process_guard.rs        # Single-instance lock
│   └── chrome_session.rs       # Chrome/Edge/Brave cookie decrypt (Windows)
├── src-tauri/                  # Pulse GUI backend (Tauri 2.0)
│   ├── src/main.rs             # Tauri app entry, system tray
│   ├── src/commands.rs         # IPC commands (analytics, Discord user, config)
│   ├── src/db.rs               # SQLite analytics (sessions, daily_stats, budgets)
│   ├── src/report.rs           # HTML + Markdown report generators
│   ├── src/analyzers/          # cchubber-style analysis (native Rust port)
│   │   ├── mod.rs              # Severity enum
│   │   ├── cache_health.rs     # A-F grade, trend-weighted ratio
│   │   ├── inflection.rs       # ≥2x cost/session deviation detector
│   │   ├── model_routing.rs    # Opus/Sonnet/Haiku split + savings estimate
│   │   ├── tool_frequency.rs   # Tool-call mix, MCP share, compact-gap detection
│   │   ├── prompt_complexity.rs# First-prompt complexity + specificity scoring
│   │   ├── session_health.rs   # Session-shape health score beyond cache
│   │   └── recommendations.rs  # 6 rule-based items with fix_prompt
│   ├── Cargo.toml              # Tauri workspace member
│   └── tauri.conf.json         # Window config, tray icon, build settings
├── frontend/                   # Pulse GUI frontend (Svelte 5 + Vite)
│   ├── src/
│   │   ├── App.svelte          # Root layout + route switch
│   │   ├── lib/api.ts          # Tauri invoke wrappers (typed)
│   │   ├── lib/stores.ts       # Reactive stores + polling
│   │   ├── lib/utils.ts        # Formatters
│   │   ├── components/         # Sidebar, TopBar, StatCard, SessionCard,
│   │   │                       # ProgressBar, Chart, Sparkline, Heatmap,
│   │   │                       # Toast, ExportModal
│   │   ├── views/              # Dashboard, Sessions, Context, Costs,
│   │   │                       # Reports (NEW), Discord, Settings
│   │   └── styles/             # global.css (tokens), animations.css
│   ├── package.json
│   └── vite.config.ts
├── scripts/                    # Plugin hook scripts
│   ├── build.sh                # Rust + Tauri build
│   ├── stop.sh / stop.ps1      # Plugin hook: stops daemon
├── docs/                       # Documentation (see docs/index.md)
├── assets/                     # Icons + branding (Discord Rich Presence sources)
└── Cargo.toml                  # Workspace root
```

## Key Concepts

### Discord IPC + Asset Resolver

- Default client ID: `1466664856261230716` (override via `CC_DISCORD_CLIENT_ID`)
- **Asset resolution tiers** (in `src/discord.rs::resolve_image_ref`):
  1. `mp:` prefix → passthrough (already a Discord media reference)
  2. Asset key confirmed in Developer Portal fetch → use key
  3. `https://...` URL → wrap as `mp:external/https/<rest>` for Media Proxy
  4. Fallback → pass key as-is (Discord attempts resolution)
- Default `large_image_key` = `"claude-code"` — must be uploaded to the portal
  at `https://discord.com/developers/applications/1466664856261230716/rich-presence/assets`
- See [`docs/discord-assets.md`](docs/discord-assets.md) for upload steps.

### Session Data Sources (priority order)

1. **Statusline** (`~/.claude/discord-presence-data.json`) — most accurate, uses
   Claude Code's own calculations. Requires statusline wrapper.
2. **JSONL transcripts** (`~/.claude/projects/<encoded>/*.jsonl`) — zero-config,
   cursor-based incremental reads, prompt cache tokens included.

### Model Pricing (`src/cost.rs`)

| Model                 | Input $/1M | Output $/1M | Cache Write | Cache Read |
| --------------------- | ---------- | ----------- | ----------- | ---------- |
| **Opus 4.7 / 4.6 / 4.5** | $5       | $25         | $6.25       | $0.50      |
| Opus 4.0 / 4.1 / 3    | $15        | $75         | $18.75      | $1.50      |
| Sonnet (all)          | $3         | $15         | $3.75       | $0.30      |
| Haiku 4.5+            | $1         | $5          | $1.25       | $0.10      |
| Haiku 3.5             | $0.80      | $4          | $1.00       | $0.08      |
| Haiku 3               | $0.25      | $1.25       | $0.30       | $0.03      |

**Opus 4.7 tokenizer warning**: `has_inflated_tokenizer()` returns true for
`claude-opus-4-7*` — the new tokenizer produces up to ~35% more tokens for the
same input text. Per-token rates are identical to 4.6 but bills inflate.

### 1M Context Pricing

- **GA (Opus 4.6+, Opus 4.7, Sonnet 4.6).** Standard
  per-token pricing across the full 1M context. No 2× / 1.5× premium.
  Source: <https://console.anthropic.com/docs/en/about-claude/pricing>
  (effective 2026-03-13). `is_ga_1m_context()` returns `true` for any of
  these.
- **Beta (Sonnet 4 / 4.5, Opus 4 / 4.5).** Surcharge of 2× input,
  1.5× output, 2× cache when total API input > 200K.
- Model IDs with `[1m]` suffix are stripped before parsing; the suffix is
  treated as an explicit GA indicator emitted by tooling.
- **Plan-level "Extra Usage"** (claude.ai Pro / Max / Teams overage) is
  a separate billing concept — NOT a token-rate multiplier. Tracked
  independently via `src/usage.rs` (reads the Anthropic usage API).

### Reasoning Effort (5 tiers, Opus 4.7)

| Label      | API value | Enum variant         | Notes                          |
| ---------- | --------- | -------------------- | ------------------------------ |
| Low        | `low`     | `ReasoningEffort::Low` | Fastest, cheapest             |
| Medium     | `medium`  | `Medium` (default)   | Balanced                       |
| High       | `high`    | `High`               | Default for Opus 4.7           |
| Extra High | `xhigh`   | `ExtraHigh`          | Opus 4.7+ exclusive            |
| Max        | `max`     | `Max`                | Maximum reasoning depth        |

Parsing accepts aliases: `xhigh`, `x-high`, `extra_high`, `extrahigh`,
`extra high`. `is_high()` returns true for High, ExtraHigh, Max.
`is_ultrathinking()` requires Max + thinking blocks present.

### Pulse GUI Views

| View       | Primary purpose                                                            |
| ---------- | -------------------------------------------------------------------------- |
| Dashboard  | Overview: stats, rate limits, active sessions, top projects               |
| Sessions   | Deep session table with effort column, filters, expandable rows           |
| Context    | Context window breakdown (memory files, skills, MCP tools)                |
| Costs      | Cost analysis, forecasts, budget editor                                   |
| **Reports**| **NEW** — cache grade, recommendations, inflections, model routing, exports |
| Discord    | RP config + live preview + user avatar/banner                             |
| Settings   | Theme, plan override, data management, asset diagnostic                   |

### "Fix with Claude Code" Feature

Each recommendation in the Reports view carries a `fix_prompt` — a
ready-to-paste prompt for the user to run in Claude Code to act on the finding.
Click the button → `copy_fix_prompt(rec_id)` Tauri command returns the prompt
→ frontend copies to clipboard → toast confirms.

Additional backend commands now exposed for frontend wiring:

- `get_tool_frequency(days?)`
- `get_prompt_complexity(days?)`
- `get_session_health(days?)`

### Discord User Auto-Detect (`src-tauri/src/commands.rs`)

`get_discord_user()` searches Discord LevelDB stores across:
- **Windows**: `%APPDATA%\discord|discordcanary|discordptb` +
  `%LOCALAPPDATA%\Discord|DiscordCanary|DiscordPTB`
- **macOS**: `~/Library/Application Support/{discord,Discord,…}`
- **Linux**: `~/.config/discord`, `~/.var/app/com.discordapp.Discord/config`
  (Flatpak), `~/snap/discord/current/.config` (Snap)

Parser returns `DiscordUserInfo` with:
- `avatar_url` — CDN URL (or default if no custom avatar; new `0` discriminator
  uses `(id >> 22) % 6`; legacy uses `discriminator % 5`)
- `banner_url` — CDN URL when user has a banner (PNG or GIF based on `a_` prefix)

### TUI Removal (v3.0)

The `src/ui/` module, `crossterm` + `viuer` deps, and TUI-specific scripts
were removed. `app::run` now routes `SmartForeground` to `run_daemon()` which
prints a one-liner pointing users at the Pulse GUI and loops through
`tick_session_cycle` updating Discord only.

## Development Commands

```bash
# Rust daemon
cargo build --release
cargo run -- status          # print current detection state
cargo run -- doctor          # environment sanity check
cargo test --workspace       # all tests (lib + analyzers)
cargo fmt
cargo clippy --all-targets -- -D warnings

# Pulse GUI (Tauri)
cd frontend && npm install
cd frontend && npm run dev   # Vite dev server on :1420
cd frontend && npm run build # production bundle → dist/
cd src-tauri && cargo run    # Tauri dev (requires frontend dev server or dist/)
cd src-tauri && cargo tauri build  # Distributable bundle (.exe / .dmg / .deb)
```

## Configuration

- Config path: `~/.claude/discord-presence-config.json`
- Schema version: **3** (auto-migrates legacy GitHub-raw-URL configs to
  `"claude-code"` asset key)
- Pulse analytics DB: `~/.claude/pulse-analytics.db`
- Debug log: `~/.claude/cc-discord-presence-debug.log`

## Cache Lifecycles

- **Pulse background poller** (`src-tauri/src/commands.rs`)
  - Runs every **5s** (`REFRESH_INTERVAL`)
  - Refreshes live sessions, rate limits, Discord presence, SQLite session upserts
  - `refresh_usage()` flips a one-shot flag; next poll tick invalidates usage cache
    and deletes `~/.claude/discord-presence-usage-cache.json`
  - **Startup behavior**: `src-tauri/src/main.rs` now calls `refresh_usage()` right
    after `start_background_poller()` so first poll cycle forces fresh usage
- **Usage API cache** (`src/usage.rs`)
  - In-memory TTL: **30s** for first 3 fetches, then **300s**
  - File cache TTL: **300s** via `~/.claude/discord-presence-usage-cache.json`
  - `invalidate_cache()` clears in-memory timestamps only; Pulse also removes file
    cache during forced refresh
- **Session collection caches**
  - Git branch cache: **30s** (`GitBranchCache::new(Duration::from_secs(30))`)
  - Pulse active-session freshness: stale **120s**, sticky **120s**, active cutoff
    **600s**, idle cutoff **300s**
- **Analytics DB**
  - SQLite opened once per process, WAL mode enabled
  - Historical queries now use `COALESCE(started_at, created_at, updated_at)` to
    avoid null-timestamp gaps across views, filters, exports, reports

## Session Timeouts

- **Daemon** (`src/app.rs`): configurable via env; defaults in `config::runtime_settings()`
- **Pulse** (`src-tauri/src/commands.rs`): `REFRESH_INTERVAL=5s`,
  `STALE_THRESHOLD=120s`, `STICKY_WINDOW=120s`, `ACTIVE_CUTOFF=600s`,
  `IDLE_CUTOFF=300s`

## Releasing

1. **Bump version** in `Cargo.toml`, `src-tauri/Cargo.toml`,
   `src-tauri/tauri.conf.json`.
2. **Build**:
   ```bash
   cargo test --workspace
   cd frontend && npm run build
   cd ../src-tauri && cargo tauri build
   ```
3. **Tag + release**:
   ```bash
   git tag vX.Y.Z
   git push origin main --tags
   gh release create vX.Y.Z src-tauri/target/release/bundle/**/* \
     --title "vX.Y.Z" --generate-notes
   ```

## Repository

- **Origin**: https://github.com/xt0n1-t3ch/Claude-Code-Discord-Presence
- **Default branch**: `main`
- **Overhaul branch**: `overhaul/opus-4-7-revamp`
