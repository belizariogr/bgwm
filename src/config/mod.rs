mod schema;

pub use schema::{
    is_executable_full_path, matches_executable, AppRule, Config, ConfigError, SettingsWindow,
    StartupSettings,
};

use std::fs;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "config.json";

pub fn config_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("bgwm")
}

pub fn config_path() -> PathBuf {
    config_dir().join(CONFIG_FILE)
}

pub fn load() -> Result<Config, ConfigError> {
    let path = config_path();
    if !path.exists() {
        let config = Config::default();
        save(&config)?;
        return Ok(config);
    }
    let data = fs::read_to_string(&path).map_err(|e| ConfigError::Io {
        path: path.clone(),
        source: e,
    })?;
    let config: Config = serde_json::from_str(&data)?;
    config.validate()?;
    Ok(config)
}

pub fn save(config: &Config) -> Result<(), ConfigError> {
    config.validate()?;
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| ConfigError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    let data = serde_json::to_string_pretty(config)?;
    fs::write(&path, data).map_err(|e| ConfigError::Io {
        path: path.clone(),
        source: e,
    })?;
    Ok(())
}

pub fn load_from_path(path: &Path) -> Result<Config, ConfigError> {
    let data = fs::read_to_string(path).map_err(|e| ConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let config: Config = serde_json::from_str(&data)?;
    config.validate()?;
    Ok(config)
}

pub fn save_to_path(config: &Config, path: &Path) -> Result<(), ConfigError> {
    config.validate()?;
    let data = serde_json::to_string_pretty(config)?;
    fs::write(path, data).map_err(|e| ConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(())
}
