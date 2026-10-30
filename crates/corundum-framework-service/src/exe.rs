//! The executable a service manager is told to start: the running one, as the outside sees
//! it.
//!
//! Usually that is [`std::env::current_exe`]. Inside an AppImage it is not: the image is
//! mounted afresh under `/tmp/.mount_*` at every start, so the path of the running binary
//! is gone by the next boot. The one path that lasts is the image itself, which starting
//! with the same arguments runs the same binary again. Which package the executable
//! belongs to, if any, is [`package::current`]'s answer, so a binary an app merely started
//! from its AppImage (and which inherited the AppImage's environment) is not mistaken for
//! the image.

use std::path::PathBuf;

use corundum_framework_platform::package::{self, PackageKind, PackageLayout};

/// The executable a service manager should start to run this one again: the AppImage
/// inside one, the running executable everywhere else.
pub fn service_exe() -> std::io::Result<PathBuf> {
    let running = std::env::current_exe()?;
    Ok(resolve(running, package::current().as_ref()))
}

/// [`service_exe`]'s rule, on values.
///
/// A Windows package is started from its `current` directory and a macOS one from inside
/// its `.app`; a link to either (macOS's `/usr/local/bin` command) would be a path the
/// package's updates leave dangling, so the executable is taken with its links resolved.
fn resolve(running: PathBuf, layout: Option<&PackageLayout>) -> PathBuf {
    match layout {
        Some(layout) if layout.kind == PackageKind::AppImage => layout.root.clone(),
        Some(_) => std::fs::canonicalize(&running).unwrap_or(running),
        None => running,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn layout(kind: PackageKind, root: &str, bin_dir: &str) -> PackageLayout {
        PackageLayout {
            kind,
            root: p(root),
            bin_dir: p(bin_dir),
        }
    }

    #[test]
    fn outside_a_package_it_is_the_running_executable() {
        assert_eq!(
            resolve(p("/usr/bin/sapphire-journal"), None),
            p("/usr/bin/sapphire-journal")
        );
    }

    #[test]
    fn in_an_appimage_it_is_the_image() {
        let layout = layout(
            PackageKind::AppImage,
            "/home/me/Apps/demo.AppImage",
            "/tmp/.mount_demoAb12/usr/bin",
        );
        assert_eq!(
            resolve(p("/tmp/.mount_demoAb12/usr/bin/demo"), Some(&layout)),
            p("/home/me/Apps/demo.AppImage")
        );
    }

    #[test]
    fn in_a_windows_package_it_is_the_executable_in_current() {
        // A path that does not exist cannot be resolved, so it comes back as given.
        let layout = layout(
            PackageKind::Windows,
            "/nowhere/Demo",
            "/nowhere/Demo/current",
        );
        assert_eq!(
            resolve(p("/nowhere/Demo/current/demo.exe"), Some(&layout)),
            p("/nowhere/Demo/current/demo.exe")
        );
    }
}
