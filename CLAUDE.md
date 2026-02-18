# CLAUDE.md - Project Context for Claude Code

## Project Overview

**cc-discord-presence** is a Discord Rich Presence plugin for Claude Code. It displays real-time session information on Discord, including project name, git branch, model, session duration, token usage (with prompt cache), and cost. Features an adaptive terminal dashboard with colored usage bars.

## Tech Stack

- **Language**: Rust 2024 edition
- **Key Dependencies**:
  - `discord-rich-presence` - Discord IPC Rich Presence client
  - `crossterm` - Cross-platform terminal rendering
  - `serde` / `serde_json` - JSON serialization
  - `chrono` - DateTime handling
  - `anyhow` - Error handling
  - `tracing` / `tracing-subscriber` - Structured logging
  - `clap` - CLI argument parsing

## Project Structure

```
cc-discord-presence/
├── src/
│   ├── main.rs           # Entry point, explorer detection, panic handler
│   ├── lib.rs            # Module declarations
│   ├── app.rs            # Main application loop, session merging, Discord updates
│   ├── session.rs        # JSONL parsing, session accumulation, statusline reading
│   ├── discord.rs        # Discord presence formatting, activity mapping
│   ├── ui.rs             # Adaptive TUI (Full/Compact/Minimal), colored bars
│   ├── config.rs         # Config loading, migration, schema validation
│   ├── cost.rs           # Model pricing (tiered per version), display names
│   ├── usage.rs          # API usage/rate limit tracking, Extra Usage toggle
│   ├── sound.rs          # Sound alerts via Win32 Beep FFI (no extra deps)
│   ├── metrics.rs        # Session metrics tracker (totals, per-model costs)
│   ├── util.rs           # Formatting helpers (tokens, cost, duration, truncate)
│   ├── cli.rs            # CLI subcommands (status, doctor)
│   └── process_guard.rs  # Single-instance lock, PID management
├── scripts/
│   ├── build.sh          # Cross-compile binaries
│   ├── start.sh / start.ps1   # Plugin hook: starts daemon
│   ├── stop.sh / stop.ps1     # Plugin hook: stops daemon
│   ├── statusline-wrapper.sh  # Wrapper script (copied to ~/.claude/)
│   └── setup-statusline.sh    # One-time statusline setup
├── .claude-plugin/
│   └── plugin.json       # Plugin manifest with SessionStart/SessionEnd hooks
├── Cargo.toml
└── Cargo.lock
```

## Key Concepts

### Discord IPC Protocol

- Uses `discord-rich-presence` crate for cross-platform IPC
- Frame format: `[opcode:4 LE][length:4 LE][JSON payload]`
- Opcodes: 0 = Handshake, 1 = Frame

### Session Data Sources (Priority Order)

1. **Statusline Data** (`~/.claude/discord-presence-data.json`)
   - Most accurate - uses Claude Code's own calculations
   - Requires user to configure statusline wrapper

2. **JSONL Parsing** (`~/.claude/projects/<encoded-path>/*.jsonl`)
   - Zero configuration needed
   - Parses session transcript files with cursor-based incremental reads
   - Includes prompt cache tokens (`cache_creation_input_tokens`, `cache_read_input_tokens`)
   - Cache-aware cost: write = 1.25x input, read = 0.10x input

### Model Display Names

- Extracted dynamically from model ID strings (handles both dated and short IDs)
- `"claude-opus-4-6-20260213"` or `"claude-opus-4-6"` → "Claude Opus 4.6"
- Located in `src/cost.rs` → `model_display_name()`

### Model Pricing (Update when new models release)

Located in `src/cost.rs` → `model_pricing()`. Pattern-based matching with version-aware tiers.

| Model              | Input $/1M | Output $/1M | Cache Write $/1M | Cache Read $/1M |
| ------------------ | ---------- | ----------- | ---------------- | --------------- |
| Opus 4.5 / 4.6     | $5         | $25         | $6.25            | $0.50           |
| Opus 4.0 / 3 (old) | $15        | $75         | $18.75           | $1.50           |
| Sonnet (all)       | $3         | $15         | $3.75            | $0.30           |
| Haiku 4.5+         | $1         | $5          | $1.25            | $0.10           |
| Haiku 3.5          | $0.80      | $4          | $1.00            | $0.08           |
| Haiku 3            | $0.25      | $1.25       | $0.30            | $0.03           |

### TUI Layout Modes

- **Full** (100x30+): ASCII banner, all sections, branch/path
- **Compact** (60x18+): Smaller banner, essential sections
- **Minimal** (<60x18): Single-line status

### Colored Usage Bars

- Usage (how much used): Green ≤40%, Yellow ≤70%, Red >70%
- Remaining (how much left): Green ≥60%, Yellow ≥30%, Red <30%

### Extra Usage Sound Alert

When the Extra Usage `used_credits` value changes (any new charge detected):
1. Plays a C5→E5→G5→C6 arpeggio via Win32 `Beep()` (no extra deps, Windows only, no-op elsewhere)
2. Shows a yellow `Alert : ! charge detected — toggling off/on` line in the TUI for ~5 seconds
3. Spawns a background thread that: disables Extra Usage → waits 3s → re-enables it

The toggle endpoint is `EXTRA_USAGE_TOGGLE_URL` in `src/usage.rs` (inferred from usage API pattern).
If it doesn't work: open Chrome DevTools on claude.ai, click the toggle, copy the network request URL.

## Development Commands

```bash
cargo build --release          # Build optimized binary
cargo run                      # Run directly
cargo test                     # Run all tests
RUST_LOG=debug cargo run       # Run with debug logging
```

## Configuration

- Default Client ID: `1455326944060248250` (shared "Claude Code" Discord app)
- Config file: `~/.claude/discord-presence-config.json`
- Schema version: 3 (auto-migrated)

## Important Notes

- Polls every 3 seconds by default
- Discord must be running for RPC to connect
- Graceful shutdown on Ctrl+C / SIGTERM
- Explorer/conhost detection prevents TUI in non-interactive contexts
- Debug log: `~/.claude/cc-discord-presence-debug.log`

## Releasing

1. **Update version** in:
   - `scripts/start.sh` - `VERSION="vX.X.X"`
   - `scripts/start.ps1` - `$Version = "vX.X.X"`
   - `.claude-plugin/plugin.json` - `"version": "X.X.X"` (no 'v' prefix)
   - `Cargo.toml` - `version = "X.X.X"`

2. **Build**: `cargo build --release`

3. **Commit, tag, release**:
   ```bash
   git tag vX.X.X
   git push origin main --tags
   gh release create vX.X.X releases/windows/cc-discord-presence.exe --title "vX.X.X" --generate-notes
   ```
