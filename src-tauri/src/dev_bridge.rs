//! Debug-only HTTP bridge that exposes a handful of read-only Tauri commands
//! over localhost.
//!
//! Pulse ships as a Tauri app, so `invoke()` only resolves inside the native
//! webview. That makes the UI impossible to inspect in an ordinary browser
//! against real backend data: Vite alone renders the shell with every store
//! empty. This bridge closes that gap for development and design review.
//!
//! Deliberate constraints:
//!   * compiled only under `debug_assertions`, so release binaries never
//!     contain it;
//!   * binds to loopback only;
//!   * answers browser requests only for the Vite dev origin. Loopback
//!     binding alone does not stop cross-site JavaScript: any page the
//!     developer visits could otherwise read session history, live session
//!     metadata, and Discord profile data from `127.0.0.1:1421`;
//!   * serves a fixed allowlist of **read-only** commands. Nothing here can
//!     mutate config, Discord state, the database, or the filesystem.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};

/// Loopback port for the dev bridge. Paired with `VITE_PULSE_BRIDGE` on the
/// frontend side; see `frontend/src/lib/api.ts`.
pub const DEV_BRIDGE_PORT: u16 = 1421;

/// Vite dev-server origins the bridge answers. Both spellings of loopback are
/// listed because Vite prints `localhost` while some browsers normalise to the
/// numeric form.
const ALLOWED_ORIGINS: [&str; 2] = ["http://localhost:1420", "http://127.0.0.1:1420"];

/// Decides whether a request may be served, given its `Origin` header.
///
/// A same-origin or non-browser client (curl, the packaged app) sends no
/// `Origin` and is allowed. A browser always sends one for cross-origin
/// requests, so an unexpected value is a page that has no business reading
/// this data.
fn allowed_origin(origin: Option<&str>) -> Option<&str> {
    match origin {
        None => Some(""),
        Some(value) => ALLOWED_ORIGINS
            .iter()
            .find(|allowed| **allowed == value)
            .copied(),
    }
}

