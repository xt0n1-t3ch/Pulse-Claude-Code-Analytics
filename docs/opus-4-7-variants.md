# Opus 4.7 reasoning-effort variants

Claude Opus 4.7 exposes a 5-tier `effort` parameter in the Anthropic API. Pulse
detects the active effort level from JSONL transcripts and renders it in the
Sessions table, the Dashboard live cards, and the Discord Rich Presence state
line.

## The five tiers

| UI label    | API value (`effort`) | Enum (`ReasoningEffort`) | Available on                 | Notes                             |
| ----------- | -------------------- | ------------------------ | ---------------------------- | --------------------------------- |
| Low         | `low`                | `Low`                    | Opus 4.5+, Sonnet (partial)  | Fastest, cheapest                 |
| Medium      | `medium`             | `Medium` (default)       | Opus 4.5+, Sonnet            | Balanced                          |
| High        | `high`               | `High`                   | Opus 4.5+, Sonnet (partial)  | **Default for Opus 4.7**          |
| Extra High  | `xhigh`              | `ExtraHigh`              | **Opus 4.7+ exclusive**       | For agentic / deep-reasoning work |
| Max         | `max`                | `Max`                    | Opus 4.5+                     | Highest depth; enables Ultrathink |

## Aliases accepted by `ReasoningEffort::from_api`

- `xhigh` · `x-high` · `extra_high` · `extrahigh` · `extra high`
- `maximum` for Max, `med` for Medium
- Case-insensitive

## JSONL numeric bucket (legacy injection)

Older Claude Code versions injected the effort into user messages as
`<reasoning_effort>NN</reasoning_effort>` (0-100). Pulse maps:

| Numeric range | Effort     |
| ------------- | ---------- |
| 0 – 25        | Low        |
| 26 – 55       | Medium     |
| 56 – 80       | High       |
| 81 – 99       | Extra High |
| 100+          | Max        |

## Tokenizer note

Opus 4.7 ships with a new tokenizer that can produce **up to ~35% more tokens**
for the same input text compared to 4.6. Per-token rates are identical
($5 input / $25 output per million), so your bill can grow even if your
prompting style is unchanged.

`cost::has_inflated_tokenizer()` returns `true` for any `claude-opus-4-7*`
model id; the Pulse UI surfaces this as a tooltip on the model badge.

## Pricing & 1M context

- Opus 4.7 pricing matches 4.6: $5 input / $25 output per million.
- 1M context is **GA** (no surcharge at any length). `is_ga_1m_context()`
  returns `true` for Opus 4.6+ and Sonnet 4.6+.
- Beta 1M models (Opus 4.5, Sonnet 4/4.5) keep a 2× input / 1.5× output /
  2× cache surcharge when total API input exceeds 200K tokens.
