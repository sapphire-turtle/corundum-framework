//! A console shim for a Windows GUI binary.
//!
//! An app's GUI binary is built for the Windows GUI subsystem and has no console, so a typed
//! command prints nothing and returns at once. A tiny console binary on `PATH` calls
//! [`forward`]: it starts the GUI binary with the same arguments and standard streams, waits,
//! and exits with its exit code.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// Runs the GUI binary `gui_exe` that sits beside this executable with this process's
/// arguments, and exits with its exit code.
#[cfg(windows)]
pub fn forward(gui_exe: &str) -> ! {
    let exe = match std::env::current_exe() {
        Ok(me) => me.with_file_name(gui_exe),
        Err(_) => Path::new(gui_exe).to_path_buf(),
    };
    std::process::exit(forward_to(&exe, std::env::args_os().skip(1)))
}

/// Runs `exe` with `args` and this process's standard streams, waits, and returns its exit
/// code (1 when it ended without one or could not start).
///
/// On Windows the child joins a job object that kills it when the shim goes away, so Ctrl+C
/// in the console takes the GUI process with it.
pub fn forward_to(exe: &Path, args: impl IntoIterator<Item = OsString>) -> i32 {
    let mut child = match Command::new(exe).args(args).spawn() {
        Ok(child) => child,
        Err(e) => {
            eprintln!("cannot start {}: {e}", exe.display());
            return 1;
        }
    };
    #[cfg(windows)]
    let _job = job::kill_on_close(&child);
    match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("cannot wait for {}: {e}", exe.display());
            1
        }
    }
}

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    /// A job object handle; closing it (when the shim ends, however it ends) kills the child.
    pub struct Job(HANDLE);

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle came from `CreateJobObjectW` and is closed only here.
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    /// Puts `child` in a new job that kills it on close. Without one (a failure is only
    /// logged) the child still runs; it just outlives a Ctrl+C.
    pub fn kill_on_close(child: &Child) -> Option<Job> {
        match try_kill_on_close(child) {
            Ok(job) => Some(job),
            Err(e) => {
                tracing::warn!("the console shim runs without a job object: {e}");
                None
            }
        }
    }

    fn try_kill_on_close(child: &Child) -> windows::core::Result<Job> {
        // SAFETY: plain Win32 calls on handles that are valid for their duration; the info
        // struct outlives the call and its size is passed along.
        unsafe {
            let job = Job(CreateJobObjectW(None, None)?);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of_val(&info) as u32,
            )?;
            AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle()))?;
            Ok(job)
        }
    }
}
