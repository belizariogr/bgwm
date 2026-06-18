# BGWM — Better Windows Workspaces Manager

A fast, Windows-only desktop utility written in **Rust** for virtual desktop (workspace) management: global hotkeys, system-tray control, per-app routing, and window moves.

> **Status:** Early development. The Rust project scaffold is not yet in place; see [Roadmap](#roadmap) and [AGENTS.md](./AGENTS.md) for planned work.

## Features

- **Workspace hotkeys** — Switch between Windows virtual desktops with custom bindings, including the **Win (Super)** key.
- **Smart Win-key handling** — Registered combos (e.g. `Win+2`) override the OS default; pressing **Win alone** still opens the Start menu.
- **Tray indicator** — Shows the active workspace as a number inside a rounded square.
- **Tray menu** — Right-click to jump to any workspace.
- **Settings: hotkeys** — Detects how many desktops exist and lets you assign a switch hotkey per workspace.
- **Settings: app rules** — Map executables to workspaces (e.g. `chrome.exe` → Workspace 1). When the app’s main window opens, it is moved there and that desktop is activated.
- **Move window hotkeys** — Send the focused window to a workspace and switch to it (e.g. `Win+Shift+6`).

## Requirements

- **Windows 10 or 11**
- [Rust](https://rustup.rs/) (stable, 1.75+ recommended)
- **Visual Studio Build Tools** with *Desktop development with C++* (MSVC linker)

Optional, depending on the settings UI stack:

- [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) runtime

## Build & run

Once the Cargo project is initialized:

```powershell
# Install Rust (if needed)
# https://rustup.rs/

rustup default stable
rustup target add x86_64-pc-windows-msvc

# Clone and build
git clone https://github.com/<owner>/bgwm.git
cd bgwm
cargo build --release

# Run
cargo run --release
```

Development checks:

```powershell
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

Release binary (expected path after scaffold):

```text
target\release\bgwm.exe
```

## Configuration

Settings and bindings will be stored under the user app-data directory (e.g. `%LOCALAPPDATA%\bgwm\` or `%APPDATA%\bgwm\`). Exact path and format will be documented when Phase 1 (configuration) lands.

## How it works (high level)

BGWM runs as a single background process with a system-tray icon and a settings window. It uses native Windows APIs and hooks for low latency:

- Low-level keyboard hooks for global hotkeys (with Win-key passthrough rules)
- Virtual desktop APIs to switch desktops and move windows
- WinEvent hooks to detect new application windows for app-to-workspace rules

See [AGENTS.md](./AGENTS.md) for architecture details and agent-oriented guidelines.

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

Contributions are welcome. For agents and maintainers, read [AGENTS.md](./AGENTS.md) before starting work. Pick the next unchecked roadmap item, implement it with tests where applicable, and update the roadmap when done.

```powershell
cargo test
cargo clippy -- -D warnings
```

## License

MIT — see [LICENSE](./LICENSE).
