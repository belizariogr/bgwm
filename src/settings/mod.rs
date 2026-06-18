use std::time::SystemTime;

use eframe::egui;
use eframe::egui::{Color32, CornerRadius, Margin, RichText, Stroke, Vec2};

use crate::config::{AppRule, Config, ConfigError, SettingsWindow};
use crate::hotkeys::hotkey_help_sections;
use crate::process_job::ChildProcessJob;
use crate::virtual_desktop::{self, WORKSPACE_INDEX_BASE};
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

pub const SETTINGS_WINDOW_TITLE: &str = "BGWM Settings";
const SETTINGS_INSTANCE_MUTEX: &str = "Local\\bgwm-settings";

const ACCENT: Color32 = Color32::from_rgb(84, 192, 235);
const ACCENT_MUTED: Color32 = Color32::from_rgb(76, 219, 196);
const SURFACE: Color32 = Color32::from_rgb(32, 36, 44);
const SURFACE_ELEVATED: Color32 = Color32::from_rgb(40, 45, 54);
const BORDER: Color32 = Color32::from_rgb(56, 62, 74);
const TEXT_MUTED: Color32 = Color32::from_rgb(156, 163, 175);
const SUCCESS: Color32 = Color32::from_rgb(74, 222, 128);
const ERROR: Color32 = Color32::from_rgb(248, 113, 113);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Hotkeys,
    AppRules,
}

pub struct SettingsApp {
    config: Config,
    workspace_count: u32,
    active_tab: SettingsTab,
    status: Option<String>,
    error: Option<String>,
    last_window_size: egui::Vec2,
}

impl SettingsApp {
    pub fn new(config: Config) -> Self {
        let workspace_count = virtual_desktop::workspace_count().unwrap_or(4);
        let last_window_size =
            egui::vec2(config.settings_window.width, config.settings_window.height);
        Self {
            config,
            workspace_count,
            active_tab: SettingsTab::Hotkeys,
            status: None,
            error: None,
            last_window_size,
        }
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(size) = ctx.input(|i| i.viewport().inner_rect.map(|rect| rect.size())) {
            if size.x >= 1.0 && size.y >= 1.0 {
                self.last_window_size = size;
            }
        }

        self.draw_header(ctx);
        self.draw_footer(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(ctx.style().visuals.panel_fill))
            .show(ctx, |ui| {
                let viewport = ui.available_size();
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.set_min_size(viewport);
                        egui::Frame::new()
                            .inner_margin(Margin::symmetric(24, 20))
                            .show(ui, |ui| {
                                match self.active_tab {
                                    SettingsTab::Hotkeys => self.draw_hotkeys_tab(ui),
                                    SettingsTab::AppRules => self.draw_app_rules_tab(ui),
                                }
                            });
                    });
            });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        persist_settings_window_size(self.last_window_size);
    }
}

