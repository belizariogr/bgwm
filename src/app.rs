use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tracing::{error, info, warn};
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIconEvent;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent as WinitWindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;
use winvd::DesktopEvent;

use crate::config::{self, matches_executable, Config};
use crate::hotkeys::{HotkeyAction, HotkeyEngine, HotkeyEvent};
use crate::process_job::ChildProcessJob;
use crate::settings;
use crate::tray::{
    is_add_workspace_menu, is_exit_menu, is_remove_workspace_menu, is_settings_menu,
    menu_workspace_from_id, TrayController,
};
use crate::virtual_desktop::{self, WORKSPACE_INDEX_BASE};
use crate::window_tracking::{
    existing_main_window_pids, find_main_window_for_executable, is_window_valid,
    process_has_main_window, process_id_for_hwnd, running_pids_for_executable, AppWindowEvent,
    WindowWatcher,
};

const ROUTE_RETRY_DELAYS: [Duration; 3] = [
    Duration::ZERO,
    Duration::from_millis(50),
    Duration::from_millis(100),
];

#[derive(Debug)]
struct PendingAppRoute {
    hwnd: isize,
    pid: u32,
    workspace: u32,
    executable: String,
    attempt: usize,
    due: Instant,
}

#[derive(Debug)]
pub enum UserEvent {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
    Desktop(DesktopEvent),
    Hotkey(HotkeyAction),
}

#[derive(Debug)]
enum PendingTrayAction {
    AddWorkspace,
    RemoveWorkspace,
}

pub struct BgwmApp {
    config: Arc<Mutex<Config>>,
    proxy: Option<EventLoopProxy<UserEvent>>,
    last_config_mtime: Option<SystemTime>,
    tray: Option<TrayController>,
    hotkeys: Option<HotkeyEngine>,
    _desktop_listener: Option<winvd::DesktopEventThread>,
    window_watcher: Option<WindowWatcher>,
    /// Processes whose first main window was already routed by app rules.
    routed_processes: HashSet<u32>,
    /// Main window handle routed for each process (for cleanup on destroy).
    routed_main_hwnd: HashMap<u32, isize>,
    /// Processes that already had main windows when BGWM started; excluded from app routing.
    startup_pids: HashSet<u32>,
    pending_app_routes: Vec<PendingAppRoute>,
    current_workspace: u32,
    workspace_count: u32,
    child_job: Option<ChildProcessJob>,
    settings_child: Option<std::process::Child>,
    pending_tray_action: Option<PendingTrayAction>,
    pending_tray_menu_rebuild: bool,
}

