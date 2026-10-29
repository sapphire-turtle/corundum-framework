//! macOS and Linux: a link named after the command, in a directory on `PATH`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

use corundum_framework_platform::package::{self, PackageKind};

use super::{CliSpec, CliStatus};
use crate::error::Error;

/// Where the link goes, what it points at, and whether making it takes administrator
/// rights; `None` outside a package.
struct Link {
    link: PathBuf,
    target: PathBuf,
    admin: bool,
}

fn link_of(spec: &CliSpec) -> Option<Link> {
    let layout = package::current()?;
    match layout.kind {
        // `/usr/local/bin` is on every macOS `PATH`, and owned by root.
        PackageKind::MacBundle => {
            let exe = std::env::current_exe().ok()?;
            Some(Link {
                link: Path::new("/usr/local/bin").join(spec.command),
                target: layout.bin_dir.join(exe.file_name()?),
                admin: true,
            })
        }
        // The AppImage gives the binary in it no lasting path; `$APPIMAGE` starts it.
        PackageKind::AppImage => Some(Link {
            link: std::env::home_dir()?
                .join(".local")
                .join("bin")
                .join(spec.command),
            target: layout.root,
            admin: false,
        }),
        PackageKind::Windows => None,
    }
}

pub(super) fn status(spec: &CliSpec) -> CliStatus {
    match link_of(spec) {
        Some(Link { link, target, .. }) => status_at(&link, &target),
        None => CliStatus::Unsupported,
    }
}

pub(super) fn install(spec: &CliSpec) -> Result<(), Error> {
    let Link {
        link,
        target,
        admin,
    } = link_of(spec).ok_or(Error::NotPackaged)?;
    if !admin {
        return link_at(&link, &target, spec.command);
    }
    refuse_taken(&link, spec.command)?;
    run_as_admin(&format!(
        "mkdir -p /usr/local/bin && ln -sf {} {}",
        shell_quote(&target.to_string_lossy()),
        shell_quote(&link.to_string_lossy()),
    ))
}

pub(super) fn uninstall(spec: &CliSpec) -> Result<(), Error> {
    let Link { link, admin, .. } = link_of(spec).ok_or(Error::NotPackaged)?;
    if !admin {
        return unlink_at(&link, spec.command);
    }
    if fs::symlink_metadata(&link).is_err() {
        return Ok(());
    }
    refuse_taken(&link, spec.command)?;
    run_as_admin(&format!("rm -f {}", shell_quote(&link.to_string_lossy())))
}

/// What the link at `link` is, for a package whose binary is `target`.
pub(super) fn status_at(link: &Path, target: &Path) -> CliStatus {
    let Ok(meta) = fs::symlink_metadata(link) else {
        return CliStatus::Missing;
    };
    if !meta.file_type().is_symlink() {
        return CliStatus::Taken;
    }
    // Followed, the link reaches nothing: its target is gone.
    let Ok(reached) = fs::canonicalize(link) else {
        return CliStatus::Broken;
    };
    if fs::canonicalize(target).is_ok_and(|target| target == reached) {
        CliStatus::Installed
    } else {
        CliStatus::Missing
    }
}

/// An error when something other than a link has the command's name.
fn refuse_taken(link: &Path, command: &str) -> Result<(), Error> {
    match fs::symlink_metadata(link) {
        Ok(meta) if !meta.file_type().is_symlink() => Err(Error::Taken {
            command: command.to_owned(),
            path: link.to_path_buf(),
        }),
        _ => Ok(()),
    }
}

/// Makes `link` point at `target`, replacing a link already there.
fn link_at(link: &Path, target: &Path, command: &str) -> Result<(), Error> {
    refuse_taken(link, command)?;
    if let Some(dir) = link.parent() {
        fs::create_dir_all(dir)?;
    }
    match fs::remove_file(link) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

/// Removes the link at `link`, if there is one.
fn unlink_at(link: &Path, command: &str) -> Result<(), Error> {
    refuse_taken(link, command)?;
    match fs::remove_file(link) {
        Err(e) if e.kind() != ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Runs `shell` as root, after macOS asks the user for an administrator's password.
fn run_as_admin(shell: &str) -> Result<(), Error> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg(admin_script(shell))
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::CliPath(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ))
    }
}

/// `s` as one word for `sh`.
pub(super) fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// An AppleScript that runs `shell` with administrator rights.
pub(super) fn admin_script(shell: &str) -> String {
    let escaped = shell.replace('\\', r"\\").replace('"', "\\\"");
    format!("do shell script \"{escaped}\" with administrator privileges")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn a_link_is_told_from_what_is_at_its_path() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("app");
        fs::write(&target, "").unwrap();
        let elsewhere = tmp.path().join("other");
        fs::write(&elsewhere, "").unwrap();
        let link = tmp.path().join("cmd");

        assert_eq!(status_at(&link, &target), CliStatus::Missing);

        symlink(&target, &link).unwrap();
        assert_eq!(status_at(&link, &target), CliStatus::Installed);

        fs::remove_file(&link).unwrap();
        symlink(&elsewhere, &link).unwrap();
        assert_eq!(status_at(&link, &target), CliStatus::Missing);

        fs::remove_file(&elsewhere).unwrap();
        assert_eq!(status_at(&link, &target), CliStatus::Broken);

        fs::remove_file(&link).unwrap();
        fs::write(&link, "a real binary").unwrap();
        assert_eq!(status_at(&link, &target), CliStatus::Taken);
    }

    #[test]
    fn a_link_to_a_removed_target_is_broken() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("app");
        fs::write(&target, "").unwrap();
        let link = tmp.path().join("cmd");
        symlink(&target, &link).unwrap();
        fs::remove_file(&target).unwrap();
        assert_eq!(status_at(&link, &target), CliStatus::Broken);
    }

    #[test]
    fn a_link_is_made_and_removed_without_touching_a_real_file() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("app");
        fs::write(&target, "").unwrap();
        let link = tmp.path().join("bin").join("cmd");

        link_at(&link, &target, "cmd").unwrap();
        assert_eq!(status_at(&link, &target), CliStatus::Installed);
        // A stale link is replaced.
        link_at(&link, &target, "cmd").unwrap();
        assert_eq!(status_at(&link, &target), CliStatus::Installed);
        unlink_at(&link, "cmd").unwrap();
        assert_eq!(status_at(&link, &target), CliStatus::Missing);
        unlink_at(&link, "cmd").unwrap();

        fs::write(&link, "a real binary").unwrap();
        assert!(matches!(
            link_at(&link, &target, "cmd"),
            Err(Error::Taken { .. })
        ));
        assert!(matches!(unlink_at(&link, "cmd"), Err(Error::Taken { .. })));
        assert_eq!(fs::read_to_string(&link).unwrap(), "a real binary");
    }

    #[test]
    fn a_path_is_quoted_for_the_shell_inside_an_applescript_string() {
        assert_eq!(shell_quote("/Applications/A.app"), "'/Applications/A.app'");
        assert_eq!(shell_quote("/x/it's"), "'/x/it'\\''s'");
        assert_eq!(
            admin_script("rm -f '/usr/local/bin/a'"),
            "do shell script \"rm -f '/usr/local/bin/a'\" with administrator privileges"
        );
        assert_eq!(
            admin_script("ln -sf '/x/it'\\''s \"q\"' '/b'"),
            "do shell script \"ln -sf '/x/it'\\\\''s \\\"q\\\"' '/b'\" with administrator privileges"
        );
    }
}