impl SettingsApp {
    fn draw_header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("settings_header")
            .frame(
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(Margin {
                        left: 0,
                        right: 0,
                        top: 18,
                        bottom: 0,
                    }),
            )
            .show(ctx, |ui| {
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(24, 0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("BGWM Settings")
                                        .size(22.0)
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(format!(
                                        "Manage workspace hotkeys and app routing · numbered from {WORKSPACE_INDEX_BASE}"
                                    ))
                                    .size(13.0)
                                    .color(TEXT_MUTED),
                                );
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                if self.active_tab == SettingsTab::Hotkeys {
                                    hotkey_help_popup(ui);
                                }

                                if let Some(err) = &self.error {
                                    if self.active_tab == SettingsTab::Hotkeys {
                                        ui.add_space(8.0);
                                    }
                                    badge(ui, err.clone(), ERROR);
                                } else if let Some(status) = &self.status {
                                    if self.active_tab == SettingsTab::Hotkeys {
                                        ui.add_space(8.0);
                                    }
                                    badge(ui, status.clone(), SUCCESS);
                                }
                            });
                        });
                    });

                ui.add_space(16.0);
                self.draw_tab_bar(ui);
            });
    }

    fn draw_tab_bar(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.horizontal(|ui| {
            tab_button(ui, &mut self.active_tab, SettingsTab::Hotkeys, "Hotkeys");
            tab_button(ui, &mut self.active_tab, SettingsTab::AppRules, "App rules");
        });
    }

    fn draw_hotkeys_tab(&mut self, ui: &mut egui::Ui) {
        section_card(
            ui,
            "Switch workspace",
            "Press a hotkey to jump to a virtual desktop. Example: Win+2",
            |ui| {
                egui::Grid::new("switch_hotkeys_grid")
                    .num_columns(2)
                    .spacing([20.0, 10.0])
                    .min_col_width(120.0)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(column_header("Workspace"));
                        ui.label(column_header("Hotkey"));
                        ui.end_row();

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

                            ui.label(RichText::new(format!("Workspace {ws}")).color(Color32::WHITE));
                            let mut text = binding;
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut text)
                                    .desired_width(ui.available_width())
                                    .hint_text("e.g. Win+2"),
                            );
                            if response.changed() {
                                if text.trim().is_empty() {
                                    self.config.switch_hotkeys.remove(&key);
                                } else {
                                    self.config.switch_hotkeys.insert(key.clone(), text);
                                }
                            }
                            ui.end_row();
                        }
                    });
            },
        );

        ui.add_space(16.0);

        section_card(
            ui,
            "Move focused window",
            "Move the active window to a workspace and switch to it. Example: Win+Shift+2",
            |ui| {
                egui::Grid::new("move_hotkeys_grid")
                    .num_columns(2)
                    .spacing([20.0, 10.0])
                    .min_col_width(120.0)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(column_header("Workspace"));
                        ui.label(column_header("Hotkey"));
                        ui.end_row();

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

                            ui.label(RichText::new(format!("Workspace {ws}")).color(Color32::WHITE));
                            let mut text = binding;
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut text)
                                    .desired_width(ui.available_width())
                                    .hint_text("e.g. Win+Shift+2"),
                            );
                            if response.changed() {
                                if text.trim().is_empty() {
                                    self.config.move_hotkeys.remove(&key);
                                } else {
                                    self.config.move_hotkeys.insert(key.clone(), text);
                                }
                            }
                            ui.end_row();
                        }
                    });
            },
        );
    }

    fn draw_app_rules_tab(&mut self, ui: &mut egui::Ui) {
        section_card(
            ui,
            "Launch routing",
            "When an app opens, move its main window to the chosen workspace and switch to it.",
            |ui| {
                if self.config.app_rules.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("No rules yet")
                                .size(15.0)
                                .strong()
                                .color(TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new("Add an executable such as chrome.exe to route new windows.")
                                .size(13.0)
                                .color(TEXT_MUTED),
                        );
                        ui.add_space(12.0);
                    });
                } else {
                    ui.horizontal(|ui| {
                        let field_width = (ui.available_width() - 180.0).max(360.0);
                        ui.add_sized(
                            [field_width, ui.spacing().interact_size.y],
                            egui::Label::new(column_header("Executable")),
                        );
                        ui.label(column_header("Workspace"));
                    });
                    ui.add_space(6.0);

                    let mut remove_idx = None;
                    for (idx, rule) in self.config.app_rules.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let field_width = (ui.available_width() - 180.0).max(360.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut rule.executable)
                                    .desired_width(field_width)
                                    .hint_text("chrome.exe"),
                            );
                            ui.add(
                                egui::DragValue::new(&mut rule.workspace)
                                    .range(WORKSPACE_INDEX_BASE..=99)
                                    .prefix("WS "),
                            );
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Remove").size(12.0).color(ERROR),
                                    )
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(Stroke::new(1.0, BORDER)),
                                )
                                .clicked()
                            {
                                remove_idx = Some(idx);
                            }
                        });
                        ui.add_space(4.0);
                    }

                    if let Some(idx) = remove_idx {
                        self.config.app_rules.remove(idx);
                    }
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new("+ Add rule").color(Color32::WHITE),
                            )
                            .fill(SURFACE_ELEVATED)
                            .stroke(Stroke::new(1.0, BORDER)),
                        )
                        .clicked()
                    {
                        self.config.app_rules.push(AppRule {
                            executable: String::new(),
                            workspace: WORKSPACE_INDEX_BASE,
                        });
                    }
                });
            },
        );
    }

    fn draw_footer(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("settings_actions")
            .frame(
                egui::Frame::new()
                    .fill(SURFACE)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(Margin::symmetric(24, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary_button(ui, "Save & Apply").clicked() {
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

                        ui.add_space(8.0);

                        if secondary_button(ui, "Refresh desktop count").clicked() {
                            match virtual_desktop::workspace_count() {
                                Ok(count) => {
                                    self.workspace_count = count;
                                    self.status = Some(format!("Detected {count} workspaces"));
                                    self.error = None;
                                }
                                Err(e) => self.error = Some(e.to_string()),
                            }
                        }
                    });
                });
            });
    }

    fn apply(&mut self) -> Result<(), ConfigError> {
        self.config.settings_window =
            SettingsWindow::clamp(self.last_window_size.x, self.last_window_size.y);
        self.config.validate()?;
        crate::config::save(&self.config)?;
        Ok(())
    }
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(24, 27, 33);
    visuals.window_fill = Color32::from_rgb(24, 27, 33);
    visuals.extreme_bg_color = Color32::from_rgb(18, 20, 24);
    visuals.faint_bg_color = SURFACE_ELEVATED;
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.inactive.bg_fill = SURFACE_ELEVATED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(48, 54, 64);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.45));
    visuals.widgets.active.bg_fill = Color32::from_rgb(54, 60, 72);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.65));
    visuals.widgets.inactive.corner_radius = CornerRadius::same(8);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);
    visuals.widgets.active.corner_radius = CornerRadius::same(8);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = Color32::from_rgb(251, 191, 36);
    visuals.error_fg_color = ERROR;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(14.0, 8.0);
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(18.0, egui::FontFamily::Proportional),
    );
    ctx.set_style(style);
}

