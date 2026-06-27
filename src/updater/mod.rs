//! Startup update check against GitHub releases.
//!
//! On launch the app asks GitHub for the latest published release. When a newer
//! version exists the user is prompted; on accept the installer is downloaded
//! and launched (the installer closes the running app to finish updating).
//!
//! Prompt throttling:
//! - If the user declines a version, that version is never prompted again.
//! - The timestamp of each prompt is persisted; a new prompt is only shown once
//!   at least two days have elapsed since the previous one.

mod http;

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;
use crate::config::{self, Config};

const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/belizariogr/bgwm/releases/latest";
const GITHUB_ACCEPT: &str = "application/vnd.github+json";
const UPDATE_STATE_FILE: &str = "update_state.json";
/// Minimum delay between update prompts (2 days).
const REPROMPT_INTERVAL_SECS: u64 = 2 * 24 * 60 * 60;
/// How often the background thread re-checks for updates.
const CHECK_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Persisted update prompt state (`%USERPROFILE%\.config\bgwm\update_state.json`).
#[derive(Debug, Default, Serialize, Deserialize)]
struct UpdateState {
    /// Last version the user explicitly declined; suppresses re-prompting it.
    #[serde(default)]
    declined_version: Option<String>,
    /// Unix timestamp (secs) of the last time the user was prompted.
    #[serde(default)]
    last_prompt_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone)]
struct ReleaseInfo {
    version: String,
    installer_url: String,
    installer_name: String,
}

/// Spawns a background thread that checks for updates on launch and then once
/// every hour. The check only runs while the user has auto-update enabled,
/// re-reading the setting each cycle so toggling it takes effect without a
/// restart. Network I/O and the (modal) prompt never block the event loop.
pub fn spawn_update_checker(proxy: EventLoopProxy<UserEvent>, config: Arc<Mutex<Config>>) {
    std::thread::spawn(move || loop {
        let enabled = match config.lock() {
            Ok(cfg) => cfg.auto_update,
            Err(_) => false,
        };
        if enabled {
            if let Err(e) = run_check(Some(&proxy), false) {
                warn!("update check failed: {e}");
            }
        }
        std::thread::sleep(CHECK_INTERVAL);
    });
}

/// Forces an update attempt requested by the user from the settings window.
///
/// Clears the throttle timestamp and the previously declined version so the
/// normal update routine runs immediately, then reuses it. Runs on a worker
/// thread so the settings UI stays responsive.
pub fn force_check() {
    std::thread::spawn(|| {
        clear_state();
        if let Err(e) = run_check(None, true) {
            warn!("manual update check failed: {e}");
            notify_info(
                "BGWM — Update",
                &format!("Could not check for updates.\n\n{e}"),
            );
        }
    });
}

/// Runs a single update check.
///
/// When `manual` is true the prompt throttle and declined-version guards are
/// skipped and an informational dialog is shown if there is no update, since
/// the check was explicitly requested by the user. On a successful download the
/// app is asked to exit via `proxy` (when available) so the installer can
/// replace the binary; manual checks rely on the installer to close the app.
fn run_check(proxy: Option<&EventLoopProxy<UserEvent>>, manual: bool) -> Result<(), String> {
    let Some(release) = fetch_latest_release()? else {
        if manual {
            notify_info(
                "BGWM — Update",
                "You are already running the latest version of BGWM.",
            );
        }
        return Ok(());
    };

    let mut state = load_state();

    if !manual {
        if state.declined_version.as_deref() == Some(release.version.as_str()) {
            info!(
                "update {} was previously declined; not prompting again",
                release.version
            );
            return Ok(());
        }

        if let Some(last_prompt_at) = state.last_prompt_at {
            let elapsed = now_secs().saturating_sub(last_prompt_at);
            if elapsed < REPROMPT_INTERVAL_SECS {
                info!("skipping update prompt; last prompt was less than 2 days ago");
                return Ok(());
            }
        }
    }

    // Record the prompt time before showing the message.
    state.last_prompt_at = Some(now_secs());
    save_state(&state);

    if prompt_user(&release) {
        match download_and_launch(&release) {
            Ok(()) => {
                info!("update launched; shutting down running BGWM instances");
                // Extra protection layer (independent of the installer's own
                // close logic): make sure every BGWM process is gone so nothing
                // keeps the executable locked. First terminate the *other*
                // instances (e.g. settings <-> background app), then close this
                // process — gracefully via the event loop when available.
                terminate_other_bgwm_instances();
                if let Some(proxy) = proxy {
                    let _ = proxy.send_event(UserEvent::QuitForUpdate);
                } else {
                    std::process::exit(0);
                }
            }
            Err(e) => warn!("failed to download or launch update: {e}"),
        }
    } else {
        info!("user declined update {}", release.version);
        state.declined_version = Some(release.version.clone());
        save_state(&state);
    }

    Ok(())
}

