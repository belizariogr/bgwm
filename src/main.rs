#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    let settings_mode = std::env::args().any(|arg| arg == "--settings");

    let _single_instance = if settings_mode {
        None
    } else {
        match bgwm::SingleInstance::acquire() {
            Ok(guard) => Some(guard),
            Err(bgwm::SingleInstanceError::AlreadyRunning) => std::process::exit(0),
            Err(e) => {
                eprintln!("BGWM failed: {e}");
                std::process::exit(1);
            }
        }
    };

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