impl BgwmApp {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            proxy: None,
            last_config_mtime: None,
            tray: None,
            hotkeys: None,
            _desktop_listener: None,
            window_watcher: None,
            routed_processes: HashSet::new(),
            routed_main_hwnd: HashMap::new(),
            startup_pids: HashSet::new(),
            pending_app_routes: Vec::new(),
            current_workspace: WORKSPACE_INDEX_BASE,
            workspace_count: WORKSPACE_INDEX_BASE,
            child_job: None,
            settings_child: None,
            pending_tray_action: None,
            pending_tray_menu_rebuild: false,
        }
    }

    pub fn prepare(&mut self, proxy: EventLoopProxy<UserEvent>) {
        self.proxy = Some(proxy);
    }

    fn init_services(&mut self) {
        self.workspace_count = virtual_desktop::workspace_count().unwrap_or(4);
        self.current_workspace =
            virtual_desktop::current_workspace_index().unwrap_or(WORKSPACE_INDEX_BASE);

        match TrayController::new(self.workspace_count, self.current_workspace) {
            Ok(tray) => self.tray = Some(tray),
            Err(e) => error!("failed to create tray icon: {e}"),
        }

        self.start_hotkeys();
        self.start_desktop_listener();
        self.startup_pids = existing_main_window_pids();
        let app_rules = self
            .config
            .lock()
            .expect("config poisoned")
            .app_rules
            .clone();
        for rule in &app_rules {
            for pid in running_pids_for_executable(&rule.executable) {
                self.startup_pids.insert(pid);
            }
        }
        info!(
            "ignoring {} pre-existing process(es) for app routing",
            self.startup_pids.len()
        );
        self.window_watcher = Some(WindowWatcher::start());
        self.last_config_mtime = settings::config_mtime();
        virtual_desktop::init_focus_exclusions();
        self.sync_startup_registration();

        match ChildProcessJob::new() {
            Ok(job) => self.child_job = Some(job),
            Err(e) => error!("failed to create child process job: {e}"),
        }
    }

    fn start_hotkeys(&mut self) {
        let config = self.config.lock().expect("config poisoned").clone();
        let switch = config.switch_bindings().unwrap_or_default();
        let move_bindings = config.move_bindings().unwrap_or_default();
        let launch_bindings = config.launch_bindings().unwrap_or_default();
        let Some(proxy) = self.proxy.clone() else {
            error!("event loop proxy not set before hotkey init");
            return;
        };

        match HotkeyEngine::start(
            switch,
            move_bindings,
            launch_bindings,
            move |action| {
                let _ = proxy.send_event(UserEvent::Hotkey(action));
            },
        ) {
            Ok(engine) => self.hotkeys = Some(engine),
            Err(e) => error!("failed to start hotkey engine: {e}"),
        }
    }

    fn reload_hotkeys(&mut self) {
        let config = self.config.lock().expect("config poisoned").clone();
        if let Some(engine) = &self.hotkeys {
            let switch = config.switch_bindings().unwrap_or_default();
            let move_bindings = config.move_bindings().unwrap_or_default();
            let launch_bindings = config.launch_bindings().unwrap_or_default();
            engine.update_bindings(switch, move_bindings, launch_bindings);
        } else {
            self.start_hotkeys();
        }
    }

    fn start_desktop_listener(&mut self) {
        let Some(proxy) = self.proxy.clone() else {
            error!("event loop proxy not set before desktop listener init");
            return;
        };

        let (tx, rx) = crossbeam_channel::unbounded();
        match virtual_desktop::listen_events(tx) {
            Ok(listener) => {
                self._desktop_listener = Some(listener);
                std::thread::spawn(move || {
                    while let Ok(event) = rx.recv() {
                        let _ = proxy.send_event(UserEvent::Desktop(event));
                    }
                });
            }
            Err(e) => error!("failed to listen for desktop events: {e}"),
        }
    }

    fn handle_hotkey(&mut self, action: HotkeyAction) {
        match action {
            HotkeyAction::SwitchWorkspace(ws) => {
                if self.current_workspace == ws {
                    return;
                }
                if let Err(e) = virtual_desktop::switch_to_workspace(ws) {
                    warn!("switch workspace failed: {e}");
                }
            }
            HotkeyAction::MoveWindowToWorkspace(ws) => {
                if self.current_workspace == ws {
                    return;
                }
                match virtual_desktop::move_focused_window_to_workspace(ws) {
                    Ok(hwnd) => {
                        if let Err(e) = virtual_desktop::switch_to_workspace_focusing(ws, hwnd) {
                            warn!("switch after move failed: {e}");
                        }
                    }
                    Err(e) => warn!("move window failed: {e}"),
                }
            }
            HotkeyAction::LaunchExecutable(executable) => {
                self.launch_or_focus_executable(&executable);
            }
        }
    }

    fn launch_or_focus_executable(&mut self, executable: &str) {
        let workspace = {
            let config = self.config.lock().expect("config poisoned");
            let Some(rule) = config.app_rules.iter().find(|rule| rule.executable == executable)
            else {
                warn!("launch hotkey fired for unknown executable: {executable}");
                return;
            };
            rule.workspace
        };

        if let Some(hwnd) = find_main_window_for_executable(executable) {
            if let Some(workspace) = workspace {
                match virtual_desktop::move_window_to_workspace(hwnd, workspace) {
                    Ok(()) => {
                        if self.current_workspace != workspace {
                            if let Err(e) =
                                virtual_desktop::switch_to_workspace_focusing(workspace, hwnd)
                            {
                                warn!("failed to switch workspace for launch hotkey: {e}");
                            }
                        } else if let Err(e) = virtual_desktop::focus_window(hwnd) {
                            warn!("failed to focus launched app window: {e}");
                        }
                    }
                    Err(e) => warn!("failed to move launched app to workspace {workspace}: {e}"),
                }
            } else if let Err(e) = virtual_desktop::focus_window(hwnd) {
                warn!("failed to focus launched app window: {e}");
            }
            return;
        }

        match std::process::Command::new(executable).spawn() {
            Ok(_) => info!("launched {executable}"),
            Err(e) => warn!("failed to launch {executable}: {e}"),
        }
    }

    fn refresh_workspace_state(&mut self) {
        if let Ok(count) = virtual_desktop::workspace_count() {
            self.workspace_count = count;
        }
        if let Ok(current) = virtual_desktop::current_workspace_index() {
            self.current_workspace = current
                .min(self.workspace_count)
                .max(WORKSPACE_INDEX_BASE);
        }
    }

    fn schedule_tray_menu_rebuild(&mut self) {
        self.pending_tray_menu_rebuild = true;
    }

    fn poll_pending_tray_action(&mut self) {
        let Some(action) = self.pending_tray_action.take() else {
            return;
        };

        let result = match action {
            PendingTrayAction::AddWorkspace => virtual_desktop::add_workspace().map(|_| ()),
            PendingTrayAction::RemoveWorkspace => virtual_desktop::remove_current_workspace(),
        };

        match result {
            Ok(()) => {
                self.refresh_workspace_state();
                self.schedule_tray_menu_rebuild();
            }
            Err(e) => warn!("tray workspace action failed: {e}"),
        }
    }

    fn poll_pending_tray_menu_rebuild(&mut self) {
        if !self.pending_tray_menu_rebuild {
            return;
        }
        self.pending_tray_menu_rebuild = false;
        self.refresh_workspace_state();

        if let Some(tray) = &mut self.tray {
            if let Err(e) = tray.rebuild_menu(self.workspace_count, self.current_workspace) {
                warn!("failed to rebuild tray menu: {e}");
            }
        }
        self.reload_hotkeys();
    }

    fn handle_desktop_event(&mut self, event: DesktopEvent) {
        if matches!(event, DesktopEvent::DesktopChanged { .. }) {
            virtual_desktop::on_desktop_changed();
        }

        let count_changed = virtual_desktop::desktop_count_may_have_changed(&event);
        if count_changed {
            if let Ok(count) = virtual_desktop::workspace_count() {
                self.workspace_count = count;
            }
            self.schedule_tray_menu_rebuild();
        }

        if let Some(ws) = virtual_desktop::workspace_index_from_event(&event) {
            self.current_workspace = ws
                .min(self.workspace_count)
                .max(WORKSPACE_INDEX_BASE);

            if !count_changed {
                if let Some(tray) = &self.tray {
                    if let Err(e) =
                        tray.set_workspace(self.current_workspace, self.workspace_count)
                    {
                        warn!("failed to update tray icon: {e}");
                    }
                }
            }
        }
    }

    fn handle_app_window_event(&mut self, event: AppWindowEvent) {
        match event {
            AppWindowEvent::MainWindowDestroyed { pid, hwnd } => {
                self.pending_app_routes.retain(|route| route.pid != pid);
                if self.startup_pids.contains(&pid) && !process_has_main_window(pid) {
                    self.startup_pids.remove(&pid);
                }
                if self.routed_main_hwnd.get(&pid).is_some_and(|&h| h == hwnd) {
                    self.routed_processes.remove(&pid);
                    self.routed_main_hwnd.remove(&pid);
                }
            }
            AppWindowEvent::MainWindowShown { hwnd, executable } => {
                let Some(pid) = process_id_for_hwnd(hwnd) else {
                    return;
                };
                if self.startup_pids.contains(&pid) {
                    return;
                }
                if self.routed_processes.contains(&pid) {
                    return;
                }

                let rules = self
                    .config
                    .lock()
                    .expect("config poisoned")
                    .app_rules
                    .clone();

                for rule in rules {
                    if !matches_executable(&rule.executable, &executable) {
                        continue;
                    }

                    let Some(workspace) = rule.workspace else {
                        continue;
                    };

                    if self.pending_app_routes.iter().any(|route| route.pid == pid) {
                        break;
                    }

                    info!(
                        "routing {executable} to workspace {workspace} (hwnd {hwnd}, pid {pid})",
                    );

                    self.pending_app_routes.push(PendingAppRoute {
                        hwnd,
                        pid,
                        workspace,
                        executable,
                        attempt: 0,
                        due: Instant::now(),
                    });
                    break;
                }
            }
        }
    }

    fn poll_pending_app_routes(&mut self) {
        let now = Instant::now();
        let mut completed = Vec::new();

        for (index, route) in self.pending_app_routes.iter_mut().enumerate() {
            if route.due > now {
                continue;
            }

            if !is_window_valid(route.hwnd) {
                completed.push(index);
                continue;
            }

            match virtual_desktop::move_window_to_workspace(route.hwnd, route.workspace) {
                Ok(()) => {
                    if self.current_workspace != route.workspace {
                        if let Err(e) = virtual_desktop::switch_to_workspace_focusing(
                            route.workspace,
                            route.hwnd,
                        ) {
                            warn!("failed to switch workspace for app rule: {e}");
                        }
                    }

                    if route.attempt > 0 {
                        info!(
                            "routed {} to workspace {} after {} retries",
                            route.executable, route.workspace, route.attempt
                        );
                    }

                    self.routed_processes.insert(route.pid);
                    self.routed_main_hwnd.insert(route.pid, route.hwnd);
                    completed.push(index);
                }
                Err(e) if virtual_desktop::is_window_not_found(&e) => {
                    if route.attempt + 1 < ROUTE_RETRY_DELAYS.len() {
                        route.attempt += 1;
                        route.due = now + ROUTE_RETRY_DELAYS[route.attempt];
                    } else {
                        warn!(
                            "failed to route {} to workspace {} after {} attempts: {e}",
                            route.executable,
                            route.workspace,
                            route.attempt + 1
                        );
                        completed.push(index);
                    }
                }
                Err(e) => {
                    warn!(
                        "failed to route {} to workspace {}: {e}",
                        route.executable, route.workspace
                    );
                    completed.push(index);
                }
            }
        }

        for index in completed.into_iter().rev() {
            self.pending_app_routes.swap_remove(index);
        }
    }

    fn open_settings(&mut self) {
        let Some(job) = self.child_job.as_ref() else {
            error!("child process job not initialized");
            return;
        };
        if let Err(e) = settings::open_settings(job, &mut self.settings_child) {
            error!("failed to open settings window: {e}");
        }
    }

    fn shutdown(&mut self) {
        if let Some(job) = self.child_job.as_ref() {
            job.terminate_all();
        }
    }

    fn poll_config_reload(&mut self) {
        let Some(mtime) = settings::config_mtime() else {
            return;
        };

        if self.last_config_mtime.is_some_and(|prev| mtime <= prev) {
            return;
        }

        self.last_config_mtime = Some(mtime);

        match config::load() {
            Ok(updated) => {
                let startup = updated.startup.clone();
                if let Ok(mut cfg) = self.config.lock() {
                    *cfg = updated;
                }
                self.reload_hotkeys();
                if let Err(e) = crate::startup::apply(&startup) {
                    warn!("startup registration sync failed: {e}");
                }
                info!("config reloaded after settings save");
            }
            Err(e) => warn!("config reload failed: {e}"),
        }
    }

    fn sync_startup_registration(&self) {
        let startup = self.config.lock().expect("config poisoned").startup.clone();
        if let Err(e) = crate::startup::apply(&startup) {
            warn!("startup registration sync failed: {e}");
        }
    }

    fn poll_background(&mut self) {
        let mut hotkey_events = Vec::new();
        if let Some(engine) = &self.hotkeys {
            while let Ok(event) = engine.events().try_recv() {
                hotkey_events.push(event);
            }
        }
        for event in hotkey_events {
            if let HotkeyEvent::HookError(msg) = event {
                error!("hotkey hook error: {msg}");
            }
        }

        let mut window_events = Vec::new();
        if let Some(watcher) = &self.window_watcher {
            while let Ok(event) = watcher.events().try_recv() {
                window_events.push(event);
            }
        }
        for event in window_events {
            self.handle_app_window_event(event);
        }

        self.poll_pending_app_routes();
        self.poll_config_reload();
        self.refresh_settings_child();
        self.poll_pending_tray_action();
        self.poll_pending_tray_menu_rebuild();
    }

    fn refresh_settings_child(&mut self) {
        let Some(child) = &mut self.settings_child else {
            return;
        };

        match child.try_wait() {
            Ok(None) => {}
            Ok(Some(_)) | Err(_) => self.settings_child = None,
        }
    }
}

