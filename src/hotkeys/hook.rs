//! Low-level keyboard hook (`WH_KEYBOARD_LL`) for global hotkeys.
//!
//! # Why a hook instead of `RegisterHotKey`
//!
//! Windows reserves many `Win+N` shortcuts, so registration fails. A low-level hook
//! lets us override those combos while keeping correct Win-key behavior.
//!
//! # Deferred Win-key strategy
//!
//! 1. **Win key-down** — swallow (OS never sees it yet).
//! 2. **Registered combo while Win held** — swallow combo key, fire action, remember chord.
//! 3. **Win key-up after chord** — swallow; inject marked synthetic key-ups to clear
//!    `GetAsyncKeyState` without opening Start menu.
//! 4. **Win key-up alone (no chord)** — swallow physical up; inject a synthetic Win
//!    tap (down + up, unmarked) so Start menu opens normally.
//! 5. **Injected events** — always passed through unchanged (Start tap + our cleanup).

use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use thiserror::Error;
use tracing::error;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY, VK_CONTROL, VK_LWIN,
    VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION,
    KBDLLHOOKSTRUCT, LLKHF_INJECTED, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

use super::{Hotkey, Modifiers};

const VK_SHIFT_I32: i32 = 0x10;
const VK_CONTROL_I32: i32 = 0x11;

/// Tag for synthetic chord-release events (swallowed so they never reach apps/Start).
const INJECTED_MARKER: usize = 0x4247_574D;

type BindingKey = (Modifiers, u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    SwitchWorkspace(u32),
    MoveWindowToWorkspace(u32),
}

#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    Triggered(HotkeyAction),
    HookError(String),
}

#[derive(Debug, Error)]
pub enum HotkeyEngineError {
    #[error("failed to install keyboard hook: {0}")]
    HookInstall(String),
}

#[derive(Debug, Clone, Copy)]
struct ActiveChord {
    main_vk: u16,
    modifiers: Modifiers,
    win_vk: VIRTUAL_KEY,
}

struct HookState {
    switch: HashMap<BindingKey, u32>,
    r#move: HashMap<BindingKey, u32>,
    /// Win key-down was swallowed; waiting for combo or release.
    win_held: Option<VIRTUAL_KEY>,
    active_chord: Option<ActiveChord>,
    /// Non-Win hotkey main key whose key-up should be swallowed.
    suppressed_key: Option<u16>,
    /// Swallow one extra physical Win up after chord cleanup (user still holding Win).
    swallow_extra_win_up: bool,
}

static HOOK_STATE_GLOBAL: OnceLock<Arc<Mutex<HookState>>> = OnceLock::new();
static HOOK_TX_GLOBAL: OnceLock<Sender<HotkeyEvent>> = OnceLock::new();

pub struct HotkeyEngine {
    state: Arc<Mutex<HookState>>,
    _thread: JoinHandle<()>,
    event_rx: Receiver<HotkeyEvent>,
}

impl HotkeyEngine {
    pub fn start(
        switch: Vec<(u32, Hotkey)>,
        r#move: Vec<(u32, Hotkey)>,
    ) -> Result<Self, HotkeyEngineError> {
        let (event_tx, event_rx) = crossbeam_channel::unbounded();

        let state = Arc::new(Mutex::new(HookState {
            switch: bindings_map(switch),
            r#move: bindings_map(r#move),
            win_held: None,
            active_chord: None,
            suppressed_key: None,
            swallow_extra_win_up: false,
        }));

        HOOK_STATE_GLOBAL.set(Arc::clone(&state)).ok();
        HOOK_TX_GLOBAL.set(event_tx.clone()).ok();

        let handle = thread::spawn(move || {
            if let Err(e) = run_hook_thread() {
                error!("keyboard hook thread failed: {e}");
                let _ = event_tx.send(HotkeyEvent::HookError(e.to_string()));
            }
        });

        Ok(Self {
            state,
            _thread: handle,
            event_rx,
        })
    }

    pub fn events(&self) -> &Receiver<HotkeyEvent> {
        &self.event_rx
    }

    pub fn update_bindings(&self, switch: Vec<(u32, Hotkey)>, r#move: Vec<(u32, Hotkey)>) {
        if let Ok(mut state) = self.state.lock() {
            state.switch = bindings_map(switch);
            state.r#move = bindings_map(r#move);
        }
    }
}

fn bindings_map(bindings: Vec<(u32, Hotkey)>) -> HashMap<BindingKey, u32> {
    bindings
        .into_iter()
        .map(|(ws, hk)| (binding_key(&hk), ws))
        .collect()
}

fn binding_key(hk: &Hotkey) -> BindingKey {
    (hk.modifiers, hk.key.0)
}

