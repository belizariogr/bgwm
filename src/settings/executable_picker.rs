use std::mem::size_of;
use std::path::Path;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OPENFILENAMEW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextW, GetWindowThreadProcessId};
use windows::core::PWSTR;

use crate::window_tracking::{executable_for_hwnd, is_main_window};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickableWindow {
    pub title: String,
    pub executable: String,
}

pub fn list_pickable_windows() -> Vec<PickableWindow> {
    let mut windows = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect_pickable_window), LPARAM(&mut windows as *mut _ as isize));
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
        Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    }
}

unsafe extern "system" fn collect_pickable_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = &mut *(lparam.0 as *mut Vec<PickableWindow>);
    if !is_main_window(hwnd) || is_own_process_window(hwnd) {
        return BOOL(1);
    }

    let Some(executable) = executable_for_hwnd(hwnd.0 as isize) else {
        return BOOL(1);
    };
    if executable.eq_ignore_ascii_case("bgwm.exe") {
        return BOOL(1);
    }

    let title = window_title(hwnd).unwrap_or_else(|| "(Untitled)".into());
    windows.push(PickableWindow { title, executable });
    BOOL(1)
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

fn is_own_process_window(hwnd: HWND) -> bool {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid == GetCurrentProcessId()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_pickable_windows_does_not_panic() {
        let _ = list_pickable_windows();
    }
}