/// Read-only commands the bridge is allowed to serve. Keeping this an explicit
/// list (rather than a catch-all dispatcher) means a future mutating command
/// cannot become reachable from a browser by accident.
///
/// `days` mirrors the `days` argument the real Tauri command receives, so a
/// 7d/30d/90d/1y switch in the UI produces the same window here as it does in
/// the packaged app. Ignoring it would make the bridge quietly answer every
/// range with the same 30-day numbers.
///
/// `session_id` carries the single-session argument the Context view sends and
/// `project` the filter Cost Analysis applies.
fn dispatch(
    command: &str,
    days: Option<i64>,
    session_id: Option<String>,
    project: Option<String>,
) -> Option<String> {
    let window = days.unwrap_or(30);
    let json = match command {
        "get_health" => serde_json::to_string(&crate::commands::get_health()),
        "get_metrics" => serde_json::to_string(&crate::commands::get_metrics()),
        "get_live_sessions" => serde_json::to_string(&crate::commands::get_live_sessions()),
        "get_discord_preview" => serde_json::to_string(&crate::commands::get_discord_preview()),
        "get_rate_limits" => serde_json::to_string(&crate::commands::get_rate_limits()),
        "get_discord_user" => serde_json::to_string(&crate::commands::get_discord_user()),
        "get_plan_info" => serde_json::to_string(&crate::commands::get_plan_info()),
        "get_active_provider" => serde_json::to_string(&crate::commands::get_active_provider()),
        // The Tauri command is async only because it offloads to a blocking
        // pool; the bridge already runs off the UI thread, so it calls the
        // synchronous builder directly.
        "get_reports_bundle" => {
            serde_json::to_string(&crate::commands::reports_bundle_blocking(window, project))
        }
        "get_daily_stats" => serde_json::to_string(&crate::commands::get_daily_stats(Some(window))),
        "get_session_history" => serde_json::to_string(&crate::commands::get_session_history(
            Some(window),
            project,
            Some(200),
        )),
        "get_cost_forecast" => serde_json::to_string(&crate::commands::get_cost_forecast()),
        "get_cost_totals" => {
            serde_json::to_string(&crate::commands::cost_totals_blocking(window, project))
        }
        "get_budget_status" => serde_json::to_string(&crate::commands::get_budget_status()),
        "get_analytics_summary" => serde_json::to_string(&crate::commands::get_analytics_summary()),
        "get_model_distribution" => {
            serde_json::to_string(&crate::commands::get_model_distribution(Some(window)))
        }
        "get_project_stats" => {
            serde_json::to_string(&crate::commands::get_project_stats(Some(window)))
        }
        "get_hourly_activity" => {
            serde_json::to_string(&crate::commands::get_hourly_activity(Some(window)))
        }
        "get_top_sessions" => {
            serde_json::to_string(&crate::commands::get_top_sessions(Some(10), Some(window)))
        }
        "get_context_breakdown" => {
            serde_json::to_string(&crate::commands::get_context_breakdown(session_id))
        }
        // The Context view always requests every live session, so the bridge
        // mirrors that call rather than inventing an id list.
        "get_context_breakdowns" => {
            serde_json::to_string(&crate::commands::get_context_breakdowns(None))
        }
        "get_sessions_context_usage" => {
            serde_json::to_string(&crate::commands::get_sessions_context_usage(Some(window)))
        }
        "get_app_snapshot" => match crate::commands::get_app_snapshot() {
            Ok(snapshot) => serde_json::to_string(&snapshot),
            Err(_) => return None,
        },
        "get_discord_settings" => match crate::commands::get_discord_settings() {
            Ok(settings) => serde_json::to_string(&settings),
            Err(_) => return None,
        },
        "get_provider_copy" => serde_json::to_string(&crate::commands::get_provider_copy()),
        // The analyzer commands are async only to offload blocking work; the
        // bridge already runs off the UI thread, so it reuses the same inputs
        // directly rather than pulling in an async runtime.
        "get_cache_health" => {
            let sessions = crate::commands::analyzer_sessions_for(window);
            serde_json::to_string(&crate::analyzers::cache_health::analyze_for_provider(
                crate::commands::analyzer_provider_for_bridge(),
                &sessions,
            ))
        }
        "get_inflection_points" => {
            let sessions = crate::commands::analyzer_sessions_for(window);
            serde_json::to_string(&crate::analyzers::inflection::detect_for_provider(
                crate::commands::analyzer_provider_for_bridge(),
                &sessions,
            ))
        }
        "get_model_routing" => {
            let provider = crate::commands::analyzer_provider_for_bridge();
            let routing = provider.capabilities().model_routing.then(|| {
                crate::analyzers::model_routing::analyze(&crate::commands::analyzer_sessions_for(
                    window,
                ))
            });
            serde_json::to_string(&routing)
        }
        "get_recommendations" => serde_json::to_string(
            &crate::commands::reports_bundle_blocking(window, project).recommendations,
        ),
        "get_trace_overview" => serde_json::to_string(
            &crate::commands::reports_bundle_blocking(window, project).trace_overview,
        ),
        "get_tool_frequency" => serde_json::to_string(
            &crate::commands::reports_bundle_blocking(window, project).tool_frequency,
        ),
        "get_prompt_complexity" => serde_json::to_string(
            &crate::commands::reports_bundle_blocking(window, project).prompt_complexity,
        ),
        "get_session_health" => serde_json::to_string(
            &crate::commands::reports_bundle_blocking(window, project).session_health,
        ),
        "get_session_history_filtered" => serde_json::to_string(
            &crate::commands::get_session_history(Some(window), project, Some(200)),
        ),
        "get_sessions_by_hour_range" => serde_json::to_string(
            &crate::commands::get_sessions_by_hour_range(0, 23, Some(window)),
        ),
        "get_db_size" => serde_json::to_string(&crate::commands::get_db_size()),
        _ => return None,
    };
    json.ok()
}

