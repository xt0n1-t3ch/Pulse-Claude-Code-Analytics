# Plan detection, manual override, and Codex service tier

How Pulse decides which subscription plan and Codex service tier to show, how the
manual override in **Settings** behaves, and where the signals come from.

## Claude plan

Resolved in `get_plan_info` (Tauri command) and the background poller:

1. **Manual override** — if `PresenceConfig.plan` (in
   `~/.claude/discord-presence-config.json`) is set, it wins and is reported as
   `Manual`. The config file is the single source of truth: `get_plan_info`, the
   Settings select, and the live broadcast all read it, so a manual choice
   persists across restarts and reaches Discord.
2. **Auto-detect** — otherwise the plan is detected from the Claude credentials
   file (`subscriptionType` / `rateLimitTier`), read **fresh from disk** on each
   detection so an upgrade (e.g. Max 5x → Max 20x) is reflected without
   restarting Pulse.

### Canonical plan keys

The Settings select emits and receives canonical keys, not display labels, so the
round-trip is exact and the control never snaps back to Auto-detect:

`auto · free · pro · max_5x · max_20x · max · team · enterprise`

The key ⇄ name ⇄ display-name ⇄ badge mapping lives in one place,
`cc_discord_presence::plan` (`src/plan.rs`); `config.rs` and the Tauri command
layer delegate to it. `key_from_override` is tolerant of any label form
(`"Max 20x ($200/mo)"` → `max_20x`, `"auto"`/empty → none).

## Codex plan

Detected from the Codex session rate-limit envelopes (`PlanDetector`), or set
manually via the Codex plan override (persisted to the Codex presence config).
Canonical keys: `free · go · plus · business · enterprise · pro`.

## Codex service tier (Fast mode)

`ServiceTier` is resolved in `src/codex/telemetry/service_tier.rs`:

1. **Primary** — `service_tier` in `~/.codex/config.toml` (where current Codex
   versions persist the active tier). Parsed with a bounded root-key TOML read,
   no extra dependency.
2. **Fallback** — the legacy `default-service-tier` key in
   `~/.codex/.codex-global-state.json`, kept for older Codex App builds.

The freshest source by file mtime wins. `fast` and `priority` (OpenAI's
expedited tier) map to **Fast**; everything else (`default`, `flex`, `auto`,
`standard`) maps to **Standard**. Fast renders on the model line as
`⚡ <model> · Fast`.

## Surface (Codex App vs CLI)

Detected from the session `originator`: values containing `desktop` or
`opencode` resolve to the **Codex App** surface (client `1478395304624652345`,
asset `codex-app`); everything else resolves to **Codex CLI / VS Code** (client
`1470480085453770854`, asset `codex-logo`). See
[discord-assets.md](discord-assets.md) for the asset uploads.
