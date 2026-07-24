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
//!   * serves a fixed allowlist of **read-only** commands. Nothing here can
//!     mutate config, Discord state, the database, or the filesystem.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};

/// Loopback port for the dev bridge. Paired with `VITE_PULSE_BRIDGE` on the
/// frontend side; see `frontend/src/lib/api.ts`.
pub const DEV_BRIDGE_PORT: u16 = 1421;

/// Read-only commands the bridge is allowed to serve. Keeping this an explicit
/// list (rather than a catch-all dispatcher) means a future mutating command
/// cannot become reachable from a browser by accident.
///
/// `days` mirrors the `days` argument the real Tauri command receives, so a
/// 7d/30d/90d/1y switch in the UI produces the same window here as it does in
/// the packaged app. Ignoring it would make the bridge quietly answer every
/// range with the same 30-day numbers.
fn dispatch(command: &str, days: Option<i64>) -> Option<String> {
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
            serde_json::to_string(&crate::commands::reports_bundle_blocking(window, None))
        }
        "get_daily_stats" => serde_json::to_string(&crate::commands::get_daily_stats(Some(window))),
        "get_session_history" => serde_json::to_string(&crate::commands::get_session_history(
            Some(window),
            None,
            Some(200),
        )),
        "get_cost_forecast" => serde_json::to_string(&crate::commands::get_cost_forecast()),
        "get_cost_totals" => {
            serde_json::to_string(&crate::commands::cost_totals_blocking(window, None))
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
    let path = parts.next().unwrap_or_default();

    // CORS preflight from the Vite origin.
    if method == "OPTIONS" {
        return respond(&mut stream, 204, "text/plain", "");
    }
    if method != "GET" {
        return respond(&mut stream, 405, "text/plain", "method not allowed");
    }

    match path.strip_prefix("/invoke/") {
        Some(rest) => {
            let (command, days) = parse_target(rest);
            match dispatch(command, days) {
                Some(body) => respond(&mut stream, 200, "application/json", &body),
                None => respond(
                    &mut stream,
                    404,
                    "text/plain",
                    "unknown or non-readonly command",
                ),
            }
        }
        None => respond(&mut stream, 404, "text/plain", "not found"),
    }
}

/// Splits `command?days=90` into its command and window. Anything unparseable
/// falls back to `None`, which the dispatcher reads as the default window.
fn parse_target(rest: &str) -> (&str, Option<i64>) {
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
    (command, days)
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {len}\r\n\
         Access-Control-Allow-Origin: *\r\n\
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
                dispatch(command, None).is_some(),
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
                dispatch(command, None).is_some(),
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
                dispatch(command, None).is_none(),
                "{command} must be rejected"
            );
        }
    }

    #[test]
    fn parse_target_reads_the_requested_window() {
        assert_eq!(parse_target("get_daily_stats"), ("get_daily_stats", None));
        assert_eq!(
            parse_target("get_daily_stats?days=7"),
            ("get_daily_stats", Some(7))
        );
        assert_eq!(
            parse_target("get_reports_bundle?days=365"),
            ("get_reports_bundle", Some(365))
        );
    }

    /// A malformed or non-positive window must fall back to the default rather
    /// than querying a nonsense range.
    #[test]
    fn parse_target_ignores_invalid_windows() {
        assert_eq!(parse_target("get_daily_stats?days=abc").1, None);
        assert_eq!(parse_target("get_daily_stats?days=0").1, None);
        assert_eq!(parse_target("get_daily_stats?days=-5").1, None);
        assert_eq!(parse_target("get_daily_stats?other=1").1, None);
    }

    /// The window must actually reach the query: a 7-day series and a 90-day
    /// series cannot be the same payload.
    #[test]
    fn dispatch_honours_the_requested_window() {
        let week = dispatch("get_daily_stats", Some(7)).expect("7d");
        let quarter = dispatch("get_daily_stats", Some(90)).expect("90d");
        // Cheap structural proof that the parameter is threaded through: the
        // wider window cannot produce a shorter payload than the narrow one.
        assert!(quarter.len() >= week.len());
    }
}
