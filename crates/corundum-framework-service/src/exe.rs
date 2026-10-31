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
    Ok(resolve(running, package::current().as_ref(), cfg!(windows)))
}

/// [`service_exe`]'s rule, on values; `on_windows` is whether this runs on Windows.
///
/// A macOS package is started from inside its `.app`; a link to it (macOS's
/// `/usr/local/bin` command) would be a path the package's updates leave dangling, so the
/// executable is taken with its links resolved. Not on Windows — the rule
/// `corundum_framework_platform::package` follows too: the canonical form there is a `\\?\`
/// path, which would end up in the task's XML, and a Windows package is started from its
/// `current` directory, never through a link.
fn resolve(running: PathBuf, layout: Option<&PackageLayout>, on_windows: bool) -> PathBuf {
    match layout {
        Some(layout) if layout.kind == PackageKind::AppImage => layout.root.clone(),
        Some(_) if on_windows => running,
        Some(_) => std::fs::canonicalize(&running).unwrap_or(running),
        None => running,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

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
            resolve(p("/usr/bin/sapphire-journal"), None, false),
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
            resolve(p("/tmp/.mount_demoAb12/usr/bin/demo"), Some(&layout), false),
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
            resolve(p("/nowhere/Demo/current/demo.exe"), Some(&layout), true),
            p("/nowhere/Demo/current/demo.exe")
        );
    }

    // A path that exists and is not canonical: canonicalizing drops the `..`.
    fn roundabout() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/../Cargo.toml")
    }

    #[test]
    fn on_windows_a_package_path_is_not_canonicalized() {
        // The canonical form there is a `\\?\` path, which a task's XML must not carry.
        let layout = layout(PackageKind::Windows, "/somewhere", "/somewhere/current");
        assert_eq!(resolve(roundabout(), Some(&layout), true), roundabout());
    }

    #[test]
    fn elsewhere_a_package_path_has_its_links_resolved() {
        let layout = layout(PackageKind::MacBundle, "/somewhere", "/somewhere/MacOS");
        assert_eq!(
            resolve(roundabout(), Some(&layout), false),
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("Cargo.toml")
                .canonicalize()
                .unwrap()
        );
    }
}
