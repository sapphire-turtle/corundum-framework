//! The app's command line on `PATH`.
//!
//! Windows puts the package's `current` directory on the user `Path` from the install hook
//! and takes it off from the uninstall hook. macOS links `/usr/local/bin/<command>` to the
//! binary in the `.app`, with administrator rights asked through `osascript`; Linux links
//! `~/.local/bin/<command>` to the AppImage. Both are made and removed when the user asks.
//! An update keeps the binary's path, so nothing is made again after one.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::error::Error;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

/// The command line a package puts on `PATH`.
#[derive(Clone, Copy, Debug)]
pub struct CliSpec {
    /// The name typed in a terminal: `sapphire-journal`. It need not be the cargo binary's
    /// name. On macOS and Linux the command links to the running executable, whatever
    /// `vpk --mainExe` named it; on Windows it is the console shim's file name without
    /// `.exe`, and the shim sits beside the GUI executable in `current\`.
    pub command: &'static str,
}

/// Whether the command line is on `PATH`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliStatus {
    /// On `PATH`, running this package.
    Installed,
    /// Not on `PATH`.
    Missing,
    /// A link whose target is gone: the `.app` or the AppImage was deleted or moved.
    Broken,
    /// Another program has the name, left alone: a file that is not a link (a
    /// command-line-only install of the same app), or a link to another file (Homebrew's,
    /// pipx's).
    Taken,
    /// Outside a package.
    Unsupported,
}

/// Whether the command line is on `PATH`.
pub fn status(spec: &CliSpec) -> CliStatus {
    #[cfg(windows)]
    return windows::status(spec);
    #[cfg(unix)]
    return unix::status(spec);
}

/// Puts the command line on `PATH`. Never replaces another program's: a file that is not a
/// link, or a link to another file ([`CliStatus::Taken`]).
pub fn install(spec: &CliSpec) -> Result<(), Error> {
    #[cfg(windows)]
    return windows::install(spec);
    #[cfg(unix)]
    return unix::install(spec);
}

/// Takes the command line off `PATH`. Nothing to take off is not an error.
pub fn uninstall(spec: &CliSpec) -> Result<(), Error> {
    #[cfg(windows)]
    return windows::uninstall(spec);
    #[cfg(unix)]
    return unix::uninstall(spec);
}

/// Whether `dir` is on this process's `PATH`: Linux's `~/.local/bin` is not on every
/// distribution's.
pub fn on_path(dir: &Path) -> bool {
    on_path_in(dir, &std::env::var_os("PATH").unwrap_or_default())
}

/// Whether `dir` is on the `PATH` a terminal gets: the one the user's login shell sets up,
/// or `None` when the shell does not answer within [`LOGIN_PATH_WAIT`]. A window started from
/// the desktop's menu often has a shorter `PATH` than a terminal, since shells add
/// `~/.local/bin` in their own start files, so [`on_path`] would warn about a directory every
/// terminal has; and a busy machine can take seconds to start a shell, so a missing answer
/// is not a "no". It blocks while the shell starts: call it off the UI thread.
#[cfg(unix)]
pub fn on_login_path(dir: &Path) -> Option<bool> {
    login_path().map(|path_var| on_path_in(dir, &path_var))
}

/// How long [`on_login_path`] waits for the shell: an interactive shell's start files can
/// take several seconds on a machine busy with its disk.
#[cfg(unix)]
pub const LOGIN_PATH_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// What a terminal's `PATH` would be: asked of `$SHELL` as a login, interactive shell, which
/// reads the start files a terminal does. Its output is searched for a marker, since start
/// files may print greetings of their own.
#[cfg(unix)]
fn login_path() -> Option<std::ffi::OsString> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", r#"printf '\n%s%s\n' "$1" "$PATH""#, "sh"])
        .arg(LOGIN_PATH_MARKER)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + LOGIN_PATH_WAIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = Vec::new();
    child.stdout.take()?.read_to_end(&mut out).ok()?;
    parse_login_path(&out)
}

/// Put before `$PATH` in the shell's output, so a greeting from a start file is not taken
/// for it.
#[cfg(unix)]
const LOGIN_PATH_MARKER: &str = "corundum-login-path:";

/// The `PATH` after the last marker in a login shell's output.
#[cfg(unix)]
fn parse_login_path(out: &[u8]) -> Option<std::ffi::OsString> {
    use std::os::unix::ffi::OsStringExt;

    let marker = LOGIN_PATH_MARKER.as_bytes();
    let start = out
        .windows(marker.len())
        .rposition(|window| window == marker)?
        + marker.len();
    let line = out[start..].split(|&b| b == b'\n').next()?;
    (!line.is_empty()).then(|| std::ffi::OsString::from_vec(line.to_vec()))
}

/// [`on_path`] for a given `PATH` value.
pub(crate) fn on_path_in(dir: &Path, path_var: &OsStr) -> bool {
    let dir = clean(dir);
    std::env::split_paths(path_var)
        .filter(|entry| !entry.as_os_str().is_empty())
        .any(|entry| clean(&entry) == dir)
}

