# BGWM — Better Windows Workspaces Manager

Windows-only desktop utility written in **Rust** for fast virtual-desktop (workspace) switching, window routing, and system-tray control.

## Overview

BGWM enhances Windows virtual desktops (workspaces) with configurable global hotkeys, a system-tray indicator, per-app workspace assignment, and window-to-workspace moves. The app must feel instant: prefer native Windows APIs and low-level hooks over polling or heavy abstractions.

**Platform:** Windows 10/11 only  
**Language:** Rust (edition 2021+)  
**Runtime:** Single background process with system-tray UI and a settings window

---

## Core Features

### 1. Workspace switch hotkeys

- User-defined hotkeys to switch quickly between Windows virtual desktops.
- Hotkeys may include the **Windows (Super) key**.
- **Super-key behavior:**
  - When a registered hotkey uses Super (e.g. `Win+E`), only that combo is intercepted; the default OS action for that combo is suppressed (Explorer does not open).
  - Pressing **Super alone** must still open the Start menu normally.
  - Implement via a low-level keyboard hook (`SetWindowsHookExW` / `WH_KEYBOARD_LL`) or equivalent, registering combos explicitly and passing through unregistered keys.
  - Take care to not lock Win hotkey.

### 2. Tray icon and App Icon — current workspace indicator

- Show a system-tray icon reflecting the **active workspace index**.
- Visual: a **number inside a rounded square** (e.g. workspace 3 → `3` in a rounded rect).
- Update immediately on workspace change (listen for desktop switch events, not periodic polling).
- Use the assets/tray/ref to get the numbers to show witch Workspace is currently being displayed. 
- The app icon is saved in SVG format on assets/icon/bgwm.svg.

### 3. Tray context menu

- Right-click tray icon → menu listing all workspaces.
- Selecting an item switches to that workspace.
- Optional: show current workspace with a checkmark or highlight.

### 4. Settings — hotkey configuration

- Settings UI reads the **current number of virtual desktops** from Windows.
- For each workspace, allow the user to assign a **switch-to** hotkey.
- Persist bindings to disk (e.g. JSON or TOML under `%APPDATA%` or `%LOCALAPPDATA%`).
- Re-register hotkeys when desktop count or bindings change.

### 5. Settings — app-to-workspace rules

- Separate tab/screen to map **executables** to target workspaces.
- Example: `chrome.exe` → always Workspace 1.
- **On launch behavior:** when a configured app’s **main window** appears:
  1. Move that window to the configured workspace.
  2. Switch to that workspace so the user sees the new window.
- Detect new windows via WinEvent hooks (`SetWinEventHook`) or equivalent; match by process executable path/name.

### 6. Move focused window to workspace hotkeys

- Hotkeys to move the **currently focused window** to a given workspace and **activate that workspace**.
- Example: `Win+Shift+6` → move focused window to Workspace 6 and switch to Workspace 6.
- Use Windows virtual-desktop APIs (e.g. `IVirtualDesktopManager`, `IVirtualDesktop`, or documented/undocumented interfaces as needed) for move + switch.

### 7. Performance and Windows integration

- Prefer **Windows APIs and hooks** over timers/polling:
  - Low-level keyboard hook for hotkeys (with Super passthrough rules above).
  - `RegisterHotKey` where sufficient; hook when Super or custom suppression is required.
  - Virtual desktop COM interfaces / `VirtualDesktopAccessor`-style patterns if stable.
  - WinEvent hooks for window creation/focus.
- Minimize allocations on hot paths; keep hook callbacks short (defer work to a channel/thread).
- Fail gracefully when APIs are unavailable (log + user-visible error in settings).

### 8. Code quality

- Idiomatic Rust: clear module boundaries, `Result` for errors, no `unwrap()` in production paths.
- Separate concerns: `hotkeys`, `virtual_desktop`, `tray`, `settings`, `config`, `window_tracking`.
- Document non-obvious Windows API usage and safety assumptions (`unsafe` blocks small and commented).
- Use `clippy` and `rustfmt`; keep public APIs typed and testable where possible.

### 9. Tests

- **Unit tests:** config load/save, hotkey parsing/serialization, rule matching (executable → workspace).
- **Integration tests:** mock or stub Windows layers where feasible; test pure logic without HWND.
- **Manual / E2E checklist:** document in README or `docs/testing.md` for tray, hooks, and desktop APIs (hard to automate in CI without Windows runners).
- Run `cargo test` in CI on `windows-latest` when GitHub Actions is added.

### 10. Roadmap (agent tracking)

Update checkboxes as work completes. Do not mark done without implemented, reviewable code.

#### Phase 0 — Project scaffold

- [ ] Initialize Cargo workspace/binary (`bgwm` crate)
- [ ] Add `README.md` with build/run instructions
- [ ] Pin Rust toolchain (`rust-toolchain.toml`) and Windows target
- [ ] CI: `cargo fmt --check`, `clippy`, `test` on Windows

#### Phase 1 — Configuration and persistence

- [ ] Config schema: workspace hotkeys, move-window hotkeys, app rules
- [ ] Load/save config from user app data directory
- [ ] Unit tests for config round-trip and validation

#### Phase 2 — Virtual desktop abstraction

- [ ] Detect workspace count and current index
- [ ] Switch to workspace by index
- [ ] Move window to workspace by index
- [ ] Subscribe to desktop change notifications (tray updates)

#### Phase 3 — Global hotkeys

