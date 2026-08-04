//! Standalone debug bridge for reviewing Pulse in the Vite browser.
//!
//! This binary starts the real shared poller before serving `/invoke`; it does
//! not seed fixtures or provide a browser-only mock implementation.

#[cfg(debug_assertions)]
fn main() {
    if let Err(error) = pulse::dev_bridge::run() {
        eprintln!("Pulse dev bridge failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(debug_assertions))]
fn main() {
    eprintln!("pulse-dev-bridge is available only in debug builds");
    std::process::exit(1);
}
