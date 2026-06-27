use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

use crate::hotkeys::Hotkey;

pub const CONFIG_VERSION: u32 = 1;

const MIN_SETTINGS_WINDOW_WIDTH: f32 = 480.0;
const MIN_SETTINGS_WINDOW_HEIGHT: f32 = 360.0;
const MAX_SETTINGS_WINDOW_WIDTH: f32 = 3840.0;
const MAX_SETTINGS_WINDOW_HEIGHT: f32 = 2160.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingsWindow {
    pub width: f32,
    pub height: f32,
}

impl Default for SettingsWindow {
    fn default() -> Self {
        Self {
            width: 820.0,
            height: 620.0,
        }
    }
}

impl SettingsWindow {
    pub fn clamp(width: f32, height: f32) -> Self {
        Self {
            width: width.clamp(MIN_SETTINGS_WINDOW_WIDTH, MAX_SETTINGS_WINDOW_WIDTH),
            height: height.clamp(MIN_SETTINGS_WINDOW_HEIGHT, MAX_SETTINGS_WINDOW_HEIGHT),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StartupSettings {
    #[serde(default)]
    pub launch_at_login: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppRule {
    /// Full path to the executable (legacy configs may store only the `.exe` name).
    pub executable: String,
    /// 1-based workspace index shown in the UI. `None` disables launch routing.
    pub workspace: Option<u32>,
    /// Hotkey to launch or focus this app. Empty string disables the binding.
    #[serde(default)]
    pub launch_hotkey: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub version: u32,
    /// 1-based workspace index → hotkey string.
    pub switch_hotkeys: HashMap<String, String>,
    /// 1-based workspace index → hotkey string.
    pub move_hotkeys: HashMap<String, String>,
    /// 1-based workspace index → icon spec (`"<style>:<name>"`) shown in the tray.
    /// Missing/empty means the workspace number is rendered instead.
    #[serde(default)]
    pub workspace_icons: HashMap<String, String>,
    pub app_rules: Vec<AppRule>,
    #[serde(default)]
    pub settings_window: SettingsWindow,
    #[serde(default)]
    pub startup: StartupSettings,
}

impl Default for Config {
    fn default() -> Self {
        let mut switch_hotkeys = HashMap::new();
        let mut move_hotkeys = HashMap::new();
        for i in 1..=9 {
            switch_hotkeys.insert(i.to_string(), format!("Win+{i}"));
            move_hotkeys.insert(i.to_string(), format!("Win+Shift+{i}"));
        }
        Self {
            version: CONFIG_VERSION,
            switch_hotkeys,
            move_hotkeys,
            workspace_icons: HashMap::new(),
            app_rules: Vec::new(),
            settings_window: SettingsWindow::default(),
            startup: StartupSettings::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config validation failed: {0}")]
    Validation(String),
    #[error("failed to read/write {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

impl Config {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != CONFIG_VERSION {
            return Err(ConfigError::Validation(format!(
                "unsupported config version {} (expected {CONFIG_VERSION})",
                self.version
            )));
        }

        let mut seen = Vec::new();
        for (ws, binding) in &self.switch_hotkeys {
            validate_workspace_key(ws)?;
            if binding.trim().is_empty() {
                continue;
            }
            let hotkey = Hotkey::parse(binding).map_err(|e| {
                ConfigError::Validation(format!("invalid switch hotkey for workspace {ws}: {e}"))
            })?;
            if seen.contains(&hotkey) {
                return Err(ConfigError::Validation(format!(
                    "duplicate hotkey binding: {binding}"
                )));
            }
            seen.push(hotkey);
        }

        for (ws, binding) in &self.move_hotkeys {
            validate_workspace_key(ws)?;
            if binding.trim().is_empty() {
                continue;
            }
            let hotkey = Hotkey::parse(binding).map_err(|e| {
                ConfigError::Validation(format!("invalid move hotkey for workspace {ws}: {e}"))
            })?;
            if seen.contains(&hotkey) {
                return Err(ConfigError::Validation(format!(
                    "duplicate hotkey binding: {binding}"
                )));
            }
            seen.push(hotkey);
        }

        for rule in &self.app_rules {
            if rule.executable.trim().is_empty() {
                return Err(ConfigError::Validation(
                    "app rule executable cannot be empty".into(),
                ));
            }
            if rule.workspace == Some(0) {
                return Err(ConfigError::Validation(
                    "app rule workspace must be >= 1 when set".into(),
                ));
            }
            if rule.workspace.is_none() && rule.launch_hotkey.trim().is_empty() {
                return Err(ConfigError::Validation(format!(
                    "app rule for {} must define a workspace and/or launch hotkey",
                    rule.executable
                )));
            }
            if rule.launch_hotkey.trim().is_empty() {
                continue;
            }
            let hotkey = Hotkey::parse(&rule.launch_hotkey).map_err(|e| {
                ConfigError::Validation(format!(
                    "invalid launch hotkey for {}: {e}",
                    rule.executable
                ))
            })?;
            if seen.contains(&hotkey) {
                return Err(ConfigError::Validation(format!(
                    "duplicate hotkey binding: {}",
                    rule.launch_hotkey
                )));
            }
            seen.push(hotkey);
        }

        for ws in self.workspace_icons.keys() {
            validate_workspace_key(ws)?;
        }

        if self.settings_window.width < MIN_SETTINGS_WINDOW_WIDTH
            || self.settings_window.width > MAX_SETTINGS_WINDOW_WIDTH
            || self.settings_window.height < MIN_SETTINGS_WINDOW_HEIGHT
            || self.settings_window.height > MAX_SETTINGS_WINDOW_HEIGHT
        {
            return Err(ConfigError::Validation(
                "settings window size out of allowed range".into(),
            ));
        }

        Ok(())
    }

    pub fn switch_bindings(&self) -> Result<Vec<(u32, Hotkey)>, ConfigError> {
        let mut out = Vec::new();
        for (ws, binding) in &self.switch_hotkeys {
            if binding.trim().is_empty() {
                continue;
            }
            let ws: u32 = ws
                .parse()
                .map_err(|_| ConfigError::Validation(format!("invalid workspace key: {ws}")))?;
            let hotkey = Hotkey::parse(binding).map_err(|e| {
                ConfigError::Validation(format!("invalid switch hotkey for workspace {ws}: {e}"))
            })?;
            out.push((ws, hotkey));
        }
        out.sort_by_key(|(ws, _)| *ws);
        Ok(out)
    }

    pub fn move_bindings(&self) -> Result<Vec<(u32, Hotkey)>, ConfigError> {
        let mut out = Vec::new();
        for (ws, binding) in &self.move_hotkeys {
            if binding.trim().is_empty() {
                continue;
            }
            let ws: u32 = ws
                .parse()
                .map_err(|_| ConfigError::Validation(format!("invalid workspace key: {ws}")))?;
            let hotkey = Hotkey::parse(binding).map_err(|e| {
                ConfigError::Validation(format!("invalid move hotkey for workspace {ws}: {e}"))
            })?;
            out.push((ws, hotkey));
        }
        out.sort_by_key(|(ws, _)| *ws);
        Ok(out)
    }

    /// Icon spec (`"<style>:<name>"`) configured for a 1-based workspace, if any.
    pub fn workspace_icon(&self, workspace: u32) -> Option<&str> {
        self.workspace_icons
            .get(&workspace.to_string())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
    }

    pub fn launch_bindings(&self) -> Result<Vec<(String, Hotkey)>, ConfigError> {
        let mut out = Vec::new();
        for rule in &self.app_rules {
            if rule.launch_hotkey.trim().is_empty() {
                continue;
            }
            let hotkey = Hotkey::parse(&rule.launch_hotkey).map_err(|e| {
                ConfigError::Validation(format!(
                    "invalid launch hotkey for {}: {e}",
                    rule.executable
                ))
            })?;
            out.push((rule.executable.clone(), hotkey));
        }
        out.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(out)
    }
}

fn validate_workspace_key(key: &str) -> Result<(), ConfigError> {
    let ws: u32 = key
        .parse()
        .map_err(|_| ConfigError::Validation(format!("invalid workspace key: {key}")))?;
    if ws == 0 {
        return Err(ConfigError::Validation(
            "workspace index must be >= 1".into(),
        ));
    }
    Ok(())
}

pub fn executable_basename(path_or_name: &str) -> String {
    path_or_name
        .trim()
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn is_executable_full_path(path_or_name: &str) -> bool {
    let trimmed = path_or_name.trim();
    trimmed.contains('\\') || trimmed.contains('/')
}

fn normalize_executable_path(path: &str) -> String {
    path.trim().replace('/', "\\").to_ascii_lowercase()
}

/// Matches a configured rule against a running process executable path.
///
/// When the rule stores a full path, only that exact executable matches.
/// When the rule stores only a file name, any process with the same file name matches.
pub fn matches_executable(rule_exe: &str, process_path: &str) -> bool {
    let rule = rule_exe.trim();
    if rule.is_empty() {
        return false;
    }

    if is_executable_full_path(rule) {
        is_executable_full_path(process_path)
            && normalize_executable_path(rule) == normalize_executable_path(process_path)
    } else {
        let rule_base = executable_basename(rule);
        let process_base = executable_basename(process_path);
        !rule_base.is_empty() && rule_base == process_base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trip() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let loaded: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config, loaded);
        loaded.validate().unwrap();
    }

    #[test]
    fn rejects_duplicate_hotkeys() {
        let mut config = Config::default();
        config.move_hotkeys.insert("1".into(), "Win+1".into());
        assert!(config.validate().is_err());
    }

    #[test]
    fn matches_executable_by_name() {
        assert!(matches_executable(
            "chrome.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        ));
        assert!(!matches_executable("firefox.exe", "chrome.exe"));
    }

    #[test]
    fn matches_executable_full_path_exact_only() {
        let chrome = r"C:\Program Files\Google\Chrome\Application\chrome.exe";
        let chromium = r"C:\Tools\chromium\chrome.exe";
        assert!(matches_executable(chrome, chrome));
        assert!(!matches_executable(chrome, chromium));
        assert!(!matches_executable(chrome, "chrome.exe"));
    }

    #[test]
    fn matches_executable_full_path_case_insensitive() {
        assert!(matches_executable(
            r"C:\Apps\MyApp.exe",
            r"c:\apps\myapp.exe"
        ));
    }

    #[test]
    fn matches_executable_filename_matches_any_path() {
        assert!(matches_executable(
            "chrome.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        ));
        assert!(matches_executable(
            "chrome.exe",
            r"C:\Tools\chromium\chrome.exe"
        ));
    }

    #[test]
    fn launch_only_rule_without_workspace_is_valid() {
        let mut config = Config::default();
        config.app_rules.push(AppRule {
            executable: r"C:\Apps\launch.exe".into(),
            workspace: None,
            launch_hotkey: "Win+Alt+L".into(),
        });
        config.validate().unwrap();
    }

    #[test]
    fn rule_without_workspace_or_launch_hotkey_is_invalid() {
        let mut config = Config::default();
        config.app_rules.push(AppRule {
            executable: r"C:\Apps\orphan.exe".into(),
            workspace: None,
            launch_hotkey: String::new(),
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn launch_hotkey_must_be_unique() {
        let mut config = Config::default();
        config.app_rules.push(AppRule {
            executable: r"C:\Apps\a.exe".into(),
            workspace: Some(1),
            launch_hotkey: "Win+A".into(),
        });
        config.app_rules.push(AppRule {
            executable: r"C:\Apps\b.exe".into(),
            workspace: Some(2),
            launch_hotkey: "Win+A".into(),
        });
        assert!(config.validate().is_err());
    }

    #[test]
    fn settings_window_defaults_when_missing_from_json() {
        let json = r#"{"version":1,"switch_hotkeys":{},"move_hotkeys":{},"app_rules":[]}"#;
        let config: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config.settings_window, SettingsWindow::default());
        assert_eq!(config.startup, StartupSettings::default());
        config.validate().unwrap();
    }
}