- [ ] Hotkey registration and parsing (including Super)
- [ ] Low-level hook: suppress combo defaults, passthrough Super alone
- [ ] Wire switch-workspace and move-window actions
- [ ] Tests for hotkey string parse/normalize

#### Phase 4 — System tray

- [ ] Tray icon with dynamic workspace number (rounded square asset or programmatic)
- [ ] Context menu: list workspaces and switch
- [ ] Reflect current workspace on change

#### Phase 5 — Settings UI

- [ ] Window with tabs: **Hotkeys** and **App rules**
- [ ] Hotkeys tab: enumerate desktops, assign per-workspace switch/move bindings
- [ ] App rules tab: add/remove executable → workspace mappings
- [ ] Apply/reload hotkeys without full restart where possible

#### Phase 6 — App launch routing

- [ ] WinEvent (or equivalent) for new main windows
- [ ] Match process executable against rules
- [ ] Move window + switch workspace on match
- [ ] Tests for rule matching logic

#### Phase 7 — Polish and release

- [ ] Logging (e.g. `tracing`) and error surfaces in UI
- [ ] Installer or portable build notes
- [ ] Performance pass on hook callbacks
- [ ] Full manual test pass on Windows 10 and 11

---

## Suggested Architecture

```
bgwm/
├── src/
│   ├── main.rs              # Entry, tokio/async or message loop as needed
│   ├── config/              # Schema, load, save
│   ├── hotkeys/             # Registration, LL hook, Super passthrough
│   ├── virtual_desktop/     # COM/API wrapper, switch, move, events
│   ├── window_tracking/     # Focus, WinEvent, executable resolution
│   ├── tray/                # Icon, menu, workspace badge
│   └── settings/            # UI (e.g. egui, slint, or native Win32)
├── assets/                  # Tray icon templates if not drawn in code
├── tests/                   # Integration tests
└── AGENTS.md                # This file
```

**Likely dependencies (evaluate and adjust):**


| Area              | Crates / APIs                                             |
| ----------------- | --------------------------------------------------------- |
| Windows FFI       | `windows` crate                                           |
| Tray              | `tray-icon`, or raw `Shell_NotifyIcon`                    |
| UI                | `egui` + `eframe`, or `windows-rs` dialogs for minimal UI |
| Config            | `serde`, `serde_json` or `toml`                           |
| Logging           | `tracing`, `tracing-subscriber`                           |
| Async / threading | `crossbeam-channel` for hook → worker messages            |


Avoid unnecessary async runtime if a single-threaded Win32 message loop suffices.

---

## Build Requirements

### Toolchain

- **Rust:** stable (1.75+ recommended), via [rustup](https://rustup.rs/)
- **Target:** `x86_64-pc-windows-msvc` (default on Windows)

```powershell
rustup default stable
rustup target add x86_64-pc-windows-msvc
```

### Build

```powershell
cargo build --release
cargo test
cargo clippy -- -D warnings
```

### Optional (agents may install if missing)

- **Visual Studio Build Tools** with “Desktop development with C++” (MSVC linker)
- **WebView2** runtime if the settings UI uses it (some GUI stacks require it)

Verify build after adding dependencies:

```powershell
cargo check
cargo test
```

---

## Agent Guidelines

1. **Read the roadmap** before starting work; pick the next unchecked phase or task.
2. **Update roadmap checkboxes** in this file when completing a task (one PR/commit scope per logical item when possible).
3. **Windows-only:** do not add cross-platform abstractions unless they simplify testing of pure logic.
4. **Super key:** always verify alone vs combo behavior manually after hotkey changes.
5. **Hooks:** keep callback bodies minimal; post events to the main loop via channel.
6. **Tests:** add unit tests with each feature; run `cargo test` before marking roadmap items done.
7. **Secrets:** never commit user config from local machines; use `.gitignore` for `%APPDATA%` copies if synced locally.
8. **Commits:** small, focused messages; reference roadmap phase when relevant.

---

## Key Behavioral Examples


| Action                           | Expected behavior                                                      |
| -------------------------------- | ---------------------------------------------------------------------- |
| `Win+2` (configured)             | Switch to Workspace 2; Start menu does not open                        |
| `Win` alone                      | Start menu opens (not intercepted)                                     |
| `Win+E` (not configured)         | Default OS behavior (Explorer)                                         |
| `Win+E` (registered to switch)   | Switch workspace; Explorer does not open                               |
| Launch `chrome.exe` (rule: WS 1) | Main window moves to WS 1; desktop switches to WS 1                    |
| `Win+Shift+6` (move binding)     | Focused window → WS 6; activate WS 6                                   |
| Tray shows `4`                   | User is on Workspace 4 (1-based or 0-based — pick one, document in UI) |


Document index base (0 vs 1) in UI labels consistently.

---

## References

- [Virtual desktops on Windows](https://learn.microsoft.com/en-us/windows/win32/taskbar/virtual-desktops)
- [Low-level keyboard hooks](https://learn.microsoft.com/en-us/windows//previous-versions/windows/desktop/legacy/ms644985(v=vs.85))
- [SetWinEventHook](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook)
- Community: Virtual Desktop Accessor patterns (use with care; prefer documented APIs when possible)

---

## Glossary

- **Workspace:** Windows virtual desktop (not VS Code / IDE workspace).
- **Super / Win key:** Left or right Windows logo key.
- **Main window:** Primary top-level HWND for an app (exclude tooltips, splash-only windows where detectable).

