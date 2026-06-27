use std::mem::size_of;
use std::path::Path;

use windows::core::PWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
};

use crate::window_tracking::{
    executable_for_hwnd, full_process_image_path_for_hwnd, is_main_window,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickableWindow {
    pub title: String,
    pub executable: String,
    pub full_path: String,
}

pub fn list_pickable_windows() -> Vec<PickableWindow> {
    let mut windows = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(collect_pickable_window),
            LPARAM(&mut windows as *mut _ as isize),
        );
    }
    windows.sort_by(|left: &PickableWindow, right: &PickableWindow| {
        left.title
            .to_ascii_lowercase()
            .cmp(&right.title.to_ascii_lowercase())
    });
    windows
}

pub fn pick_executable_file() -> Option<String> {
    unsafe {
        let mut file = [0u16; 260];
        let filter: Vec<u16> = "Executables (*.exe)\0*.exe\0All Files (*.*)\0*.*\0\0"
            .encode_utf16()
            .collect();

        let mut dialog = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: windows::core::PCWSTR(filter.as_ptr()),
            lpstrFile: PWSTR(file.as_mut_ptr()),
            nMaxFile: file.len() as u32,
            Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_EXPLORER,
            ..Default::default()
        };

        if !GetOpenFileNameW(&mut dialog).as_bool() {
            return None;
        }

        let path = String::from_utf16_lossy(&file);
        let path = path.trim_end_matches('\0');
        let path = path.trim();
        (!path.is_empty()).then(|| path.to_owned())
    }
}

unsafe extern "system" fn collect_pickable_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &mut *(lparam.0 as *mut Vec<PickableWindow>);
    if !is_pickable_window(hwnd) {
        return BOOL(1);
    }

    let Some(executable) = executable_for_hwnd(hwnd.0 as isize) else {
        return BOOL(1);
    };
    if executable.eq_ignore_ascii_case("bgwm.exe") {
        return BOOL(1);
    }

    let full_path =
        full_process_image_path_for_hwnd(hwnd.0 as isize).unwrap_or_else(|| executable.clone());
    let title = window_title(hwnd).unwrap_or_else(|| "(Untitled)".into());
    windows.push(PickableWindow {
        title,
        executable,
        full_path,
    });
    BOOL(1)
}

/// Window-picker filter only: shown main windows from every workspace.
/// Hidden windows and Windows shell surfaces are excluded.
fn is_pickable_window(hwnd: HWND) -> bool {
    unsafe {
        if hwnd.0.is_null() || !IsWindow(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
    }

    !is_own_process_window(hwnd) && is_main_window(hwnd) && !is_windows_shell_hwnd(hwnd)
}

fn is_windows_shell_hwnd(hwnd: HWND) -> bool {
    if let Some(class) = window_class(hwnd) {
        if is_shell_window_class(&class) {
            return true;
        }
    }

    let Some(path) = full_process_image_path_for_hwnd(hwnd.0 as isize) else {
        return false;
    };
    let executable = Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&path);
    is_shell_executable(executable, hwnd)
}

fn is_shell_window_class(class: &str) -> bool {
    matches!(
        class,
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "Progman"
            | "WorkerW"
            | "DV2ControlHost"
            | "MultitaskingOverlayFrame"
    )
}

fn is_shell_executable(executable: &str, hwnd: HWND) -> bool {
    if executable.eq_ignore_ascii_case("explorer.exe") {
        return is_shell_explorer_window(hwnd);
    }

    matches!(
        executable.to_ascii_lowercase().as_str(),
        "searchhost.exe" | "shellexperiencehost.exe" | "startmenuexperiencehost.exe"
    )
}

fn is_shell_explorer_window(hwnd: HWND) -> bool {
    match window_class(hwnd).as_deref() {
        Some("CabinetWClass" | "ExploreWClass") => {
            window_title(hwnd).is_none_or(|t| t.trim().is_empty())
        }
        _ => true,
    }
}

fn window_class(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut buffer = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut buffer);
        if len <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..len as usize]))
    }
}

fn is_own_process_window(hwnd: HWND) -> bool {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid == GetCurrentProcessId()
    }
}

fn window_title(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut buffer = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buffer);
        if len <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buffer[..len as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_pickable_windows_does_not_panic() {
        let _ = list_pickable_windows();
    }

    #[test]
    fn shell_window_classes_are_detected() {
        assert!(is_shell_window_class("Progman"));
        assert!(is_shell_window_class("Shell_TrayWnd"));
        assert!(!is_shell_window_class("CabinetWClass"));
    }

    #[test]
    fn shell_host_executables_are_detected() {
        assert!(is_shell_executable(
            "SearchHost.exe",
            HWND(std::ptr::null_mut())
        ));
        assert!(!is_shell_executable(
            "chrome.exe",
            HWND(std::ptr::null_mut())
        ));
    }
}
