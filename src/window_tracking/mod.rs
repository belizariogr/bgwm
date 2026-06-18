mod watcher;

pub use watcher::{AppWindowEvent, WindowWatcher};

use std::collections::HashSet;
use std::path::Path;
use windows::Win32::Foundation::{BOOL, CloseHandle, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId,
    IsWindowVisible, GWL_EXSTYLE, GWL_STYLE, GW_OWNER, WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::config::matches_executable;

const MAIN_WINDOW_MIN_WIDTH: i32 = 100;
const MAIN_WINDOW_MIN_HEIGHT: i32 = 100;

pub fn process_id_for_hwnd(hwnd: isize) -> Option<u32> {
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        (pid != 0).then_some(pid)
    }
}

pub fn is_window_valid(hwnd: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::IsWindow;

    let hwnd = HWND(hwnd as *mut _);
    unsafe { IsWindow(hwnd).as_bool() }
}

/// Process IDs that already had main windows when BGWM started (excluded from app routing).
/// Includes windows on other virtual desktops (not visible from the current desktop).
pub fn existing_main_window_pids() -> HashSet<u32> {
    let mut pids = HashSet::new();
    unsafe {
        let lparam = LPARAM(&mut pids as *mut _ as isize);
        let _ = EnumWindows(Some(collect_existing_main_pid), lparam);
    }
    pids
}

/// Running process IDs whose executable matches a configured app rule.
pub fn running_pids_for_executable(executable: &str) -> HashSet<u32> {
    let mut pids = HashSet::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return pids;
        };
        if snapshot.is_invalid() {
            return pids;
        }

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                if matches_executable(executable, &name) {
                    pids.insert(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }
    pids
}

struct EnumPidContext {
    target_pid: u32,
    found: bool,
}

pub fn process_has_main_window(pid: u32) -> bool {
    let mut ctx = EnumPidContext {
        target_pid: pid,
        found: false,
    };
    unsafe {
        let lparam = LPARAM(&mut ctx as *mut _ as isize);
        let _ = EnumWindows(Some(enum_process_main_window), lparam);
    }
    ctx.found
}

pub fn executable_for_hwnd(hwnd: isize) -> Option<String> {
    let hwnd = HWND(hwnd as *mut _);
    if !is_main_window(hwnd) {
        return None;
    }

    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }

        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
        .ok()?;

        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        Some(normalize_exe_path(&path))
    }
}

pub fn is_main_window(hwnd: HWND) -> bool {
    is_main_window_inner(hwnd, true)
}

fn is_main_window_inner(hwnd: HWND, require_visible: bool) -> bool {
    unsafe {
        if hwnd.0.is_null() {
            return false;
        }
        if require_visible && !IsWindowVisible(hwnd).as_bool() {
            return false;
        }

        let owner = GetWindow(hwnd, GW_OWNER);
        if owner.is_ok() && !owner.unwrap().0.is_null() {
            return false;
        }

        let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
        if (style & WS_POPUP.0) != 0 && (style & 0x00C00000) == 0 {
            // WS_POPUP without WS_CAPTION
            return false;
        }

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if (ex_style & WS_EX_TOOLWINDOW.0) != 0 {
            return false;
        }

        let mut class_name = [0u16; 256];
        let len = GetClassNameW(hwnd, &mut class_name);
        if len > 0 {
            let class = String::from_utf16_lossy(&class_name[..len as usize]);
            if class == "Shell_TrayWnd" || class == "Progman" || class == "WorkerW" {
                return false;
            }
        }

        let rect = window_rect(hwnd);
        rect.map(|(w, h)| w >= MAIN_WINDOW_MIN_WIDTH && h >= MAIN_WINDOW_MIN_HEIGHT)
            .unwrap_or(false)
    }
}

fn window_rect(hwnd: HWND) -> Option<(i32, i32)> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).ok()?;
        Some((rect.right - rect.left, rect.bottom - rect.top))
    }
}

fn normalize_exe_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

unsafe extern "system" fn collect_existing_main_pid(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let pids = &mut *(lparam.0 as *mut HashSet<u32>);
    if is_main_window_inner(hwnd, false) && !is_own_process_window(hwnd) {
        if let Some(pid) = process_id_for_hwnd(hwnd.0 as isize) {
            pids.insert(pid);
        }
    }
    BOOL(1)
}

unsafe extern "system" fn enum_process_main_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut EnumPidContext);
    if ctx.found {
        return BOOL(0);
    }

    if process_id_for_hwnd(hwnd.0 as isize) == Some(ctx.target_pid)
        && is_main_window_inner(hwnd, false)
        && !is_own_process_window(hwnd)
    {
        ctx.found = true;
        return BOOL(0);
    }

    BOOL(1)
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
    fn normalize_exe_basename() {
        assert_eq!(
            normalize_exe_path(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            "chrome.exe"
        );
    }

    #[test]
    fn existing_main_window_pids_does_not_panic() {
        let _ = existing_main_window_pids();
    }
}
