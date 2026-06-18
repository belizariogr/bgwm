pub mod config;
pub mod hotkeys;
pub mod virtual_desktop;
pub mod window_tracking;

mod app;
pub mod settings;
mod tray;

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
