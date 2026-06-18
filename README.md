# BGWM — Better Windows Workspaces Manager

A fast, Windows-only desktop utility written in **Rust** for virtual desktop (workspace) management: global hotkeys, system-tray control, per-app routing, and window moves.

## Features

- **Workspace hotkeys** — Switch between Windows virtual desktops with custom bindings, including the **Win (Super)** key.
- **Smart Win-key handling** — Registered combos (e.g. `Win+2`) override the OS default; pressing **Win alone** still opens the Start menu.
- **Tray indicator** — Shows the active workspace number using assets from `assets/tray/ref/`.
- **Tray menu** — Right-click to jump to any workspace.
- **Settings: hotkeys** — Detects how many desktops exist and lets you assign switch/move hotkeys per workspace.
- **Settings: app rules** — Map executables to workspaces (e.g. `chrome.exe` → Workspace 1). When the app’s main window opens, it is moved there and that desktop is activated.
- **Move window hotkeys** — Send the focused window to a workspace and switch to it (default: `Win+Shift+1..9`).

## Requirements

- **Windows 10 or 11**
- [Rust](https://rustup.rs/) stable (1.75+)
- **Visual Studio Build Tools** with *Desktop development with C++* (MSVC linker)
- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) runtime (used by the settings UI)

Virtual desktop switching uses the [`winvd`](https://crates.io/crates/winvd) crate (undocumented Windows COM APIs). Some Windows builds may require recent updates for full compatibility.

## Build & run

```powershell
rustup default stable
rustup target add x86_64-pc-windows-msvc

git clone https://github.com/belizariogr/bgwm.git
cd bgwm
cargo build --release
cargo run --release
```

Development checks:

```powershell
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

Release binary:

```text
target\release\bgwm.exe
```

## Configuration

Settings are stored at:

```text
%LOCALAPPDATA%\bgwm\config.json
```

Workspace indices in the UI are **1-based** (Workspace 1 = first virtual desktop).

Default bindings:

| Action | Default hotkey |
|--------|----------------|
| Switch to workspace N | `Win+N` (N = 1..9) |
| Move focused window to workspace N | `Win+Shift+N` |

Example config fragment:

```json
{
  "version": 1,
  "switch_hotkeys": { "1": "Win+1", "2": "Win+2" },
  "move_hotkeys": { "1": "Win+Shift+1" },
  "app_rules": [
    { "executable": "chrome.exe", "workspace": 1 }
  ]
}
```

Open **Settings** from the tray menu to edit bindings and app rules.

## Architecture

BGWM runs as a single background process with a system-tray icon. It uses:

- Low-level keyboard hook (`WH_KEYBOARD_LL`) for global hotkeys with Win-key passthrough rules
- [`winvd`](https://github.com/ciantic/VirtualDesktopAccessor/tree/rust/) for desktop switch/move and change notifications
- WinEvent hooks for new application windows (app-to-workspace rules)
- `winit` event loop + `tray-icon` for the tray; `eframe`/`egui` for settings

See [AGENTS.md](./AGENTS.md) for module layout and agent guidelines.

## Testing

- Automated: `cargo test` (config, hotkey parsing, rule matching)
- Manual checklist: [docs/testing.md](./docs/testing.md)

## Roadmap

| Phase | Scope |
|-------|--------|
| 0 | Project scaffold, README, toolchain, CI |
| 1 | Configuration and persistence |
| 2 | Virtual desktop abstraction |
| 3 | Global hotkeys |
| 4 | System tray |
| 5 | Settings UI |
| 6 | App launch routing |
| 7 | Polish and release |

Detailed checklist: [AGENTS.md § Roadmap](./AGENTS.md#10-roadmap-agent-tracking).

## Contributing

Read [AGENTS.md](./AGENTS.md) before starting work. Pick the next unchecked roadmap item, implement with tests where applicable, and update the roadmap when done.

## License

MIT — see [LICENSE](./LICENSE).
