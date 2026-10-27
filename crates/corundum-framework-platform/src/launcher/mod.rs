//! An app's entry in the OS's app menu, so that a desktop app can be started like any
//! other: a `.desktop` file and an icon on Linux, a bundle on macOS, a Start Menu shortcut
//! on Windows.
//!
//! [`register`] writes the entry for the running executable, [`unregister`] removes it and
//! [`status`] says whether it is there and current. Each has an `_at` form that takes the
//! home directories and the executable instead of finding them, for tests and for tools
//! that register another program. Nothing is registered for a build under cargo's
//! `target` directory, nor when `CORUNDUM_NO_LAUNCHER` is set, and on iOS and Android there
//! is no app menu to register in.

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
mod linux;
// The pure parts are compiled everywhere under test so their tests run on any OS.
#[cfg(any(windows, test))]
mod windows;

use std::io;
use std::path::{Path, PathBuf};

/// What an app tells the OS about itself to be listed in the app menu.
#[derive(Clone, Copy, Debug)]
pub struct LauncherSpec {
    /// The app's identifier, from which the entry's file names are made.
    pub id: &'static str,
    /// The name the menu shows.
    pub name: &'static str,
    /// The one-line description the menu shows.
    pub description: &'static str,
    /// The menu category the entry is filed under.
    pub category: &'static str,
    /// The icon, as the bytes of an SVG file.
    pub icon_svg: &'static [u8],
    /// The prefix of the bundle's name where the OS has bundles.
    pub bundle_prefix: &'static str,
}

/// What [`register`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Registered {
    /// The entry was written, new or changed.
    Written,
    /// The entry was already as it would be written.
    Unchanged,
    /// Nothing was done: a cargo build, the variable, or an OS with no app menu.
    Skipped,
}

/// Whether an app's entry is in the app menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The entry is there and names the given executable.
    Registered,
    /// The entry is there but differs from what would be written now.
    Stale,
    /// There is no entry.
    Absent,
    /// There is no app menu to look in: the OS has none, or there is no home directory.
    Unsupported,
}

/// The directories the entries are kept under, as found in the environment.
#[derive(Clone, Debug)]
pub struct Root {
    /// The user's home directory.
    pub home: PathBuf,
    /// `$XDG_DATA_HOME`, where it is set.
    pub xdg_data_home: Option<PathBuf>,
    /// `%APPDATA%`, where it is set.
    pub appdata: Option<PathBuf>,
}

impl Root {
    /// The directories of the running user, or `None` where there is no home directory.
    pub fn detect() -> Option<Root> {
        Some(Root {
            home: ::dirs::home_dir()?,
            xdg_data_home: std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            appdata: std::env::var_os("APPDATA").map(PathBuf::from),
        })
    }
}

/// Registers the running executable, unless it is a cargo build or the variable
/// `CORUNDUM_NO_LAUNCHER` is set.
pub fn register(spec: &LauncherSpec) -> io::Result<Registered> {
    let exe = std::env::current_exe()?;
    if skipped(&exe, std::env::var_os("CORUNDUM_NO_LAUNCHER").is_some()) {
        return Ok(Registered::Skipped);
    }
    match Root::detect() {
        Some(root) => register_at(spec, &root, &exe),
        None => Ok(Registered::Skipped),
    }
}

/// Removes the app's entry, if there is one.
pub fn unregister(spec: &LauncherSpec) -> io::Result<()> {
    match Root::detect() {
        Some(root) => unregister_at(spec, &root),
        None => Ok(()),
    }
}

/// Whether the app's entry is there and names the running executable. On iOS and Android,
/// where there is no home directory or the executable cannot be found, and where
/// [`register`] would skip the executable (a cargo build, or `CORUNDUM_NO_LAUNCHER` set),
/// there is no entry to look at: [`Status::Unsupported`].
pub fn status(spec: &LauncherSpec) -> Status {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return Status::Unsupported;
    }
    status_in(
        spec,
        Root::detect(),
        std::env::current_exe(),
        std::env::var_os("CORUNDUM_NO_LAUNCHER").is_some(),
    )
}

/// [`status`] for the directories and the executable as found, or not found, and whether
/// the variable `CORUNDUM_NO_LAUNCHER` is set.
fn status_in(
    spec: &LauncherSpec,
    root: Option<Root>,
    exe: io::Result<PathBuf>,
    env_set: bool,
) -> Status {
    match (root, exe) {
        (Some(root), Ok(exe)) if !skipped(&exe, env_set) => status_at(spec, &root, &exe),
        _ => Status::Unsupported,
    }
}

/// Tells the OS that this process is the app `spec` describes, so that its window is grouped
/// in the taskbar with the app's entry however it was started. Call it before the first
/// window is created. Only Windows groups by an id the process sets; elsewhere it does
/// nothing.
pub fn set_process_app_id(spec: &LauncherSpec) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::set_process_app_id(spec)
    }
    #[cfg(not(windows))]
    {
        let _ = spec;
        Ok(())
    }
}

