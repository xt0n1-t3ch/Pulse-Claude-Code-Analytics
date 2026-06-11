/// Model pricing per million tokens.
/// Update these when new models are released: https://www.anthropic.com/pricing
///
/// Prompt caching pricing (5-minute TTL, default):
///   cache write = 1.25x base input price
///   cache read  = 0.10x base input price
///
/// Note: 1-hour TTL cache writes cost 2x base input, but Claude Code JSONL
/// doesn't distinguish cache TTL, so we use the 5-minute rate.
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_write_per_million: f64,
    pub cache_read_per_million: f64,
}

/// Fast-mode (priority speed) rate multiplier. When a turn runs at Fast speed on
/// a fast-capable model, every token category is billed at this multiple of the
/// model's standard rate. Opus 4.8 fast mode is exactly 2x standard.
pub const FAST_RATE_MULTIPLIER: f64 = 2.0;

/// Strips context window suffixes like `[1m]` from model IDs.
/// e.g. "claude-opus-4-6[1m]" → "claude-opus-4-6"
fn strip_context_suffix(model_id: &str) -> &str {
    model_id.split('[').next().unwrap_or(model_id)
}

fn is_mythos_class(id: &str) -> bool {
    id.contains("fable") || id.contains("mythos")
}

pub fn model_pricing(model_id: &str) -> ModelPricing {
    let id = strip_context_suffix(model_id).to_lowercase();
    if id.contains("opus") {
        if is_new_opus(&id) {
            ModelPricing {
                input_per_million: 5.0,
                output_per_million: 25.0,
                cache_write_per_million: 6.25,
                cache_read_per_million: 0.50,
            }
        } else {
            ModelPricing {
                input_per_million: 15.0,
                output_per_million: 75.0,
                cache_write_per_million: 18.75,
                cache_read_per_million: 1.50,
            }
        }
    } else if id.contains("haiku") {
        if id.contains("3-5") || id.contains("3.5") {
            ModelPricing {
                input_per_million: 0.80,
                output_per_million: 4.0,
                cache_write_per_million: 1.0,
                cache_read_per_million: 0.08,
            }
        } else if id.contains("haiku-3") && !id.contains("3-5") {
            ModelPricing {
                input_per_million: 0.25,
                output_per_million: 1.25,
                cache_write_per_million: 0.30,
                cache_read_per_million: 0.03,
            }
        } else {
            ModelPricing {
                input_per_million: 1.0,
                output_per_million: 5.0,
                cache_write_per_million: 1.25,
                cache_read_per_million: 0.10,
            }
        }
    } else if is_mythos_class(&id) {
        ModelPricing {
            input_per_million: 10.0,
            output_per_million: 50.0,
            cache_write_per_million: 12.5,
            cache_read_per_million: 1.0,
        }
    } else {
        ModelPricing {
            input_per_million: 3.0,
            output_per_million: 15.0,
            cache_write_per_million: 3.75,
            cache_read_per_million: 0.30,
        }
    }
}

/// Opus 4.5 and 4.6 use new pricing ($5/$25). Detected by version segments after "opus".
fn is_new_opus(id: &str) -> bool {
    // Don't match: opus-4, opus-4-0, opus-4-1, opus-3, opus (bare)
    let after = match id.find("opus") {
        Some(pos) => &id[pos + 4..],
        None => return false,
    };
    let segments: Vec<&str> = after
        .split('-')
        .filter(|s| !s.is_empty() && s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit()))
        .collect();
    if segments.len() >= 2
        && let (Ok(major), Ok(minor)) = (segments[0].parse::<u32>(), segments[1].parse::<u32>())
    {
        return major >= 4 && minor >= 5;
    }
    false
}

pub fn calculate_cost(
    model_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
) -> f64 {
    let pricing = model_pricing(model_id);
    let input_cost = (input_tokens as f64 / 1_000_000.0) * pricing.input_per_million;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * pricing.output_per_million;
    let cache_write_cost =
        (cache_creation_tokens as f64 / 1_000_000.0) * pricing.cache_write_per_million;
    let cache_read_cost = (cache_read_tokens as f64 / 1_000_000.0) * pricing.cache_read_per_million;
    input_cost + output_cost + cache_write_cost + cache_read_cost
}

/// Extract a display name like "Claude Opus 4.6" from any model ID format.
/// Handles both dated IDs ("claude-opus-4-6-20260213") and short IDs ("claude-opus-4-6").
pub fn model_display_name(model_id: &str) -> String {
    let id = strip_context_suffix(model_id).to_lowercase();
    let trimmed = id.trim();

    if trimmed.is_empty() || trimmed == "<synthetic>" {
        return "Claude".to_string();
    }

    let family = if id.contains("opus") {
        "Opus"
    } else if id.contains("haiku") {
        "Haiku"
    } else if id.contains("sonnet") {
        "Sonnet"
    } else if id.contains("fable") {
        "Fable"
    } else if id.contains("mythos") {
        "Mythos"
    } else {
        let cleaned = id
            .trim_end_matches(|c: char| c == '-' || c.is_ascii_digit())
            .trim_end_matches('-');
        let display = if cleaned.is_empty() {
            model_id
        } else {
            cleaned
        };
        return format!("Claude ({})", display);
    };

    let family_lower = family.to_lowercase();
    let after_family = id
        .find(&family_lower)
        .map(|pos| &id[pos + family_lower.len()..])
        .unwrap_or("");

    let segments: Vec<&str> = after_family
        .split('-')
        .filter(|s| !s.is_empty())
        .take_while(|s| s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit()))
        .collect();

    if segments.is_empty() {
        format!("Claude {family}")
    } else {
        format!("Claude {} {}", family, segments.join("."))
    }
}

