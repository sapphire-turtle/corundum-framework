//! The parent's console for a Windows GUI binary.
//!
//! An app that registers itself as a background service is built for the Windows GUI
//! subsystem: a console-subsystem executable started by the logon task opens a console
//! window, and closing that window stops the service. A GUI-subsystem process has no
//! console, so when the same binary is also the app's command line, what it prints goes
//! nowhere. The package's console shim (`corundum_framework_package::console::forward`)
//! fixes that from outside; a binary without the shim (installed with `cargo install`, or
//! run before there is a package) calls [`attach_parent`] instead, once it knows the start
//! is a command (a subcommand, `--help`) and before it prints.
//!
//! Not for a start that opens the window. Attached, the process shares the terminal's
//! console: the terminal's Ctrl+C and its closing end the process, and whatever it logs to
//! standard output or error lands in the shell. A window opened from a terminal stays
//! detached from it.
//!
//! [`attach_parent`] makes output visible; it does not make a full command line. `cmd` and
//! PowerShell do not wait for a GUI-subsystem process: the prompt comes back at once and
//! the output lands after it, the exit code is lost, and typed input goes to the shell.
//! The shim is the full command line.
//!
//! On macOS and Linux a binary has its terminal already, and [`attach_parent`] does nothing.

/// Gives this process its parent's console for the standard streams it lacks; `true` when
/// it attached. Does nothing outside Windows.
///
/// A process whose three standard handles are all there (started through the shim, with
/// its output piped or redirected to a file, or a console-subsystem binary) keeps them and
/// gets `false`. Otherwise, when the process that started it has a console (a terminal),
/// the missing handles, and only those, are opened on that console: a stdout redirected to
/// a file stays the file. Started without one (by the Task Scheduler, a double-click), it
/// opens nothing, so no window appears. Errors are not reported: there is no console to
/// report them on. Rust's standard streams look the handle up at every write, so
/// `println!` and `eprintln!` reach the console from the return on.
pub fn attach_parent() -> bool {
    #[cfg(windows)]
    {
        win32::attach()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
mod win32 {
    use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE,
        STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };
    use windows::core::{PCWSTR, w};

    /// Each standard handle and the console device that stands in for it.
    const STREAMS: [(STD_HANDLE, PCWSTR); 3] = [
        (STD_INPUT_HANDLE, w!("CONIN$")),
        (STD_OUTPUT_HANDLE, w!("CONOUT$")),
        (STD_ERROR_HANDLE, w!("CONOUT$")),
    ];

    pub fn attach() -> bool {
        // SAFETY: `GetStdHandle` only reads the process's handle table; windows 0.62 maps a
        // null handle and `INVALID_HANDLE_VALUE` alike to `Err`, which is "missing".
        let missing: Vec<_> = STREAMS
            .into_iter()
            .filter(|(id, _)| unsafe { GetStdHandle(*id) }.is_err())
            .collect();
        if missing.is_empty() {
            return false;
        }
        // SAFETY: no pointer is passed; it fails when the parent has no console or this
        // process has one already.
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_err() {
            return false;
        }
        for (id, device) in missing {
            // SAFETY: `device` is a static NUL-terminated wide string; no security
            // attributes or template are passed. The handle is kept for the process's
            // lifetime as its standard handle, so it is never closed.
            let opened = unsafe {
                CreateFileW(
                    device,
                    (GENERIC_READ | GENERIC_WRITE).0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(0),
                    None,
                )
            };
            if let Ok(handle) = opened {
                // SAFETY: `handle` is the console device just opened, valid for the process.
                let _ = unsafe { SetStdHandle(id, handle) };
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn with_every_standard_handle_present_it_does_not_attach() {
        // cargo gives a test process its handles (pipes or the terminal), as the shim does.
        assert!(!super::attach_parent());
        assert!(!super::attach_parent(), "a second call changes nothing either");
    }
}