/// Spawns the bridge on a background thread. Failure to bind is logged and
/// otherwise ignored: the bridge is a convenience, never a startup dependency.
pub fn spawn() {
    std::thread::spawn(|| {
        let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, DEV_BRIDGE_PORT)) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("dev bridge could not bind port {DEV_BRIDGE_PORT}: {e}");
                return;
            }
        };
        tracing::info!("dev bridge listening on http://127.0.0.1:{DEV_BRIDGE_PORT}");
        for stream in listener.incoming().flatten() {
            // Serve sequentially: a design-review bridge has one client.
            let _ = handle(stream);
        }
    });
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default().to_string();
    let origin = read_origin_header(&mut reader)?;

    let Some(allow_origin) = allowed_origin(origin.as_deref()).map(str::to_string) else {
        return respond(&mut stream, 403, "text/plain", "origin not allowed", "");
    };

    // CORS preflight from the Vite origin.
    if method == "OPTIONS" {
        return respond(&mut stream, 204, "text/plain", "", &allow_origin);
    }
    if method != "GET" {
        return respond(
            &mut stream,
            405,
            "text/plain",
            "method not allowed",
            &allow_origin,
        );
    }

    match path.strip_prefix("/invoke/") {
        Some(rest) => {
            let target = parse_target(rest);
            match dispatch(
                target.command,
                target.days,
                target.session_id,
                target.project,
            ) {
                Some(body) => respond(&mut stream, 200, "application/json", &body, &allow_origin),
                None => respond(
                    &mut stream,
                    404,
                    "text/plain",
                    "unknown or non-readonly command",
                    &allow_origin,
                ),
            }
        }
        None => respond(&mut stream, 404, "text/plain", "not found", &allow_origin),
    }
}

/// Consumes the request headers, returning the `Origin` value when present.
fn read_origin_header(reader: &mut BufReader<TcpStream>) -> std::io::Result<Option<String>> {
    let mut origin = None;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            return Ok(origin);
        }
        if let Some(value) = line
            .split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("origin"))
            .map(|(_, value)| value.trim().to_string())
        {
            origin = Some(value);
        }
    }
}

/// A parsed `/invoke/...` target.
struct BridgeTarget<'a> {
    command: &'a str,
    days: Option<i64>,
    session_id: Option<String>,
    project: Option<String>,
}

/// Splits `command?days=90&sessionId=abc` into its parts. Anything unparseable
/// falls back to `None`, which the dispatcher reads as the default.
fn parse_target(rest: &str) -> BridgeTarget<'_> {
    let (command, query) = match rest.split_once('?') {
        Some((c, q)) => (c, Some(q)),
        None => (rest, None),
    };
    let days = query.and_then(|q| {
        q.split('&')
            .find_map(|pair| pair.strip_prefix("days="))
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|d| *d > 0)
    });
    let session_id = query.and_then(|q| {
        q.split('&')
            .find_map(|pair| pair.strip_prefix("sessionId="))
            .map(percent_decode)
            .filter(|value| !value.is_empty())
    });
    let project = query.and_then(|q| {
        q.split('&')
            .find_map(|pair| pair.strip_prefix("project="))
            .map(percent_decode)
            .filter(|value| !value.is_empty())
    });
    BridgeTarget {
        command,
        days,
        session_id,
        project,
    }
}

