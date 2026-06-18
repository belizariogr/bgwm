use std::time::SystemTime;

use eframe::egui;

use crate::config::{AppRule, Config, ConfigError};
use crate::virtual_desktop::{self, WORKSPACE_INDEX_BASE};

pub struct SettingsApp {
    config: Config,
    workspace_count: u32,
    status: Option<String>,
    error: Option<String>,
}

impl SettingsApp {
    pub fn new(config: Config) -> Self {
        let workspace_count = virtual_desktop::workspace_count().unwrap_or(4);
        Self {
            config,
            workspace_count,
            status: None,
            error: None,
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::bottom("settings_actions").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Refresh desktop count").clicked() {
                    match virtual_desktop::workspace_count() {
                        Ok(count) => {
                            self.workspace_count = count;
                            self.status = Some(format!("Detected {count} workspaces"));
                            self.error = None;
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                if ui.button("Save & Apply").clicked() {
                    match self.apply() {
                        Ok(()) => {
                            self.status = Some("Settings saved and applied".into());
                            self.error = None;
                        }
                        Err(e) => {
                            self.error = Some(e.to_string());
                            self.status = None;
                        }
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("BGWM Settings");
            ui.label(format!(
                "Workspaces are numbered from {WORKSPACE_INDEX_BASE} (Windows virtual desktops)."
            ));

            if let Some(err) = &self.error {
                ui.colored_label(egui::Color32::RED, err);
            }
            if let Some(status) = &self.status {
                ui.colored_label(egui::Color32::GREEN, status);
            }

            ui.separator();
            ui.label(format!(
                "Virtual desktops detected: {}",
                self.workspace_count
            ));

            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new("Switch hotkeys")
                    .default_open(true)
                    .show(ui, |ui| {
                        for ws in
                            WORKSPACE_INDEX_BASE..=self.workspace_count.max(WORKSPACE_INDEX_BASE)
                        {
                            let key = ws.to_string();
                            let binding = self
                                .config
                                .switch_hotkeys
                                .get(&key)
                                .cloned()
                                .unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.label(format!("Workspace {ws}:"));
                                let mut text = binding;
                                if ui.text_edit_singleline(&mut text).changed() {
                                    if text.trim().is_empty() {
                                        self.config.switch_hotkeys.remove(&key);
                                    } else {
                                        self.config.switch_hotkeys.insert(key.clone(), text);
                                    }
                                }
                            });
                        }
                    });

                egui::CollapsingHeader::new("Move window hotkeys")
                    .default_open(true)
                    .show(ui, |ui| {
                        for ws in
                            WORKSPACE_INDEX_BASE..=self.workspace_count.max(WORKSPACE_INDEX_BASE)
                        {
                            let key = ws.to_string();
                            let binding = self
                                .config
                                .move_hotkeys
                                .get(&key)
                                .cloned()
                                .unwrap_or_default();
                            ui.horizontal(|ui| {
                                ui.label(format!("Move to workspace {ws}:"));
                                let mut text = binding;
                                if ui.text_edit_singleline(&mut text).changed() {
                                    if text.trim().is_empty() {
                                        self.config.move_hotkeys.remove(&key);
                                    } else {
                                        self.config.move_hotkeys.insert(key.clone(), text);
                                    }
                                }
                            });
                        }
                    });

                egui::CollapsingHeader::new("App rules")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label("Map executables to workspaces (e.g. chrome.exe → 1).");
                        let mut remove_idx = None;
                        for (idx, rule) in self.config.app_rules.iter_mut().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label("Executable:");
                                ui.text_edit_singleline(&mut rule.executable);
                                ui.label("Workspace:");
                                ui.add(
                                    egui::DragValue::new(&mut rule.workspace)
                                        .range(WORKSPACE_INDEX_BASE..=99),
                                );
                                if ui.button("Remove").clicked() {
                                    remove_idx = Some(idx);
                                }
                            });
                        }
                        if let Some(idx) = remove_idx {
                            self.config.app_rules.remove(idx);
                        }
                        if ui.button("Add rule").clicked() {
                            self.config.app_rules.push(AppRule {
                                executable: String::new(),
                                workspace: WORKSPACE_INDEX_BASE,
                            });
                        }
                    });
            });
        });
    }
}

impl SettingsApp {
    fn apply(&mut self) -> Result<(), ConfigError> {
        self.config.validate()?;
        crate::config::save(&self.config)?;
        Ok(())
    }
}

/// Runs the settings UI in its own process (separate winit EventLoop).
pub fn run_standalone() -> Result<(), eframe::Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("bgwm=info")),
        )
        .init();

    let config = crate::config::load().map_err(|e| {
        eframe::Error::AppCreation(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )))
    })?;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 520.0])
            .with_title("BGWM Settings"),
        ..Default::default()
    };

    eframe::run_native(
        "BGWM Settings",
        native_options,
        Box::new(|_cc| Ok(Box::new(SettingsApp::new(config)))),
    )
}

pub fn spawn_settings_process() -> Result<(), std::io::Error> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe).arg("--settings").spawn()?;
    Ok(())
}

pub fn config_mtime() -> Option<SystemTime> {
    std::fs::metadata(crate::config::config_path())
        .ok()
        .and_then(|meta| meta.modified().ok())
}
