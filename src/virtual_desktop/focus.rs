use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use tracing::debug;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, BringWindowToTop, EnumWindows, GetForegroundWindow,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible, SetForegroundWindow, ShowWindow,
    ASFW_ANY, SW_RESTORE,
};

use crate::window_tracking::{is_main_window, process_id_for_hwnd};

static FOCUS_EXCLUDED_PIDS: LazyLock<Mutex<HashSet<u32>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub fn allow_foreground_from_background() {
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
}

/// Records PIDs of processes that already had main windows when BGWM started.
pub fn seed_startup_focus_exclusions() {
    let mut pids = HashSet::new();
    unsafe {
        let lparam = LPARAM(&mut pids as *mut _ as isize);
        let _ = EnumWindows(Some(collect_existing_process_pids), lparam);
    }

    let count = pids.len();
    if let Ok(mut excluded) = FOCUS_EXCLUDED_PIDS.lock() {
        *excluded = pids;
    }
    debug!("seeded {count} pre-existing process(es) into focus exclusion list");
}

pub fn restore_focus_after_desktop_change() {
    if let Some(hwnd) = topmost_window_on_current_desktop() {
        activate_window(hwnd);
    }
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
    if !is_auto_focus_candidate(hwnd) {
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

unsafe extern "system" fn collect_existing_process_pids(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let pids = &mut *(lparam.0 as *mut HashSet<u32>);
    if !is_main_window(hwnd) || is_own_process_window(hwnd) {
        return BOOL(1);
    }

    if let Some(pid) = process_id_for_hwnd(hwnd.0 as isize) {
        pids.insert(pid);
    }

    BOOL(1)
}

fn is_auto_focus_candidate(hwnd: HWND) -> bool {
    is_focus_candidate(hwnd) && !is_focus_excluded(hwnd)
}

fn is_focus_excluded(hwnd: HWND) -> bool {
    let Some(pid) = process_id_for_hwnd(hwnd.0 as isize) else {
        return false;
    };
    FOCUS_EXCLUDED_PIDS
        .lock()
        .ok()
        .is_some_and(|excluded| excluded.contains(&pid))
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
    fn seed_startup_focus_exclusions_does_not_panic() {
        seed_startup_focus_exclusions();
    }
}
