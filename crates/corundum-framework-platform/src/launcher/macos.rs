//! The macOS entry: an app bundle `<name>.app` under `~/Applications`, which Finder,
//! Spotlight and Launchpad list with its icon. The bundle holds an `Info.plist`, the icon
//! as an icns file and, as the executable, a symbolic link to the real one: macOS then
//! shows the bundle's icon and name in the Dock and the menu bar for the linked process.

use super::{LauncherSpec, Registered, Root, Status, write_if_changed};
use std::io;
use std::path::{Path, PathBuf};

/// `text` with the characters XML reserves written as entities.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// The text of the bundle's `Info.plist` for the app `spec` describes.
pub(crate) fn info_plist(spec: &LauncherSpec) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
<plist version=\"1.0\">\n\
<dict>\n\
\t<key>CFBundleIdentifier</key>\n\
\t<string>{bundle_id}</string>\n\
\t<key>CFBundleName</key>\n\
\t<string>{name}</string>\n\
\t<key>CFBundleDisplayName</key>\n\
\t<string>{name}</string>\n\
\t<key>CFBundleExecutable</key>\n\
\t<string>{id}</string>\n\
\t<key>CFBundleIconFile</key>\n\
\t<string>{id}</string>\n\
\t<key>CFBundlePackageType</key>\n\
\t<string>APPL</string>\n\
\t<key>CFBundleInfoDictionaryVersion</key>\n\
\t<string>6.0</string>\n\
\t<key>LSMinimumSystemVersion</key>\n\
\t<string>11.0</string>\n\
\t<key>NSHighResolutionCapable</key>\n\
\t<true/>\n\
</dict>\n\
</plist>\n",
        bundle_id = escape(&format!("{}.{}", spec.bundle_prefix, spec.id)),
        name = escape(spec.name),
        id = escape(spec.id),
    )
}

/// The app's bundle: `<name>.app` in the user's `Applications` directory.
pub(crate) fn bundle_dir(spec: &LauncherSpec, root: &Root) -> PathBuf {
    root.home
        .join("Applications")
        .join(format!("{}.app", spec.name))
}

fn plist_path(spec: &LauncherSpec, root: &Root) -> PathBuf {
    bundle_dir(spec, root).join("Contents/Info.plist")
}

fn icon_path(spec: &LauncherSpec, root: &Root) -> PathBuf {
    bundle_dir(spec, root)
        .join("Contents/Resources")
        .join(format!("{}.icns", spec.id))
}

fn launcher_path(spec: &LauncherSpec, root: &Root) -> PathBuf {
    bundle_dir(spec, root).join("Contents/MacOS").join(spec.id)
}

/// Makes the launcher a link to `exe`; `true` when it changed. A launcher that is not a link
/// to `exe` is removed and linked anew.
fn link_launcher(path: &Path, exe: &Path) -> io::Result<bool> {
    if std::fs::read_link(path).is_ok_and(|target| target == exe) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    std::os::unix::fs::symlink(exe, path)?;
    Ok(true)
}

/// Writes the app's bundle. The icon is rendered first, so that an icon that cannot be
/// rendered leaves nothing behind.
pub(crate) fn register(spec: &LauncherSpec, root: &Root, exe: &Path) -> io::Result<Registered> {
    let icns = crate::icon::icns(spec.icon_svg)?;
    let plist = write_if_changed(&plist_path(spec, root), info_plist(spec).as_bytes())?;
    let icon = write_if_changed(&icon_path(spec, root), &icns)?;
    let link = link_launcher(&launcher_path(spec, root), exe)?;
    Ok(if plist || icon || link {
        Registered::Written
    } else {
        Registered::Unchanged
    })
}