fn section_card(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::same(18))
        .show(ui, |ui| {
            ui.label(
                RichText::new(title)
                    .size(16.0)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.add_space(4.0);
            ui.label(RichText::new(subtitle).size(13.0).color(TEXT_MUTED));
            ui.add_space(14.0);
            ui.separator();
            ui.add_space(12.0);
            add_contents(ui);
        });
}

fn tab_button(ui: &mut egui::Ui, active: &mut SettingsTab, tab: SettingsTab, label: &str) {
    let selected = *active == tab;
    let fill = if selected {
        ACCENT.gamma_multiply(0.22)
    } else {
        Color32::TRANSPARENT
    };
    let stroke = if selected {
        Stroke::new(1.0, ACCENT.gamma_multiply(0.8))
    } else {
        Stroke::new(1.0, BORDER)
    };
    let text_color = if selected {
        Color32::WHITE
    } else {
        TEXT_MUTED
    };

    if ui
        .add(
            egui::Button::new(RichText::new(label).strong().color(text_color))
                .fill(fill)
                .stroke(stroke)
                .corner_radius(CornerRadius {
                    nw: 8,
                    ne: 8,
                    sw: 0,
                    se: 0,
                }),
        )
        .clicked()
    {
        *active = tab;
    }
}

fn badge(ui: &mut egui::Ui, text: String, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.55)))
        .corner_radius(CornerRadius::same(24))
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).strong().color(color));
        });
}

fn column_header(text: &str) -> RichText {
    RichText::new(text).size(12.0).strong().color(TEXT_MUTED)
}

fn hotkey_help_button(ui: &mut egui::Ui) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new("?").size(15.0).strong().color(ACCENT))
            .fill(SURFACE_ELEVATED)
            .stroke(Stroke::new(1.0, ACCENT.gamma_multiply(0.5)))
            .corner_radius(CornerRadius::same(14))
            .min_size(Vec2::splat(28.0)),
    )
    .on_hover_text("Available hotkey tokens")
}

