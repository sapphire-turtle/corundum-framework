//! File calls std has no portable form of.
//!
//! [`rename_no_replace`] renames a file or directory in one call that never replaces what
//! is at the destination, which `std::fs::rename` cannot promise: it replaces a file on
//! every OS, and on Unix an empty directory too. Checking first and then renaming leaves a
//! moment in which another writer's file at the destination is replaced; a hard link and
//! then a removal leaves both names behind if the process stops between them.

use std::io;
use std::path::Path;

/// Rename `from` to `to` atomically, failing with [`io::ErrorKind::AlreadyExists`] and
/// changing nothing if anything is at `to`: a file, a directory (an empty one included),
/// or a symlink, a dangling one included (the link is not followed).
///
/// It never copies. `from` and `to` on different filesystems (volumes) fail with
/// [`io::ErrorKind::CrossesDevices`], and the caller copies if it wants to.
///
/// When the OS, or the filesystem `to` is on, has no rename that refuses to replace, it
/// fails with [`io::ErrorKind::Unsupported`] and changes nothing, so that the caller can
/// fall back on a way of its own (such as a hard link then a removal, or a check then
/// [`std::fs::rename`] where it is the only writer).
///
/// | OS | Call | Unsupported when |
/// |---|---|---|
/// | Linux, Android | `renameat2(…, RENAME_NOREPLACE)` | the kernel is older than 3.15 (`ENOSYS`), or the filesystem does not support the flag (`EINVAL`: some FUSE and network filesystems, and older kernels' others; ext4, btrfs, xfs and tmpfs do) |
/// | macOS, iOS | `renameatx_np(…, RENAME_EXCL)` | macOS is older than 10.12, or the filesystem does not support the flag (`ENOTSUP`; APFS and HFS+ do) |
/// | Windows | `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` or `MOVEFILE_COPY_ALLOWED` | never: atomic within an NTFS volume |
/// | others | — | always |
///
/// ## A name that differs only in case
///
/// On a case-insensitive filesystem (macOS's and Windows's by default), `from` and `to`
/// that differ only in case (`photo.JPG` to `photo.jpg`) name the same file:
///
/// - **Windows** (NTFS) and **macOS and iOS** (APFS): the file is renamed to the new
///   case, as in Explorer or the Finder: `to` is `from` itself, which the call does not
///   count as being there.
/// - **Linux** with a case-insensitive directory (ext4's or f2fs's casefold): it fails with
///   [`io::ErrorKind::AlreadyExists`], as the kernel checks `RENAME_NOREPLACE` against the
///   name `to` finds before it looks at which file that is. (From the kernel's code; not
///   tried by the tests, which run on no casefold directory.) A caller renames through a
///   temporary name (`photo.JPG` to a name of its own, then that to `photo.jpg`).
///
/// On a case-sensitive filesystem the two are different names, and the rename is an
/// ordinary one.
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    imp::rename_no_replace(from, to)
}

/// The error for a rename the OS or filesystem cannot make without replacing.
#[cfg_attr(windows, allow(dead_code))]
fn unsupported(why: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("no rename that refuses to replace: {why}"),
    )
}

#[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
mod imp {
    use std::io;
    use std::path::Path;

    use rustix::fs::{CWD, RenameFlags, renameat_with};
    use rustix::io::Errno;