/// [`register`] for the given directories and executable.
pub fn register_at(spec: &LauncherSpec, root: &Root, exe: &Path) -> io::Result<Registered> {
    if skipped(exe, false) {
        return Ok(Registered::Skipped);
    }
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    {
        linux::register(spec, root, exe)
    }
    #[cfg(windows)]
    {
        windows::register(spec, root, exe)
    }
    #[cfg(not(any(
        all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        ),
        windows
    )))]
    {
        let _ = (spec, root);
        // Each OS's arm comes with its own module.
        Ok(Registered::Skipped)
    }
}

/// [`unregister`] for the given directories.
pub fn unregister_at(spec: &LauncherSpec, root: &Root) -> io::Result<()> {
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    {
        linux::unregister(spec, root)
    }
    #[cfg(windows)]
    {
        windows::unregister(spec, root)
    }
    #[cfg(not(any(
        all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        ),
        windows
    )))]
    {
        let _ = (spec, root);
        // Each OS's arm comes with its own module.
        Ok(())
    }
}

/// [`status`] for the given directories and executable.
pub fn status_at(spec: &LauncherSpec, root: &Root, exe: &Path) -> Status {
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    {
        linux::status(spec, root, exe)
    }
    #[cfg(windows)]
    {
        windows::status(spec, root, exe)
    }
    #[cfg(not(any(
        all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        ),
        windows
    )))]
    {
        let _ = (spec, root, exe);
        // Each OS's arm comes with its own module.
        Status::Unsupported
    }
}

/// Whether `exe` is not to be registered: a build under cargo's `target` directory
/// (`target/debug`, `target/release`, with a target triple between them when
/// cross-compiled), or `env_set` for the variable `CORUNDUM_NO_LAUNCHER`.
pub(crate) fn skipped(exe: &Path, env_set: bool) -> bool {
    if env_set {
        return true;
    }
    let parts: Vec<_> = exe.components().map(|c| c.as_os_str()).collect();
    let profile = |s: &std::ffi::OsStr| s == "debug" || s == "release";
    parts.iter().enumerate().any(|(i, c)| {
        *c == "target"
            && (parts.get(i + 1).is_some_and(|p| profile(p))
                || parts.get(i + 2).is_some_and(|p| profile(p)))
    })
}

/// Writes `bytes` to `path`, with its parent directories, unless the file already holds
/// them; `true` when it wrote.
// Only the Linux entry is a file; Windows writes its shortcut through COM instead, and the
// other OSes have no entry yet.
#[cfg_attr(
    not(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    )),
    allow(dead_code)
)]
pub(crate) fn write_if_changed(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn without_a_home_or_an_executable_there_is_no_app_menu() {
        let spec = LauncherSpec {
            id: "net.example.Test",
            name: "Test",
            description: "A test app",
            category: "Utility",
            icon_svg: b"<svg/>",
            bundle_prefix: "net.example",
        };
        let root = Root {
            home: PathBuf::from("/nonexistent/home"),
            xdg_data_home: None,
            appdata: None,
        };
        assert_eq!(
            status_in(&spec, None, Ok(PathBuf::from("/usr/bin/app")), false),
            Status::Unsupported
        );
        assert_eq!(
            status_in(&spec, Some(root), Err(io::Error::other("no exe")), false),
            Status::Unsupported
        );
    }

    #[test]
    fn a_skipped_executable_has_no_entry_to_look_at() {
        let spec = LauncherSpec {
            id: "net.example.Test",
            name: "Test",
            description: "A test app",
            category: "Utility",
            icon_svg: b"<svg/>",
            bundle_prefix: "net.example",
        };
        let root = || Root {
            home: PathBuf::from("/nonexistent/home"),
            xdg_data_home: None,
            appdata: None,
        };
        let cargo = || Ok(PathBuf::from("/src/app/target/debug/app"));
        assert_eq!(
            status_in(&spec, Some(root()), cargo(), false),
            Status::Unsupported
        );
        // The variable skips any path, so nothing is looked at even for an installed one.
        assert_eq!(
            status_in(&spec, Some(root()), Ok(PathBuf::from("/usr/bin/app")), true),
            Status::Unsupported
        );
    }

    #[test]
    fn a_cargo_build_is_skipped() {
        assert!(skipped(
            Path::new("/home/u/src/app/target/debug/app"),
            false
        ));
        assert!(skipped(
            Path::new("/home/u/src/app/target/release/app"),
            false
        ));
        assert!(skipped(
            Path::new("/home/u/src/app/target/x86_64-unknown-linux-gnu/release/app"),
            false
        ));
        assert!(!skipped(Path::new("/home/u/.local/bin/app"), false));
        assert!(!skipped(Path::new("/home/u/target/app"), false)); // a dir merely named target
    }

    #[test]
    fn the_variable_skips_any_path() {
        assert!(skipped(Path::new("/home/u/.local/bin/app"), true));
    }

    #[test]
    fn an_unchanged_file_is_not_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b/file");
        assert!(write_if_changed(&path, b"x").unwrap());
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert!(!write_if_changed(&path, b"x").unwrap());
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before
        );
        assert!(write_if_changed(&path, b"y").unwrap());
    }
}