fn hotkey_help_popup(ui: &mut egui::Ui) {
    let popup_id = ui.id().with("hotkey_help_popup");
    let help_response = hotkey_help_button(ui);
    if help_response.clicked() {
        ui.memory_mut(|mem| mem.toggle_popup(popup_id));
    }

    egui::popup::popup_below_widget(
        ui,
        popup_id,
        &help_response,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(360.0);
            ui.label(
                RichText::new("Hotkey reference")
                    .size(15.0)
                    .strong()
                    .color(Color32::WHITE),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new("Combine tokens with +. Example: Win+Shift+2")
                    .size(12.0)
                    .color(TEXT_MUTED),
            );
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            for section in hotkey_help_sections() {
                ui.label(
                    RichText::new(section.title)
                        .size(13.0)
                        .strong()
                        .color(ACCENT_MUTED),
                );
                ui.add_space(6.0);

                for entry in section.entries {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(entry.primary).color(Color32::WHITE));
                        if !entry.aliases.is_empty() {
                            ui.label(
                                RichText::new(format!("({})", entry.aliases.join(", ")))
                                    .size(12.0)
                                    .color(TEXT_MUTED),
                            );
                        }
                    });
                }

                ui.add_space(10.0);
            }
        },
    );
}

fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).strong().color(Color32::from_rgb(16, 24, 32)))
            .fill(ACCENT)
            .stroke(Stroke::new(1.0, ACCENT_MUTED.gamma_multiply(0.6))),
    )
}

fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(Color32::WHITE))
            .fill(SURFACE_ELEVATED)
            .stroke(Stroke::new(1.0, BORDER)),
    )
}

fn persist_settings_window_size(size: egui::Vec2) {
    if size.x < 1.0 || size.y < 1.0 {
        return;
    }

    let clamped = SettingsWindow::clamp(size.x, size.y);
    match crate::config::load() {
        Ok(mut config) => {
            if config.settings_window == clamped {
                return;
            }
            config.settings_window = clamped;
            if let Err(e) = crate::config::save(&config) {
                tracing::warn!("failed to save settings window size: {e}");
            }
        }
        Err(e) => tracing::warn!("failed to load config for settings window size: {e}"),
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

    let Some(_instance_lock) = acquire_settings_instance_lock() else {
        focus_settings_window();
        return Ok(());
    };

    let config = crate::config::load().map_err(|e| {
        eframe::Error::AppCreation(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e.to_string(),
        )))
    })?;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([
                config.settings_window.width,
                config.settings_window.height,
            ])
            .with_title(SETTINGS_WINDOW_TITLE),
        ..Default::default()
    };

    eframe::run_native(
        SETTINGS_WINDOW_TITLE,
        native_options,
        Box::new(|cc| {
            apply_theme(&cc.egui_ctx);
            Ok(Box::new(SettingsApp::new(config)))
        }),
    )
}

struct SettingsInstanceLock {
    handle: HANDLE,
}

impl Drop for SettingsInstanceLock {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}

fn acquire_settings_instance_lock() -> Option<SettingsInstanceLock> {
    unsafe {
        let name: Vec<u16> = SETTINGS_INSTANCE_MUTEX
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = CreateMutexW(None, true, PCWSTR(name.as_ptr())).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            return None;
        }
        Some(SettingsInstanceLock { handle })
    }
}

pub fn focus_settings_window() -> bool {
    virtual_desktop::focus_window_by_title(SETTINGS_WINDOW_TITLE)
}

pub fn open_settings(job: &ChildProcessJob) -> Result<(), std::io::Error> {
    if focus_settings_window() {
        return Ok(());
    }
    spawn_settings_process(job)
}

pub fn spawn_settings_process(job: &ChildProcessJob) -> Result<(), std::io::Error> {
    let exe = std::env::current_exe()?;
    let child = std::process::Command::new(exe).arg("--settings").spawn()?;
    job.assign_child(&child);
    Ok(())
}

pub fn config_mtime() -> Option<SystemTime> {
    std::fs::metadata(crate::config::config_path())
        .ok()
        .and_then(|meta| meta.modified().ok())
}
