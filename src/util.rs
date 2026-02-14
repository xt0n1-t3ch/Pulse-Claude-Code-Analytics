use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use tracing_subscriber::{fmt, EnvFilter};

pub fn setup_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).without_time().try_init();
}

pub fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

pub fn format_delta_tokens(tokens: u64) -> String {
    format_tokens(tokens)
}

pub fn format_token_triplet(_delta: Option<u64>, last: Option<u64>, total: Option<u64>) -> String {
    let mut parts = Vec::new();
    if let Some(value) = last {
        parts.push(format!("Last response {}", format_tokens(value)));
    }
    if let Some(value) = total {
        parts.push(format!("Session total {}", format_tokens(value)));
    }
    if parts.is_empty() {
        "Tokens: unavailable".to_string()
    } else {
        format!("Tokens: {}", parts.join(" | "))
    }
}

pub fn format_time_until(target: Option<DateTime<Utc>>) -> String {
    let Some(target) = target else {
        return "n/a".to_string();
    };

    let now = Utc::now();
    if target <= now {
        return "now".to_string();
    }

    let delta = (target - now).to_std().unwrap_or_default();
    human_duration(delta)
}

pub fn human_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    let seconds = secs % 60;

    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

pub fn progress_bar(percent: f64, width: usize) -> String {
    let pct = percent.clamp(0.0, 100.0);
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "#".repeat(filled), "-".repeat(empty))
}

pub fn truncate(input: &str, max_len: usize) -> String {
    let char_count = input.chars().count();
    if char_count <= max_len {
        return input.to_string();
    }
    if max_len <= 3 {
        return input.chars().take(max_len).collect();
    }
    let truncated: String = input.chars().take(max_len - 3).collect();
    format!("{truncated}...")
}

pub fn now_local() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn format_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

pub fn format_time_until_reset(reset_time: DateTime<Utc>) -> String {
    let duration = reset_time.signed_duration_since(Utc::now());
    if duration.num_seconds() < 0 {
        return "now".to_string();
    }
    human_duration(duration.to_std().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_formatting() {
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1_500), "1.5K");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn token_triplet_formatting() {
        assert_eq!(
            format_token_triplet(Some(1500), Some(2500), Some(60_000)),
            "Tokens: Last response 2.5K | Session total 60.0K"
        );
        assert_eq!(
            format_token_triplet(None, None, None),
            "Tokens: unavailable"
        );
    }

    #[test]
    fn cost_formatting() {
        assert_eq!(format_cost(0.001), "$0.0010");
        assert_eq!(format_cost(0.042), "$0.042");
        assert_eq!(format_cost(1.50), "$1.50");
        assert_eq!(format_cost(12.345), "$12.35");
    }

    #[test]
    fn duration_formatting() {
        assert_eq!(human_duration(Duration::from_secs(45)), "45s");
        assert_eq!(human_duration(Duration::from_secs(125)), "2m 5s");
        assert_eq!(human_duration(Duration::from_secs(3700)), "1h 1m");
        assert_eq!(human_duration(Duration::from_secs(90_000)), "1d 1h");
    }

    #[test]
    fn truncation() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a very long string here", 10), "a very ...");
        // Unicode: chars should not be split mid-byte
        let unicode = "██████╗██╗ █████╗";
        assert_eq!(truncate(unicode, 50), unicode); // fits
        assert_eq!(truncate(unicode, 10), "██████╗...");
    }

    #[test]
    fn progress_bar_rendering() {
        assert_eq!(progress_bar(50.0, 10), "#####-----");
        assert_eq!(progress_bar(0.0, 10), "----------");
        assert_eq!(progress_bar(100.0, 10), "##########");
    }
}