fn fetch_latest_release() -> Result<Option<ReleaseInfo>, String> {
    let body = http::get(GITHUB_LATEST_RELEASE_URL, Some(GITHUB_ACCEPT))?;
    let release: GithubRelease =
        serde_json::from_slice(&body).map_err(|e| format!("failed to parse release JSON: {e}"))?;

    if release.draft || release.prerelease {
        return Ok(None);
    }

    let latest = normalize_version(&release.tag_name);
    let current = env!("CARGO_PKG_VERSION");
    if !is_newer(&latest, current) {
        info!("BGWM is up to date (current {current}, latest {latest})");
        return Ok(None);
    }

    let Some(asset) = release
        .assets
        .into_iter()
        .find(|asset| is_installer_asset(&asset.name))
    else {
        warn!("release {latest} has no installer asset; skipping update");
        return Ok(None);
    };

    info!("update available: {latest} (current {current})");
    Ok(Some(ReleaseInfo {
        version: latest,
        installer_url: asset.browser_download_url,
        installer_name: asset.name,
    }))
}

fn download_and_launch(release: &ReleaseInfo) -> Result<(), String> {
    info!(
        "downloading update {} from {}",
        release.version, release.installer_url
    );
    let bytes = http::get(&release.installer_url, None)?;
    if bytes.is_empty() {
        return Err("downloaded installer was empty".into());
    }

    let path = std::env::temp_dir().join(sanitize_filename(&release.installer_name));
    std::fs::write(&path, &bytes).map_err(|e| format!("failed to write installer: {e}"))?;

    info!("launching installer {}", path.display());
    // Inno Setup `/SILENT`: shows the install progress window but skips the
    // wizard pages and confirmation prompts, so the user sees the setup running
    // without having to click through it. The installer closes the running app
    // and relaunches it afterwards.
    //
    // CREATE_BREAKAWAY_FROM_JOB detaches the installer from BGWM's job object.
    // Otherwise, when the settings window (a job member) launches the installer,
    // terminating the background app would close the job and kill the installer
    // mid-update. The job sets JOB_OBJECT_LIMIT_BREAKAWAY_OK to allow this.
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let args = ["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART"];
    let spawn = std::process::Command::new(&path)
        .args(args)
        .creation_flags(CREATE_BREAKAWAY_FROM_JOB)
        .spawn()
        // If breakaway isn't permitted (e.g. an external job), fall back to a
        // normal spawn rather than failing the update outright.
        .or_else(|_| std::process::Command::new(&path).args(args).spawn());
    spawn.map_err(|e| format!("failed to launch installer: {e}"))?;
    Ok(())
}

fn prompt_user(release: &ReleaseInfo) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONINFORMATION, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
    };

    let current = env!("CARGO_PKG_VERSION");
    let text = format!(
        "A new version of BGWM is available.\n\n\
         Current version: {current}\n\
         New version: {}\n\n\
         Do you want to download and install the update now?",
        release.version
    );
    let text_w = wide(&text);
    let title_w = wide("BGWM — Update available");

    let result = unsafe {
        MessageBoxW(
            None,
            PCWSTR(text_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            MB_YESNO | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST,
        )
    };
    result == IDYES
}

/// Shows an informational, OK-only message box.
fn notify_info(title: &str, text: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MB_TOPMOST,
    };

    let text_w = wide(text);
    let title_w = wide(title);
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND | MB_TOPMOST,
        );
    }
}

fn is_installer_asset(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.starts_with("bgwm-setup") && name.ends_with(".exe")
}

/// Strips a leading `v`/`V` from a release tag (`v0.9.0` -> `0.9.0`).
fn normalize_version(tag: &str) -> String {
    tag.trim().trim_start_matches(['v', 'V']).to_string()
}

