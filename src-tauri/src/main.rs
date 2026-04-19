#![windows_subsystem = "windows"]

pub mod analyzers;
mod commands;
pub mod db;
pub mod report;

use tauri::{
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::TrayIconBuilder,
};

const TRAY_ID: &str = "pulse-tray";

fn show_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(false);
    }
}

fn create_or_show_tray(app: &tauri::AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_visible(true);
        return;
    }

    let show = MenuItemBuilder::with_id("show", "Show Pulse")
        .build(app)
        .unwrap();
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app).unwrap();
    let menu = MenuBuilder::new(app)
        .items(&[&show, &quit])
        .build()
        .unwrap();

    let _ = TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("../assets/icon.png"))
        .tooltip("Pulse — Claude Code Analytics")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::DoubleClick { .. } = event {
                show_window(tray.app_handle());
            }
        })
        .build(app);
}

fn main() {
    commands::start_background_poller();
    commands::refresh_usage();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_health,
            commands::get_metrics,
            commands::get_live_sessions,
            commands::get_rate_limits,
            commands::refresh_usage,
            commands::get_discord_user,
            commands::set_discord_enabled,
            commands::set_discord_display_prefs,
            commands::get_plan_info,
            commands::set_plan_override,
            commands::get_session_history,
            commands::get_session_history_filtered,
            commands::get_sessions_by_hour_range,
            commands::search_sessions,
            commands::get_daily_stats,
            commands::get_analytics_summary,
            commands::get_context_breakdown,
            commands::get_project_stats,
            commands::get_hourly_activity,
            commands::get_top_sessions,
            commands::get_cost_forecast,
            commands::get_budget_status,
            commands::set_budget,
            commands::get_model_distribution,
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
            commands::get_prompt_complexity,
            commands::get_session_health,
            commands::copy_fix_prompt,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                create_or_show_tray(window.app_handle());
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to run Pulse");
}
