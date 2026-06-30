# Claude Fable 5 and Mythos 5

Pulse v1.2.0 adds first-class handling for Anthropic's Claude Fable 5 and Claude Mythos 5 model family.

## Official model specs

| Model | API ID | Availability | Context window | Max output | Pricing |
|:---|:---|:---|---:|---:|:---|
| Claude Fable 5 | `claude-fable-5` | Generally available | 1,000,000 tokens | 128,000 tokens | $10 input / $50 output per MTok |
| Claude Mythos 5 | `claude-mythos-5` | Limited availability | 1,000,000 tokens | 128,000 tokens | $10 input / $50 output per MTok |

Prompt caching rates are also identical for both models:

| Cache event | Official rate |
|:---|---:|
| 5-minute cache write | $12.50 / MTok |
| 1-hour cache write | $20.00 / MTok |
| Cache hit / refresh | $1.00 / MTok |

## Implementation notes

- `src/cost.rs` treats Fable 5 and Mythos 5 as the same pricing class.
- `is_ga_1m_context()` returns true for both models, so the beta long-context surcharge is never applied.
- `supports_1m_context()` reports a 1M window for raw, dated, and `[1m]` suffixed IDs.
- Rich Presence renders `Fable 5 (1M)` and `Mythos 5 (1M)` instead of falling back to raw model IDs.
- `has_inflated_tokenizer()` intentionally stays false for both models; the Opus 4.7+ tokenizer warning does not apply unless Anthropic documents otherwise.
- `is_fast_capable()` intentionally stays false for both models until Anthropic documents a fast tier.

## Cache TTL limitation

Claude Code JSONL currently reports cache creation tokens but does not say whether a write used the 5-minute or 1-hour TTL. Pulse therefore prices cache writes at the official 5-minute rate in runtime calculations and documents the 1-hour rate for reference.

## Sources

- Anthropic models overview: <https://platform.claude.com/docs/en/about-claude/models/overview>
- Anthropic pricing: <https://platform.claude.com/docs/en/about-claude/pricing>
- Anthropic context windows: <https://platform.claude.com/docs/en/build-with-claude/context-windows>
- Launch note: <https://www.anthropic.com/news/claude-fable-5-mythos-5>
