# Changelog

All notable changes to **Pulse** are documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). Versioning is [SemVer](https://semver.org/).

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

[1.0.0]: https://github.com/xt0n1-t3ch/Pulse/releases/tag/v1.0.0
