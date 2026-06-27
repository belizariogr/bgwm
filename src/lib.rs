pub mod config;
pub mod font_icons;
pub mod hotkeys;
pub mod virtual_desktop;
pub mod window_tracking;

mod app;
mod process_job;
pub mod settings;
mod single_instance;
mod startup;
mod tray;

pub use single_instance::{SingleInstance, SingleInstanceError};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
