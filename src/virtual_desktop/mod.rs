use thiserror::Error;
use tracing::debug;
use windows::Win32::Foundation::HWND;
use winvd::{DesktopEvent, DesktopEventThread};

mod focus;

pub const WORKSPACE_INDEX_BASE: u32 = 1;
pub const MAX_WORKSPACES: u32 = 15;

#[derive(Debug, Error)]
pub enum VirtualDesktopError {
    #[error("virtual desktop API error: {0}")]
    Api(String),
    #[error("invalid workspace index {index} (valid range: {min}..={max})")]
    InvalidIndex { index: u32, min: u32, max: u32 },
    #[error("no focused window")]
    NoFocusedWindow,
    #[error("cannot remove the last workspace")]
    LastWorkspace,
    #[error("maximum of {max} workspaces allowed")]
    MaxWorkspaces { max: u32 },
}

impl From<winvd::Error> for VirtualDesktopError {
    fn from(value: winvd::Error) -> Self {
        Self::Api(format!("{value:?}"))
    }
}

pub fn workspace_count() -> Result<u32, VirtualDesktopError> {
    Ok(winvd::get_desktop_count()? as u32)
}

pub fn current_workspace_index() -> Result<u32, VirtualDesktopError> {
    let desktop = winvd::get_current_desktop()?;
    Ok(desktop.get_index()? + WORKSPACE_INDEX_BASE)
}

pub fn switch_to_workspace(index: u32) -> Result<(), VirtualDesktopError> {
    switch_to_workspace_impl(index)
}

pub fn switch_to_workspace_focusing(index: u32, _hwnd: isize) -> Result<(), VirtualDesktopError> {
    switch_to_workspace(index)
}

pub fn on_desktop_changed() {
    focus::restore_focus_after_desktop_change();
}

pub fn init_focus_exclusions() {
    focus::seed_startup_focus_exclusions();
}

pub fn focus_window_by_title(title: &str) -> bool {
    focus::focus_window_by_title(title)
}

pub fn focus_window(hwnd: isize) -> Result<(), VirtualDesktopError> {
    focus::allow_foreground_from_background();
    if focus::focus_window_hwnd(hwnd) {
        Ok(())
    } else {
        Err(VirtualDesktopError::Api("failed to focus window".into()))
    }
}

fn switch_to_workspace_impl(index: u32) -> Result<(), VirtualDesktopError> {
    validate_index(index)?;
    let zero_based = index - WORKSPACE_INDEX_BASE;

    focus::allow_foreground_from_background();
    debug!("switching to workspace {index} (api index {zero_based})");
    winvd::switch_desktop(zero_based)?;
    Ok(())
}

pub fn move_window_to_workspace(hwnd: isize, index: u32) -> Result<(), VirtualDesktopError> {
    validate_index(index)?;
    let zero_based = index - WORKSPACE_INDEX_BASE;
    let hwnd = HWND(hwnd as *mut _);
    debug!("moving hwnd {hwnd:?} to workspace {index}");
    winvd::move_window_to_desktop(zero_based, &hwnd)?;
    Ok(())
}

pub fn is_window_not_found(err: &VirtualDesktopError) -> bool {
    matches!(
        err,
        VirtualDesktopError::Api(msg) if msg.contains("WindowNotFound")
    )
}

pub fn move_focused_window_to_workspace(index: u32) -> Result<isize, VirtualDesktopError> {
    let hwnd = focused_hwnd().ok_or(VirtualDesktopError::NoFocusedWindow)?;
    move_window_to_workspace(hwnd, index)?;
    Ok(hwnd)
}

pub fn listen_events(
    sender: crossbeam_channel::Sender<DesktopEvent>,
) -> Result<DesktopEventThread, VirtualDesktopError> {
    Ok(winvd::listen_desktop_events(sender)?)
}

pub fn add_workspace() -> Result<u32, VirtualDesktopError> {
    let count = workspace_count()?;
    if count >= MAX_WORKSPACES {
        return Err(VirtualDesktopError::MaxWorkspaces {
            max: MAX_WORKSPACES,
        });
    }

    let desktop = winvd::create_desktop()?;
    let index = desktop.get_index()? + WORKSPACE_INDEX_BASE;
    debug!("created workspace {index}");
    focus::allow_foreground_from_background();
    winvd::switch_desktop(desktop)?;
    Ok(index)
}

pub fn remove_current_workspace() -> Result<(), VirtualDesktopError> {
    let count = workspace_count()?;
    if count <= WORKSPACE_INDEX_BASE {
        return Err(VirtualDesktopError::LastWorkspace);
    }

    let current = winvd::get_current_desktop()?;
    let current_zero = current.get_index()?;
    let fallback_zero = if current_zero == 0 { 1 } else { current_zero - 1 };
    if fallback_zero == current_zero || fallback_zero as u32 >= count {
        return Err(VirtualDesktopError::Api(
            "no valid fallback workspace for removal".into(),
        ));
    }

    debug!(
        "removing workspace {} (fallback api index {fallback_zero})",
        current_zero + WORKSPACE_INDEX_BASE
    );
    winvd::remove_desktop(current_zero, fallback_zero)?;
    Ok(())
}

pub fn remove_workspace(index: u32) -> Result<(), VirtualDesktopError> {
    validate_index(index)?;
    let count = workspace_count()?;
    if count <= WORKSPACE_INDEX_BASE {
        return Err(VirtualDesktopError::LastWorkspace);
    }

    let remove_zero = index - WORKSPACE_INDEX_BASE;
    let fallback_zero = if remove_zero == 0 { 1 } else { remove_zero - 1 };
    if fallback_zero == remove_zero || fallback_zero >= count {
        return Err(VirtualDesktopError::Api(
            "no valid fallback workspace for removal".into(),
        ));
    }

    debug!("removing workspace {index} (fallback api index {fallback_zero})");
    winvd::remove_desktop(remove_zero, fallback_zero)?;
    Ok(())
}

fn validate_index(index: u32) -> Result<(), VirtualDesktopError> {
    if index < WORKSPACE_INDEX_BASE {
        return Err(VirtualDesktopError::InvalidIndex {
            index,
            min: WORKSPACE_INDEX_BASE,
            max: workspace_count().unwrap_or(WORKSPACE_INDEX_BASE),
        });
    }
    let count = workspace_count()?;
    if index > count {
        return Err(VirtualDesktopError::InvalidIndex {
            index,
            min: WORKSPACE_INDEX_BASE,
            max: count,
        });
    }
    Ok(())
}

fn focused_hwnd() -> Option<isize> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindow, IsWindowVisible, GW_OWNER,
    };

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

pub fn workspace_index_from_event(event: &DesktopEvent) -> Option<u32> {
    match event {
        DesktopEvent::DesktopChanged { new, .. } => {
            new.get_index().ok().map(|idx| idx + WORKSPACE_INDEX_BASE)
        }
        DesktopEvent::DesktopMoved { new_index, .. } => {
            Some(*new_index as u32 + WORKSPACE_INDEX_BASE)
        }
        DesktopEvent::DesktopCreated(_)
        | DesktopEvent::DesktopDestroyed { .. }
        | DesktopEvent::DesktopNameChanged(_, _)
        | DesktopEvent::DesktopWallpaperChanged(_, _)
        | DesktopEvent::WindowChanged(_) => current_workspace_index().ok(),
    }
}

pub fn desktop_count_may_have_changed(event: &DesktopEvent) -> bool {
    matches!(
        event,
        DesktopEvent::DesktopCreated(_) | DesktopEvent::DesktopDestroyed { .. }
    )
}
