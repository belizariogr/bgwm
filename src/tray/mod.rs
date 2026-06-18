mod icons;

pub use icons::workspace_icon;

use thiserror::Error;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::virtual_desktop::WORKSPACE_INDEX_BASE;

const MENU_PREFIX_WS: &str = "ws:";
const MENU_SETTINGS: &str = "settings";
const MENU_EXIT: &str = "exit";

#[derive(Debug, Error)]
pub enum TrayError {
    #[error("tray icon error: {0}")]
    Icon(#[from] tray_icon::Error),
    #[error("tray menu error: {0}")]
    Menu(String),
}

pub struct TrayController {
    tray: TrayIcon,
    workspace_items: Vec<MenuItem>,
    settings_id: MenuId,
    exit_id: MenuId,
}

impl TrayController {
    pub fn new(workspace_count: u32, current_workspace: u32) -> Result<Self, TrayError> {
        let icon = workspace_icon(current_workspace)?;
        let (menu, workspace_items, settings_id, exit_id) =
            build_menu(workspace_count, current_workspace)?;

        let tray = TrayIconBuilder::new()
            .with_tooltip(format!("BGWM — Workspace {current_workspace}"))
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()?;

        Ok(Self {
            tray,
            workspace_items,
            settings_id,
            exit_id,
        })
    }

    pub fn set_workspace(&self, current_workspace: u32) -> Result<(), TrayError> {
        let icon = workspace_icon(current_workspace)?;
        self.tray.set_icon(Some(icon))?;
        self.tray
            .set_tooltip(Some(format!("BGWM — Workspace {current_workspace}")))?;

        for (idx, item) in self.workspace_items.iter().enumerate() {
            let ws = idx as u32 + WORKSPACE_INDEX_BASE;
            let label = if ws == current_workspace {
                format!("✓ Workspace {ws}")
            } else {
                format!("Workspace {ws}")
            };
            item.set_text(label);
        }

        Ok(())
    }

    pub fn rebuild_menu(
        &mut self,
        workspace_count: u32,
        current_workspace: u32,
    ) -> Result<(), TrayError> {
        let (menu, workspace_items, settings_id, exit_id) =
            build_menu(workspace_count, current_workspace)?;
        self.tray.set_menu(Some(Box::new(menu)));
        self.workspace_items = workspace_items;
        self.settings_id = settings_id;
        self.exit_id = exit_id;
        self.set_workspace(current_workspace)
    }
}

fn build_menu(
    workspace_count: u32,
    current_workspace: u32,
) -> Result<(Menu, Vec<MenuItem>, MenuId, MenuId), TrayError> {
    let menu = Menu::new();
    let mut workspace_items = Vec::new();

    for ws in WORKSPACE_INDEX_BASE..=workspace_count.max(WORKSPACE_INDEX_BASE) {
        let label = if ws == current_workspace {
            format!("✓ Workspace {ws}")
        } else {
            format!("Workspace {ws}")
        };
        let item = MenuItem::with_id(format!("{MENU_PREFIX_WS}{ws}"), label, true, None);
        menu.append(&item)
            .map_err(|e| TrayError::Menu(e.to_string()))?;
        workspace_items.push(item);
    }

    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    let settings = MenuItem::with_id(MENU_SETTINGS, "Settings", true, None);
    menu.append(&settings)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    let exit = MenuItem::with_id(MENU_EXIT, "Exit", true, None);
    menu.append(&exit)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    Ok((
        menu,
        workspace_items,
        settings.id().clone(),
        exit.id().clone(),
    ))
}

pub fn install_event_forwarders(proxy: winit::event_loop::EventLoopProxy<crate::app::UserEvent>) {
    let proxy_tray = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy_tray.send_event(crate::app::UserEvent::Tray(event));
    }));

    let proxy_menu = proxy;
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy_menu.send_event(crate::app::UserEvent::Menu(event));
    }));
}

pub fn menu_workspace_from_id(id: &MenuId) -> Option<u32> {
    id.0.strip_prefix(MENU_PREFIX_WS)
        .and_then(|s| s.parse().ok())
}

pub fn is_settings_menu(id: &MenuId) -> bool {
    id.0 == MENU_SETTINGS
}

pub fn is_exit_menu(id: &MenuId) -> bool {
    id.0 == MENU_EXIT
}
