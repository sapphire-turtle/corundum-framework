//! The Linux entry: a `.desktop` file and an SVG icon under the user's data directory,
//! as the freedesktop.org Desktop Entry and Icon Theme specifications lay out.

use super::{LauncherSpec, Registered, Root, Status, write_if_changed};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The text of the `.desktop` file that starts `exe` as the app `spec` describes.
pub(crate) fn desktop_entry(spec: &LauncherSpec, exe: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Comment={description}\n\
         Exec={exec}\n\
         Icon={id}\n\
         Categories={category};\n\
         StartupWMClass={id}\n\
         Terminal=false\n",
        name = spec.name,
        description = spec.description,
        exec = exec_line(exe),
        id = spec.id,
        category = spec.category,
    )
}

/// The value of the `Exec=` key for `exe`: its quoted path and the field code that hands
/// it the URLs it is asked to open.
fn exec_line(exe: &Path) -> String {
    format!("{} %U", quote_exec(exe))
}

/// `path` as one quoted argument of an `Exec=` value. Inside the double quotes `"`, `` ` ``,
/// `$` and `\` are each preceded by `\`, and as the value is a Desktop Entry string every
/// `\` is then doubled. A `%` is written `%%`, as it would else begin a field code.
pub(crate) fn quote_exec(path: &Path) -> String {
    let mut out = String::from("\"");
    for c in path.to_string_lossy().chars() {
        match c {
            '"' | '`' | '$' => out.push_str("\\\\"),
            '\\' => out.push_str("\\\\\\"),
            '%' => out.push('%'),
            _ => {}
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// The user's data directory: `$XDG_DATA_HOME` when it is an absolute path, else
/// `.local/share` under the home directory.
pub(crate) fn data_home(root: &Root) -> PathBuf {
    match &root.xdg_data_home {
        Some(dir) if dir.is_absolute() => dir.clone(),
        _ => root.home.join(".local/share"),
    }
}

/// The directory the `.desktop` files are kept in.
fn apps_dir(root: &Root) -> PathBuf {
    data_home(root).join("applications")
}

/// The path of the app's `.desktop` file.
fn entry_path(spec: &LauncherSpec, root: &Root) -> PathBuf {
    apps_dir(root).join(format!("{}.desktop", spec.id))
}

/// The path of the app's icon.
fn icon_path(spec: &LauncherSpec, root: &Root) -> PathBuf {
    data_home(root)
        .join("icons/hicolor/scalable/apps")
        .join(format!("{}.svg", spec.id))
}

/// Writes the app's entry and icon, and has the desktop database refreshed when the entry
/// changed.
pub(crate) fn register(spec: &LauncherSpec, root: &Root, exe: &Path) -> io::Result<Registered> {
    let entry = write_if_changed(&entry_path(spec, root), desktop_entry(spec, exe).as_bytes())?;
    let icon = write_if_changed(&icon_path(spec, root), spec.icon_svg)?;
    if entry {
        // The menu works without the refresh, so a missing tool or a failure is no error.
        let _ = Command::new("update-desktop-database")
            .arg(apps_dir(root))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    Ok(if entry || icon {
        Registered::Written
    } else {
        Registered::Unchanged
    })
}

/// Removes the app's entry and icon; a file that is not there is no error.
pub(crate) fn unregister(spec: &LauncherSpec, root: &Root) -> io::Result<()> {
    for path in [entry_path(spec, root), icon_path(spec, root)] {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// Whether the app's entry is there and starts `exe`. Only the `Exec=` line is compared: a
/// changed name or icon is not seen as stale, but [`register`] rewrites it on the next start.
pub(crate) fn status(spec: &LauncherSpec, root: &Root, exe: &Path) -> Status {
    let Ok(text) = std::fs::read_to_string(entry_path(spec, root)) else {
        return Status::Absent;
    };
    let want = format!("Exec={}", exec_line(exe));
    if text.lines().any(|line| line == want) {
        Status::Registered
    } else {
        Status::Stale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::{LauncherSpec, Registered, Root, Status};
    use crate::launcher::{refresh_at, register_at, status_at, unregister_at};
    use std::path::{Path, PathBuf};

    #[test]
    fn an_appimage_start_writes_no_entry_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root {
            home: dir.path().into(),
            xdg_data_home: None,
            appdata: None,
        };
        let appimage = Path::new("/home/u/Downloads/Corundum Demo.AppImage");
        assert_eq!(
            refresh_at(&SPEC, &root, appimage).unwrap(),
            Registered::Skipped
        );
        assert_eq!(status_at(&SPEC, &root, appimage), Status::Absent);
    }

    #[test]
    fn an_appimage_start_follows_an_appimage_a_manager_moved() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root {
            home: dir.path().into(),
            xdg_data_home: None,
            appdata: None,
        };
        let before = Path::new("/home/u/Downloads/Corundum Demo.AppImage");
        let after = Path::new("/home/u/Applications/corundum-demo.appimage");
        // The user's About button.
        register_at(&SPEC, &root, before).unwrap();
        assert_eq!(
            refresh_at(&SPEC, &root, after).unwrap(),
            Registered::Written
        );
        assert_eq!(status_at(&SPEC, &root, after), Status::Registered);
        assert_eq!(
            refresh_at(&SPEC, &root, after).unwrap(),
            Registered::Unchanged
        );
    }

    const SPEC: LauncherSpec = LauncherSpec {
        id: "corundum-demo",
        name: "Corundum Demo",
        description: "Every piece corundum shares",
        category: "Development",
        icon_svg: b"<svg/>",
        bundle_prefix: "net.sapphire-turtle",
    };

    #[test]
    fn the_entry_names_the_app_and_its_window() {
        assert_eq!(
            desktop_entry(&SPEC, Path::new("/home/u/.local/bin/corundum-demo")),
            "[Desktop Entry]
Type=Application
Name=Corundum Demo
Comment=Every piece corundum shares
Exec=\"/home/u/.local/bin/corundum-demo\" %U
Icon=corundum-demo
Categories=Development;
StartupWMClass=corundum-demo
Terminal=false
"
        );
    }

    #[test]
    fn reserved_characters_are_escaped() {
        assert_eq!(
            quote_exec(Path::new("/a b/$x`y\"z\\w")),
            r#""/a b/\\$x\\`y\\"z\\\\w""#
        );
        assert_eq!(quote_exec(Path::new("/a/100%/x")), r#""/a/100%%/x""#);
    }

    #[test]
    fn a_relative_xdg_data_home_is_ignored() {
        let root = Root {
            home: "/h".into(),
            xdg_data_home: Some("rel".into()),
            appdata: None,
        };
        assert_eq!(data_home(&root), PathBuf::from("/h/.local/share"));
    }

    #[test]
    fn an_appimage_entry_execs_the_appimage() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root {
            home: dir.path().into(),
            xdg_data_home: None,
            appdata: None,
        };
        let appimage = Path::new("/home/u/Applications/Corundum Demo-x86_64.AppImage");
        assert_eq!(
            register_at(&SPEC, &root, appimage).unwrap(),
            Registered::Written
        );
        let text = std::fs::read_to_string(
            dir.path()
                .join(".local/share/applications/corundum-demo.desktop"),
        )
        .unwrap();
        assert!(
            text.lines()
                .any(|l| l == "Exec=\"/home/u/Applications/Corundum Demo-x86_64.AppImage\" %U")
        );
        assert_eq!(status_at(&SPEC, &root, appimage), Status::Registered);
    }

    #[test]
    fn register_writes_once_then_rewrites_a_moved_exe() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root {
            home: dir.path().into(),
            xdg_data_home: None,
            appdata: None,
        };
        let icon = dir
            .path()
            .join(".local/share/icons/hicolor/scalable/apps/corundum-demo.svg");
        let a = Path::new("/opt/a/corundum-demo");
        assert_eq!(status_at(&SPEC, &root, a), Status::Absent);
        assert_eq!(register_at(&SPEC, &root, a).unwrap(), Registered::Written);
        assert_eq!(register_at(&SPEC, &root, a).unwrap(), Registered::Unchanged);
        assert_eq!(status_at(&SPEC, &root, a), Status::Registered);
        let b = Path::new("/opt/b/corundum-demo");
        assert_eq!(status_at(&SPEC, &root, b), Status::Stale);
        assert_eq!(register_at(&SPEC, &root, b).unwrap(), Registered::Written);
        let apps = dir.path().join(".local/share/applications");
        // The desktop database tool may leave its cache beside the entry.
        let entries = std::fs::read_dir(&apps)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "desktop")
            })
            .count();
        assert_eq!(entries, 1);
        assert!(icon.is_file());
        unregister_at(&SPEC, &root).unwrap();
        assert_eq!(status_at(&SPEC, &root, b), Status::Absent);
        assert!(!icon.exists());
        unregister_at(&SPEC, &root).unwrap();
    }
}
