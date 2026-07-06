# Pulse & cc-discord-presence — Documentation

Pulse is the Tauri 2.0 analytics GUI for Claude Code and OpenAI Codex, paired with the `cc-discord-presence` daemon that pushes Rich Presence to Discord.

## Table of contents

| Doc | Purpose |
| --- | --- |
| [architecture.md](architecture.md) | High-level component map: daemon -> Tauri -> SQLite -> Svelte |
| [discord-assets.md](discord-assets.md) | Upload assets to the Developer Portal so the RP logo actually renders; in-app preview art |
| [plan-detection.md](plan-detection.md) | Claude/Codex plan detection, manual override persistence, Codex service tier + surface |
| [fable-5.md](fable-5.md) | Claude Fable 5 + Mythos 5 pricing, 1M context, cache TTL note, Rich Presence labels |
| [sonnet-5.md](sonnet-5.md) | Claude Sonnet 5 native support: time-boxed introductory pricing (the date-driven badge system), official cache multipliers, inflated-tokenizer warning, the 1M-context bug it fixed |
| [context-tracking.md](context-tracking.md) | Current context fill vs. all-time peak: why they're separate fields, the compaction-boundary bug this fixed, and the Dashboard-vs-Costs aggregation-scope question |
| [opus-4-7-variants.md](opus-4-7-variants.md) | Reasoning-effort tiers (Low / Medium / High / Extra High / Max) + tokenizer note |
| [opus-4-8.md](opus-4-8.md) | Opus 4.8 — fast mode (priority speed) + billing impact |
| [analyzers.md](analyzers.md) | How the cchubber-style analyzers work + how to add new recommendations |
| [cost-calculation.md](cost-calculation.md) | Pricing tiers, cache math, 1M-context GA/beta handling + fast-mode rules |
| [codex-rich-presence-upstream.md](codex-rich-presence-upstream.md) | Codex Rich Presence source-of-truth repo, sync scripts, CI freshness gate, compatibility overlay |
| [update-checks.md](update-checks.md) | Backend GitHub Release checks, popup behavior, skip controls, signed-updater note |
| [troubleshooting.md](troubleshooting.md) | Diagnostics: doctor, RUST_LOG, data sources, common failures + fixes |

## v1.5.1 docs refresh

- Kept the mirrored Codex runtime aligned with standalone Codex Discord Rich Presence v1.7.1 WSL behavior: Windows WSL transcript roots are opt-in and use hidden subprocess launchers when enabled.
- Added sync-script safeguards so future Codex mirror pulls keep `wsl.exe` behind `CC_PRESENCE_INCLUDE_WSL=1` / `CODEX_PRESENCE_INCLUDE_WSL=1` instead of reintroducing visible console windows.
- Added an upstream contract test covering hidden WSL launchers and native-default session discovery.
- Kept the release as a patch bump: v1.5.1 ships a Windows runtime safety fix and Codex mirror freshness update without changing the public app contract.

## v1.4.2 docs refresh

- Finished the Discord Rich Presence settings contract: the Live Preview and broadcaster now read the same backend payload, and the Git branch toggle persists through the Tauri IPC argument names the backend actually receives.
- Removed the stale plain-thinking workflow label from user-facing systems copy. The Systems field is reserved for safe, generic signals such as `ULTRACODE`, `Tool active`, and `1 agent`.
- Corrected the Sonnet 5 release copy to use Anthropic's published introductory cache prices: $2.50 / MTok for 5-minute writes, $4.00 / MTok for 1-hour writes, and $0.20 / MTok for cache reads.
- Kept v1.4.1 immutable after publication; v1.4.2 is the patch release for docs, release metadata, and final Discord contract cleanup.

## v1.4.1 docs refresh

- Added [context-tracking.md](context-tracking.md): the `max_turn_api_input` lifetime peak vs. `current_context_tokens` point-in-time fill split, the compaction-boundary parsing that makes current fill correct, and why Dashboard's and Costs' cost totals differ by aggregation scope.
- Fixed a live-confirmed bug where every UI surface claiming to show current context fill read a monotonically increasing all-time peak field that never decreased across compactions.
- Dashboard's "Total Cost" and Costs' "Total Spent" KPI tiles now carry explicit scope labels: "(Live)" and "(30d)".
- Kept the release a patch bump: v1.4.1 is a correctness fix with no public API removed.

## v1.4.0 docs refresh

- Added [sonnet-5.md](sonnet-5.md): Claude Sonnet 5 native support, including the generic introductory-pricing mechanism (`cost::active_intro_pricing`, clock-injected `cost::model_pricing_at`) that automatically reverts to standard pricing once the August 31, 2026 window closes.
- Fixed a pre-existing bug where `cost::is_ga_1m_context("claude-sonnet-5")` returned `false` due to the id's single version segment not fitting the generic two-segment Sonnet/Opus parser.
- Extended `cost::has_inflated_tokenizer()` to Sonnet 5 and generalized the Sessions/Dashboard tokenizer-warning tooltip.
- Sessions and Dashboard live-session cards show an "Intro Pricing" badge sourced entirely from backend `SessionInfo.intro_pricing`.
- Kept the release a minor SemVer bump: v1.4.0 adds model support, a pricing-correctness fix, and a UI capability without removing public API.

## v1.3.0 docs refresh

- Added [plan-detection.md](plan-detection.md): Claude/Codex plan resolution, the canonical plan-key contract behind the Settings override, where the manual override is persisted, and fresh-from-disk auto-detect.
- Documented the Codex service-tier source moving to `~/.codex/config.toml` `service_tier`, with the legacy global-state key kept as a fallback.
- Expanded [discord-assets.md](discord-assets.md) with the two Codex Discord applications, their `codex-logo` / `codex-app` uploads, and the in-app Live Preview art that bundles real Rich Presence images locally.
- Kept the release as a minor SemVer bump: v1.3.0 adds the faithful preview plus canonical plan mapping and fixes detection/override without removing public API.

## v1.2.0 docs refresh

- Added Claude Fable 5 and Claude Mythos 5 support notes: 1M context by default, 128k max output, $10 / $50 MTok input/output, 5-minute and 1-hour cache-write rates.
- Documented that runtime JSONL cost math models 5-minute cache writes because Claude Code transcripts do not expose cache TTL.
- Updated Context Window coverage for the new multi-session top-card flow.
- Added Pulse release-check coverage and Codex Rich Presence upstream-sync coverage.
- Kept the release lane as a minor SemVer bump because v1.2.0 adds model support and a UI capability without removing public API.

## Quick links

- **Install**: see [README](../README.md#install)
- **Architecture** (full component map): [architecture.md](architecture.md)
- **Contributing + local dev**: [../CONTRIBUTING.md](../CONTRIBUTING.md)
- **Test suite**: [../tests/index.md](../tests/index.md)
- **Bug / feature requests**: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/issues

## Version

- App release: **v1.5.1**
- Schema: **Claude config v5 / Codex config v9** (DB remains v3)
- Last docs refresh: 2026-07-06 (Codex WSL opt-in safety + upstream sync contract)
- Windows WSL transcript roots are opt-in with `CC_PRESENCE_INCLUDE_WSL=1`; default Windows polling stays native and does not spawn `wsl.exe`.
