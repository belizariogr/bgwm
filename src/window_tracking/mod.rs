mod watcher;

pub use watcher::{AppWindowEvent, WindowWatcher};

pub fn process_id_for_hwnd(hwnd: isize) -> Option<u32> {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

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

use std::path::Path;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetWindow, GetWindowLongPtrW, GetWindowThreadProcessId, IsWindowVisible,
    GWL_EXSTYLE, GWL_STYLE, GW_OWNER, WS_EX_TOOLWINDOW, WS_POPUP,
};

const MAIN_WINDOW_MIN_WIDTH: i32 = 100;
const MAIN_WINDOW_MIN_HEIGHT: i32 = 100;

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
    unsafe {
        if hwnd.0.is_null() || !IsWindowVisible(hwnd).as_bool() {
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
}
