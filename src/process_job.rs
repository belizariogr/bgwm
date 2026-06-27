use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use std::process::Child;

use tracing::warn;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_BREAKAWAY_OK, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

/// Windows job object that terminates assigned child processes when closed.
pub struct ChildProcessJob {
    handle: HANDLE,
}

impl ChildProcessJob {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(None, None)?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            // KILL_ON_JOB_CLOSE: assigned children die with the app.
            // BREAKAWAY_OK: lets the updater spawn the installer with
            // CREATE_BREAKAWAY_FROM_JOB so it survives when the app (and its
            // job) is terminated during an update.
            info.BasicLimitInformation.LimitFlags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
            Ok(Self { handle })
        }
    }

    pub fn assign(&self, child: &Child) -> windows::core::Result<()> {
        unsafe { AssignProcessToJobObject(self.handle, HANDLE(child.as_raw_handle() as _)) }
    }

    pub fn assign_child(&self, child: &Child) {
        if let Err(e) = self.assign(child) {
            warn!("failed to assign child process to exit job: {e}");
        }
    }

    pub fn terminate_all(&self) {
        unsafe {
            if let Err(e) = TerminateJobObject(self.handle, 0) {
                warn!("failed to terminate child processes: {e}");
            }
        }
    }
}

impl Drop for ChildProcessJob {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