/// Returns true if the model supports the 1M token context window.
/// Supported: Opus 4.5+, Sonnet 4.6/4.5/4. Not supported: Haiku, legacy Opus, Sonnet 3.x.
/// Also returns true if the model ID contains a `[1m]` suffix (explicit 1M context indicator).
pub fn supports_1m_context(model_id: &str) -> bool {
    if model_id.contains("[1m]") {
        return true;
    }
    let id = strip_context_suffix(model_id).to_lowercase();
    if is_mythos_class(&id) {
        return true;
    }
    if id.contains("haiku") {
        return false;
    }
    if id.contains("opus") {
        return is_new_opus(&id);
    }
    if id.contains("sonnet") {
        let sonnet_pos = id.find("sonnet").unwrap_or(id.len());
        let before = &id[..sonnet_pos];
        let prefix_version: Option<u32> = before
            .split('-')
            .filter(|s| !s.is_empty() && s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit()))
            .filter_map(|s| s.parse::<u32>().ok())
            .next();
        if let Some(v) = prefix_version
            && v < 4
        {
            return false;
        }
        let after = &id[sonnet_pos + 6..];
        let first_seg = after
            .split('-')
            .find(|s| !s.is_empty() && s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit()));
        return match first_seg {
            Some(seg) => seg.parse::<u32>().map(|v| v >= 4).unwrap_or(true),
            None => true,
        };
    }
    false
}

/// Returns true if the model has 1M context GA for **API token-rate pricing**
/// (no long-context surcharge on input / output / cache).
///
/// Per Anthropic's official pricing docs
/// (<https://console.anthropic.com/docs/en/about-claude/pricing>, effective
/// 2026-03-13):
/// > *"Opus 4.7, Opus 4.6, and Sonnet 4.6 include the full 1M token context
/// > window at standard pricing."*
///
/// GA models (flat per-token rate at any context length, no 2×/1.5× premium):
///   • Opus 4.6+ · Opus 4.7 · **Sonnet 4.6**
///
/// Beta models (2× input, 1.5× output, 2× cache when total API input > 200K):
///   • Sonnet 4 / 4.5 · Opus 4 / 4.5
///
/// NOTE: **Plan-level "Extra Usage" accounting** (claude.ai Pro / Max / Teams)
/// is a *separate* concept and is NOT represented here. When a subscriber
/// exceeds their baseline plan quota, overage is billed pay-as-you-go against
/// their Extra Usage cap. Pulse tracks that independently via `crate::usage`.
/// This function only concerns token-rate surcharges applied at the API layer.
///
/// Also returns true for any model with an explicit `[1m]` suffix (an
/// explicit GA indicator emitted by tooling that has already reconciled
/// pricing).
pub fn is_ga_1m_context(model_id: &str) -> bool {
    if model_id.contains("[1m]") {
        return true;
    }
    let id = strip_context_suffix(model_id).to_lowercase();
    if is_mythos_class(&id) {
        return true;
    }
    if id.contains("opus") {
        return is_version_at_least(&id, "opus", 4, 6);
    }
    if id.contains("sonnet") {
        return is_version_at_least(&id, "sonnet", 4, 6);
    }
    false
}

/// Checks if a model ID has a version >= major.minor after the family name.
fn is_version_at_least(id: &str, family: &str, min_major: u32, min_minor: u32) -> bool {
    let after = match id.find(family) {
        Some(pos) => &id[pos + family.len()..],
        None => return false,
    };
    let segments: Vec<&str> = after
        .split('-')
        .filter(|s| !s.is_empty() && s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit()))
        .collect();
    if segments.len() >= 2
        && let (Ok(major), Ok(minor)) = (segments[0].parse::<u32>(), segments[1].parse::<u32>())
    {
        return major > min_major || (major == min_major && minor >= min_minor);
    }
    false
}

/// Like `calculate_cost` but applies the 1M context surcharge for **beta** models only.
///
/// GA models (Opus 4.6+, Opus 4.7, Sonnet 4.6): standard pricing at any context length.
/// Per Anthropic pricing docs (2026-03-13 update): standard per-token rate across the full 1M window.
///
/// Beta models (Sonnet 4 / 4.5, Opus 4 / 4.5): 2× input, 1.5× output, 2× cache when >200K total API input.
///
/// Plan-level "Extra Usage" (claude.ai Pro / Max / Teams) is a separate
/// concept tracked via `crate::usage` — not a token-rate multiplier.
pub fn calculate_cost_with_context(
    model_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
) -> f64 {
    let total_api_input = input_tokens + cache_creation_tokens + cache_read_tokens;
    if supports_1m_context(model_id) && !is_ga_1m_context(model_id) && total_api_input > 200_000 {
        let p = model_pricing(model_id);
        (input_tokens as f64 / 1_000_000.0) * p.input_per_million * 2.0
            + (output_tokens as f64 / 1_000_000.0) * p.output_per_million * 1.5
            + (cache_creation_tokens as f64 / 1_000_000.0) * p.cache_write_per_million * 2.0
            + (cache_read_tokens as f64 / 1_000_000.0) * p.cache_read_per_million * 2.0
    } else {
        calculate_cost(
            model_id,
            input_tokens,
            output_tokens,
            cache_creation_tokens,
            cache_read_tokens,
        )
    }
}

