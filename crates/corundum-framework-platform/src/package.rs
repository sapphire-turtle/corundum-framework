//! Whether the running executable is a Velopack package, told from the files around it.
//!
//! Velopack's own locator decides this in its updater; the checks are repeated here, with
//! plain path tests, so that an app can ask without linking the updater (a cargo build, an
//! unpackaged binary and a phone all answer `None`). Every OS's rule is checked on every OS,
//! so [`current_with`] is testable anywhere.

use std::path::{Path, PathBuf};

/// The kind of package the executable was installed from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageKind {
    /// `<root>/current/app.exe` beside `<root>/Update.exe`.
    Windows,
    /// `X.app/Contents/MacOS/` holding `UpdateMac`.
    MacBundle,
    /// An AppImage whose `usr/bin` holds `UpdateNix`.
    AppImage,
}

/// Where a package keeps its files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageLayout {
    pub kind: PackageKind,
    /// The install directory on Windows, the `.app` on macOS, the AppImage file on Linux.
    pub root: PathBuf,
    /// The directory holding the executable and Velopack's updater binary.
    pub bin_dir: PathBuf,
}

/// The package the running executable belongs to, or `None` when it is not installed from
/// one (and always on iOS and Android).
pub fn current() -> Option<PackageLayout> {
    if cfg!(any(target_os = "ios", target_os = "android")) {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    current_with(&exe, appimage.as_deref())
}

/// [`current`] for a given executable path and `$APPIMAGE` value.
pub fn current_with(exe: &Path, appimage: Option<&Path>) -> Option<PackageLayout> {
    windows(exe)
        .or_else(|| mac_bundle(exe))
        .or_else(|| app_image(exe, appimage))
}

fn windows(exe: &Path) -> Option<PackageLayout> {
    let bin_dir = exe.parent()?;
    if bin_dir.file_name()? != "current" {
        return None;
    }
    let root = bin_dir.parent()?;
    root.join("Update.exe").exists().then(|| PackageLayout {
        kind: PackageKind::Windows,
        root: root.to_path_buf(),
        bin_dir: bin_dir.to_path_buf(),
    })
}

fn mac_bundle(exe: &Path) -> Option<PackageLayout> {
    let root = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))?;
    let bin_dir = root.join("Contents").join("MacOS");
    bin_dir.join("UpdateMac").exists().then(|| PackageLayout {
        kind: PackageKind::MacBundle,
        root: root.to_path_buf(),
        bin_dir,
    })
}

fn app_image(exe: &Path, appimage: Option<&Path>) -> Option<PackageLayout> {
    let appimage = appimage?;
    let bin_dir = exe.parent()?;
    (bin_dir.ends_with("usr/bin") && bin_dir.join("UpdateNix").exists()).then(|| PackageLayout {
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

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }

    #[test]
    fn a_windows_install_is_current_beside_update_exe() {
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("Update.exe"));
        let exe = d.path().join("current").join("app.exe");
        touch(&exe);
        assert_eq!(
            current_with(&exe, None),
            Some(PackageLayout {
                kind: PackageKind::Windows,
                root: d.path().to_path_buf(),
                bin_dir: d.path().join("current"),
            })
        );
    }

    #[test]
    fn current_without_update_exe_is_not_a_package() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("current").join("app.exe");
        touch(&exe);
        assert_eq!(current_with(&exe, None), None);
    }

    #[test]
    fn a_mac_bundle_holds_update_mac() {
        let d = tempfile::tempdir().unwrap();
        let macos = d.path().join("Demo.app/Contents/MacOS");
        touch(&macos.join("UpdateMac"));
        let exe = macos.join("demo");
        touch(&exe);
        assert_eq!(
            current_with(&exe, None),
            Some(PackageLayout {
                kind: PackageKind::MacBundle,
                root: d.path().join("Demo.app"),
                bin_dir: macos,
            })
        );
    }

    #[test]
    fn an_app_without_update_mac_is_not_a_package() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("Demo.app/Contents/MacOS/demo");
        touch(&exe);
        assert_eq!(current_with(&exe, None), None);
    }

    #[test]
    fn an_appimage_needs_the_variable_and_update_nix() {
        let m = tempfile::tempdir().unwrap();
        let bin = m.path().join("usr/bin");
        touch(&bin.join("UpdateNix"));
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
    fn a_cargo_build_is_not_a_package() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("target/debug/demo");
        touch(&exe);
        assert_eq!(current_with(&exe, None), None);
    }
}
