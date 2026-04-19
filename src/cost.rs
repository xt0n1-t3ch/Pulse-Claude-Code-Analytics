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

/// Strips context window suffixes like `[1m]` from model IDs.
/// e.g. "claude-opus-4-6[1m]" → "claude-opus-4-6"
fn strip_context_suffix(model_id: &str) -> &str {
    model_id.split('[').next().unwrap_or(model_id)
}

pub fn model_pricing(model_id: &str) -> ModelPricing {
    let id = strip_context_suffix(model_id).to_lowercase();
    if id.contains("opus") {
        // Opus 4.5/4.6 have new lower pricing; legacy Opus 4.0/4.1/3 keep old pricing
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
        // Haiku 4.5+ = $1/$5; Haiku 3.5 = $0.80/$4; Haiku 3 = $0.25/$1.25
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
            // Haiku 4.5+ and unknown haiku
            ModelPricing {
                input_per_million: 1.0,
                output_per_million: 5.0,
                cache_write_per_million: 1.25,
                cache_read_per_million: 0.10,
            }
        }
    } else {
        // Sonnet and unknown models default to Sonnet pricing
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
    // Match: opus-4-5, opus-4-6, opus-4-5-20251101, opus-4-6-20260213
    // Don't match: opus-4, opus-4-0, opus-4-1, opus-3, opus (bare)
    let after = match id.find("opus") {
        Some(pos) => &id[pos + 4..],
        None => return false,
    };
    // Parse version segments: take only short numeric parts (<=3 chars), skip dates (8+ digits)
    let segments: Vec<&str> = after
        .split('-')
        .filter(|s| !s.is_empty() && s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit()))
        .collect();
    // Need at least 2 version segments where second is 5+
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

    // Determine the family name
    let family = if id.contains("opus") {
        "Opus"
    } else if id.contains("haiku") {
        "Haiku"
    } else if id.contains("sonnet") {
        "Sonnet"
    } else {
        // Unknown family — clean up the raw ID into something readable
        // e.g. "claude-unknown-model-20260101" → "claude-unknown-model"
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

    // Extract version from the ID by finding the family name and parsing the numbers after it.
    // e.g. "claude-opus-4-6-20260213" -> after "opus" we get "-4-6-20260213"
    //      "claude-sonnet-4-5-20250929" -> after "sonnet" we get "-4-5-20250929"
    //      "claude-opus-4-6" -> after "opus" we get "-4-6"
    //      "haiku" -> after "haiku" we get ""
    let family_lower = family.to_lowercase();
    let after_family = id
        .find(&family_lower)
        .map(|pos| &id[pos + family_lower.len()..])
        .unwrap_or("");

    // Parse version segments: take leading dash-separated numbers, skip trailing date (8+ digits)
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
    // Explicit [1m] suffix is an immediate indicator
    if model_id.contains("[1m]") {
        return true;
    }
    let id = strip_context_suffix(model_id).to_lowercase();
    if id.contains("haiku") {
        return false;
    }
    if id.contains("opus") {
        return is_new_opus(&id);
    }
    if id.contains("sonnet") {
        // Old format: "claude-3-7-sonnet-..." — version number appears BEFORE "sonnet"
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
            return false; // Claude 3.x Sonnet — no 1M support
        }
        // New format: "claude-sonnet-4-6-..." — version appears AFTER "sonnet"
        let after = &id[sonnet_pos + 6..];
        let first_seg = after
            .split('-')
            .find(|s| !s.is_empty() && s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit()));
        return match first_seg {
            Some(seg) => seg.parse::<u32>().map(|v| v >= 4).unwrap_or(true),
            None => true, // bare "sonnet" — assume modern
        };
    }
    false
}

