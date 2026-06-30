# Claude Sonnet 5 — native support and introductory pricing

Pulse v1.4.0 adds first-class handling for Claude Sonnet 5, including the time-boxed
introductory pricing Anthropic launched it with.

## Official model specs

| Field | Value |
|:---|:---|
| API ID | `claude-sonnet-5` |
| Context window | 1,000,000 tokens (GA — no long-context surcharge at any length) |
| Max output | 128,000 tokens |
| Introductory pricing | $2.00 input / $10.00 output per MTok, through August 31, 2026 |
| Standard pricing | $3.00 input / $15.00 output per MTok, from September 1, 2026 |

Source: <https://www.anthropic.com/news/claude-sonnet-5>.

## Introductory pricing — the intelligent system

`src/cost.rs` models the promotional window as a generic, reusable concept rather than a
one-off `if` for this single model, so the next time-boxed launch is a new registry entry,
not new branching logic:

- `model_pricing_at(model_id, now)` is the real source of truth — a pure function of an
  injected clock. `model_pricing(model_id)` is a thin wrapper calling it with the real wall
  clock (`Utc::now()`), preserving the existing public signature every call site already uses.
- The cutoff is modeled as the **exclusive** UTC instant `2026-09-01T00:00:00Z`. Anthropic
  published a calendar date ("through August 31, 2026"), not a time or timezone, so Pulse
  treats the promotion as active for the entirety of August 31 and reverts at the first
  instant of September 1, UTC. This is a local cost-estimate tool, not Anthropic's billing
  system, so exact alignment to Anthropic's internal cutoff instant is unknowable; UTC
  midnight is the defensible, documented default.
- `active_intro_pricing(model_id, now)` returns the active promo (both rate sets plus the
  exact end instant) only while genuinely inside the window for that model, and `None` both
  for models with no promo and for a promo'd model once its window has closed. Callers never
  run their own expiry check — including the frontend, which renders the badge purely off
  whatever this function reports and never computes a date itself.
- `src-tauri/src/commands.rs`'s `SessionInfo.intro_pricing` carries this per session
  (`Option<cost::IntroPricingBadge>`), refreshed on every poll cycle from the real clock, so
  the badge disappears automatically the moment the real-world date crosses the cutoff — no
  manual flag, no redeploy.

## Cache pricing (derived, not separately published)

Anthropic did not publish a standalone cache-write/cache-read rate for Sonnet 5. Pulse
applies the same universal cache-pricing multiplier already used for every other model in
this file (5-minute cache write = 1.25x base input, cache read = 0.10x base input):

| Period | Input | Output | Cache Write (5m) | Cache Read |
|:---|---:|---:|---:|---:|
| Introductory (through Aug 31, 2026) | $2.00 | $10.00 | $2.50 | $0.20 |
| Standard (from Sep 1, 2026) | $3.00 | $15.00 | $3.75 | $0.30 |

The standard-period figures are identical to the flat Sonnet rate Pulse already used before
this release.

## Inflated tokenizer (permanent, independent of the promo)

Per Anthropic's own Sonnet 5 launch post (footnote 2): Sonnet 5 ships an updated tokenizer —
"similar to the tokenizer change we introduced with Claude Opus 4.7" — that maps the same
input to roughly 1.0–1.35x as many tokens depending on content type. The introductory price
is set so the transition is *roughly* cost-neutral, but **after the promo ends, the nominal
rate returns to the unchanged $3/$15 while the tokenizer inflation remains** — real bills for
equivalent work are higher than they were on Sonnet 4.6, even though the headline per-token
rate looks the same.

`cost::has_inflated_tokenizer()` now returns `true` for Sonnet 5 (previously Opus 4.7+ only),
unconditionally — this warning does not expire with the promo. The Sessions/Dashboard `⚠`
marker tooltip was generalized from "Opus 4.7+" wording to read for any model family.

## 1M context bug fixed for this id shape

Like Claude Fable 5 / Mythos 5, Sonnet 5's id (`claude-sonnet-5`) is a single version segment
("5"), not the two-segment major-minor shape (`"4-6"`) the generic Sonnet/Opus version parser
expects. Before this release, `cost::is_ga_1m_context("claude-sonnet-5")` silently returned
`false`, which would have applied the beta long-context 2x/1.5x surcharge above 200K tokens —
incorrect, since Sonnet 5 is GA at 1M context like its predecessor. A dedicated
`is_sonnet_5_class()` classifier (mirroring `is_mythos_class()`) now short-circuits
`is_ga_1m_context`, `supports_1m_context`, `has_inflated_tokenizer`, and the pricing lookup
itself, so all four agree.

`is_sonnet_5_class()` is digit-boundary-safe: `"claude-sonnet-5"`, dated ids
(`"claude-sonnet-5-20260625"`), and `[1m]`-suffixed ids all match; a hypothetical future
`"claude-sonnet-50"` does not collide, since the character immediately after `"sonnet-5"` must
be the end of the string, `-`, or `[` — never a digit.

## Not affected

- `cost::is_fast_capable()` stays `false` for Sonnet 5 — fast mode (priority speed) launched
  with Opus 4.8 and remains Opus-only.
- `cost::model_display_name()` already rendered `"Claude Sonnet 5"` correctly before this
  release; its generic family-then-version-segments parser handles the single-segment id with
  no code change.
- Discord Rich Presence text is unchanged — no promo marker is added to the details/state/
  tooltip lines (character-budget and clutter concerns). The Pulse GUI badge is the surface
  for this; see below.

## UI surfaces

The Sessions and Dashboard live-session cards (`SessionCard.svelte`, shared by both views)
render an "Intro Pricing" badge next to the model badge whenever `session.intro_pricing` is
present, with a tooltip showing the discounted rate, the human-readable end date (the last
inclusive day, "Aug 31, 2026" — not the exclusive UTC cutoff instant the backend stores), and
the rate it reverts to. The badge and its contents are sourced entirely from the backend; the
frontend performs zero date arithmetic or price hardcoding for this feature.

## Sources

- Launch note: <https://www.anthropic.com/news/claude-sonnet-5>
- Anthropic pricing: <https://platform.claude.com/docs/en/about-claude/pricing>
- System card: <https://www.anthropic.com/claude-sonnet-5-system-card>
