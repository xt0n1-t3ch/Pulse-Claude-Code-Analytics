# Claude Opus 4.8 — pricing, fast mode, and reasoning effort

Pulse models Claude Opus 4.8 (launched 2026-05-28). This supersedes
[opus-4-7-variants.md](opus-4-7-variants.md) for the 4.8 specifics; the reasoning-effort
tier table there still applies unchanged.

## Model id and context

- API id: `claude-opus-4-8`. It is a **1M-context model by default** — there is no separate
  `[1m]`-suffixed alias. Pulse still strips a `[1m]` suffix before any lookup
  (`cost::strip_context_suffix`), so a suffixed id prices identically.
- 1M context is **GA** (no long-context surcharge at any length). `cost::is_ga_1m_context`
  gates this at Opus `>= 4.6`, so 4.8 is GA automatically.

## Standard pricing (per million tokens)

Identical to Opus 4.7 — no per-token change at launch:

| Category | Rate |
|----------|------|
| Input | $5.00 |
| Output | $25.00 |
| Cache write (5-minute) | $6.25 |
| Cache write (1-hour) | $10.00 |
| Cache read | $0.50 |

`cost::is_new_opus` (major 4, minor ≥ 5) routes 4.8 to these rates with no dedicated branch.
The 4.7 tokenizer carries forward (`cost::has_inflated_tokenizer`, Opus `>= 4.7`): the same
fixed text can produce up to ~35% more tokens than 4.6, so a bill can grow with no change in
prompting style.

## Fast mode

Opus 4.8 adds a **fast mode** (research preview): up to ~2.5x faster, billed at **2x the
standard rate for every token category** (input $10 / output $50 / cache scaled 2x). In
Claude Code it is the `/fast` toggle (CLI only, not the VS Code extension) and is the
fast-mode default on Opus 4.8 in CC 2.1.154+.

### How Pulse detects + prices it

- Detection is **per turn**: each JSONL assistant message's `usage.speed` is `"fast"` or
  `"standard"` (absent → standard). Parsed in `session.rs` into `Speed { Standard, Fast }`.
- Pricing applies a single centralized constant `cost::FAST_RATE_MULTIPLIER = 2.0` via
  `cost::speed_multiplier(model_id, fast)`, and only when the model is fast-capable
  (`cost::is_fast_capable`, Opus `>= 4.8`).
- Cost is **accumulated per turn**, so a session that falls back from fast to standard
  mid-run (e.g. on a rate-limit hit) prices each turn at its own speed. The four per-category
  costs are accumulated per turn too, so the Cost-by-Type breakdown sums to the headline total.
- When the Claude Code statusline sidecar is present, its `total_cost_usd` is authoritative
  (it already reflects fast mode); Pulse scales the JSONL-derived category proportions to that
  total so the breakdown stays reconciled.
- The UI marks fast usage with a ⚡ badge (Sessions + Discord rich presence).

## Service tier (priority) — orthogonal to speed

`usage.service_tier` (`"priority"` | `"standard"`) is a **separate** field from `usage.speed`.
Pulse tracks it for display only; it does not change the priced rate. Priority Tier and fast
mode are not compatible.

## Sources

- https://www.anthropic.com/news/claude-opus-4-8
- https://platform.claude.com/docs/en/about-claude/pricing
- https://platform.claude.com/docs/en/build-with-claude/fast-mode
- https://code.claude.com/docs/en/fast-mode
- https://platform.claude.com/docs/en/api/service-tiers