/// Returns true if the model has 1M context GA (no surcharge).
/// GA models: Opus 4.6+, Sonnet 4.6+. Beta models (4.5, 4) may still have surcharge.
/// Also returns true for any model with explicit `[1m]` suffix (GA indicator).
pub fn is_ga_1m_context(model_id: &str) -> bool {
    if model_id.contains("[1m]") {
        return true;
    }
    let id = strip_context_suffix(model_id).to_lowercase();
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
/// GA models (Opus 4.6+, Sonnet 4.6+): standard pricing at any context length.
/// Per Anthropic blog (2026-03-13): "Standard pricing applies uniformly — no long-context premium."
///
/// Beta models (Opus 4.5, Sonnet 4/4.5): 2× input, 1.5× output, 2× cache when >200K total API input.
pub fn calculate_cost_with_context(
    model_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
) -> f64 {
    let total_api_input = input_tokens + cache_creation_tokens + cache_read_tokens;
    // GA models never get surcharge; beta models get surcharge when >200K
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_calculation_sonnet() {
        let cost = calculate_cost("claude-sonnet-4-20250514", 1_000_000, 500_000, 0, 0);
        assert!((cost - 10.5).abs() < 0.001); // 3.0 + 7.5
    }

    #[test]
    fn cost_calculation_opus_new() {
        // Opus 4.5: input $5/M, output $25/M
        let cost = calculate_cost("claude-opus-4-5-20251101", 1_000_000, 100_000, 0, 0);
        let expected = 5.0 + 2.5; // 1M * $5 + 0.1M * $25
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn cost_calculation_opus_legacy() {
        // Opus 4.1: input $15/M, output $75/M
        let cost = calculate_cost("claude-opus-4-1-20250414", 1_000_000, 100_000, 0, 0);
        let expected = 15.0 + 7.5; // 1M * $15 + 0.1M * $75
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn cost_with_cache_tokens_new_opus() {
        // Opus 4.6: input $5/M, output $25/M, cache write $6.25/M, cache read $0.50/M
        let cost = calculate_cost(
            "claude-opus-4-6-20260213",
            3,      // input
            9,      // output
            12_487, // cache creation
            22_766, // cache read
        );
        let expected = (3.0 * 5.0 / 1_000_000.0)
            + (9.0 * 25.0 / 1_000_000.0)
            + (12_487.0 * 6.25 / 1_000_000.0)
            + (22_766.0 * 0.50 / 1_000_000.0);
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn cost_with_cache_tokens_legacy_opus() {
        // Opus 4.0: input $15/M, output $75/M, cache write $18.75/M, cache read $1.50/M
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
        // New Opus (4.5, 4.6) → $5/$25
        let p1 = model_pricing("claude-opus-4-5-20251101");
        assert!((p1.input_per_million - 5.0).abs() < 0.001);
        assert!((p1.output_per_million - 25.0).abs() < 0.001);

        let p2 = model_pricing("claude-opus-4-6-20260213");
        assert!((p2.input_per_million - 5.0).abs() < 0.001);

        let p3 = model_pricing("claude-opus-4-6");
        assert!((p3.input_per_million - 5.0).abs() < 0.001);

        // Legacy Opus (4.0, 4.1, 4, 3) → $15/$75
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
        // Haiku 4.5 → $1/$5
        let p1 = model_pricing("claude-haiku-4-5-20251001");
        assert!((p1.input_per_million - 1.0).abs() < 0.001);

        // Haiku 3.5 → $0.80/$4
        let p2 = model_pricing("claude-haiku-3-5-20241022");
        assert!((p2.input_per_million - 0.80).abs() < 0.001);
        assert!((p2.output_per_million - 4.0).abs() < 0.001);

        // Haiku 3 → $0.25/$1.25
        let p3 = model_pricing("claude-haiku-3-20240307");
        assert!((p3.input_per_million - 0.25).abs() < 0.001);
        assert!((p3.output_per_million - 1.25).abs() < 0.001);

        // Bare haiku → default haiku 4.5 pricing
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
        // Sonnet 4.6 is GA — no surcharge even with >200K total API input
        let cost = calculate_cost_with_context(
            "claude-sonnet-4-6",
            50_000,  // input
            5_000,   // output
            100_000, // cache_creation
            100_000, // cache_read  (total_api_input = 250K > 200K)
        );
        let standard = calculate_cost("claude-sonnet-4-6", 50_000, 5_000, 100_000, 100_000);
        assert!((cost - standard).abs() < 0.0001);
    }

    #[test]
    fn cost_1m_beta_still_has_surcharge() {
        // Sonnet 4.5 is beta — surcharge applies when >200K
        let cost = calculate_cost_with_context(
            "claude-sonnet-4-5",
            50_000,  // input
            5_000,   // output
            100_000, // cache_creation
            100_000, // cache_read  (total_api_input = 250K > 200K)
        );
        let standard = calculate_cost("claude-sonnet-4-5", 50_000, 5_000, 100_000, 100_000);
        assert!(cost > standard);
    }

    #[test]
    fn cost_1m_below_threshold_uses_standard() {
        // 100K total API input → standard rates
        let cost_std = calculate_cost("claude-sonnet-4-6", 50_000, 5_000, 25_000, 25_000);
        let cost_ctx =
            calculate_cost_with_context("claude-sonnet-4-6", 50_000, 5_000, 25_000, 25_000);
        assert!((cost_std - cost_ctx).abs() < 0.0001);
    }

    #[test]
    fn cost_1m_haiku_no_surcharge() {
        // Haiku doesn't support 1M context → always standard pricing
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
        // [1m] suffix must not affect pricing detection
        let p = model_pricing("claude-opus-4-6[1m]");
        assert!((p.input_per_million - 5.0).abs() < 0.001);
        assert!((p.output_per_million - 25.0).abs() < 0.001);
    }

    #[test]
    fn test_is_ga_1m_context() {
        // GA: Opus 4.6+, Sonnet 4.6+
        assert!(is_ga_1m_context("claude-opus-4-6"));
        assert!(is_ga_1m_context("claude-opus-4-6-20260213"));
        assert!(is_ga_1m_context("claude-opus-4-6[1m]"));
        assert!(is_ga_1m_context("claude-sonnet-4-6"));
        assert!(is_ga_1m_context("claude-sonnet-4-6-20260213"));
        // Beta: Opus 4.5, Sonnet 4.5/4
        assert!(!is_ga_1m_context("claude-opus-4-5"));
        assert!(!is_ga_1m_context("claude-sonnet-4-5"));
        assert!(!is_ga_1m_context("claude-sonnet-4"));
        // Not supported at all
        assert!(!is_ga_1m_context("claude-haiku-4-5"));
        assert!(!is_ga_1m_context("opus"));
    }

    #[test]
    fn test_supports_1m_context() {
        // Explicit [1m] suffix — always yes
        assert!(supports_1m_context("claude-opus-4-6[1m]"));
        assert!(supports_1m_context("claude-sonnet-4-6[1m]"));
        // Opus 4.5 / 4.6 — yes
        assert!(supports_1m_context("claude-opus-4-6-20260213"));
        assert!(supports_1m_context("claude-opus-4-5-20251101"));
        assert!(supports_1m_context("claude-opus-4-6"));
        // Legacy Opus — no
        assert!(!supports_1m_context("claude-opus-4-1-20250414"));
        assert!(!supports_1m_context("claude-opus-4-20250514"));
        // Sonnet 4.x — yes
        assert!(supports_1m_context("claude-sonnet-4-6-20260213"));
        assert!(supports_1m_context("claude-sonnet-4-5-20250929"));
        assert!(supports_1m_context("claude-sonnet-4-20250514"));
        assert!(supports_1m_context("claude-sonnet-4-6"));
        // Sonnet 3.x (old format) — no
        assert!(!supports_1m_context("claude-3-7-sonnet-20250219"));
        assert!(!supports_1m_context("claude-3-5-sonnet-20241022"));
        // Haiku — never
        assert!(!supports_1m_context("claude-haiku-4-5-20251001"));
        assert!(!supports_1m_context("claude-haiku-3-5-20241022"));
    }

    #[test]
    fn test_model_display_with_context() {
        // GA model (Sonnet 4.6) → always shows "(1M)" regardless of turn size
        assert_eq!(
            model_display_with_context("claude-sonnet-4-6", "Claude Sonnet 4.6", 300_000),
            "Claude Sonnet 4.6 (1M)"
        );
        assert_eq!(
            model_display_with_context("claude-sonnet-4-6", "Claude Sonnet 4.6", 0),
            "Claude Sonnet 4.6 (1M)"
        );
        // GA model with [1m] suffix
        assert_eq!(
            model_display_with_context("claude-opus-4-6[1m]", "Claude Opus 4.6", 0),
            "Claude Opus 4.6 (1M)"
        );
        // Beta model (Sonnet 4.5) → only shows when turn > 200K
        assert_eq!(
            model_display_with_context("claude-sonnet-4-5", "Claude Sonnet 4.5", 300_000),
            "Claude Sonnet 4.5 (1M Context)"
        );
        assert_eq!(
            model_display_with_context("claude-sonnet-4-5", "Claude Sonnet 4.5", 150_000),
            "Claude Sonnet 4.5"
        );
        // Haiku never supports 1M context
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
        assert_eq!(strip_claude_prefix("Claude"), "Claude"); // bare "Claude" stays
        assert_eq!(strip_claude_prefix("Unknown Model"), "Unknown Model");
    }

    // ── Opus 4.7 explicit coverage ──

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
        // Opus 4.7 ≥ 4.6 threshold → GA no-surcharge
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
        // GA model: calculate_cost_with_context must equal calculate_cost at any size
        let with = calculate_cost_with_context(
            "claude-opus-4-7",
            50_000,
            5_000,
            100_000,
            100_000, // total API input 250K > 200K
        );
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
}
