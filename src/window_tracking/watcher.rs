use crossbeam_channel::{Receiver, Sender};
use std::thread::{self, JoinHandle};
use tracing::error;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
};

use super::{executable_for_hwnd, is_main_window};

#[derive(Debug, Clone)]
pub enum AppWindowEvent {
    MainWindowShown { hwnd: isize, executable: String },
}

pub struct WindowWatcher {
    _thread: JoinHandle<()>,
    event_rx: Receiver<AppWindowEvent>,
}

impl WindowWatcher {
    pub fn start() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();

        let handle = thread::spawn(move || {
            if let Err(e) = run_winevent_thread(tx.clone()) {
                error!("WinEvent watcher failed: {e}");
            }
        });

        Self {
            _thread: handle,
            event_rx: rx,
        }
    }

    pub fn events(&self) -> &Receiver<AppWindowEvent> {
        &self.event_rx
    }
}

#[derive(thiserror::Error, Debug)]
enum WatcherError {
    #[error("failed to install WinEvent hook: {0}")]
    HookInstall(String),
}

fn run_winevent_thread(tx: Sender<AppWindowEvent>) -> Result<(), WatcherError> {
    static TX: std::sync::OnceLock<Sender<AppWindowEvent>> = std::sync::OnceLock::new();
    TX.set(tx).ok();

    unsafe extern "system" fn callback(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd_raw: HWND,
        _id_object: i32,
        _id_child: i32,
        _id_thread: u32,
        _time: u32,
    ) {
        let Some(tx) = TX.get() else {
            return;
        };

        if hwnd_raw.0.is_null() {
            return;
        }

        let hwnd = hwnd_raw.0 as isize;
        if !is_main_window(hwnd_raw) {
            return;
        }

        let Some(executable) = executable_for_hwnd(hwnd) else {
            return;
        };

        match event {
            x if x == EVENT_OBJECT_SHOW || x == EVENT_SYSTEM_FOREGROUND => {
                let _ = tx.send(AppWindowEvent::MainWindowShown { hwnd, executable });
            }
            _ => {}
        }
    }

    let hook_show = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_SHOW,
            None,
            Some(callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook_show.is_invalid() {
        return Err(WatcherError::HookInstall("EVENT_OBJECT_SHOW".into()));
    }

    let hook_foreground = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook_foreground.is_invalid() {
        unsafe {
            let _ = UnhookWinEvent(hook_show);
        }
        return Err(WatcherError::HookInstall("EVENT_SYSTEM_FOREGROUND".into()));
    }

    loop {
        let mut msg = std::mem::MaybeUninit::uninit();
        let ret = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetMessageW(msg.as_mut_ptr(), None, 0, 0)
        };
        if ret.0 <= 0 {
            break;
        }
    }

    unsafe {
        let _ = UnhookWinEvent(hook_show);
        let _ = UnhookWinEvent(hook_foreground);
    }

    Ok(())
}