fn run_hook_thread() -> Result<(), HotkeyEngineError> {
    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let state = match HOOK_STATE_GLOBAL.get() {
            Some(s) => s,
            None => return CallNextHookEx(None, code, wparam, lparam),
        };
        let tx = match HOOK_TX_GLOBAL.get() {
            Some(s) => s,
            None => return CallNextHookEx(None, code, wparam, lparam),
        };

        if code < 0 || code != HC_ACTION as i32 {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        let kb = *(lparam.0 as *const KBDLLHOOKSTRUCT);

        // Our marked cleanup events must not reach the shell or re-enter state logic.
        if is_ours(&kb) {
            return LRESULT(1);
        }

        // Unmarked injected events (e.g. Win tap for Start menu) pass through as-is.
        if kb.flags.contains(LLKHF_INJECTED) {
            return CallNextHookEx(None, code, wparam, lparam);
        }

        let vk = VIRTUAL_KEY(kb.vkCode as u16);
        let msg = wparam.0 as u32;
        let is_key_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_key_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;

        if is_key_up {
            if handle_key_up(state, vk) {
                return LRESULT(1);
            }
            return CallNextHookEx(None, code, wparam, lparam);
        }

        if is_key_down {
            if handle_key_down(state, tx, vk) {
                return LRESULT(1);
            }
            return CallNextHookEx(None, code, wparam, lparam);
        }

        CallNextHookEx(None, code, wparam, lparam)
    }

    let hook =
        unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), HINSTANCE::default(), 0) }
            .map_err(|e| HotkeyEngineError::HookInstall(format!("{e}")))?;

    loop {
        let mut msg = std::mem::MaybeUninit::uninit();
        let ret = unsafe { GetMessageW(msg.as_mut_ptr(), None, 0, 0) };
        if ret.0 <= 0 {
            break;
        }
    }

    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }

    Ok(())
}

fn handle_key_up(state: &Arc<Mutex<HookState>>, vk: VIRTUAL_KEY) -> bool {
    let mut st = state.lock().expect("hook state poisoned");

    if is_win_key(vk) {
        if st.swallow_extra_win_up {
            st.swallow_extra_win_up = false;
            return true;
        }

        if let Some(chord) = st.active_chord.take() {
            st.win_held = None;
            inject_chord_release(chord);
            if win_still_down() {
                st.swallow_extra_win_up = true;
            }
            return true;
        }

        if st.win_held.take().is_some() {
            inject_win_tap(vk);
            return true;
        }

        return false;
    }

    if let Some(chord) = st.active_chord {
        if vk.0 == chord.main_vk {
            return true;
        }
    }

    if st.suppressed_key == Some(vk.0) {
        st.suppressed_key = None;
        return true;
    }

    false
}

fn handle_key_down(
    state: &Arc<Mutex<HookState>>,
    tx: &Sender<HotkeyEvent>,
    vk: VIRTUAL_KEY,
) -> bool {
    if is_win_key(vk) {
        let mut st = state.lock().expect("hook state poisoned");
        st.win_held = Some(vk);
        st.active_chord = None;
        return true;
    }

    let win_held = {
        let st = state.lock().expect("hook state poisoned");
        st.win_held
    };

    let modifiers = current_modifiers(win_held.is_some());
    let key = binding_key(&Hotkey {
        modifiers,
        key: vk,
        display: String::new(),
    });

    let mut st = state.lock().expect("hook state poisoned");

    let matched = st
        .switch
        .get(&key)
        .copied()
        .map(HotkeyAction::SwitchWorkspace)
        .or_else(|| {
            st.r#move
                .get(&key)
                .copied()
                .map(HotkeyAction::MoveWindowToWorkspace)
        });

    let Some(action) = matched else {
        return false;
    };

    if let Some(win_vk) = win_held {
        st.active_chord = Some(ActiveChord {
            main_vk: vk.0,
            modifiers,
            win_vk,
        });
    } else {
        st.suppressed_key = Some(vk.0);
    }

    let _ = tx.send(HotkeyEvent::Triggered(action));
    true
}

/// Synthetic Win tap to open Start menu after we swallowed the physical Win down.
fn inject_win_tap(vk: VIRTUAL_KEY) {
    let down = key_event(vk, false, false);
    let up = key_event(vk, true, false);
    unsafe {
        SendInput(&[down, up], size_of::<INPUT>() as i32);
    }
}

fn inject_chord_release(chord: ActiveChord) {
    let mut inputs = Vec::with_capacity(5);
    inputs.push(key_event(VIRTUAL_KEY(chord.main_vk), true, true));

    if chord.modifiers.shift {
        inputs.push(key_event(VK_SHIFT, true, true));
    }
    if chord.modifiers.ctrl {
        inputs.push(key_event(VK_CONTROL, true, true));
    }
    if chord.modifiers.alt {
        inputs.push(key_event(VK_MENU, true, true));
    }
    inputs.push(key_event(chord.win_vk, true, true));

    unsafe {
        SendInput(&inputs, size_of::<INPUT>() as i32);
    }
}

fn key_event(vk: VIRTUAL_KEY, key_up: bool, marked: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) as u16 };
    let mut flags = if key_up {
        KEYEVENTF_KEYUP
    } else {
        Default::default()
    };
    if is_win_key(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: if marked { INJECTED_MARKER } else { 0 },
            },
        },
    }
}

fn is_ours(kb: &KBDLLHOOKSTRUCT) -> bool {
    kb.dwExtraInfo == INJECTED_MARKER
}

fn is_win_key(vk: VIRTUAL_KEY) -> bool {
    vk == VK_LWIN || vk == VK_RWIN
}

fn win_still_down() -> bool {
    is_down(VK_LWIN.0 as i32) || is_down(VK_RWIN.0 as i32)
}

fn current_modifiers(win_held: bool) -> Modifiers {
    Modifiers::from_parts(
        is_down(VK_CONTROL_I32),
        is_down(VK_MENU.0 as i32),
        is_down(VK_SHIFT_I32),
        win_held || is_down(VK_LWIN.0 as i32) || is_down(VK_RWIN.0 as i32),
    )
}

fn is_down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16) & 0x8000 != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win_held_counts_as_win_modifier() {
        let m = current_modifiers(true);
        assert!(m.win);
    }
}