/// Parses a semantic version into a comparable `(major, minor, patch)` tuple,
/// ignoring any pre-release/build metadata.
fn parse_version(version: &str) -> (u64, u64, u64) {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let mut parts = core
        .split('.')
        .map(|part| part.trim().parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

/// Keeps only the file name component to avoid path traversal from the asset name.
fn sanitize_filename(name: &str) -> String {
    let base = name.rsplit(['\\', '/']).next().unwrap_or(name).trim();
    if base.is_empty() {
        "bgwm-setup.exe".to_string()
    } else {
        base.to_string()
    }
}

fn state_path() -> PathBuf {
    config::config_dir().join(UPDATE_STATE_FILE)
}

/// Resets the persisted prompt throttle and declined-version, so the next check
/// prompts again regardless of prior history.
fn clear_state() {
    save_state(&UpdateState::default());
}

fn load_state() -> UpdateState {
    match std::fs::read_to_string(state_path()) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => UpdateState::default(),
    }
}

fn save_state(state: &UpdateState) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(state) {
        Ok(data) => {
            if let Err(e) = std::fs::write(&path, data) {
                warn!("failed to save update state: {e}");
            }
        }
        Err(e) => warn!("failed to serialize update state: {e}"),
    }
}

/// Force-terminates every other running BGWM process (background app and/or
/// settings window), leaving the current process untouched. The installer
/// (`bgwm-setup-*.exe`) has a different image name and is never matched.
fn terminate_other_bgwm_instances() {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, TerminateProcess, PROCESS_TERMINATE,
    };

    let current_pid = unsafe { GetCurrentProcessId() };
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return;
        };
        if snapshot.is_invalid() {
            return;
        }

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let end = entry
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
                if entry.th32ProcessID != current_pid && name.eq_ignore_ascii_case("bgwm.exe") {
                    if let Ok(handle) = OpenProcess(PROCESS_TERMINATE, false, entry.th32ProcessID) {
                        let _ = TerminateProcess(handle, 0);
                        let _ = CloseHandle(handle);
                        info!("terminated BGWM instance pid {}", entry.th32ProcessID);
                    }
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_tag_prefix() {
        assert_eq!(normalize_version("v1.2.3"), "1.2.3");
        assert_eq!(normalize_version("V0.8.1"), "0.8.1");
        assert_eq!(normalize_version(" 1.0.0 "), "1.0.0");
    }

    #[test]
    fn parses_version_components() {
        assert_eq!(parse_version("1.2.3"), (1, 2, 3));
        assert_eq!(parse_version("0.8"), (0, 8, 0));
        assert_eq!(parse_version("2"), (2, 0, 0));
        assert_eq!(parse_version("1.2.3-beta.1"), (1, 2, 3));
        assert_eq!(parse_version("1.2.3+build5"), (1, 2, 3));
    }

    #[test]
    fn detects_newer_versions() {
        assert!(is_newer("0.9.0", "0.8.1"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.8.2", "0.8.1"));
        assert!(!is_newer("0.8.1", "0.8.1"));
        assert!(!is_newer("0.8.0", "0.8.1"));
    }

    #[test]
    fn recognizes_installer_assets() {
        assert!(is_installer_asset("bgwm-setup-0.9.0.exe"));
        assert!(is_installer_asset("BGWM-Setup-1.0.0.EXE"));
        assert!(!is_installer_asset("bgwm-0.9.0.zip"));
        assert!(!is_installer_asset("source-code.tar.gz"));
    }

    #[test]
    fn sanitizes_asset_filenames() {
        assert_eq!(
            sanitize_filename("bgwm-setup-1.0.0.exe"),
            "bgwm-setup-1.0.0.exe"
        );
        assert_eq!(
            sanitize_filename(r"..\..\evil\bgwm-setup.exe"),
            "bgwm-setup.exe"
        );
        assert_eq!(sanitize_filename(""), "bgwm-setup.exe");
    }

    #[test]
    fn update_state_round_trips() {
        let state = UpdateState {
            declined_version: Some("0.9.0".into()),
            last_prompt_at: Some(1_700_000_000),
        };
        let json = serde_json::to_string(&state).unwrap();
        let parsed: UpdateState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.declined_version, state.declined_version);
        assert_eq!(parsed.last_prompt_at, state.last_prompt_at);
    }

    #[test]
    fn update_state_defaults_when_empty() {
        let parsed: UpdateState = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed.declined_version, None);
        assert_eq!(parsed.last_prompt_at, None);
    }
}
