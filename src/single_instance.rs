use thiserror::Error;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex};

/// Session-local mutex name; prevents multiple tray/background instances.
const MUTEX_NAME: &str = "Local\\BGWM.SingleInstance";

#[derive(Debug, Error)]
pub enum SingleInstanceError {
    #[error("BGWM is already running")]
    AlreadyRunning,
    #[error("failed to create single-instance mutex: {0}")]
    CreateFailed(#[from] windows::core::Error),
}

/// Holds an exclusive named mutex for the lifetime of the BGWM background process.
pub struct SingleInstance {
    handle: HANDLE,
}

impl SingleInstance {
    pub fn acquire() -> Result<Self, SingleInstanceError> {
        let name = wide_null(MUTEX_NAME);
        unsafe {
            let handle = CreateMutexW(None, true, PCWSTR(name.as_ptr()))?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(handle);
                return Err(SingleInstanceError::AlreadyRunning);
            }
            Ok(Self { handle })
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

fn wide_null(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