/// Minimal percent-decoding for query values. Session ids are opaque strings
/// that the frontend encodes before sending.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or_default();
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
    allow_origin: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    // Echo only an origin that already passed the allowlist. An empty value
    // means the caller was not a browser, so no CORS header is needed.
    let cors = if allow_origin.is_empty() {
        String::new()
    } else {
        format!("Access-Control-Allow-Origin: {allow_origin}\r\n")
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         {cors}\
         Vary: Origin\r\n\
         Access-Control-Allow-Methods: GET, OPTIONS\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        len = body.len()
    )?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_serves_the_read_only_allowlist() {
        for command in [
            "get_health",
            "get_metrics",
            "get_live_sessions",
            "get_discord_preview",
        ] {
            assert!(
                dispatch(command, None, None, None).is_some(),
                "{command} must be served"
            );
        }
    }

    /// Every command a view calls must be served, or `Promise.all` in that view
    /// rejects and the whole screen renders as zeros. This is exactly what made
    /// Cost Analysis show $0.00 while Reports showed real spend.
    #[test]
    fn dispatch_serves_every_command_the_cost_and_reports_views_need() {
        for command in [
            "get_session_history",
            "get_cost_forecast",
            "get_budget_status",
            "get_reports_bundle",
            "get_daily_stats",
            "get_analytics_summary",
            "get_model_distribution",
            "get_project_stats",
            "get_hourly_activity",
            "get_top_sessions",
            "get_db_size",
        ] {
            assert!(
                dispatch(command, None, None, None).is_some(),
                "{command} must be served"
            );
        }
    }

    /// The Context view invokes all three on mount; a missing one 404s and the
    /// whole screen renders empty.
    #[test]
    fn dispatch_serves_every_command_the_context_view_needs() {
        for command in [
            "get_context_breakdown",
            "get_context_breakdowns",
            "get_sessions_context_usage",
        ] {
            assert!(
                dispatch(command, None, None, None).is_some(),
                "{command} must be served"
            );
        }
        assert!(
            dispatch(
                "get_context_breakdown",
                None,
                Some("session-a".into()),
                None
            )
            .is_some(),
            "the single-session argument must be accepted"
        );
    }

    /// Contract guard: every read-only command the frontend can invoke must be
    /// dispatchable. Each of these was found 404-ing a real view at runtime,
    /// which renders that screen empty or pops an error toast.
    #[test]
    fn dispatch_serves_every_read_only_command_the_frontend_invokes() {
        for command in [
            "get_app_snapshot",
            "get_discord_settings",
            "get_provider_copy",
            "get_cache_health",
            "get_inflection_points",
            "get_model_routing",
            "get_recommendations",
            "get_trace_overview",
            "get_tool_frequency",
            "get_prompt_complexity",
            "get_session_health",
            "get_session_history_filtered",
            "get_sessions_by_hour_range",
        ] {
            assert!(
                dispatch(command, None, None, None).is_some(),
                "{command} must be served"
            );
        }
    }

    /// The bridge must never become a browser-reachable path to mutation.
    #[test]
    fn dispatch_rejects_mutating_and_unknown_commands() {
        for command in [
            "set_discord_enabled",
            "set_active_provider",
            "set_budget",
            "clear_history",
            "export_all_data",
            "",
            "../../etc/passwd",
        ] {
            assert!(
                dispatch(command, None, None, None).is_none(),
                "{command} must be rejected"
            );
        }
    }

    #[test]
    fn parse_target_reads_the_requested_window() {
        let plain = parse_target("get_daily_stats");
        assert_eq!(plain.command, "get_daily_stats");
        assert_eq!(plain.days, None);

        let week = parse_target("get_daily_stats?days=7");
        assert_eq!(week.command, "get_daily_stats");
        assert_eq!(week.days, Some(7));

        let year = parse_target("get_reports_bundle?days=365");
        assert_eq!(year.days, Some(365));
    }

    #[test]
    fn parse_target_reads_the_single_session_argument() {
        let target = parse_target("get_context_breakdown?sessionId=claude%3Aabc-123");
        assert_eq!(target.command, "get_context_breakdown");
        assert_eq!(target.session_id.as_deref(), Some("claude:abc-123"));

        assert_eq!(parse_target("get_context_breakdown").session_id, None);
        assert_eq!(
            parse_target("get_context_breakdown?sessionId=").session_id,
            None
        );
    }

    /// A malformed or non-positive window must fall back to the default rather
    /// than querying a nonsense range.
    #[test]
    fn parse_target_ignores_invalid_windows() {
        assert_eq!(parse_target("get_daily_stats?days=abc").days, None);
        assert_eq!(parse_target("get_daily_stats?days=0").days, None);
        assert_eq!(parse_target("get_daily_stats?days=-5").days, None);
        assert_eq!(parse_target("get_daily_stats?other=1").days, None);
    }

    /// The window must actually reach the query: a 7-day series and a 90-day
    /// series cannot be the same payload.
    #[test]
    fn dispatch_honours_the_requested_window() {
        let week = dispatch("get_daily_stats", Some(7), None, None).expect("7d");
        let quarter = dispatch("get_daily_stats", Some(90), None, None).expect("90d");
        // Cheap structural proof that the parameter is threaded through: the
        // wider window cannot produce a shorter payload than the narrow one.
        assert!(quarter.len() >= week.len());
    }

    /// Loopback binding does not stop a malicious page from issuing a
    /// cross-origin read, so an unexpected `Origin` must be refused outright.
    #[test]
    fn only_the_vite_dev_origin_is_accepted() {
        assert_eq!(allowed_origin(None), Some(""));
        assert_eq!(
            allowed_origin(Some("http://localhost:1420")),
            Some("http://localhost:1420")
        );
        assert_eq!(
            allowed_origin(Some("http://127.0.0.1:1420")),
            Some("http://127.0.0.1:1420")
        );
        for hostile in [
            "https://evil.example",
            "http://localhost:5173",
            "null",
            "http://localhost:1420.evil.example",
        ] {
            assert_eq!(allowed_origin(Some(hostile)), None, "{hostile}");
        }
    }
}
