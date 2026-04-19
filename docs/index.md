# Pulse & cc-discord-presence — Documentation

Pulse is the Tauri 2.0 analytics GUI for Claude Code, paired with the
`cc-discord-presence` daemon that pushes Rich Presence to your Discord profile.

## Table of contents

| Doc | Purpose |
| --- | --- |
| [architecture.md](architecture.md) | High-level component map: daemon → Tauri → SQLite → Svelte |
| [discord-assets.md](discord-assets.md) | Upload assets to the Developer Portal so the RP logo actually renders |
| [opus-4-7-variants.md](opus-4-7-variants.md) | Reasoning-effort tiers (Low / Medium / High / Extra High / Max) + tokenizer note |
| [analyzers.md](analyzers.md) | How the cchubber-style analyzers work + how to add new recommendations |
| [cost-calculation.md](cost-calculation.md) | Pricing tiers, cache math, 1M-context surcharge rules |

## Quick links

- **Install**: see [README](../README.md#installation)
- **Main CLAUDE.md** (full project context): [../CLAUDE.md](../CLAUDE.md)
- **Bug / feature requests**: https://github.com/xt0n1-t3ch/Claude-Code-Discord-Presence/issues

## Version

- Schema: **v3** (config + DB)
- Last docs refresh: 2026-04-17 (Opus 4.7 + cchubber-analyzer overhaul)
