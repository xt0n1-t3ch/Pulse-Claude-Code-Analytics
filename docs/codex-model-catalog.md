# Codex model catalog

`src/codex/model_catalog.json` is the machine-readable owner for Codex model identity, aliases, reasoning efforts, speed capability, context, API pricing, Codex credit rates, and source URLs. Pulse consumes that vendored catalog through `src/codex/model.rs`; TypeScript and report layers do not maintain parallel pricing tables.

## GPT-5.6 family

| App label | Canonical ID | Alias | Reasoning efforts | Raw context | Usable context |
| --- | --- | --- | --- | ---: | ---: |
| 5.6 Sol | `gpt-5.6-sol` | `gpt-5.6` | Light, Medium, High, Extra High, Max, Ultra | 372,000 | 353,400 |
| 5.6 Terra | `gpt-5.6-terra` | — | Light, Medium, High, Extra High, Max, Ultra | 372,000 | 353,400 |
| 5.6 Luna | `gpt-5.6-luna` | — | Light, Medium, High, Extra High, Max | 372,000 | 353,400 |

The Codex App labels `5.5`, `5.6 Sol`, `5.6 Terra`, `5.6 Luna`, `5.4`, `5.4 Mini`, and `5.3 Codex Spark` are presentation labels. `gpt-5.6` resolves to Sol. There is no invented `gpt-5.6-pro` model; Pro is a plan/reasoning concept outside model identity.

Reasoning and speed are independent. The serialized effort values map to `Light`, `Medium`, `High`, `Extra High`, `Max`, and `Ultra`. Speed is `Standard` or `Fast`. The catalog records that the GPT-5.6 family can expose Fast, but it does not assign an unpublished Fast cost or credit multiplier.

## Pricing per million tokens

| Model | Input | Cache write | Cache read | Output | Codex input credits | Codex cache-read credits | Codex output credits |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 5.6 Sol | $5.00 | $6.25 | $0.50 | $30.00 | 125 | 12.5 | 750 |
| 5.6 Terra | $2.50 | $3.125 | $0.25 | $15.00 | 62.5 | 6.25 | 375 |
| 5.6 Luna | $1.00 | $1.25 | $0.10 | $6.00 | 25 | 2.5 | 150 |

OpenAI publishes GPT-5.6 cache writes at 1.25x uncached input and cache reads at a 90% discount. The Codex rate card publishes input, cache-read, and output credits but not cache-write credits. Pulse therefore leaves Codex cache-write credits unavailable unless observed telemetry supplies an authoritative value.

## Context provenance

Pulse resolves context in this order:

1. observed session JSONL;
2. the local Codex App `models_cache.json` inventory;
3. the included model catalog;
4. unavailable.

The local App inventory records 372,000 raw tokens for GPT-5.6. Pulse presents the raw value separately from the 95% usable budget of 353,400 tokens and records the selected source with the session. An unknown model never inherits another model's context.

## Cache policy

Prompt caching becomes eligible at 1,024 input tokens. For GPT-5.6 and later families, OpenAI documents a default and minimum 30-minute cache lifetime; a prefix may remain reusable longer. JSONL cache-read tokens are clamped to total input tokens. Missing cache-write telemetry remains missing.

## Completeness

- `exact`: every required component was observed and has a sourced rate.
- `partial`: Pulse can price a known subset and reports the known subtotal.
- `unavailable`: the model or required rates are unknown.

Historical Codex rows migrated without provenance use `legacy/unknown`. Unknown models never fall back to GPT-5.1 or another convenient rate.

## Sources

- [GPT-5.6 announcement and API pricing](https://openai.com/index/previewing-gpt-5-6-sol/)
- [Codex rate card](https://help.openai.com/en/articles/20001106-codex-rate-card-2)
- [Prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching)
- Local Codex App inventory: `%USERPROFILE%/.codex/models_cache.json`
