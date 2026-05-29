# Changelog

All notable changes to **Pulse** are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning is [SemVer](https://semver.org/).

## [1.1.0] — 2026-05-28

### Added

- Claude Opus 4.8 support: pricing, 1M-context GA, and inflated-tokenizer detection.
- Fast mode: per-turn `usage.speed` detection, 2x speed-aware pricing, and a fast marker in Sessions, Discord presence, and the HTML report.
- OpenAI Codex GPT-5.5 pricing ($5 / $30 per Mtok) and Codex Fast mode (`/fast`) cost: GPT-5.5 bills at 2.5x and GPT-5.4 at 2x the standard rate.
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

[1.1.0]: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.1.0
[1.0.0]: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/v1.0.0
