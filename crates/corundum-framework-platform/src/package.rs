//! Whether the running executable was installed from a package (cargo-packager's), told
//! from the files around it.
//!
//! Plain path tests, so that an app can ask without linking the updater (a cargo build, an
//! unpackaged binary and a phone all answer `None`). Every OS's rule is checked on every OS,
//! so [`current_with`] is testable anywhere.
//!
//! The executable is looked at with its links resolved: on macOS a start through the
//! command line's `/usr/local/bin` link reports the link as the executable, and only the
//! file it points at is inside the `.app`.

use std::path::{Path, PathBuf};

/// The kind of package the executable was installed from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageKind {
    /// The NSIS installer's install directory: `app.exe` beside its `uninstall.exe`.
    Windows,
    /// `X.app/Contents/MacOS/`, in a bundle with its `Info.plist`.
    MacBundle,
    /// The `usr/bin` of a running AppImage (`$APPIMAGE` set).
    AppImage,
}

/// Where a package keeps its files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageLayout {
    pub kind: PackageKind,
    /// The install directory on Windows, the `.app` on macOS, the AppImage file on Linux.
    pub root: PathBuf,
    /// The directory holding the executable.
    pub bin_dir: PathBuf,
}

/// The package the running executable belongs to, or `None` when it is not installed from
/// one (and always on iOS and Android).
pub fn current() -> Option<PackageLayout> {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return None;
    }
    let exe = current_exe()?;
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    current_with(&exe, appimage.as_deref())
}

/// The running executable as [`current`] looks at it: with its links resolved on macOS
/// and Linux.
pub fn current_exe() -> Option<PathBuf> {
    std::env::current_exe().ok().map(|exe| resolved(&exe))
}

/// [`current`] for a given executable path and `$APPIMAGE` value. Links in `exe` are
/// resolved first, but on Windows.
pub fn current_with(exe: &Path, appimage: Option<&Path>) -> Option<PackageLayout> {
    let exe = &resolved(exe);
    windows(exe)
        .or_else(|| mac_bundle(exe))
        .or_else(|| app_image(exe, appimage))
}

/// Whether `exe` was built by cargo and runs from its `target` directory (`target/debug`,
/// `target/release`, with a target triple between them when cross-compiled). Such a build
/// is a development one: nothing the system keeps (a launcher entry, a background service)
/// should point into a directory the next `cargo clean` empties.
pub fn is_cargo_build(exe: &Path) -> bool {
    let parts: Vec<_> = exe.components().map(|c| c.as_os_str()).collect();
    let profile = |s: &std::ffi::OsStr| s == "debug" || s == "release";
    parts.iter().enumerate().any(|(i, c)| {
        *c == "target"
            && (parts.get(i + 1).is_some_and(|p| profile(p))
                || parts.get(i + 2).is_some_and(|p| profile(p)))
    })
}

/// `exe` with its links resolved, or as it is when that fails. Not on Windows: the canonical
/// form there is a `\\?\` path, which would end up on the user `Path`, and a Windows
/// package is started from its install directory, never through a link.
fn resolved(exe: &Path) -> PathBuf {
    if cfg!(windows) {
        return exe.to_path_buf();
    }
    exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf())
}

fn windows(exe: &Path) -> Option<PackageLayout> {
    let dir = exe.parent()?;
    dir.join("uninstall.exe").exists().then(|| PackageLayout {
        kind: PackageKind::Windows,
        root: dir.to_path_buf(),
        bin_dir: dir.to_path_buf(),
    })
}

fn mac_bundle(exe: &Path) -> Option<PackageLayout> {
    let root = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))?;
    let bin_dir = root.join("Contents").join("MacOS");
    root.join("Contents")
        .join("Info.plist")
        .exists()
        .then(|| PackageLayout {
            kind: PackageKind::MacBundle,
            root: root.to_path_buf(),
            bin_dir,
        })
}

