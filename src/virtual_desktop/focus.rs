use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::thread;
use std::time::Duration;

use tracing::debug;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, BringWindowToTop, EnumWindows, GetForegroundWindow,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow, ShowWindow,
    ASFW_ANY, SW_RESTORE,
};

use crate::window_tracking::is_main_window;

static LAST_FOCUSED_BY_WORKSPACE: LazyLock<Mutex<HashMap<u32, isize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static PENDING_PREFERRED_FOCUS: Mutex<Option<isize>> = Mutex::new(None);

const FOCUS_RESTORE_DELAY: Duration = Duration::from_millis(100);

pub fn allow_foreground_from_background() {
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

pub fn remember_focused_workspace(workspace: u32) {
    let Some(hwnd) = foreground_hwnd() else {
        return;
    };

    if is_pinned(HWND(hwnd as *mut _)) {
        return;
    }

    if let Ok(mut map) = LAST_FOCUSED_BY_WORKSPACE.lock() {
        map.insert(workspace, hwnd);
    }
}

pub fn set_pending_focus(hwnd: isize) {
    if let Ok(mut pending) = PENDING_PREFERRED_FOCUS.lock() {
        *pending = Some(hwnd);
    }
}

pub fn restore_focus_after_desktop_change() {
    thread::sleep(FOCUS_RESTORE_DELAY);

    let preferred = PENDING_PREFERRED_FOCUS
        .lock()
        .ok()
        .and_then(|mut pending| pending.take());

    if let Some(hwnd) = preferred {
        if focus_window_if_valid(hwnd) {
            return;
        }
    }

    if let Ok(ws) = super::current_workspace_index() {
        if let Some(hwnd) = LAST_FOCUSED_BY_WORKSPACE
            .lock()
            .ok()
            .and_then(|map| map.get(&ws).copied())
        {
            if focus_window_if_valid(hwnd) {
                return;
            }
        }
    }

    if let Some(hwnd) = topmost_window_on_current_desktop() {
        activate_window(hwnd);
    }
}

fn focus_window_if_valid(hwnd: isize) -> bool {
    let hwnd = HWND(hwnd as *mut _);
    if !is_focus_candidate(hwnd) {
        return false;
    }
    activate_window(hwnd)
}

fn topmost_window_on_current_desktop() -> Option<HWND> {
    let mut found = None;
    unsafe {
        let lparam = LPARAM(&mut found as *mut _ as isize);
        let _ = EnumWindows(Some(enum_top_window), lparam);
    }
    found
}

unsafe extern "system" fn enum_top_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let found = &mut *(lparam.0 as *mut Option<HWND>);
    if !is_focus_candidate(hwnd) {
        return BOOL(1);
    }

    match winvd::is_window_on_current_desktop(hwnd) {
        Ok(true) => {
            *found = Some(hwnd);
            BOOL(0)
        }
        _ => BOOL(1),
    }
}

fn is_focus_candidate(hwnd: HWND) -> bool {
    unsafe {
        if hwnd.0.is_null() || !IsWindow(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
            return false;
        }
    }

    if is_own_process_window(hwnd) {
        return false;
    }

    is_main_window(hwnd)
}

fn activate_window(hwnd: HWND) -> bool {
    unsafe {
        if hwnd.0.is_null() || !IsWindow(hwnd).as_bool() {
            return false;
        }

        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }

        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let target_thread = GetWindowThreadProcessId(hwnd, None);
        let current_thread = GetCurrentThreadId();

        let _ = AttachThreadInput(current_thread, target_thread, true);
        let _ = AttachThreadInput(foreground_thread, target_thread, true);
        let _ = AllowSetForegroundWindow(ASFW_ANY);

        let focused = SetForegroundWindow(hwnd).as_bool();
        let _ = BringWindowToTop(hwnd);

        let _ = AttachThreadInput(foreground_thread, target_thread, false);
        let _ = AttachThreadInput(current_thread, target_thread, false);

        if focused {
            debug!("focused hwnd {:?}", hwnd);
        }
        focused
    }
}

fn foreground_hwnd() -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindow, GW_OWNER};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }

        let mut current = hwnd;
        loop {
            let owner = GetWindow(current, GW_OWNER);
            match owner {
                Ok(owner_hwnd) if !owner_hwnd.0.is_null() => current = owner_hwnd,
                _ => break,
            }
        }

        if !IsWindowVisible(current).as_bool() {
            return None;
        }

        Some(current.0 as isize)
    }
}

fn is_pinned(hwnd: HWND) -> bool {
    winvd::is_pinned_window(hwnd).unwrap_or(false)
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
    fn remember_and_pending_focus_do_not_panic() {
        remember_focused_workspace(1);
        set_pending_focus(0);
    }
}
