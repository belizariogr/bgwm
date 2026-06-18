# Manual testing checklist

Run on **Windows 10 or 11** after building with `cargo run --release`.

## Tray

- [ ] Tray icon appears and shows the current workspace number (1-based).
- [ ] Icon updates immediately when switching desktops (Win+Tab, Ctrl+Win+Left/Right, or configured hotkeys).
- [ ] Right-click menu lists all workspaces; current workspace shows a checkmark.
- [ ] Selecting a workspace from the menu switches desktops.
- [ ] **Settings** opens the configuration window.
- [ ] **Exit** closes the app.

## Hotkeys (defaults: `Win+1..9` switch, `Win+Shift+1..9` move window)

- [ ] `Win+2` switches to workspace 2 without opening the Start menu.
- [ ] Pressing **Win alone** still opens the Start menu.
- [ ] Unregistered combos (e.g. `Win+E` if not configured) keep default OS behavior.
- [ ] `Win+Shift+N` moves the focused window to workspace N and activates it.

## Settings

- [ ] Desktop count matches Windows virtual desktop count after **Refresh desktop count**.
- [ ] Editing hotkeys and clicking **Save & Apply** persists to `%LOCALAPPDATA%\bgwm\config.json`.
- [ ] New hotkeys work without restarting the app.

## App rules

- [ ] Add a rule (e.g. `notepad.exe` → workspace 2).
- [ ] Launch the app; its main window moves to workspace 2 and that desktop is activated.

## Logging

Set `RUST_LOG=bgwm=debug` to see routing and desktop API activity in the console when running from a terminal.
