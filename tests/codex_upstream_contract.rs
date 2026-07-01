use std::time::{Duration, SystemTime};

use cc_discord_presence::codex::config::PresenceConfig;
use cc_discord_presence::codex::cost::{self, PricingSource, TokenCostBreakdown};
use cc_discord_presence::codex::session::{
    CodexSessionSnapshot, SessionActivityKind, SessionActivitySnapshot, preferred_active_session,
};
use cc_discord_presence::codex::telemetry::limits::RateLimits;
use cc_discord_presence::codex::telemetry::service_tier::{ResolvedServiceTier, ServiceTier};

#[test]
fn codex_upstream_contract_exposes_pulse_facing_modules() {
    let config = PresenceConfig::default();
    let model = "gpt-5.5";
    let fast_tier = ResolvedServiceTier {
        tier: ServiceTier::Fast,
        raw_tier: Some("fast".to_string()),
        observed_at: None,
        source_path: None,
    };

    let display = cc_discord_presence::codex::util::format_model_display(model, None, true);

    assert!(config.effective_client_id().is_some());
    assert_eq!(cost::default_model_context_window(model), Some(400_000));
    assert!(cost::speed_multiplier(model, fast_tier.is_fast()) > 1.0);
    assert!(display.contains("GPT-5.5"), "display: {display}");
    assert!(
        cc_discord_presence::codex::config::codex_home()
            .to_string_lossy()
            .contains(".codex")
    );
}

#[test]
fn codex_upstream_contract_preserves_session_selection_boundary() {
    let now = SystemTime::now();
    let older = snapshot(
        "older",
        now - Duration::from_secs(120),
        SessionActivityKind::WaitingInput,
    );
    let newer = snapshot("newer", now, SessionActivityKind::Thinking);

    let sessions = [older, newer];
    let selected = preferred_active_session(&sessions).expect("selected active session");

    assert_eq!(selected.session_id, "newer");
}

#[test]
fn codex_process_probe_remains_pulse_compatibility_glue() {
    let _ = cc_discord_presence::codex::process::is_opencode_running();
}

fn snapshot(
    session_id: &str,
    last_activity: SystemTime,
    activity_kind: SessionActivityKind,
) -> CodexSessionSnapshot {
    CodexSessionSnapshot {
        session_id: session_id.to_string(),
        cwd: std::env::temp_dir(),
        project_name: "pulse".to_string(),
        git_branch: None,
        originator: None,
        source: None,
        model: Some("gpt-5.5".to_string()),
        reasoning_effort: None,
        approval_policy: None,
        sandbox_policy: None,
        session_total_tokens: None,
        last_turn_tokens: None,
        session_delta_tokens: None,
        input_tokens_total: 0,
        cached_input_tokens_total: 0,
        output_tokens_total: 0,
        last_input_tokens: None,
        last_cached_input_tokens: None,
        last_output_tokens: None,
        total_cost_usd: 0.0,
        cost_breakdown: TokenCostBreakdown::default(),
        pricing_source: PricingSource::Fallback,
        context_window: None,
        limits: RateLimits::default(),
        rate_limit_envelopes: Vec::new(),
        activity: Some(SessionActivitySnapshot {
            kind: activity_kind,
            ..SessionActivitySnapshot::default()
        }),
        started_at: None,
        last_token_event_at: None,
        last_activity,
        source_file: std::env::temp_dir().join(format!("{session_id}.jsonl")),
        is_subagent: false,
    }
}
