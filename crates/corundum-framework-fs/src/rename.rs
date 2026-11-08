//! A rename that never replaces what is at its destination.

use std::io;

use cap_std::fs::Dir;

/// Renames `from` in `from_dir` to `to` in `to_dir`, failing with
/// [`io::ErrorKind::AlreadyExists`] if anything is at `to`. Both names are single segments.
///
/// On Linux and Apple systems the kernel checks atomically (`renameat2` with
/// `RENAME_NOREPLACE`, `renameatx_np` with `RENAME_EXCL`). Elsewhere (Windows, other unixes),
/// and on a Linux file system without the flag (`EINVAL`), it falls back to looking first
/// and renaming then. The fallback has a window: an entry made at `to` between the look and
/// the rename is replaced (a file; a folder only if empty, and on unix only by a folder).
pub(crate) fn rename_noreplace(
    from_dir: &Dir,
    from: &str,
    to_dir: &Dir,
    to: &str,
) -> io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        use rustix::fs::{RenameFlags, renameat_with};
        match renameat_with(from_dir, from, to_dir, to, RenameFlags::NOREPLACE) {
            Ok(()) => return Ok(()),
            #[cfg(any(target_os = "linux", target_os = "android"))]
            Err(rustix::io::Errno::INVAL) => {}
            Err(e) => return Err(e.into()),
        }
    }
    rename_checked(from_dir, from, to_dir, to)
}

/// Looks at `to`, and renames only if nothing is there; see [`rename_noreplace`]'s window.
fn rename_checked(from_dir: &Dir, from: &str, to_dir: &Dir, to: &str) -> io::Result<()> {
    match to_dir.symlink_metadata(to) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => from_dir.rename(from, to_dir, to),
        Err(e) => Err(e),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{to:?} exists"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use cap_std::ambient_authority;

    use super::*;

    fn dir() -> (tempfile::TempDir, Dir) {
        let t = tempfile::tempdir().unwrap();
        let d = Dir::open_ambient_dir(t.path(), ambient_authority()).unwrap();
        (t, d)
    }

    // The fallback is the only path on Windows: run it here too.
    #[test]
    fn the_fallback_renames_and_never_replaces() {
        let (_t, d) = dir();
        d.write("a", b"a").unwrap();
        d.write("b", b"b").unwrap();
        let e = rename_checked(&d, "a", &d, "b").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(d.read("b").unwrap(), b"b");
        rename_checked(&d, "a", &d, "c").unwrap();
        assert_eq!(d.read("c").unwrap(), b"a");
        assert!(!d.exists("a"));
    }

    #[test]
    fn the_rename_never_replaces() {
        let (_t, d) = dir();
        d.write("a", b"a").unwrap();
        d.create_dir("f").unwrap();
        let e = rename_noreplace(&d, "a", &d, "f").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        rename_noreplace(&d, "a", &d, "c").unwrap();
        assert_eq!(d.read("c").unwrap(), b"a");
    }
}
