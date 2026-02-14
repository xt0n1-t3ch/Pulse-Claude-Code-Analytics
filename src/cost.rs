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
        ModelPricing {
            input_per_million: 15.0,
            output_per_million: 75.0,
            cache_write_per_million: 18.75,
            cache_read_per_million: 1.50,
        }
    } else if id.contains("haiku") {
        ModelPricing {
            input_per_million: 1.0,
            output_per_million: 5.0,
            cache_write_per_million: 1.25,
            cache_read_per_million: 0.10,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_calculation() {
        let cost = calculate_cost("claude-sonnet-4-20250514", 1_000_000, 500_000, 0, 0);
        assert!((cost - 10.5).abs() < 0.001); // 3.0 + 7.5
    }

    #[test]
    fn cost_calculation_opus() {
        let cost = calculate_cost("claude-opus-4-5-20251101", 1_000_000, 100_000, 0, 0);
        assert!((cost - 22.5).abs() < 0.001); // 15.0 + 7.5
    }

    #[test]
    fn cost_with_cache_tokens() {
        // Opus: input $15/M, output $75/M, cache write $18.75/M, cache read $1.50/M
        let cost = calculate_cost(
            "claude-opus-4-6-20260213",
            3,      // input
            9,      // output
            12_487, // cache creation
            22_766, // cache read
        );
        // input:  3 * 15/1M = 0.000045
        // output: 9 * 75/1M = 0.000675
        // cache write: 12487 * 18.75/1M = 0.234131
        // cache read:  22766 * 1.50/1M = 0.034149
        let expected = 0.000045 + 0.000675 + 0.234131 + 0.034149;
        assert!((cost - expected).abs() < 0.001);
    }

    #[test]
    fn unknown_model_uses_sonnet_pricing() {
        let cost = calculate_cost("claude-unknown-model", 1_000_000, 0, 0, 0);
        assert!((cost - 3.0).abs() < 0.001);
    }

    #[test]
    fn display_names_dated() {
        // Full dated IDs
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
        // Short IDs (as seen in JSONL files)
        assert_eq!(model_display_name("claude-opus-4-6"), "Claude Opus 4.6");
        assert_eq!(model_display_name("claude-opus-4-5"), "Claude Opus 4.5");
        assert_eq!(model_display_name("claude-sonnet-4-5"), "Claude Sonnet 4.5");
        assert_eq!(model_display_name("claude-sonnet-4"), "Claude Sonnet 4");
        assert_eq!(model_display_name("claude-haiku-4-5"), "Claude Haiku 4.5");
    }

    #[test]
    fn display_names_bare() {
        // Bare family names
        assert_eq!(model_display_name("opus"), "Claude Opus");
        assert_eq!(model_display_name("sonnet"), "Claude Sonnet");
        assert_eq!(model_display_name("haiku"), "Claude Haiku");
        assert_eq!(model_display_name("totally-unknown"), "Claude");
    }
}