/// `path` as it names a directory: its canonical form, or, when it does not exist, its
/// components without `.` and a trailing separator.
fn clean(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|_| path.components().collect())
}

/// The user `Path` value with `dir` added at the end, or `None` when it is already there.
/// The other entries are kept as they are, a trailing `;` too.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn path_with(value: &str, dir: &str) -> Option<String> {
    if value.split(';').any(|entry| same_entry(entry, dir)) {
        return None;
    }
    Some(if value.is_empty() {
        dir.to_owned()
    } else if let Some(rest) = value.strip_suffix(';') {
        format!("{rest};{dir};")
    } else {
        format!("{value};{dir}")
    })
}

/// The user `Path` value with every entry naming `dir` removed, or `None` when there is
/// none. The other entries are kept as they are, empty ones too.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn path_without(value: &str, dir: &str) -> Option<String> {
    let entries: Vec<&str> = value.split(';').collect();
    let kept: Vec<&str> = entries
        .iter()
        .copied()
        .filter(|entry| !same_entry(entry, dir))
        .collect();
    (kept.len() != entries.len()).then(|| kept.join(";"))
}

/// Whether a `Path` entry names `dir`: Windows paths ignore case, and an entry may be
/// quoted or end with a `\`. An empty entry names nothing.
fn same_entry(entry: &str, dir: &str) -> bool {
    let norm = |s: &str| {
        let s = s
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(s);
        s.strip_suffix('\\').unwrap_or(s).to_lowercase()
    };
    !entry.is_empty() && norm(entry) == norm(dir)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    const DIR: &str = "C:\\x\\current";

    #[cfg(unix)]
    #[test]
    fn the_login_path_is_read_after_the_marker() {
        let out = b"Welcome back!\ncorundum-login-path: in a greeting\n\ncorundum-login-path:/home/u/.local/bin:/usr/bin\n";
        assert_eq!(
            parse_login_path(out),
            Some(OsString::from("/home/u/.local/bin:/usr/bin"))
        );
        assert_eq!(parse_login_path(b"no marker here\n"), None);
        assert_eq!(parse_login_path(b"\ncorundum-login-path:\n"), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_login_shell_is_asked_for_its_path() {
        // This test process's shell sees at least the directories its own `PATH` has.
        let dir = tempfile::tempdir().unwrap();
        assert_ne!(on_login_path(dir.path()), Some(true));
        assert_ne!(on_login_path(Path::new("/usr/bin")), Some(false));
    }

    #[test]
    fn a_dir_is_added_at_the_end_of_a_path_without_it() {
        let cases: &[(&str, Option<&str>)] = &[
            ("C:\\a;C:\\b", Some("C:\\a;C:\\b;C:\\x\\current")),
            ("", Some("C:\\x\\current")),
            ("C:\\a;", Some("C:\\a;C:\\x\\current;")),
            ("C:\\a;;C:\\b", Some("C:\\a;;C:\\b;C:\\x\\current")),
            ("C:\\a;C:\\X\\CURRENT\\", None),
            ("\"C:\\x\\current\";C:\\b", None),
            ("C:\\x\\current", None),
        ];
        for (value, want) in cases {
            assert_eq!(path_with(value, DIR).as_deref(), *want, "{value:?}");
        }
    }

    #[test]
    fn a_dir_is_removed_from_a_path_keeping_the_other_entries() {
        let cases: &[(&str, Option<&str>)] = &[
            (
                "%USERPROFILE%\\bin;C:\\x\\current;C:\\b",
                Some("%USERPROFILE%\\bin;C:\\b"),
            ),
            ("C:\\a;C:\\x\\current;", Some("C:\\a;")),
            ("C:\\a;;C:\\X\\Current\\;C:\\b", Some("C:\\a;;C:\\b")),
            ("\"C:\\x\\current\"", Some("")),
            ("C:\\x\\current;C:\\a;C:\\x\\current", Some("C:\\a")),
            ("C:\\a;C:\\b", None),
            ("", None),
            ("C:\\x\\current2", None),
        ];
        for (value, want) in cases {
            assert_eq!(path_without(value, DIR).as_deref(), *want, "{value:?}");
        }
    }

    #[test]
    fn a_dir_is_on_a_path_naming_it_with_or_without_a_trailing_separator() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("bin");
        std::fs::create_dir(&dir).unwrap();
        let other = tmp.path().join("other");
        let sep = std::path::MAIN_SEPARATOR;
        let path_of = |entries: &[PathBuf]| -> OsString { std::env::join_paths(entries).unwrap() };

        assert!(on_path_in(&dir, &path_of(&[other.clone(), dir.clone()])));
        let trailing = PathBuf::from(format!("{}{sep}", dir.display()));
        assert!(on_path_in(&dir, &path_of(&[trailing])));
        let dotted = tmp.path().join(".").join("bin");
        assert!(on_path_in(&dir, &path_of(&[dotted])));
        assert!(!on_path_in(&dir, &path_of(&[other])));
        assert!(!on_path_in(&dir, OsStr::new("")));
    }
}
