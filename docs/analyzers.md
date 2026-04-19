# Analyzers

The `src-tauri/src/analyzers/` module is a native Rust port of the
[cchubber](https://github.com/azkhh/cchubber) Node.js CLI (MIT licensed).
Everything runs in-process from the stored SQLite session history — no
Node subprocess, no fresh JSONL re-parses.

## Module layout

| File                 | Responsibility                                                        |
| -------------------- | --------------------------------------------------------------------- |
| `mod.rs`             | Shared `Severity` enum (critical/warning/info/positive) + color hints |
| `cache_health.rs`    | A-F grade based on prompt cache hit ratio (trend-weighted)            |
| `inflection.rs`      | Detects day-over-day cost/session multipliers ≥ 2× or ≤ 0.5×          |
| `model_routing.rs`   | Opus / Sonnet / Haiku split + `estimated_savings_if_rerouted`         |
| `recommendations.rs` | Rule-based engine that consumes the three reports above               |

## Grade thresholds

| Grade | Cache hit ratio | Label      | Color     |
| ----- | --------------- | ---------- | --------- |
| A     | 80 – 100%       | Excellent  | `#57F287` |
| B     | 65 – 79%        | Healthy    | `#A8D08D` |
| C     | 50 – 64%        | Fair       | `#F5A524` |
| D     | 30 – 49%        | Poor       | `#E87638` |
| F     | 0 – 29%         | Broken     | `#ED4245` |

The overall grade uses the **trend-weighted** ratio: sessions started in the
last 7 days count 2× so recent improvements lift the grade quickly.

## Recommendation contract

Every recommendation is a `Recommendation { id, severity, title, description,
estimated_savings, action, fix_prompt, color }`. The `fix_prompt` is the
ready-to-paste Claude Code prompt behind the UI's "Fix with Claude Code" button
— keep it concrete (file paths, metrics, the actual ask) so pasting it into CC
produces useful output without extra context from the user.

Built-in rules (order matters — highest-severity first in the resulting list):

1. **`cache-hit-low`** — weighted ratio < 50% and > 3 sessions analyzed.
   Critical below 30%, Warning otherwise.
2. **`cache-healthy`** — weighted ratio ≥ 70% and > 5 sessions. Positive.
3. **`opus-dominance`** — Opus cost share ≥ 90% and total cost > $10. Warning.
4. **`inflection-spike-<date>`** — most recent 2× cost/session spike. Warning.
5. **`long-sessions`** — ≥ 5 sessions over 2 hours. Info.
6. **`high-cost-sessions`** — any sessions > $20. Info.

When no rules fire, the engine emits a single `all-good` Positive card.

## Adding a new recommendation

1. Compute the finding inside `generate()` in `recommendations.rs` (you already
   have `ctx.sessions`, `ctx.cache`, `ctx.routing`, `ctx.inflections`).
2. Push a new `Recommendation` with a unique `id` (used by `copy_fix_prompt`),
   a severity, and a `fix_prompt` that a Claude Code session could act on cold.
3. Sort-by-severity is done in the frontend (`Reports.svelte`) — you don't need
   to maintain order in Rust.

## Upstream attribution

Core analysis ideas and thresholds trace back to cchubber
(https://github.com/azkhh/cchubber, MIT). This module is a **reimplementation**
in Rust against our existing SQLite schema — no cchubber code or packaging is
shipped.