impl ApplicationHandler<UserEvent> for BgwmApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.poll_background();
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(200),
        ));
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if matches!(cause, winit::event::StartCause::Init) {
            self.init_services();
        }
        self.poll_background();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Hotkey(action) => self.handle_hotkey(action),
            UserEvent::Tray(_event) => {}
            UserEvent::Menu(menu_event) => {
                let id = menu_event.id;
                if is_exit_menu(&id) {
                    self.shutdown();
                    event_loop.exit();
                    return;
                }
                if is_settings_menu(&id) {
                    self.open_settings();
                    return;
                }
                if is_add_workspace_menu(&id) {
                    self.pending_tray_action = Some(PendingTrayAction::AddWorkspace);
                    return;
                }
                if is_remove_workspace_menu(&id) {
                    self.pending_tray_action = Some(PendingTrayAction::RemoveWorkspace);
                    return;
                }
                if let Some(ws) = menu_workspace_from_id(&id) {
                    if let Err(e) = virtual_desktop::switch_to_workspace(ws) {
                        warn!("tray switch failed: {e}");
                    }
                }
            }
            UserEvent::Desktop(event) => self.handle_desktop_event(event),
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WinitWindowEvent,
    ) {
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("bgwm=info")),
        )
        .init();

    info!("starting BGWM");

    let config = config::load()?;
    let event_loop = EventLoop::<UserEvent>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    crate::tray::install_event_forwarders(proxy.clone());

    let mut app = BgwmApp::new(config);
    app.prepare(proxy);
    event_loop.run_app(&mut app)?;
    Ok(())
}
