//! Replacing another program's executable, which may be running.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use crate::Error;
use crate::Result;
use crate::install::stage;

/// The file operations a replacement makes, so the order they happen in can be tested without
/// a Windows machine.
pub(crate) trait Fs {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove(&self, path: &Path) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
}

pub(crate) struct RealFs;

impl Fs for RealFs {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    fn remove(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_file(path)
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

/// Replace the executable at `path` with `bytes`, even while it runs: a running program keeps
/// the old file (unix) or can be renamed but not overwritten (Windows), so the new one is
/// written beside it and moved into its place. Running copies go on with the old code until
/// they are restarted.
///
/// The staging file is removed on every error.
pub fn replace_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    // Close the file: Windows will not rename one that is open. The path still removes it
    // when dropped.
    let staged = stage(dir, bytes)?.into_temp_path();
    if cfg!(windows) {
        replace_windows(&RealFs, path, &staged)
    } else {
        replace_unix(&RealFs, path, &staged)
    }
    .map_err(Error::Io)
}

/// A rename over the target replaces it atomically; processes running it keep the old inode.
pub(crate) fn replace_unix(fs: &dyn Fs, path: &Path, staged: &Path) -> io::Result<()> {
    fs.rename(staged, path)
}

/// `path` with `.old`, and `.<n>` after it when `n` is not 0, appended.
fn old_name(path: &Path, n: u32) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(".old");
    if n > 0 {
        name.push(format!(".{n}"));
    }
    PathBuf::from(name)
}

/// Windows cannot overwrite or delete a running executable but can rename it: set the target
/// aside as `path.old`, then move the new file in, putting the old one back if that fails.
///
/// A set-aside file stays locked until its process exits, so the next update removes
/// `path.old` and `path.old.<n>` first; one that cannot be removed is skipped, and the
/// target goes to the first name that is free.
pub(crate) fn replace_windows(fs: &dyn Fs, path: &Path, staged: &Path) -> io::Result<()> {
    // Stops at the first absent `.old*` name; a later leftover is reached once the next update
    // fills the gap.
    let mut n = 0;
    while fs.exists(&old_name(path, n)) {
        // Still locked by a running copy: leave it and use another name.
        let _ = fs.remove(&old_name(path, n));
        n += 1;
    }
    let old = (0..)
        .map(|n| old_name(path, n))
        .find(|old| !fs.exists(old))
        .expect("an unbounded range has a free name");
    fs.rename(path, &old)?;
    if let Err(e) = fs.rename(staged, path) {
        let _ = fs.rename(&old, path);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashSet;

    /// Records the calls that change anything, and fails the one named by `fail`.
    #[derive(Default)]
    struct RecordingFs {
        calls: RefCell<Vec<String>>,
        present: RefCell<HashSet<PathBuf>>,
        fail: Option<String>,
    }

    impl RecordingFs {
        fn with(present: &[&str]) -> Self {
            Self {
                present: RefCell::new(present.iter().map(PathBuf::from).collect()),
                ..Self::default()
            }
        }
        fn failing(mut self, call: &str) -> Self {
            self.fail = Some(call.to_owned());
            self
        }
        fn record(&self, call: String) -> io::Result<()> {
            self.calls.borrow_mut().push(call.clone());
            if self.fail.as_deref() == Some(call.as_str()) {
                return Err(io::Error::other("injected"));
            }
            Ok(())
        }
        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl Fs for RecordingFs {
        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.record(format!("rename {} {}", from.display(), to.display()))?;
            let mut present = self.present.borrow_mut();
            present.remove(from);
            present.insert(to.to_owned());
            Ok(())
        }
        fn remove(&self, path: &Path) -> io::Result<()> {
            self.record(format!("remove {}", path.display()))?;
            self.present.borrow_mut().remove(path);
            Ok(())
        }
        fn exists(&self, path: &Path) -> bool {
            self.present.borrow().contains(path)
        }
    }

    #[test]
    fn unix_renames_over_the_target() {
        let fs = RecordingFs::default();
        replace_unix(&fs, Path::new("/d/app"), Path::new("/d/.update-x")).unwrap();
        assert_eq!(fs.calls(), ["rename /d/.update-x /d/app"]);
    }

    #[test]
    fn windows_sets_the_running_exe_aside() {
        let fs = RecordingFs::with(&["/d/app", "/d/.update-x"]);
        replace_windows(&fs, Path::new("/d/app"), Path::new("/d/.update-x")).unwrap();
        assert_eq!(
            fs.calls(),
            ["rename /d/app /d/app.old", "rename /d/.update-x /d/app"]
        );
    }

    #[test]
    fn windows_removes_an_earlier_leftover_first() {
        let fs = RecordingFs::with(&["/d/app", "/d/app.old", "/d/app.old.1"]);
        replace_windows(&fs, Path::new("/d/app"), Path::new("/d/.update-x")).unwrap();
        assert_eq!(
            fs.calls(),
            [
                "remove /d/app.old",
                "remove /d/app.old.1",
                "rename /d/app /d/app.old",
                "rename /d/.update-x /d/app"
            ]
        );
    }

    #[test]
    fn windows_goes_on_when_the_leftover_is_locked() {
        let fs = RecordingFs::with(&["/d/app", "/d/app.old"]).failing("remove /d/app.old");
        replace_windows(&fs, Path::new("/d/app"), Path::new("/d/.update-x")).unwrap();
        assert_eq!(
            fs.calls(),
            [
                "remove /d/app.old",
                "rename /d/app /d/app.old.1",
                "rename /d/.update-x /d/app"
            ]
        );
    }

    #[test]
    fn windows_puts_the_old_one_back_when_the_move_fails() {
        let fs = RecordingFs::with(&["/d/app"]).failing("rename /d/.update-x /d/app");
        let result = replace_windows(&fs, Path::new("/d/app"), Path::new("/d/.update-x"));
        assert!(result.is_err());
        assert_eq!(fs.calls().last().unwrap(), "rename /d/app.old /d/app");
    }

    #[test]
    fn the_real_thing_replaces_content_and_keeps_it_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("f");
        std::fs::write(&f, b"old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        replace_file(&f, b"new").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                std::fs::metadata(&f).unwrap().permissions().mode() & 0o111,
                0
            );
        }
        // Windows keeps the old file beside it until the next update.
        let left = std::fs::read_dir(tmp.path()).unwrap().count();
        assert_eq!(left, if cfg!(windows) { 2 } else { 1 });
    }

    #[test]
    #[cfg(unix)]
    fn a_failed_replace_leaves_no_staging_file() {
        let tmp = tempfile::tempdir().unwrap();
        // Renaming a file over a directory fails, after the file is staged.
        let dir = tmp.path().join("d");
        std::fs::create_dir(&dir).unwrap();
        assert!(replace_file(&dir, b"new").is_err());
        let left: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].as_ref().unwrap().path(), dir);
    }
}
