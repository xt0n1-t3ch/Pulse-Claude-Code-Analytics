use chrono::{TimeZone, Utc};

use cc_discord_presence::codex::account_usage::parse_rate_limits_response;
use cc_discord_presence::codex::telemetry::limits::RateLimitScope;

const ACCOUNT_RESPONSE: &str = r#"{
  "id": 2,
  "result": {
    "rateLimits": {
      "limitId": "codex",
      "primary": { "usedPercent": 83, "windowDurationMins": 10080, "resetsAt": 1785369546 },
      "credits": { "hasCredits": true, "unlimited": false, "balance": "2014.0899875000" },
      "planType": "pro"
    },
    "rateLimitsByLimitId": {
      "codex": {
        "limitId": "codex",
        "primary": { "usedPercent": 83, "windowDurationMins": 10080, "resetsAt": 1785369546 },
        "credits": { "hasCredits": true, "unlimited": false, "balance": "2014.0899875000" },
        "planType": "pro"
      },
      "codex_bengalfox": {
        "limitId": "codex_bengalfox",
        "limitName": "GPT-5.3-Codex-Spark",
        "primary": { "usedPercent": 0, "windowDurationMins": 10080, "resetsAt": 1785596138 },
        "credits": null,
        "planType": "pro"
      }
    }
  }
}"#;

#[test]
fn account_response_keeps_global_quota_and_credits_coherent() {
    let observed_at = Utc.timestamp_opt(1_785_000_000, 0).single().unwrap();
    let reading = parse_rate_limits_response(ACCOUNT_RESPONSE, observed_at).unwrap();

    assert_eq!(reading.envelopes.len(), 2);
    let global = reading
        .envelopes
        .iter()
        .find(|item| item.scope == RateLimitScope::GlobalCodex)
        .expect("global account quota");
    let weekly = global.limits.primary.as_ref().expect("weekly window");
    assert_eq!(weekly.used_percent, 83.0);
    assert_eq!(weekly.remaining_percent, 17.0);
    assert_eq!(weekly.window_minutes, 10_080);
    assert_eq!(
        global
            .credits
            .as_ref()
            .and_then(|item| item.balance.as_deref()),
        Some("2014.0899875000")
    );
    assert_eq!(global.observed_at, Some(observed_at));
}

#[test]
fn sparse_or_null_response_is_not_presented_as_live_usage() {
    let observed_at = Utc.timestamp_opt(1_785_000_000, 0).single().unwrap();
    let err = parse_rate_limits_response(
        r#"{"id":2,"result":{"rateLimits":{"primary":null},"rateLimitsByLimitId":null}}"#,
        observed_at,
    )
    .expect_err("empty quota response must stay unavailable");

    assert!(err.to_string().contains("no quota windows"));
}

#[test]
fn credits_only_account_response_remains_renderable() {
    let observed_at = Utc.timestamp_opt(1_785_000_000, 0).single().unwrap();
    let reading = parse_rate_limits_response(
        r#"{"id":2,"result":{"rateLimits":{"limitId":"codex","primary":null,"secondary":null,"credits":{"hasCredits":true,"unlimited":false,"balance":"42.50"}},"rateLimitsByLimitId":null}}"#,
        observed_at,
    )
    .expect("credits-only account snapshot should remain available");

    assert_eq!(reading.envelopes.len(), 1);
    assert_eq!(
        reading.envelopes[0]
            .credits
            .as_ref()
            .and_then(|credits| credits.balance.as_deref()),
        Some("42.50")
    );
    assert!(reading.envelopes[0].limits.primary.is_none());
}

#[test]
fn quota_percentage_survives_an_unknown_window_duration() {
    let observed_at = Utc.timestamp_opt(1_785_000_000, 0).single().unwrap();
    let reading = parse_rate_limits_response(
        r#"{"id":2,"result":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":85,"windowDurationMins":null,"resetsAt":null},"credits":null},"rateLimitsByLimitId":null}}"#,
        observed_at,
    )
    .expect("unknown-duration quota should remain available");

    let window = reading.envelopes[0]
        .limits
        .primary
        .as_ref()
        .expect("primary quota");
    assert_eq!(window.used_percent, 85.0);
    assert_eq!(window.remaining_percent, 15.0);
    assert_eq!(window.window_minutes, 0);
}

#[test]
fn map_key_supplies_a_missing_limit_id() {
    let observed_at = Utc.timestamp_opt(1_785_000_000, 0).single().unwrap();
    let reading = parse_rate_limits_response(
        r#"{"id":2,"result":{"rateLimits":{"primary":null},"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":10,"windowDurationMins":10080,"resetsAt":null}}}}}"#,
        observed_at,
    )
    .expect("map key is the canonical limit ID");

    assert_eq!(reading.envelopes[0].limit_id.as_deref(), Some("codex"));
    assert_eq!(reading.envelopes[0].scope, RateLimitScope::GlobalCodex);
}

#[test]
fn individual_spend_limit_is_exposed_as_an_account_window() {
    let observed_at = Utc.timestamp_opt(1_785_000_000, 0).single().unwrap();
    let reading = parse_rate_limits_response(
        r#"{"id":2,"result":{"rateLimits":{"limitId":"workspace","primary":null,"secondary":null,"credits":null,"individualLimit":{"limit":"100.00","used":"25.00","remainingPercent":75,"resetsAt":1785000100}},"rateLimitsByLimitId":null}}"#,
        observed_at,
    )
    .expect("individual spend control is current account usage");

    let envelope = &reading.envelopes[0];
    assert_eq!(envelope.limit_id.as_deref(), Some("workspace:individual"));
    assert!(
        envelope
            .limit_name
            .as_deref()
            .is_some_and(|name| name.contains("25.00 of 100.00"))
    );
    let window = envelope
        .limits
        .primary
        .as_ref()
        .expect("spend limit window");
    assert_eq!(window.used_percent, 25.0);
    assert_eq!(window.remaining_percent, 75.0);
}
