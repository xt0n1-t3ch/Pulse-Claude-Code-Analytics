#[test]
fn single_instance_plugin_is_registered_before_the_background_poller() {
    let source = include_str!("../src/main.rs");
    let single_instance = source
        .find(".plugin(tauri_plugin_single_instance::init")
        .expect("single-instance plugin registration");
    let setup = source.find(".setup(|app|").expect("Tauri setup");
    let poller = source
        .find("commands::start_background_poller")
        .expect("background poller startup");

    assert!(
        single_instance < setup && setup < poller,
        "a rejected second launch must not reach poller startup"
    );
}
