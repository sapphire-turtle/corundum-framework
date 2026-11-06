mod common;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use common::{HidesXmp, ManagesJpg, workspace, write};
use corundum_framework_fs::api::{EntryKind, MAX_READ_LEN, ReadParams, StatParams};
use corundum_framework_fs::{FsError, WsPath, read, stat};

fn st(path: &str) -> StatParams {
    StatParams { path: path.into() }
}

fn rd(path: &str, offset: u64, len: u32) -> ReadParams {
    ReadParams {
        path: path.into(),
        offset,
        len,
    }
}

#[test]
fn stat_gives_one_entry() {
    let (dir, ws) = workspace(&["a/"]);
    write(dir.path(), "a/b.txt", b"hello");
    let e = stat(&ws, &(), &st("a/b.txt")).unwrap();
    assert_eq!(e.name, "b.txt");
    assert_eq!(e.kind, EntryKind::File);
    assert_eq!(e.size, 5);
}

#[test]
fn stat_is_annotated() {
    let (_dir, ws) = workspace(&["p.jpg"]);
    let app = ManagesJpg {
        expect_dir: WsPath::root(),
    };
    assert!(stat(&ws, &app, &st("p.jpg")).unwrap().managed.is_some());
}

#[test]
fn stat_of_the_root() {
    let (_dir, ws) = workspace(&[]);
    let e = stat(&ws, &(), &st("")).unwrap();
    assert_eq!(e.name, "");
    assert_eq!(e.kind, EntryKind::Dir);
}

#[test]
fn reads_a_range() {
    let (dir, ws) = workspace(&[]);
    write(dir.path(), "f", b"hello world");
    let r = read(&ws, &(), &rd("f", 6, 5)).unwrap();
    assert_eq!(STANDARD.decode(&r.data).unwrap(), b"world");
    assert_eq!(r.total, 11);
}

#[test]
fn reading_past_the_end_is_empty() {
    let (dir, ws) = workspace(&[]);
    write(dir.path(), "f", b"hello world");
    let r = read(&ws, &(), &rd("f", 100, 10)).unwrap();
    assert!(STANDARD.decode(&r.data).unwrap().is_empty());
    assert_eq!(r.total, 11);
}

#[test]
fn read_len_is_capped() {
    let (dir, ws) = workspace(&[]);
    write(dir.path(), "f", b"small");
    assert!(matches!(
        read(&ws, &(), &rd("f", 0, MAX_READ_LEN + 1)),
        Err(FsError::TooLarge(_))
    ));
    let r = read(&ws, &(), &rd("f", 0, MAX_READ_LEN)).unwrap();
    assert_eq!(STANDARD.decode(&r.data).unwrap(), b"small");
}

#[test]
fn reading_a_folder_or_a_symlink_is_invalid_path() {
    let (dir, ws) = workspace(&["a/b.txt"]);
    assert!(matches!(
        read(&ws, &(), &rd("a", 0, 1)),
        Err(FsError::InvalidPath(_))
    ));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("a/b.txt"), dir.path().join("l")).unwrap();
        assert!(matches!(
            read(&ws, &(), &rd("l", 0, 1)),
            Err(FsError::InvalidPath(_))
        ));
    }
    let _ = dir;
}

#[test]
fn hidden_paths_are_invalid_path() {
    let (_dir, ws) = workspace(&["p.jpg.xmp"]);
    assert!(matches!(
        stat(&ws, &HidesXmp, &st("p.jpg.xmp")),
        Err(FsError::InvalidPath(_))
    ));
    assert!(matches!(
        read(&ws, &HidesXmp, &rd("p.jpg.xmp", 0, 1)),
        Err(FsError::InvalidPath(_))
    ));
}