    pub(super) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        // NOREPLACE is RENAME_NOREPLACE on Linux and RENAME_EXCL on Apple's.
        match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
            Ok(()) => Ok(()),
            // No renameat2 (Linux before 3.15) or renameatx_np (macOS before 10.12).
            Err(Errno::NOSYS) => Err(super::unsupported("the OS has none")),
            // The filesystem has none: Apple's answer.
            Err(Errno::NOTSUP) => Err(super::unsupported(format!(
                "the filesystem of {} has none",
                to.display()
            ))),
            // Linux's answer for a filesystem without the flag, but also the answer for
            // a directory renamed into itself, which is not about the flag.
            Err(Errno::INVAL) if !into_itself(from, to) => Err(super::unsupported(format!(
                "the filesystem of {} has none",
                to.display()
            ))),
            Err(errno) => Err(errno.into()),
        }
    }

    /// Whether `to` is in `from`, a directory: the other reason a rename fails with
    /// `EINVAL`. Only asked once it has.
    fn into_itself(from: &Path, to: &Path) -> bool {
        let Some(parent) = to.parent() else {
            return false;
        };
        // `to` itself need not be there; its directory is, or the rename would have
        // failed with ENOENT.
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        match (from.canonicalize(), parent.canonicalize()) {
            (Ok(from), Ok(parent)) => parent.starts_with(from),
            _ => false,
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::Win32::Foundation::{ERROR_FILE_EXISTS, ERROR_NOT_SAME_DEVICE};
    use windows::Win32::Storage::FileSystem::{MOVE_FILE_FLAGS, MoveFileExW};
    use windows::core::PCWSTR;

    pub(super) fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        let wide = |path: &Path| {
            path.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<u16>>()
        };
        let (from_wide, to_wide) = (wide(from), wide(to));
        // No MOVEFILE_REPLACE_EXISTING: an existing `to` fails the call. No
        // MOVEFILE_COPY_ALLOWED: another volume fails it rather than copy.
        // SAFETY: both are NUL-terminated UTF-16 strings that outlive the call.
        unsafe {
            MoveFileExW(
                PCWSTR(from_wide.as_ptr()),
                PCWSTR(to_wide.as_ptr()),
                MOVE_FILE_FLAGS(0),
            )
        }
        .map_err(|err| {
            // An HRESULT from a Win32 error code (facility 7) carries the code in its
            // low word, which std maps to its kinds.
            let hresult = err.code().0 as u32;
            if hresult & 0xFFFF_0000 != 0x8007_0000 {
                return io::Error::other(err);
            }
            let code = hresult & 0xFFFF;
            // Named here rather than left to std's mapping, which has changed between
            // Rust versions for these two.
            if code == ERROR_NOT_SAME_DEVICE.0 {
                io::Error::new(
                    io::ErrorKind::CrossesDevices,
                    format!(
                        "{} and {} are on different volumes",
                        from.display(),
                        to.display()
                    ),
                )
            } else if code == ERROR_FILE_EXISTS.0 {
                io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("{} is there already", to.display()),
                )
            } else {
                io::Error::from_raw_os_error(code as i32)
            }
        })
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
)))]
mod imp {
    use std::io;
    use std::path::Path;

    pub(super) fn rename_no_replace(_from: &Path, _to: &Path) -> io::Result<()> {
        Err(super::unsupported("not on this OS"))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    /// The rename, or `None` where this filesystem has none (the test has nothing to try).
    fn rename(from: &Path, to: &Path) -> Option<io::Result<()>> {
        match rename_no_replace(from, to) {
            Err(err) if err.kind() == io::ErrorKind::Unsupported => {
                eprintln!("skipped: {err}");
                None
            }
            result => Some(result),
        }
    }

    #[test]
    fn renames_when_nothing_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        fs::write(&from, "a").unwrap();
        let Some(result) = rename(&from, &to) else {
            return;
        };
        result.unwrap();
        assert!(!from.exists());
        assert_eq!(fs::read_to_string(&to).unwrap(), "a");
    }