/// Removes the app's bundle; one that is not there is no error.
pub(crate) fn unregister(spec: &LauncherSpec, root: &Root) -> io::Result<()> {
    match std::fs::remove_dir_all(bundle_dir(spec, root)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Whether `bundle` is one [`register`] wrote: its executable is a link, and it holds none
/// of the `sq.version` files a Velopack package's bundle has.
pub(crate) fn is_generated(spec: &LauncherSpec, bundle: &Path) -> bool {
    let contents = bundle.join("Contents");
    std::fs::symlink_metadata(contents.join("MacOS").join(spec.id))
        .is_ok_and(|m| m.file_type().is_symlink())
        && !contents.join("MacOS/sq.version").exists()
        && !contents.join("Resources/sq.version").exists()
}

/// Removes the bundle [`register`] wrote under the app's name, as a package's installer
/// now provides the app's bundle. A bundle that is not a generated one is left, and so is
/// one that is, or holds, `running`, the bundle the app runs from: paths are compared
/// once links and `..` are resolved, and when either cannot be resolved nothing is removed.
pub(crate) fn remove_generated(spec: &LauncherSpec, root: &Root, running: &Path) -> io::Result<()> {
    let bundle = bundle_dir(spec, root);
    if !is_generated(spec, &bundle) {
        return Ok(());
    }
    let (Ok(bundle), Ok(running)) = (bundle.canonicalize(), running.canonicalize()) else {
        return Ok(());
    };
    if running.starts_with(&bundle) || bundle.starts_with(&running) {
        return Ok(());
    }
    match std::fs::remove_dir_all(bundle) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Whether the app's bundle is there and its launcher links to `exe`. Only the link is
/// compared: a changed name or icon is not seen as stale, but [`register`] rewrites it on
/// the next start.
pub(crate) fn status(spec: &LauncherSpec, root: &Root, exe: &Path) -> Status {
    if !bundle_dir(spec, root).is_dir() {
        return Status::Absent;
    }
    match std::fs::read_link(launcher_path(spec, root)) {
        Ok(target) if target == exe => Status::Registered,
        _ => Status::Stale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="red"/></svg>"#;

    const SPEC: LauncherSpec = LauncherSpec {
        id: "corundum-demo",
        name: "Corundum Demo",
        description: "Every piece corundum shares",
        category: "Development",
        icon_svg: SVG,
        bundle_prefix: "net.fireturtle.corundum",
    };

    fn root(dir: &Path) -> Root {
        Root {
            home: dir.into(),
            xdg_data_home: None,
            appdata: None,
        }
    }

    #[test]
    fn the_plist_names_the_app() {
        assert_eq!(
            info_plist(&SPEC),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
<dict>
\t<key>CFBundleIdentifier</key>
\t<string>net.fireturtle.corundum.corundum-demo</string>
\t<key>CFBundleName</key>
\t<string>Corundum Demo</string>
\t<key>CFBundleDisplayName</key>
\t<string>Corundum Demo</string>
\t<key>CFBundleExecutable</key>
\t<string>corundum-demo</string>
\t<key>CFBundleIconFile</key>
\t<string>corundum-demo</string>
\t<key>CFBundlePackageType</key>
\t<string>APPL</string>
\t<key>CFBundleInfoDictionaryVersion</key>
\t<string>6.0</string>
\t<key>LSMinimumSystemVersion</key>
\t<string>11.0</string>
\t<key>NSHighResolutionCapable</key>
\t<true/>
</dict>
</plist>
"
        );
    }

    #[test]
    fn reserved_characters_in_the_name_are_escaped() {
        let spec = LauncherSpec {
            name: "A & <B>",
            ..SPEC
        };
        assert!(info_plist(&spec).contains("<string>A &amp; &lt;B&gt;</string>"));
    }

    #[test]
    fn register_writes_once_then_relinks_a_moved_exe() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        let app = dir.path().join("Applications/Corundum Demo.app/Contents");
        let a = Path::new("/opt/a/corundum-demo");
        assert_eq!(status(&SPEC, &root, a), Status::Absent);
        assert_eq!(register(&SPEC, &root, a).unwrap(), Registered::Written);
        assert!(app.join("Info.plist").is_file());
        assert!(app.join("Resources/corundum-demo.icns").is_file());
        assert_eq!(
            std::fs::read_link(app.join("MacOS/corundum-demo")).unwrap(),
            a
        );
        assert_eq!(register(&SPEC, &root, a).unwrap(), Registered::Unchanged);
        assert_eq!(status(&SPEC, &root, a), Status::Registered);
        let b = Path::new("/opt/b/corundum-demo");
        assert_eq!(status(&SPEC, &root, b), Status::Stale);
        assert_eq!(register(&SPEC, &root, b).unwrap(), Registered::Written);
        assert_eq!(
            std::fs::read_link(app.join("MacOS/corundum-demo")).unwrap(),
            b
        );
        assert_eq!(status(&SPEC, &root, b), Status::Registered);
        unregister(&SPEC, &root).unwrap();
        assert!(!dir.path().join("Applications/Corundum Demo.app").exists());
        assert_eq!(status(&SPEC, &root, b), Status::Absent);
        unregister(&SPEC, &root).unwrap();
    }

    /// A bundle as a Velopack package lays it out: the real executable, the updater and
    /// the version file.
    fn package_bundle(at: &Path) {
        let macos = at.join("Contents/MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::create_dir_all(at.join("Contents/Resources")).unwrap();
        std::fs::write(macos.join("corundum-demo"), b"binary").unwrap();
        std::fs::write(macos.join("UpdateMac"), b"updater").unwrap();
        std::fs::write(macos.join("sq.version"), b"<package/>").unwrap();
    }

    #[test]
    fn a_generated_bundle_is_told_from_a_package() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        register(&SPEC, &root, Path::new("/opt/a/corundum-demo")).unwrap();
        assert!(is_generated(&SPEC, &bundle_dir(&SPEC, &root)));
        let package = dir.path().join("Package/Corundum Demo.app");
        package_bundle(&package);
        assert!(!is_generated(&SPEC, &package));
        // A bundle holding only the version file is still a package's.
        let half = dir.path().join("Half/Corundum Demo.app");
        std::fs::create_dir_all(half.join("Contents/MacOS")).unwrap();
        std::os::unix::fs::symlink("/opt/a/x", half.join("Contents/MacOS/corundum-demo")).unwrap();
        std::fs::create_dir_all(half.join("Contents/Resources")).unwrap();
        std::fs::write(half.join("Contents/Resources/sq.version"), b"").unwrap();
        assert!(!is_generated(&SPEC, &half));
        assert!(!is_generated(&SPEC, &dir.path().join("Missing.app")));
    }

    #[test]
    fn stepping_aside_removes_a_generated_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        register(&SPEC, &root, Path::new("/opt/a/corundum-demo")).unwrap();
        let running = dir.path().join("Elsewhere/Corundum Demo.app");
        package_bundle(&running);
        remove_generated(&SPEC, &root, &running).unwrap();
        assert!(!bundle_dir(&SPEC, &root).exists());
        assert!(running.is_dir());
        // Nothing there is no error.
        remove_generated(&SPEC, &root, &running).unwrap();
    }

    #[test]
    fn stepping_aside_never_removes_a_package() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        let installed = bundle_dir(&SPEC, &root);
        package_bundle(&installed);
        let running = dir.path().join("Elsewhere/Corundum Demo.app");
        package_bundle(&running);
        remove_generated(&SPEC, &root, &running).unwrap();
        assert!(installed.join("Contents/MacOS/UpdateMac").is_file());
    }

    #[test]
    fn stepping_aside_never_removes_the_running_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let root = root(dir.path());
        // A bundle at the generated one's path that looks generated, and is the one running.
        register(&SPEC, &root, Path::new("/opt/a/corundum-demo")).unwrap();
        let bundle = bundle_dir(&SPEC, &root);
        assert!(is_generated(&SPEC, &bundle));
        remove_generated(&SPEC, &root, &bundle).unwrap();
        assert!(bundle.join("Contents/Info.plist").is_file());
        // The same bundle by another spelling of its path: through a link and with a `..`.
        let alias = dir.path().join("Alias");
        std::os::unix::fs::symlink(dir.path(), &alias).unwrap();
        let spelled = alias.join("Applications/../Applications/Corundum Demo.app");
        remove_generated(&SPEC, &root, &spelled).unwrap();
        assert!(bundle.join("Contents/Info.plist").is_file());
        // An executable inside it, as a path from the running process might be.
        remove_generated(&SPEC, &root, &spelled.join("Contents/MacOS")).unwrap();
        assert!(bundle.join("Contents/Info.plist").is_file());
    }

    #[test]
    fn a_bad_icon_leaves_no_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let spec = LauncherSpec {
            icon_svg: b"not an svg",
            ..SPEC
        };
        assert!(register(&spec, &root(dir.path()), Path::new("/opt/a/x")).is_err());
        assert!(!dir.path().join("Applications/Corundum Demo.app").exists());
    }
}