/// Returns the model display name with a 1M context indicator when applicable.
///
/// - GA models (Opus 4.6+, Sonnet 4.6+): always shows "(1M)" since 1M is their native context.
/// - Beta models: shows "(1M Context)" only when `max_turn_api_input > 200K` (single turn indicator).
/// - Unsupported models: no suffix.
pub fn model_display_with_context(
    model_id: &str,
    base_display: &str,
    max_turn_api_input: u64,
) -> String {
    if is_ga_1m_context(model_id) {
        format!("{} (1M)", base_display)
    } else if supports_1m_context(model_id) && max_turn_api_input > 200_000 {
        format!("{} (1M Context)", base_display)
    } else {
        base_display.to_string()
    }
}

/// Strips the "Claude " prefix from a display name for compact Discord display.
/// "Claude Opus 4.6" → "Opus 4.6", "Claude Sonnet 4.5" → "Sonnet 4.5".
/// The Discord app title is already "Claude Code", so repeating "Claude" is redundant.
pub fn strip_claude_prefix(display_name: &str) -> &str {
    display_name.strip_prefix("Claude ").unwrap_or(display_name)
}

/// True when the model uses a newer tokenizer that produces more tokens per prompt
/// for the same input text. Currently: Opus 4.7 ships with a tokenizer that can
/// produce up to ~35% more tokens than 4.6 for identical text, inflating billing
/// even though per-token rates are unchanged.
///
/// UI surfaces this as a tooltip/warning so users understand cost deltas vs 4.6.
pub fn has_inflated_tokenizer(model_id: &str) -> bool {
    let id = strip_context_suffix(model_id).to_lowercase();
    if !id.contains("opus") {
        return false;
    }
    is_version_at_least(&id, "opus", 4, 7)
}

/// True when the model supports fast mode (priority speed) billing.
/// Fast mode launched with Opus 4.8 — Opus 4.8+ only.
pub fn is_fast_capable(model_id: &str) -> bool {
    let id = strip_context_suffix(model_id).to_lowercase();
    if !id.contains("opus") {
        return false;
    }
    is_version_at_least(&id, "opus", 4, 8)
}

/// The effective rate multiplier for a turn: `FAST_RATE_MULTIPLIER` when the turn
/// ran at fast speed on a fast-capable model, otherwise `1.0`.
pub fn speed_multiplier(model_id: &str, fast: bool) -> f64 {
    if fast && is_fast_capable(model_id) {
        FAST_RATE_MULTIPLIER
    } else {
        1.0
    }
}

/// Speed- and context-aware cost split into the four billable token categories.
/// The four components always sum to `calculate_cost_with_context_and_speed` for
/// the same inputs, so per-turn accumulation of these components reconciles with
/// the accumulated total cost.
#[derive(Debug, Clone, Copy, Default)]
pub struct TurnCostBreakdown {
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
}

impl TurnCostBreakdown {
    pub fn total(&self) -> f64 {
        self.input_cost + self.output_cost + self.cache_write_cost + self.cache_read_cost
    }
}

/// Per-category form of `calculate_cost_with_context_and_speed`: returns the four
/// billable components for a turn, each already scaled by the beta 1M-context
/// surcharge (beta models above the threshold) and the fast-mode multiplier.
///
/// `pure_input_tokens` is the non-cached input (input minus cache write/read);
/// the beta threshold is evaluated on total API input (`pure_input + cache_write
/// + cache_read`) to match `calculate_cost_with_context`.
pub fn calculate_category_costs(
    model_id: &str,
    pure_input_tokens: u64,
    output_tokens: u64,
    cache_write_tokens: u64,
    cache_read_tokens: u64,
    fast: bool,
) -> TurnCostBreakdown {
    let pricing = model_pricing(model_id);
    let total_api_input = pure_input_tokens + cache_write_tokens + cache_read_tokens;
    let beta_surcharge =
        supports_1m_context(model_id) && !is_ga_1m_context(model_id) && total_api_input > 200_000;
    let (input_factor, output_factor, cache_factor) = if beta_surcharge {
        (2.0, 1.5, 2.0)
    } else {
        (1.0, 1.0, 1.0)
    };
    let speed = speed_multiplier(model_id, fast);

    TurnCostBreakdown {
        input_cost: (pure_input_tokens as f64 / 1_000_000.0)
            * pricing.input_per_million
            * input_factor
            * speed,
        output_cost: (output_tokens as f64 / 1_000_000.0)
            * pricing.output_per_million
            * output_factor
            * speed,
        cache_write_cost: (cache_write_tokens as f64 / 1_000_000.0)
            * pricing.cache_write_per_million
            * cache_factor
            * speed,
        cache_read_cost: (cache_read_tokens as f64 / 1_000_000.0)
            * pricing.cache_read_per_million
            * cache_factor
            * speed,
    }
}