fn app_image(exe: &Path, appimage: Option<&Path>) -> Option<PackageLayout> {
    let appimage = appimage?;
    let bin_dir = exe.parent()?;
    bin_dir.ends_with("usr/bin").then(|| PackageLayout {
        kind: PackageKind::AppImage,
        root: appimage.to_path_buf(),
        bin_dir: bin_dir.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// A temporary directory and its path as `current_with` sees it: macOS's temporary
    /// directory is under a link.
    fn tempdir() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let path = resolved(d.path());
        (d, path)
    }

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }

    #[test]
    fn a_windows_install_holds_the_installers_uninstall_exe() {
        let (_d, d) = tempdir();
        touch(&d.join("uninstall.exe"));
        let exe = d.join("app.exe");
        touch(&exe);
        assert_eq!(
            current_with(&exe, None),
            Some(PackageLayout {
                kind: PackageKind::Windows,
                root: d.clone(),
                bin_dir: d.clone(),
            })
        );
    }

    #[test]
    fn an_exe_without_uninstall_exe_is_not_a_package() {
        let (_d, d) = tempdir();
        let exe = d.join("app.exe");
        touch(&exe);
        assert_eq!(current_with(&exe, None), None);
    }

    #[test]
    fn a_mac_bundle_has_an_info_plist() {
        let (_d, d) = tempdir();
        touch(&d.join("Demo.app/Contents/Info.plist"));
        let macos = d.join("Demo.app/Contents/MacOS");
        let exe = macos.join("demo");
        touch(&exe);
        assert_eq!(
            current_with(&exe, None),
            Some(PackageLayout {
                kind: PackageKind::MacBundle,
                root: d.join("Demo.app"),
                bin_dir: macos,
            })
        );
    }

    #[test]
    fn an_app_directory_without_info_plist_is_not_a_package() {
        let (_d, d) = tempdir();
        let exe = d.join("Demo.app/Contents/MacOS/demo");
        touch(&exe);
        assert_eq!(current_with(&exe, None), None);
    }

    #[test]
    fn an_appimage_needs_the_variable_and_usr_bin() {
        let (_m, m) = tempdir();
        let bin = m.join("usr/bin");
        let exe = bin.join("demo");
        touch(&exe);
        let image = Path::new("/home/u/Demo.AppImage");
        assert_eq!(
            current_with(&exe, Some(image)),
            Some(PackageLayout {
                kind: PackageKind::AppImage,
                root: image.to_path_buf(),
                bin_dir: bin,
            })
        );
        assert_eq!(current_with(&exe, None), None);
    }

    #[test]
    fn a_binary_outside_usr_bin_is_not_the_appimage_even_with_the_variable() {
        let (_m, m) = tempdir();
        let exe = m.join("elsewhere/demo");
        touch(&exe);
        assert_eq!(
            current_with(&exe, Some(Path::new("/home/u/Demo.AppImage"))),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_link_to_the_binary_in_a_mac_bundle_is_in_the_package() {
        let (_d, d) = tempdir();
        touch(&d.join("Demo.app/Contents/Info.plist"));
        let macos = d.join("Demo.app/Contents/MacOS");
        let exe = macos.join("demo");
        touch(&exe);
        let link = d.join("bin/demo");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&exe, &link).unwrap();
        assert_eq!(
            current_with(&link, None),
            Some(PackageLayout {
                kind: PackageKind::MacBundle,
                root: d.join("Demo.app"),
                bin_dir: macos,
            })
        );
    }

    #[test]
    fn a_cargo_build_is_not_a_package() {
        let (_d, d) = tempdir();
        let exe = d.join("target/debug/demo");
        touch(&exe);
        assert_eq!(current_with(&exe, None), None);
    }

    #[test]
    fn a_build_under_a_target_profile_is_a_cargo_build() {
        assert!(is_cargo_build(Path::new("/w/target/debug/app")));
        assert!(is_cargo_build(Path::new(
            "/w/target/x86_64-unknown-linux-gnu/release/app"
        )));
    }

    #[test]
    fn an_installed_program_is_not_a_cargo_build() {
        assert!(!is_cargo_build(Path::new("/opt/app")));
        // A directory merely named `target`, with no profile under it.
        assert!(!is_cargo_build(Path::new("/w/target/app")));
    }
}
