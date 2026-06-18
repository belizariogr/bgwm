use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

use crate::hotkeys::Hotkey;

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppRule {
    pub executable: String,
    /// 1-based workspace index shown in the UI.
    pub workspace: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub version: u32,
    /// 1-based workspace index → hotkey string.
    pub switch_hotkeys: HashMap<String, String>,
    /// 1-based workspace index → hotkey string.
    pub move_hotkeys: HashMap<String, String>,
    pub app_rules: Vec<AppRule>,
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
            app_rules: Vec::new(),
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
            if rule.workspace == 0 {
                return Err(ConfigError::Validation(
                    "app rule workspace must be >= 1".into(),
                ));
            }
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

pub fn matches_executable(rule_exe: &str, process_exe: &str) -> bool {
    let rule = rule_exe.trim().to_ascii_lowercase();
    let process = process_exe.trim().to_ascii_lowercase();
    if rule.is_empty() || process.is_empty() {
        return false;
    }
    if rule.contains('\\') || rule.contains('/') {
        return process.ends_with(&rule) || process == rule;
    }
    process
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|name| name == rule)
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
    fn matches_executable_by_path_suffix() {
        assert!(matches_executable(
            r"Google\Chrome\Application\chrome.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        ));
    }
}
