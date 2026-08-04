#![windows_subsystem = "windows"]

use pulse::{commands, update_check};

use tauri::{
    Emitter, Listener, Manager,
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};

const TRAY_ID: &str = "pulse-tray";
const OPEN_NOTIFICATIONS_EVENT: &str = "pulse://open-notifications";

fn pulse_icon() -> Image<'static> {
    tauri::include_image!("../assets/icon.png")
}

fn show_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_title("Pulse");
        let _ = win.set_icon(pulse_icon());
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(false);
    }
}

fn unread_notification_count() -> Result<u32, String> {
    let connection =
        pulse::notifications::open_default_database().map_err(|error| error.to_string())?;
    pulse::notifications::NotificationStore::new(&connection)
        .unread_count()
        .map_err(|error| error.to_string())
}

fn build_tray_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    unread_count: u32,
) -> tauri::Result<tauri::menu::Menu<R>> {
    let show = MenuItemBuilder::with_id("show", "Show Pulse").build(app)?;
    let notifications = MenuItemBuilder::with_id(
        "notifications",
        pulse::notifications::tray_presentation(unread_count).menu_label,
    )
    .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    MenuBuilder::new(app)
        .items(&[&show, &notifications, &quit])
        .build()
}

fn refresh_tray_presentation(app: &tauri::AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let unread_count = match unread_notification_count() {
        Ok(count) => count,
        Err(error) => {
            tracing::warn!(%error, "failed to read Pulse unread notification count; preserving tray state");
            return;
        }
    };
    let presentation = pulse::notifications::tray_presentation(unread_count);
    if presentation.capabilities.tooltip {
        let _ = tray.set_tooltip(Some(presentation.tooltip.as_str()));
    }
    if presentation.capabilities.title {
        let _ = tray.set_title(Some(presentation.title.as_str()));
    }
    match build_tray_menu(app, unread_count) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(error) => tracing::warn!(%error, "failed to refresh Pulse tray menu"),
    }
    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_badge_label(presentation.badge_text);
    }
}

fn create_or_show_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        tray.set_visible(true)?;
        refresh_tray_presentation(app);
        return Ok(());
    }

    let unread_count = match unread_notification_count() {
        Ok(count) => count,
        Err(error) => {
            tracing::warn!(%error, "failed to read Pulse unread notification count; creating degraded tray");
            0
        }
    };
    let menu = build_tray_menu(app, unread_count)?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(pulse_icon())
        .tooltip("Pulse")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_window(app),
            "notifications" => {
                show_window(app);
                if let Err(error) = app.emit(OPEN_NOTIFICATIONS_EVENT, ()) {
                    tracing::warn!(%error, "failed to open Pulse notification center");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                show_window(tray.app_handle());
            }
        })
        .build(app)?;
    refresh_tray_presentation(app);
    Ok(())
}

fn main() {
    // Debug builds also serve the read-only dev bridge so the UI can be
    // reviewed in a browser against real backend data.
    #[cfg(debug_assertions)]
    pulse::dev_bridge::spawn();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        // Native Windows/macOS/Linux notifications are delivered by the
        // official Tauri plugin; the notification center persists its own
        // records so delivery failures never erase unread state.
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        // In-app updates: downloads and installs signed releases without
        // sending the user to the GitHub releases page.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // Register single-instance before setup starts the poller. A second
        // launch is consumed by the plugin and routed to the existing window,
        // so it cannot create a duplicate analytics/notification producer.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app);
        }))
        .setup(|app| {
            commands::start_background_poller(app.handle().clone());
            commands::refresh_usage();
            let tray_app = app.handle().clone();
            app.listen("pulse://snapshot", move |_| {
                refresh_tray_presentation(&tray_app);
            });
            let notification_tray_app = app.handle().clone();
            app.listen("pulse://notification", move |_| {
                refresh_tray_presentation(&notification_tray_app);
            });
            let notification_state_tray_app = app.handle().clone();
            app.listen("pulse://notification-state-changed", move |_| {
                refresh_tray_presentation(&notification_state_tray_app);
            });
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_title("Pulse");
                let _ = win.set_icon(pulse_icon());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_health,
            commands::get_app_snapshot,
            commands::get_access_snapshot,
            commands::get_notifications,
            commands::get_unread_notification_count,
            commands::mark_notification_read,
            commands::mark_all_notifications_read,
            commands::dismiss_notification,
            commands::get_metrics,
            commands::get_live_sessions,
            commands::get_discord_preview,
            commands::get_discord_settings,
            commands::get_rate_limits,
            commands::refresh_usage,
            commands::get_discord_user,
            commands::set_discord_enabled,
            commands::set_discord_display_prefs,
            commands::set_discord_field_order,
            commands::set_codex_desktop_design,
            commands::get_plan_info,
            commands::set_plan_override,
            commands::get_active_provider,
            commands::get_provider_copy,
            commands::set_active_provider,
            commands::get_session_history,
            commands::get_session_history_filtered,
            commands::get_sessions_by_hour_range,
            commands::search_sessions,
            commands::get_daily_stats,
            commands::get_analytics_summary,
            commands::get_context_breakdown,
            commands::get_context_breakdowns,
            commands::get_sessions_context_usage,
            commands::get_project_stats,
            commands::get_hourly_activity,
            commands::get_top_sessions,
            commands::get_cost_forecast,
            commands::get_cost_totals,
            commands::get_budget_status,
            commands::set_budget,
            commands::get_model_distribution,
            commands::get_model_distribution_v2,
            commands::export_all_data,
            commands::clear_history,
            commands::get_db_size,
            commands::generate_html_report,
            commands::generate_markdown_report,
            commands::get_cache_health,
            commands::get_recommendations,
            commands::get_inflection_points,
            commands::get_model_routing,
            commands::get_tool_frequency,
            commands::get_trace_overview,
            commands::get_prompt_complexity,
            commands::get_session_health,
            commands::copy_fix_prompt,
            commands::get_reports_bundle,
            update_check::check_app_update,
            update_check::open_app_release_page,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                match create_or_show_tray(window.app_handle()) {
                    Ok(()) => {
                        let _ = window.hide();
                    }
                    Err(error) => {
                        tracing::warn!(%error, "failed to create Pulse tray; keeping window visible");
                        let _ = window.show();
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run Pulse");
}

#[cfg(test)]
mod tests {
    use super::OPEN_NOTIFICATIONS_EVENT;

    #[test]
    fn tray_notifications_have_a_dedicated_open_event() {
        assert_eq!(OPEN_NOTIFICATIONS_EVENT, "pulse://open-notifications");
    }
}