    #[test]
    fn renames_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        fs::create_dir(&from).unwrap();
        fs::write(from.join("in"), "a").unwrap();
        let Some(result) = rename(&from, &to) else {
            return;
        };
        result.unwrap();
        assert!(!from.exists());
        assert_eq!(fs::read_to_string(to.join("in")).unwrap(), "a");
    }

    #[test]
    fn a_file_there_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        fs::write(&from, "a").unwrap();
        fs::write(&to, "b").unwrap();
        let Some(result) = rename(&from, &to) else {
            return;
        };
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&from).unwrap(), "a");
        assert_eq!(fs::read_to_string(&to).unwrap(), "b");
    }

    #[test]
    fn an_empty_directory_there_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        fs::create_dir(&from).unwrap();
        fs::write(from.join("in"), "a").unwrap();
        fs::create_dir(&to).unwrap();
        let Some(result) = rename(&from, &to) else {
            return;
        };
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(from.join("in")).unwrap(), "a");
        assert_eq!(fs::read_dir(&to).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_dangling_symlink_there_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        fs::write(&from, "a").unwrap();
        std::os::unix::fs::symlink(dir.path().join("nowhere"), &to).unwrap();
        let Some(result) = rename(&from, &to) else {
            return;
        };
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&from).unwrap(), "a");
        assert_eq!(fs::read_link(&to).unwrap(), dir.path().join("nowhere"));
        assert!(!dir.path().join("nowhere").exists());
    }

    // Windows lets only an administrator, or developer mode, make a symlink: the test
    // runs where it can make one.
    #[cfg(windows)]
    #[test]
    fn a_dangling_symlink_there_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        fs::write(&from, "a").unwrap();
        if let Err(err) = std::os::windows::fs::symlink_file(dir.path().join("nowhere"), &to) {
            eprintln!("skipped: no symlink: {err}");
            return;
        }
        let err = rename_no_replace(&from, &to).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&from).unwrap(), "a");
        assert_eq!(fs::read_link(&to).unwrap(), dir.path().join("nowhere"));
        assert!(!dir.path().join("nowhere").exists());
    }

    #[test]
    fn a_missing_source_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let Some(result) = rename(&dir.path().join("from"), &dir.path().join("to")) else {
            return;
        };
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    #[test]
    fn a_directory_into_itself_is_not_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from");
        fs::create_dir(&from).unwrap();
        let err = rename_no_replace(&from, &from.join("in")).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{err}");
        assert!(from.is_dir());
    }

    /// Across filesystems: the system's temporary directory and the test binary's,
    /// where they are on different ones (a tmpfs `/tmp`, say). Elsewhere there is
    /// nothing to try.
    #[cfg(unix)]
    #[test]
    fn across_filesystems_it_does_not_copy() {
        use std::os::unix::fs::MetadataExt;

        let exe = std::env::current_exe().unwrap();
        let here = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
        let there = tempfile::tempdir().unwrap();
        let dev = |path: &Path| fs::metadata(path).unwrap().dev();
        if dev(here.path()) == dev(there.path()) {
            eprintln!("skipped: the temporary directories are on one filesystem");
            return;
        }
        let (from, to) = (here.path().join("from"), there.path().join("to"));
        fs::write(&from, "a").unwrap();
        let Some(result) = rename(&from, &to) else {
            return;
        };
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::CrossesDevices);
        assert_eq!(fs::read_to_string(&from).unwrap(), "a");
        assert!(fs::symlink_metadata(&to).is_err());
    }

    /// A rename to a name that differs only in case, as the docs of [`rename_no_replace`]
    /// say it goes.
    #[test]
    fn a_name_differing_in_case() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("photo.JPG"), dir.path().join("photo.jpg"));
        fs::write(&from, "a").unwrap();
        let case_insensitive = fs::symlink_metadata(&to).is_ok();
        let Some(result) = rename(&from, &to) else {
            return;
        };
        let names = || {
            fs::read_dir(dir.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>()
        };
        // Windows and macOS rename to the new case; a case-sensitive filesystem has two
        // names. Linux's casefold refuses.
        if !case_insensitive || !cfg!(any(target_os = "linux", target_os = "android")) {
            result.unwrap();
            assert_eq!(names(), ["photo.jpg"]);
        } else {
            assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
            assert_eq!(names(), ["photo.JPG"]);
        }
    }
}
