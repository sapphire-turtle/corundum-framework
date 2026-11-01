# A Windows GUI binary that still answers in a terminal

Issue: corundum-framework#89. Related: #75 (the renamed executable and its console shim), #51
(the Velopack groundwork), sapphire-turtle/asteria Plan 8c.

## Goal

An app whose desktop binary is also its command line (asteria's
`asteria-<app>-desktop.exe serve`, `api-key add`) registers itself as a background service
with `corundum-framework-service`. On Windows that is a scheduled task with a
`LogonTrigger` and an `InteractiveToken`, and a console-subsystem executable started that
way opens a console window at every logon; closing the window stops the server.

The fix the framework already assumes is a GUI-subsystem binary, with the Velopack
package's console shim (`console::forward`) on `PATH` for the command line. A binary
installed with `cargo install`, or run before there is a package, has no shim, so an app
keeps the console subsystem to keep its command line, and gets the window.

Success means:

- an app can be a GUI-subsystem binary everywhere, so the task it registers opens no
  window, whether or not a shim exists;
- started from a terminal without the shim, the same binary still prints its output there;
- with the shim, or with its output redirected, nothing changes;
- the documentation says what a console-subsystem service executable does on Windows, and
  what the shimless command line cannot do.

## Decisions

1. **Attach to the parent's console, not hide the task's window** (issue option 2, with
   option 3's documentation). The task XML cannot ask for a hidden window.
   `conhost.exe --headless` is undocumented and a pattern endpoint protection flags; a
   separate GUI-subsystem launcher is one more binary to ship, which `cargo install` brings
   back to the shim's problem. An S4U task needs rights a user does not have.
2. **The piece lives in `corundum-framework-platform`**, as `console::attach_parent()`
   behind a `console` feature, off by default as the platform crate's other OS pieces are.
   Not beside the shim in `corundum-framework-package`: it is for the binary without a
   package, and a `cargo install`ed app (the demo without its `velopack` feature, asteria's
   command-line install) does not depend on Velopack. It is the shim's counterpart: the
   shim gives a GUI binary a console from outside, `attach_parent` from inside.
3. **The shim stays the full command line.** A GUI-subsystem process started from `cmd` or
   PowerShell is not waited for: the prompt comes back at once, output lands after it, the
   exit code is lost and typed input goes to the shell. `attach_parent` is the fallback
   that makes output visible, not a replacement for the shim.
4. **No change to `ServiceSpec` or the task XML.** The task already starts the right file;
   what opens the window is the file's subsystem, which is the app's choice.

## `platform::console::attach_parent`

```rust
/// Gives this process its parent's console for the standard streams it lacks; `true` when
/// it attached. Does nothing outside Windows.
pub fn attach_parent() -> bool
```

An app calls it first thing in `main`, before anything prints.

On Windows:

1. Read the three standard handles with `GetStdHandle`. If none is missing (null or
   `INVALID_HANDLE_VALUE`), return `false`: the shim's handles, a pipe or a redirection to
   a file are kept as they are.
2. Otherwise call `AttachConsole(ATTACH_PARENT_PROCESS)`. If it fails (no parent console:
   the Task Scheduler, a double-click, Explorer), return `false`; no window is opened.
3. For each missing handle only, open the console (`CONIN$` for input, `CONOUT$` for
   output and error, read-write, shared) with `CreateFileW` and set it with `SetStdHandle`.
   A handle that is present (say stdout redirected to a file, stderr not) is left alone.
   Rust's standard library looks the handle up at every write, so `println!` and
   `eprintln!` reach the console from then on.
4. Return `true`.

Elsewhere it returns `false` at once: a macOS or Linux binary has its terminal already.

Errors are not reported: there is no console to report them on, and logging may not be set
up yet. A handle that cannot be opened stays missing, and the stream stays silent, as it is
today.

`corundum-framework-platform`'s `Cargo.toml` gains the feature
`console = ["windows/Win32_Security", "windows/Win32_System_Console"]` (`CreateFileW` needs `Win32_Security`) (`Win32_Foundation` and
`Win32_Storage_FileSystem`, for `CreateFileW`, are always there), and `src/lib.rs` declares
`#[cfg(feature = "console")] pub mod console;`. The umbrella crate forwards it as
`single-instance` is: `console = ["platform", "corundum-framework-platform/console"]`, with
a row in its feature table.

## Documentation

- `corundum-framework-service`: `service_exe()` and `install` stop assuming the shim. They
  say that on Windows the registered executable opens a console window at every logon if
  it is a console-subsystem binary, so an app that registers itself is a GUI-subsystem
  binary, with its command line through the shim or `attach_parent`. The README says the
  same in a sentence.
- `corundum-framework-platform`: the `console` module documentation says what
  `attach_parent` cannot do (decision 3), and the feature is described in `Cargo.toml`'s
  feature comment.
- `corundum-framework-package`: the `console` module documentation and the README point to
  `platform::console::attach_parent` for a binary without the shim.
- `README.md` and `README.ja.md`: next to the `windows_subsystem` note, that a binary
  which is also a command line calls `attach_parent` and is shipped with the shim.

## Demo

The demo's desktop dependency on `corundum-framework` turns on `console`, and
`demo/src/main.rs` calls `corundum_framework::platform::console::attach_parent()` first in
`main`, so a `cargo install`ed demo (no `velopack`, no shim) prints its `--help` and
`--version` in a terminal, and its daemon task opens no window.

## Testing

- A unit test in the platform crate, every OS, with `console` on: in a test process the standard handles are present (cargo's
  pipes), so `attach_parent()` returns `false`, and printing still works afterwards.
- The attaching path needs a GUI-subsystem child started from a console, which the test
  harness cannot be. It is checked by hand on Windows with the demo, built with
  `cargo install --path demo`:
  1. in `cmd`, a command that prints (`corundum-framework-demo --help` or the like) shows
     its output;
  2. `corundum-framework-demo --help > out.txt` writes the file and nothing to the console;
  3. the daemon registered from the demo starts at logon with no window;
  4. through the packaged shim, output, exit code and Ctrl+C behave as before.
- `cargo test` and `cargo clippy` for the workspace, and the Windows CI job, pass.

## Out of scope

- Hiding the window of a console-subsystem executable the task starts (issue option 1).
- Making a shimless GUI-subsystem binary behave as a full command line (waiting, exit
  code, input). That is the shim's job.
