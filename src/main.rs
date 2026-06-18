#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let settings_mode = std::env::args().any(|arg| arg == "--settings");

    let result = if settings_mode {
        bgwm::settings::run_standalone().map_err(|e| e.to_string())
    } else {
        bgwm::run().map_err(|e| e.to_string())
    };

    if let Err(e) = result {
        eprintln!("BGWM failed: {e}");
        std::process::exit(1);
    }
}
