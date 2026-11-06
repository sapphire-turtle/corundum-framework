mod common;

use common::{HidesXmp, p, workspace};
use corundum_framework_fs::{FsError, Workspace};

#[test]
fn resolves_files_and_folders() {
    let (_dir, ws) = workspace(&["a/b.txt"]);
    let (path, meta) = ws.resolve(&(), &p("a/b.txt")).unwrap();
    assert!(meta.is_file());
    assert!(path.ends_with("a/b.txt"));
    assert!(ws.resolve(&(), &p("a")).unwrap().1.is_dir());
    let (root, meta) = ws.resolve(&(), &p("")).unwrap();
    assert!(meta.is_dir());
    assert_eq!(root, ws.root());
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
    let (_, meta) = ws.resolve(&(), &p("link")).unwrap();
    assert!(meta.file_type().is_symlink());
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
