# Pulse & cc-discord-presence — Documentation

Pulse is the Tauri 2.0 analytics GUI for Claude Code and OpenAI Codex, paired with the `cc-discord-presence` daemon that pushes Rich Presence to Discord.

## Table of contents

| Doc | Purpose |
| --- | --- |
| [architecture.md](architecture.md) | High-level component map: daemon -> Tauri -> SQLite -> Svelte |
| [discord-assets.md](discord-assets.md) | Upload assets to the Developer Portal so the RP logo actually renders |
| [fable-5.md](fable-5.md) | Claude Fable 5 + Mythos 5 pricing, 1M context, cache TTL note, Rich Presence labels |
| [opus-4-7-variants.md](opus-4-7-variants.md) | Reasoning-effort tiers (Low / Medium / High / Extra High / Max) + tokenizer note |
| [opus-4-8.md](opus-4-8.md) | Opus 4.8 — fast mode (priority speed) + billing impact |
| [analyzers.md](analyzers.md) | How the cchubber-style analyzers work + how to add new recommendations |
| [cost-calculation.md](cost-calculation.md) | Pricing tiers, cache math, 1M-context GA/beta handling + fast-mode rules |
| [codex-rich-presence-upstream.md](codex-rich-presence-upstream.md) | Codex Rich Presence source-of-truth repo, sync scripts, CI freshness gate, compatibility overlay |
| [update-checks.md](update-checks.md) | Backend GitHub Release checks, popup behavior, skip controls, signed-updater note |
| [troubleshooting.md](troubleshooting.md) | Diagnostics: doctor, RUST_LOG, data sources, common failures + fixes |

## v1.2.0 docs refresh

- Added Claude Fable 5 and Claude Mythos 5 support notes: 1M context by default, 128k max output, $10 / $50 MTok input/output, 5-minute and 1-hour cache-write rates.
- Documented that runtime JSONL cost math models 5-minute cache writes because Claude Code transcripts do not expose cache TTL.
- Updated Context Window coverage for the new multi-session top-card flow: all active sessions are visible at once, with a selected card driving the detailed breakdown; per-session history reflects bounded context snapshots, not lifetime tokens.
- Added Pulse release-check coverage: backend latest-release comparison, global update popup, Settings manual check, skip controls, and the signed-updater limitation.
- Added Codex Rich Presence upstream-sync coverage so Pulse stays aligned with the standalone Codex presence repo.
- Kept the release lane as a minor SemVer bump because v1.2.0 adds model support and a UI capability without removing public API.

## Quick links

- **Install**: see [README](../README.md#install)
- **Architecture** (full component map): [architecture.md](architecture.md)
- **Contributing + local dev**: [../CONTRIBUTING.md](../CONTRIBUTING.md)
- **Test suite**: [../tests/index.md](../tests/index.md)
- **Bug / feature requests**: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/issues

## Version

- App release: **v1.2.0**
- Schema: **v3** (config + DB)
- Last docs refresh: 2026-06-10 (Fable/Mythos 5 + multi-session Context Window + update checks + Codex Rich Presence upstream sync + Windows-safe WSL opt-in)
- Windows WSL transcript roots are opt-in with `CC_PRESENCE_INCLUDE_WSL=1`; default Windows polling stays native and does not spawn `wsl.exe`.
