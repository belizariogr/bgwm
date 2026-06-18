use std::path::{Path, PathBuf};

use thiserror::Error;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_SET_VALUE, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

use crate::config::StartupSettings;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const LEGACY_STARTUP_APPROVED_RUN_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const REG_VALUE_NAME: &str = "BGWM";

#[derive(Debug, Error)]
pub enum StartupError {
    #[error("failed to resolve BGWM executable path: {0}")]
    ExecutablePath(#[from] std::io::Error),
    #[error("failed to update Windows startup registration: {0}")]
    Windows(String),
}

pub fn apply(settings: &StartupSettings) -> Result<(), StartupError> {
    if !settings.launch_at_login {
        remove_run_registry()?;
        remove_legacy_startup_approved()?;
        return Ok(());
    }

    let exe = current_exe()?;
    set_run_registry(&exe)?;
    Ok(())
}

fn current_exe() -> Result<PathBuf, StartupError> {
    std::env::current_exe().map_err(StartupError::from)
}

fn startup_command(exe: &Path) -> String {
    format!("\"{}\"", exe.display())
}

fn set_run_registry(exe: &Path) -> Result<(), StartupError> {
    let command = startup_command(exe);
    let value_data = wide_null(&command);
    with_registry_key(RUN_KEY, |key| {
        set_string_value(key, REG_VALUE_NAME, &value_data)
    })
}

fn remove_run_registry() -> Result<(), StartupError> {
    delete_registry_value(RUN_KEY, REG_VALUE_NAME)
}

fn remove_legacy_startup_approved() -> Result<(), StartupError> {
    delete_registry_value(LEGACY_STARTUP_APPROVED_RUN_KEY, REG_VALUE_NAME)
}

fn with_registry_key(
    subkey: &str,
    f: impl FnOnce(HKEY) -> Result<(), StartupError>,
) -> Result<(), StartupError> {
    unsafe {
        let subkey_w = wide_null(subkey);
        let mut key = HKEY::default();
        let open_result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey_w.as_ptr()),
            0,
            KEY_SET_VALUE | KEY_WRITE,
            &mut key,
        );

        let key = if open_result == ERROR_FILE_NOT_FOUND {
            let create_result = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey_w.as_ptr()),
                0,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE | KEY_WRITE,
                None,
                &mut key,
                None,
            );
            if create_result != ERROR_SUCCESS {
                return Err(registry_error("create registry key", create_result));
            }
            key
        } else if open_result != ERROR_SUCCESS {
            return Err(registry_error("open registry key", open_result));
        } else {
            key
        };

        let result = f(key);
        let _ = RegCloseKey(key);
        result
    }
}

fn delete_registry_value(subkey: &str, value_name: &str) -> Result<(), StartupError> {
    unsafe {
        let subkey_w = wide_null(subkey);
        let value_name_w = wide_null(value_name);

        let mut key = HKEY::default();
        let open_result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey_w.as_ptr()),
            0,
            KEY_SET_VALUE | KEY_WRITE,
            &mut key,
        );
        if open_result == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        if open_result != ERROR_SUCCESS {
            return Err(registry_error("open registry key", open_result));
        }

        let delete_result = RegDeleteValueW(key, PCWSTR(value_name_w.as_ptr()));
        if delete_result != ERROR_SUCCESS && !is_not_found(delete_result) {
            let _ = RegCloseKey(key);
            return Err(registry_error("delete registry value", delete_result));
        }

        let _ = RegCloseKey(key);
        Ok(())
    }
}

fn set_string_value(key: HKEY, value_name: &str, value_data: &[u16]) -> Result<(), StartupError> {
    unsafe {
        let value_name_w = wide_null(value_name);
        let set_result = RegSetValueExW(
            key,
            PCWSTR(value_name_w.as_ptr()),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                value_data.as_ptr().cast(),
                value_data.len() * 2,
            )),
        );
        if set_result != ERROR_SUCCESS {
            return Err(registry_error("set registry string value", set_result));
        }
        Ok(())
    }
}

fn wide_null(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn registry_error(action: &str, code: WIN32_ERROR) -> StartupError {
    StartupError::Windows(format!("{action}: {code:?}"))
}

fn is_not_found(code: WIN32_ERROR) -> bool {
    code == ERROR_FILE_NOT_FOUND || code == ERROR_NOT_FOUND
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_command_quotes_path_with_spaces() {
        let command = startup_command(Path::new(r"C:\Program Files\bgwm\bgwm.exe"));
        assert_eq!(command, r#""C:\Program Files\bgwm\bgwm.exe""#);
    }
}
