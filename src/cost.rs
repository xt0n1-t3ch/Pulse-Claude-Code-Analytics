/// Model pricing per million tokens.
/// Update these when new models are released: https://www.anthropic.com/pricing
///
/// Prompt caching pricing:
///   cache write = 1.25x base input price
///   cache read  = 0.10x base input price
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_write_per_million: f64,
    pub cache_read_per_million: f64,
}

pub fn model_pricing(model_id: &str) -> ModelPricing {
    let id = model_id.to_lowercase();
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
    if segments.len() >= 2 {
        if let (Ok(major), Ok(minor)) = (segments[0].parse::<u32>(), segments[1].parse::<u32>()) {
            return major >= 4 && minor >= 5;
        }
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
    let id = model_id.to_lowercase();

    // Determine the family name
    let family = if id.contains("opus") {
        "Opus"
    } else if id.contains("haiku") {
        "Haiku"
    } else if id.contains("sonnet") {
        "Sonnet"
    } else {
        return "Claude".to_string();
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

/// Returns true if the model supports the 1M token context window (beta feature).
/// Supported: Opus 4.5+, Sonnet 4.6/4.5/4. Not supported: Haiku, legacy Opus, Sonnet 3.x.
pub fn supports_1m_context(model_id: &str) -> bool {
    let id = model_id.to_lowercase();
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

/// Like `calculate_cost` but applies the 1M context surcharge when the total API input
/// (input + cache_creation + cache_read) exceeds 200K on a 1M-capable model.
/// Per Anthropic docs: all tokens in that request are billed at premium rates
/// (2× input, 1.5× output, 2× cache).
pub fn calculate_cost_with_context(
    model_id: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_tokens: u64,
    cache_read_tokens: u64,
) -> f64 {
    let total_api_input = input_tokens + cache_creation_tokens + cache_read_tokens;
    if supports_1m_context(model_id) && total_api_input > 200_000 {
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

/// Returns the model display name appended with "(1M Context)" when the session is using
/// the 1M context window (heuristic: session_tokens > 200K on a capable model).
pub fn model_display_with_context(
    model_id: &str,
    base_display: &str,
    session_tokens: u64,
) -> String {
    if supports_1m_context(model_id) && session_tokens > 200_000 {
        format!("{} (1M Context)", base_display)
    } else {
        base_display.to_string()
    }
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
    fn cost_1m_context_surcharge() {
        // Sonnet 4.6 with 250K total API input → 2× input, 1.5× output rates
        let cost = calculate_cost_with_context(
            "claude-sonnet-4-6",
            50_000,   // input
            5_000,    // output
            100_000,  // cache_creation
            100_000,  // cache_read  (total_api_input = 250K > 200K)
        );
        let p = model_pricing("claude-sonnet-4-6");
        let expected = (50_000.0 / 1e6) * p.input_per_million * 2.0
            + (5_000.0 / 1e6) * p.output_per_million * 1.5
            + (100_000.0 / 1e6) * p.cache_write_per_million * 2.0
            + (100_000.0 / 1e6) * p.cache_read_per_million * 2.0;
        assert!((cost - expected).abs() < 0.0001);
        // Must be more expensive than standard
        let standard = calculate_cost("claude-sonnet-4-6", 50_000, 5_000, 100_000, 100_000);
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
    fn test_supports_1m_context() {
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
        assert_eq!(
            model_display_with_context("claude-sonnet-4-6", "Claude Sonnet 4.6", 300_000),
            "Claude Sonnet 4.6 (1M Context)"
        );
        assert_eq!(
            model_display_with_context("claude-sonnet-4-6", "Claude Sonnet 4.6", 150_000),
            "Claude Sonnet 4.6"
        );
        assert_eq!(
            model_display_with_context("claude-haiku-4-5", "Claude Haiku 4.5", 500_000),
            "Claude Haiku 4.5"
        );
    }

    #[test]
    fn display_names_bare() {
        assert_eq!(model_display_name("opus"), "Claude Opus");
        assert_eq!(model_display_name("sonnet"), "Claude Sonnet");
        assert_eq!(model_display_name("haiku"), "Claude Haiku");
        assert_eq!(model_display_name("totally-unknown"), "Claude");
    }
}
