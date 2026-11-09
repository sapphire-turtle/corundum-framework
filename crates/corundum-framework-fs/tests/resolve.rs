mod common;

use common::{HidesXmp, p, workspace};
use corundum_framework_fs::{FsError, Workspace};

#[test]
fn resolves_files_and_folders() {
    let (_dir, ws) = workspace(&["a/b.txt"]);
    let file = ws.resolve(&(), &p("a/b.txt")).unwrap();
    assert!(file.metadata().is_file());
    assert_eq!(file.path(), &p("a/b.txt"));
    assert!(file.open_file().is_ok());
    assert!(ws.resolve(&(), &p("a")).unwrap().metadata().is_dir());
    let root = ws.resolve(&(), &p("")).unwrap();
    assert!(root.metadata().is_dir());
    assert!(root.open_dir().is_ok());
    assert!(matches!(root.open_file(), Err(FsError::InvalidPath(_))));
    assert!(matches!(file.open_dir(), Err(FsError::InvalidPath(_))));
}

#[test]
fn missing_is_not_found() {
    let (_dir, ws) = workspace(&[]);
    assert!(matches!(
        ws.resolve(&(), &p("nope")),
        Err(FsError::NotFound(_))
    ));
    assert!(matches!(
        ws.resolve(&(), &p("nope/deeper")),
        Err(FsError::NotFound(_))
    ));
}

#[test]
fn dot_names_are_hidden() {
    let (_dir, ws) = workspace(&[".asteria/x"]);
    assert!(ws.is_hidden(&(), &p(".asteria/x")));
    assert!(matches!(
        ws.resolve(&(), &p(".asteria/x")),
        Err(FsError::InvalidPath(_))
    ));
}

#[test]
fn the_apps_hidden_covers_descendants() {
    let (_dir, ws) = workspace(&["p.jpg", "p.jpg.xmp", "meta/inner.txt"]);
    assert!(matches!(
        ws.resolve(&HidesXmp, &p("p.jpg.xmp")),
        Err(FsError::InvalidPath(_))
    ));
    assert!(matches!(
        ws.resolve(&HidesXmp, &p("meta/inner.txt")),
        Err(FsError::InvalidPath(_))
    ));
    assert!(ws.resolve(&HidesXmp, &p("p.jpg")).is_ok());
}

#[cfg(unix)]
#[test]
fn a_symlink_in_the_middle_is_refused() {
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), b"secret").unwrap();
    let (dir, ws) = workspace(&[]);
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    assert!(matches!(
        ws.resolve(&(), &p("link/secret.txt")),
        Err(FsError::InvalidPath(_))
    ));
}

#[cfg(unix)]
#[test]
fn a_final_symlink_is_returned_as_a_symlink() {
    let (dir, ws) = workspace(&["a/"]);
    std::os::unix::fs::symlink(dir.path().join("a"), dir.path().join("link")).unwrap();
    let resolved = ws.resolve(&(), &p("link")).unwrap();
    assert!(resolved.metadata().file_type().is_symlink());
    assert!(matches!(resolved.open_file(), Err(FsError::InvalidPath(_))));
    assert!(matches!(resolved.open_dir(), Err(FsError::InvalidPath(_))));
}

#[test]
fn opening_a_missing_root_fails() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        Workspace::open(dir.path().join("missing")),
        Err(FsError::NotFound(_))
    ));
    std::fs::write(dir.path().join("file"), b"").unwrap();
    assert!(matches!(
        Workspace::open(dir.path().join("file")),
        Err(FsError::InvalidPath(_))
    ));
}

/// Whether the temporary folders are on a file system that ignores case (macOS, Windows).
fn ignores_case() -> bool {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"").unwrap();
    dir.path().join("A").exists()
}

#[test]
fn hiding_holds_whatever_the_case_asked() {
    if !ignores_case() {
        eprintln!("skipped: this file system tells case apart");
        return;
    }
    let (_dir, ws) = workspace(&["p.jpg.xmp", "meta/inner.txt"]);
    for path in ["P.JPG.XMP", "p.jpg.Xmp", "META/inner.txt", "Meta"] {
        assert!(
            matches!(
                ws.resolve(&HidesXmp, &p(path)),
                Err(FsError::InvalidPath(_))
            ),
            "{path} was resolved"
        );
    }
}

#[cfg(unix)]
#[test]
fn a_middle_folder_swapped_for_a_symlink_after_resolve_is_not_followed() {
    let (dir, ws) = workspace(&["a/b/f"]);
    std::fs::write(dir.path().join("a/b/f"), b"inside").unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(outside.path().join("b")).unwrap();
    std::fs::write(outside.path().join("b/f"), b"secret").unwrap();
    let resolved = ws.resolve(&(), &p("a/b/f")).unwrap();
    std::fs::rename(dir.path().join("a"), dir.path().join("a_old")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("a")).unwrap();
    let mut got = String::new();
    std::io::Read::read_to_string(&mut resolved.open_file().unwrap(), &mut got).unwrap();
    assert_eq!(got, "inside");
}

#[cfg(unix)]
#[test]
fn a_symlinked_middle_folder_is_refused() {
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("f"), b"secret").unwrap();
    let (dir, ws) = workspace(&[]);
    std::os::unix::fs::symlink(outside.path(), dir.path().join("a")).unwrap();
    assert!(matches!(
        ws.resolve(&(), &p("a/f")),
        Err(FsError::InvalidPath(_))
    ));
}

#[cfg(unix)]
#[test]
fn a_symlink_swapped_in_for_the_file_after_resolve_is_not_followed() {
    let (dir, ws) = workspace(&["f"]);
    let resolved = ws.resolve(&(), &p("f")).unwrap();
    std::fs::write(dir.path().join("secret"), b"secret").unwrap();
    std::fs::remove_file(dir.path().join("f")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("secret"), dir.path().join("f")).unwrap();
    assert!(matches!(resolved.open_file(), Err(FsError::InvalidPath(_))));
}

#[cfg(unix)]
#[test]
fn a_fifo_swapped_in_after_resolve_does_not_block() {
    let (dir, ws) = workspace(&["f"]);
    let resolved = ws.resolve(&(), &p("f")).unwrap();
    let fifo = dir.path().join("f");
    std::fs::remove_file(&fifo).unwrap();
    let c = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    assert!(matches!(resolved.open_file(), Err(FsError::InvalidPath(_))));
}
