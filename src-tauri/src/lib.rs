pub mod analyzers;
pub mod commands;
pub mod db;
/// Debug-only localhost bridge for browser-based UI review. Never compiled
/// into release builds.
#[cfg(debug_assertions)]
pub mod dev_bridge;
pub mod report;
pub mod report_template;
pub mod update_check;