/// Like `calculate_cost_with_context` but applies the fast-mode surcharge when the
/// turn ran at fast speed on a fast-capable model.
///
/// Fast mode bills every token category at `FAST_RATE_MULTIPLIER`x the standard
/// rate. The 1M context surcharge (beta models only) is computed first, then the
/// fast multiplier scales the whole turn — so a fast turn always costs exactly
/// `FAST_RATE_MULTIPLIER`x its standard-speed equivalent for identical tokens.
pub fn calculate_cost_with_context_and_speed(
    model_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
    fast: bool,
) -> f64 {
    let base = calculate_cost_with_context(
        model_id,
        input_tokens,
        output_tokens,
        cache_creation_tokens,
        cache_read_tokens,
    );
    base * speed_multiplier(model_id, fast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_calculation_sonnet() {
        let cost = calculate_cost("claude-sonnet-4-20250514", 1_000_000, 500_000, 0, 0);
        assert!((cost - 10.5).abs() < 0.001);
    }

    #[test]
    fn cost_calculation_opus_new() {
        let cost = calculate_cost("claude-opus-4-5-20251101", 1_000_000, 100_000, 0, 0);
        let expected = 5.0 + 2.5;
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn cost_calculation_opus_legacy() {
        let cost = calculate_cost("claude-opus-4-1-20250414", 1_000_000, 100_000, 0, 0);
        let expected = 15.0 + 7.5;
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn cost_with_cache_tokens_new_opus() {
        let cost = calculate_cost("claude-opus-4-6-20260213", 3, 9, 12_487, 22_766);
        let expected = (3.0 * 5.0 / 1_000_000.0)
            + (9.0 * 25.0 / 1_000_000.0)
            + (12_487.0 * 6.25 / 1_000_000.0)
            + (22_766.0 * 0.50 / 1_000_000.0);
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn cost_with_cache_tokens_legacy_opus() {
        let cost = calculate_cost(
            "claude-opus-4-20250414",
            1_000_000,
            100_000,
            50_000,
            200_000,
        );
        let expected =
            15.0 + 7.5 + (50_000.0 * 18.75 / 1_000_000.0) + (200_000.0 * 1.50 / 1_000_000.0);
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn unknown_model_uses_sonnet_pricing() {
        let cost = calculate_cost("claude-unknown-model", 1_000_000, 0, 0, 0);
        assert!((cost - 3.0).abs() < 0.001);
    }

    #[test]
    fn pricing_opus_generation_detection() {
        let p1 = model_pricing("claude-opus-4-5-20251101");
        assert!((p1.input_per_million - 5.0).abs() < 0.001);
        assert!((p1.output_per_million - 25.0).abs() < 0.001);

        let p2 = model_pricing("claude-opus-4-6-20260213");
        assert!((p2.input_per_million - 5.0).abs() < 0.001);

        let p3 = model_pricing("claude-opus-4-6");
        assert!((p3.input_per_million - 5.0).abs() < 0.001);

        let p4 = model_pricing("claude-opus-4-1-20250414");
        assert!((p4.input_per_million - 15.0).abs() < 0.001);
        assert!((p4.output_per_million - 75.0).abs() < 0.001);

        let p5 = model_pricing("claude-opus-4-20250414");
        assert!((p5.input_per_million - 15.0).abs() < 0.001);

        let p6 = model_pricing("opus");
        assert!((p6.input_per_million - 15.0).abs() < 0.001);
    }

    #[test]
    fn pricing_haiku_generations() {
        let p1 = model_pricing("claude-haiku-4-5-20251001");
        assert!((p1.input_per_million - 1.0).abs() < 0.001);

        let p2 = model_pricing("claude-haiku-3-5-20241022");
        assert!((p2.input_per_million - 0.80).abs() < 0.001);
        assert!((p2.output_per_million - 4.0).abs() < 0.001);

        let p3 = model_pricing("claude-haiku-3-20240307");
        assert!((p3.input_per_million - 0.25).abs() < 0.001);
        assert!((p3.output_per_million - 1.25).abs() < 0.001);

        let p4 = model_pricing("haiku");
        assert!((p4.input_per_million - 1.0).abs() < 0.001);
    }

    #[test]
    fn display_names_dated() {
        assert_eq!(
            model_display_name("claude-opus-4-6-20260213"),
            "Claude Opus 4.6"
        );
        assert_eq!(
            model_display_name("claude-opus-4-5-20251101"),
            "Claude Opus 4.5"
        );
        assert_eq!(
            model_display_name("claude-sonnet-4-5-20250929"),
            "Claude Sonnet 4.5"
        );
        assert_eq!(
            model_display_name("claude-sonnet-4-20250514"),
            "Claude Sonnet 4"
        );
        assert_eq!(
            model_display_name("claude-haiku-4-5-20251001"),
            "Claude Haiku 4.5"
        );
    }

    #[test]
    fn display_names_short() {
        assert_eq!(model_display_name("claude-opus-4-6"), "Claude Opus 4.6");
        assert_eq!(model_display_name("claude-opus-4-5"), "Claude Opus 4.5");
        assert_eq!(model_display_name("claude-sonnet-4-5"), "Claude Sonnet 4.5");
        assert_eq!(model_display_name("claude-sonnet-4"), "Claude Sonnet 4");
        assert_eq!(model_display_name("claude-haiku-4-5"), "Claude Haiku 4.5");
    }

    #[test]
    fn cost_1m_ga_no_surcharge() {
        let cost =
            calculate_cost_with_context("claude-sonnet-4-6", 50_000, 5_000, 100_000, 100_000);
        let standard = calculate_cost("claude-sonnet-4-6", 50_000, 5_000, 100_000, 100_000);
        assert!((cost - standard).abs() < 0.0001);
    }

    #[test]
    fn cost_1m_beta_still_has_surcharge() {
        let cost =
            calculate_cost_with_context("claude-sonnet-4-5", 50_000, 5_000, 100_000, 100_000);
        let standard = calculate_cost("claude-sonnet-4-5", 50_000, 5_000, 100_000, 100_000);
        assert!(cost > standard);
    }

    #[test]
    fn cost_1m_below_threshold_uses_standard() {
        let cost_std = calculate_cost("claude-sonnet-4-6", 50_000, 5_000, 25_000, 25_000);
        let cost_ctx =
            calculate_cost_with_context("claude-sonnet-4-6", 50_000, 5_000, 25_000, 25_000);
        assert!((cost_std - cost_ctx).abs() < 0.0001);
    }

    #[test]
    fn cost_1m_haiku_no_surcharge() {
        let cost_std = calculate_cost("claude-haiku-4-5", 50_000, 5_000, 100_000, 100_000);
        let cost_ctx =
            calculate_cost_with_context("claude-haiku-4-5", 50_000, 5_000, 100_000, 100_000);
        assert!((cost_std - cost_ctx).abs() < 0.0001);
    }

    #[test]
    fn context_suffix_stripped_for_display() {
        assert_eq!(model_display_name("claude-opus-4-6[1m]"), "Claude Opus 4.6");
        assert_eq!(
            model_display_name("claude-sonnet-4-6[1m]"),
            "Claude Sonnet 4.6"
        );
    }

    #[test]
    fn context_suffix_stripped_for_pricing() {
        let p = model_pricing("claude-opus-4-6[1m]");
        assert!((p.input_per_million - 5.0).abs() < 0.001);
        assert!((p.output_per_million - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_is_ga_1m_context() {
        assert!(is_ga_1m_context("claude-opus-4-6"));
        assert!(is_ga_1m_context("claude-opus-4-6-20260213"));
        assert!(is_ga_1m_context("claude-opus-4-6[1m]"));
        assert!(is_ga_1m_context("claude-sonnet-4-6"));
        assert!(is_ga_1m_context("claude-sonnet-4-6-20260213"));
        assert!(!is_ga_1m_context("claude-opus-4-5"));
        assert!(!is_ga_1m_context("claude-sonnet-4-5"));
        assert!(!is_ga_1m_context("claude-sonnet-4"));
        assert!(!is_ga_1m_context("claude-haiku-4-5"));
        assert!(!is_ga_1m_context("opus"));
    }

    #[test]
    fn test_supports_1m_context() {
        assert!(supports_1m_context("claude-opus-4-6[1m]"));
        assert!(supports_1m_context("claude-sonnet-4-6[1m]"));
        assert!(supports_1m_context("claude-opus-4-6-20260213"));
        assert!(supports_1m_context("claude-opus-4-5-20251101"));
        assert!(supports_1m_context("claude-opus-4-6"));
        assert!(!supports_1m_context("claude-opus-4-1-20250414"));
        assert!(!supports_1m_context("claude-opus-4-20250514"));
        assert!(supports_1m_context("claude-sonnet-4-6-20260213"));
        assert!(supports_1m_context("claude-sonnet-4-5-20250929"));
        assert!(supports_1m_context("claude-sonnet-4-20250514"));
        assert!(supports_1m_context("claude-sonnet-4-6"));
        assert!(!supports_1m_context("claude-3-7-sonnet-20250219"));
        assert!(!supports_1m_context("claude-3-5-sonnet-20241022"));
        assert!(!supports_1m_context("claude-haiku-4-5-20251001"));
        assert!(!supports_1m_context("claude-haiku-3-5-20241022"));
    }

    #[test]
    fn test_model_display_with_context() {
        assert_eq!(
            model_display_with_context("claude-sonnet-4-6", "Claude Sonnet 4.6", 300_000),
            "Claude Sonnet 4.6 (1M)"
        );
        assert_eq!(
            model_display_with_context("claude-sonnet-4-6", "Claude Sonnet 4.6", 0),
            "Claude Sonnet 4.6 (1M)"
        );
        assert_eq!(
            model_display_with_context("claude-opus-4-6[1m]", "Claude Opus 4.6", 0),
            "Claude Opus 4.6 (1M)"
        );
        assert_eq!(
            model_display_with_context("claude-sonnet-4-5", "Claude Sonnet 4.5", 300_000),
            "Claude Sonnet 4.5 (1M Context)"
        );
        assert_eq!(
            model_display_with_context("claude-sonnet-4-5", "Claude Sonnet 4.5", 150_000),
            "Claude Sonnet 4.5"
        );
        assert_eq!(
            model_display_with_context("claude-haiku-4-5", "Claude Haiku 4.5", 500_000),
            "Claude Haiku 4.5"
        );
    }

    #[test]
    fn test_strip_claude_prefix() {
        assert_eq!(strip_claude_prefix("Claude Opus 4.6"), "Opus 4.6");
        assert_eq!(strip_claude_prefix("Claude Sonnet 4.5"), "Sonnet 4.5");
        assert_eq!(strip_claude_prefix("Claude Haiku 4.5"), "Haiku 4.5");
        assert_eq!(strip_claude_prefix("Claude"), "Claude");
        assert_eq!(strip_claude_prefix("Unknown Model"), "Unknown Model");
    }

    #[test]
    fn opus_4_7_pricing_matches_4_6() {
        let p = model_pricing("claude-opus-4-7");
        assert!((p.input_per_million - 5.0).abs() < 0.001);
        assert!((p.output_per_million - 25.0).abs() < 0.001);
        assert!((p.cache_write_per_million - 6.25).abs() < 0.001);
        assert!((p.cache_read_per_million - 0.50).abs() < 0.001);
    }

    #[test]
    fn opus_4_7_pricing_dated_id() {
        let p = model_pricing("claude-opus-4-7-20260301");
        assert!((p.input_per_million - 5.0).abs() < 0.001);
        assert!((p.output_per_million - 25.0).abs() < 0.001);
    }

    #[test]
    fn opus_4_7_1m_suffix_strips_for_pricing() {
        let p = model_pricing("claude-opus-4-7[1m]");
        assert!((p.input_per_million - 5.0).abs() < 0.001);
    }

    #[test]
    fn opus_4_7_is_ga_1m_context() {
        assert!(is_ga_1m_context("claude-opus-4-7"));
        assert!(is_ga_1m_context("claude-opus-4-7-20260301"));
        assert!(is_ga_1m_context("claude-opus-4-7[1m]"));
    }

    #[test]
    fn opus_4_7_supports_1m_context() {
        assert!(supports_1m_context("claude-opus-4-7"));
        assert!(supports_1m_context("claude-opus-4-7-20260301"));
    }

    #[test]
    fn opus_4_7_display_name() {
        assert_eq!(model_display_name("claude-opus-4-7"), "Claude Opus 4.7");
        assert_eq!(
            model_display_name("claude-opus-4-7-20260301"),
            "Claude Opus 4.7"
        );
        assert_eq!(model_display_name("claude-opus-4-7[1m]"), "Claude Opus 4.7");
    }

    #[test]
    fn opus_4_7_display_with_context_always_shows_1m() {
        assert_eq!(
            model_display_with_context("claude-opus-4-7", "Claude Opus 4.7", 300_000),
            "Claude Opus 4.7 (1M)"
        );
        assert_eq!(
            model_display_with_context("claude-opus-4-7", "Claude Opus 4.7", 0),
            "Claude Opus 4.7 (1M)"
        );
    }

    #[test]
    fn opus_4_7_no_1m_surcharge() {
        let with = calculate_cost_with_context("claude-opus-4-7", 50_000, 5_000, 100_000, 100_000);
        let plain = calculate_cost("claude-opus-4-7", 50_000, 5_000, 100_000, 100_000);
        assert!((with - plain).abs() < 0.0001);
    }

    #[test]
    fn has_inflated_tokenizer_opus_4_7() {
        assert!(has_inflated_tokenizer("claude-opus-4-7"));
        assert!(has_inflated_tokenizer("claude-opus-4-7-20260301"));
        assert!(has_inflated_tokenizer("claude-opus-4-7[1m]"));
    }

    #[test]
    fn has_inflated_tokenizer_older_opus_false() {
        assert!(!has_inflated_tokenizer("claude-opus-4-6"));
        assert!(!has_inflated_tokenizer("claude-opus-4-5"));
        assert!(!has_inflated_tokenizer("claude-opus-4-1"));
    }

    #[test]
    fn has_inflated_tokenizer_non_opus_false() {
        assert!(!has_inflated_tokenizer("claude-sonnet-4-6"));
        assert!(!has_inflated_tokenizer("claude-haiku-4-5"));
        assert!(!has_inflated_tokenizer("unknown"));
    }

    #[test]
    fn display_names_bare() {
        assert_eq!(model_display_name("opus"), "Claude Opus");
        assert_eq!(model_display_name("sonnet"), "Claude Sonnet");
        assert_eq!(model_display_name("haiku"), "Claude Haiku");
        assert_eq!(
            model_display_name("totally-unknown"),
            "Claude (totally-unknown)"
        );
        assert_eq!(model_display_name("<synthetic>"), "Claude");
    }

    #[test]
    fn opus_4_8_pricing_matches_new_opus_rates() {
        let p = model_pricing("claude-opus-4-8");
        assert!((p.input_per_million - 5.0).abs() < 0.001);
        assert!((p.output_per_million - 25.0).abs() < 0.001);
        assert!((p.cache_write_per_million - 6.25).abs() < 0.001);
        assert!((p.cache_read_per_million - 0.50).abs() < 0.001);
    }

    #[test]
    fn opus_4_8_pricing_dated_and_suffix_variants() {
        let dated = model_pricing("claude-opus-4-8-20260528");
        assert!((dated.input_per_million - 5.0).abs() < 0.001);
        assert!((dated.output_per_million - 25.0).abs() < 0.001);

        let suffixed = model_pricing("claude-opus-4-8[1m]");
        assert!((suffixed.input_per_million - 5.0).abs() < 0.001);
        assert!((suffixed.output_per_million - 25.0).abs() < 0.001);
    }

    #[test]
    fn opus_4_8_display_name() {
        assert_eq!(model_display_name("claude-opus-4-8"), "Claude Opus 4.8");
        assert_eq!(
            model_display_name("claude-opus-4-8-20260528"),
            "Claude Opus 4.8"
        );
        assert_eq!(model_display_name("claude-opus-4-8[1m]"), "Claude Opus 4.8");
    }

    #[test]
    fn opus_4_8_is_ga_1m_context() {
        assert!(is_ga_1m_context("claude-opus-4-8"));
        assert!(is_ga_1m_context("claude-opus-4-8-20260528"));
        assert!(is_ga_1m_context("claude-opus-4-8[1m]"));
    }

    #[test]
    fn opus_4_8_has_inflated_tokenizer() {
        assert!(has_inflated_tokenizer("claude-opus-4-8"));
        assert!(has_inflated_tokenizer("claude-opus-4-8-20260528"));
        assert!(has_inflated_tokenizer("claude-opus-4-8[1m]"));
    }

    #[test]
    fn opus_4_8_dated_and_suffix_price_identically() {
        let short = calculate_cost("claude-opus-4-8", 12_345, 6_789, 4_321, 9_876);
        let dated = calculate_cost("claude-opus-4-8-20260528", 12_345, 6_789, 4_321, 9_876);
        let suffixed = calculate_cost("claude-opus-4-8[1m]", 12_345, 6_789, 4_321, 9_876);
        assert!((short - dated).abs() < 0.0000001);
        assert!((short - suffixed).abs() < 0.0000001);
    }

    #[test]
    fn opus_4_8_is_fast_capable() {
        assert!(is_fast_capable("claude-opus-4-8"));
        assert!(is_fast_capable("claude-opus-4-8-20260528"));
        assert!(is_fast_capable("claude-opus-4-8[1m]"));
    }

    #[test]
    fn older_models_are_not_fast_capable() {
        assert!(!is_fast_capable("claude-opus-4-7"));
        assert!(!is_fast_capable("claude-opus-4-6"));
        assert!(!is_fast_capable("claude-opus-4-5"));
        assert!(!is_fast_capable("claude-sonnet-4-6"));
        assert!(!is_fast_capable("claude-haiku-4-5"));
    }

    #[test]
    fn opus_4_8_fast_is_exactly_2x_standard() {
        let model = "claude-opus-4-8";
        let (input, output, cache_write, cache_read) =
            (123_456u64, 78_901u64, 45_678u64, 234_567u64);

        let standard = calculate_cost_with_context_and_speed(
            model,
            input,
            output,
            cache_write,
            cache_read,
            false,
        );
        let fast = calculate_cost_with_context_and_speed(
            model,
            input,
            output,
            cache_write,
            cache_read,
            true,
        );

        assert!((fast - standard * FAST_RATE_MULTIPLIER).abs() < 0.0000001);
        assert!((fast - standard * 2.0).abs() < 0.0000001);
    }

    #[test]
    fn opus_4_8_fast_doubles_every_token_category() {
        let model = "claude-opus-4-8";
        let std_in = calculate_cost_with_context_and_speed(model, 1_000_000, 0, 0, 0, false);
        let fast_in = calculate_cost_with_context_and_speed(model, 1_000_000, 0, 0, 0, true);
        assert!((fast_in - std_in * 2.0).abs() < 0.0000001);
        let std_out = calculate_cost_with_context_and_speed(model, 0, 1_000_000, 0, 0, false);
        let fast_out = calculate_cost_with_context_and_speed(model, 0, 1_000_000, 0, 0, true);
        assert!((fast_out - std_out * 2.0).abs() < 0.0000001);
        let std_cw = calculate_cost_with_context_and_speed(model, 0, 0, 1_000_000, 0, false);
        let fast_cw = calculate_cost_with_context_and_speed(model, 0, 0, 1_000_000, 0, true);
        assert!((fast_cw - std_cw * 2.0).abs() < 0.0000001);
        let std_cr = calculate_cost_with_context_and_speed(model, 0, 0, 0, 1_000_000, false);
        let fast_cr = calculate_cost_with_context_and_speed(model, 0, 0, 0, 1_000_000, true);
        assert!((fast_cr - std_cr * 2.0).abs() < 0.0000001);
    }

    #[test]
    fn fast_flag_is_ignored_on_non_fast_capable_models() {
        let std = calculate_cost_with_context_and_speed(
            "claude-opus-4-7",
            50_000,
            5_000,
            10_000,
            20_000,
            false,
        );
        let fast = calculate_cost_with_context_and_speed(
            "claude-opus-4-7",
            50_000,
            5_000,
            10_000,
            20_000,
            true,
        );
        assert!((std - fast).abs() < 0.0000001);
    }

    #[test]
    fn category_costs_sum_to_headline_total_standard_and_fast() {
        let model = "claude-opus-4-8";
        let (input, output, cache_write, cache_read) =
            (123_456u64, 78_901u64, 45_678u64, 234_567u64);

        for fast in [false, true] {
            let bd = calculate_category_costs(model, input, output, cache_write, cache_read, fast);
            let headline = calculate_cost_with_context_and_speed(
                model,
                input,
                output,
                cache_write,
                cache_read,
                fast,
            );
            assert!((bd.total() - headline).abs() < 0.0000001, "fast={fast}");
        }
    }

    #[test]
    fn category_costs_fast_doubles_every_component() {
        let model = "claude-opus-4-8";
        let std = calculate_category_costs(model, 100_000, 20_000, 8_000, 60_000, false);
        let fast = calculate_category_costs(model, 100_000, 20_000, 8_000, 60_000, true);
        assert!((fast.input_cost - std.input_cost * 2.0).abs() < 0.0000001);
        assert!((fast.output_cost - std.output_cost * 2.0).abs() < 0.0000001);
        assert!((fast.cache_write_cost - std.cache_write_cost * 2.0).abs() < 0.0000001);
        assert!((fast.cache_read_cost - std.cache_read_cost * 2.0).abs() < 0.0000001);
    }

    #[test]
    fn category_costs_beta_surcharge_matches_headline() {
        let model = "claude-sonnet-4-5";
        let (input, output, cache_write, cache_read) =
            (50_000u64, 5_000u64, 100_000u64, 100_000u64);
        let bd = calculate_category_costs(model, input, output, cache_write, cache_read, false);
        let headline = calculate_cost_with_context(model, input, output, cache_write, cache_read);
        assert!((bd.total() - headline).abs() < 0.0000001);
    }

    #[test]
    fn standard_matrix_prices_every_model_correctly() {
        let cases = [
            ("claude-opus-4-5-20251101", 5.0, 25.0),
            ("claude-opus-4-6-20260213", 5.0, 25.0),
            ("claude-opus-4-7-20260301", 5.0, 25.0),
            ("claude-opus-4-8-20260528", 5.0, 25.0),
            ("claude-sonnet-4-5-20250929", 3.0, 15.0),
            ("claude-haiku-4-5-20251001", 1.0, 5.0),
            ("claude-fable-5-20260609", 10.0, 50.0),
            ("claude-mythos-5", 10.0, 50.0),
        ];
        for (model, input_rate, output_rate) in cases {
            let p = model_pricing(model);
            assert!(
                (p.input_per_million - input_rate).abs() < 0.001,
                "{model} input rate"
            );
            assert!(
                (p.output_per_million - output_rate).abs() < 0.001,
                "{model} output rate"
            );
        }
    }

    #[test]
    fn fable_5_pricing() {
        for model in ["claude-fable-5", "claude-mythos-5"] {
            let p = model_pricing(model);
            assert!((p.input_per_million - 10.0).abs() < 0.001, "{model} input");
            assert!(
                (p.output_per_million - 50.0).abs() < 0.001,
                "{model} output"
            );
            assert!(
                (p.cache_write_per_million - 12.5).abs() < 0.001,
                "{model} cache_write"
            );
            assert!(
                (p.cache_read_per_million - 1.0).abs() < 0.001,
                "{model} cache_read"
            );
        }
    }

    #[test]
    fn fable_5_dated_variant_pricing() {
        let p = model_pricing("claude-fable-5-20260609");
        assert!((p.input_per_million - 10.0).abs() < 0.001, "input dated");
        assert!((p.output_per_million - 50.0).abs() < 0.001, "output dated");
        let p = model_pricing("claude-mythos-5-20260609");
        assert!(
            (p.input_per_million - 10.0).abs() < 0.001,
            "mythos input dated"
        );
        assert!(
            (p.output_per_million - 50.0).abs() < 0.001,
            "mythos output dated"
        );
    }

    #[test]
    fn fable_5_display_name() {
        assert_eq!(model_display_name("claude-fable-5"), "Claude Fable 5");
        assert_eq!(
            model_display_name("claude-fable-5-20260609"),
            "Claude Fable 5"
        );
        assert_eq!(model_display_name("claude-fable-5[1m]"), "Claude Fable 5");
        assert_eq!(model_display_name("claude-mythos-5"), "Claude Mythos 5");
        assert_eq!(
            model_display_name("claude-mythos-5-20260609"),
            "Claude Mythos 5"
        );
    }

    #[test]
    fn fable_5_is_ga_1m_context() {
        assert!(is_ga_1m_context("claude-fable-5"));
        assert!(is_ga_1m_context("claude-fable-5-20260609"));
        assert!(is_ga_1m_context("claude-mythos-5"));
        assert!(is_ga_1m_context("claude-mythos-5-20260609"));
    }

    #[test]
    fn fable_5_supports_1m_context() {
        assert!(supports_1m_context("claude-fable-5"));
        assert!(supports_1m_context("claude-fable-5-20260609"));
        assert!(supports_1m_context("claude-mythos-5"));
        assert!(supports_1m_context("claude-mythos-5-20260609"));
    }

    #[test]
    fn fable_5_no_1m_surcharge() {
        let with = calculate_cost_with_context("claude-fable-5", 50_000, 5_000, 100_000, 100_000);
        let plain = calculate_cost("claude-fable-5", 50_000, 5_000, 100_000, 100_000);
        assert!((with - plain).abs() < 0.0001, "GA model: no surcharge");
        let with = calculate_cost_with_context("claude-mythos-5", 50_000, 5_000, 100_000, 100_000);
        let plain = calculate_cost("claude-mythos-5", 50_000, 5_000, 100_000, 100_000);
        assert!(
            (with - plain).abs() < 0.0001,
            "Mythos GA model: no surcharge"
        );
    }

    #[test]
    fn fable_5_display_with_context_shows_1m() {
        assert_eq!(
            model_display_with_context("claude-fable-5", "Claude Fable 5", 0),
            "Claude Fable 5 (1M)"
        );
        assert_eq!(
            model_display_with_context("claude-mythos-5", "Claude Mythos 5", 0),
            "Claude Mythos 5 (1M)"
        );
    }

    #[test]
    fn fable_5_not_inflated_tokenizer() {
        assert!(!has_inflated_tokenizer("claude-fable-5"));
        assert!(!has_inflated_tokenizer("claude-mythos-5"));
    }

    #[test]
    fn fable_5_not_fast_capable() {
        assert!(!is_fast_capable("claude-fable-5"));
        assert!(!is_fast_capable("claude-mythos-5"));
    }

    #[test]
    fn fable_5_strip_display_for_discord() {
        let display = model_display_name("claude-fable-5");
        assert_eq!(display, "Claude Fable 5");
        assert_eq!(strip_claude_prefix(&display), "Fable 5");
        let display = model_display_name("claude-mythos-5");
        assert_eq!(display, "Claude Mythos 5");
        assert_eq!(strip_claude_prefix(&display), "Mythos 5");
    }
}
