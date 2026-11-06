mod common;

use common::{HidesXmp, ManagesJpg, p, workspace, write};
use corundum_framework_fs::api::{EntryKind, ListParams, Listing};
use corundum_framework_fs::{FsError, WsPath, list};

fn params(path: &str, cursor: Option<&str>, limit: Option<u32>) -> ListParams {
    ListParams {
        path: path.into(),
        cursor: cursor.map(Into::into),
        limit,
    }
}

fn names(l: &Listing) -> Vec<&str> {
    l.entries.iter().map(|e| e.name.as_str()).collect()
}

#[test]
fn lists_in_name_order_with_kinds() {
    let (dir, ws) = workspace(&["a/"]);
    write(dir.path(), "b.txt", b"abc");
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.path().join("a"), dir.path().join("l")).unwrap();
    let l = list(&ws, &(), &params("", None, None)).unwrap();
    let mut want = vec!["a", "b.txt"];
    if cfg!(unix) {
        want.push("l");
    }
    assert_eq!(names(&l), want);
    assert_eq!(l.entries[0].kind, EntryKind::Dir);
    assert_eq!(l.entries[1].kind, EntryKind::File);
    assert_eq!(l.entries[1].size, 3);
    assert!(l.entries[1].modified.is_some());
    if cfg!(unix) {
        assert_eq!(l.entries[2].kind, EntryKind::Symlink);
    }
    assert_eq!(l.next_cursor, None);
}

#[test]
fn hidden_entries_are_not_listed() {
    let (_dir, ws) = workspace(&[".asteria/", ".DS_Store", "p.jpg", "p.jpg.xmp"]);
    let l = list(&ws, &HidesXmp, &params("", None, None)).unwrap();
    assert_eq!(names(&l), ["p.jpg"]);
}

#[test]
fn pages_follow_the_cursor() {
    let (_dir, ws) = workspace(&["f0", "f1", "f2", "f3", "f4"]);
    let a = list(&ws, &(), &params("", None, Some(2))).unwrap();
    assert_eq!(names(&a), ["f0", "f1"]);
    assert_eq!(a.next_cursor.as_deref(), Some("f1"));
    let b = list(&ws, &(), &params("", a.next_cursor.as_deref(), Some(2))).unwrap();
    assert_eq!(names(&b), ["f2", "f3"]);
    let c = list(&ws, &(), &params("", b.next_cursor.as_deref(), Some(2))).unwrap();
    assert_eq!(names(&c), ["f4"]);
    assert_eq!(c.next_cursor, None);
}

#[test]
fn a_page_after_a_removed_cursor_continues() {
    let (dir, ws) = workspace(&["f0", "f1", "f2", "f3", "f4"]);
    let a = list(&ws, &(), &params("", None, Some(2))).unwrap();
    std::fs::remove_file(dir.path().join("f1")).unwrap();
    std::fs::remove_file(dir.path().join("f2")).unwrap();
    let b = list(&ws, &(), &params("", a.next_cursor.as_deref(), Some(2))).unwrap();
    assert_eq!(names(&b), ["f3", "f4"]);
}

#[test]
fn limit_is_clamped() {
    let (dir, ws) = workspace(&[]);
    for i in 0..1005 {
        write(dir.path(), &format!("f{i:04}"), b"");
    }
    assert_eq!(
        list(&ws, &(), &params("", None, Some(0)))
            .unwrap()
            .entries
            .len(),
        1
    );
    let big = list(&ws, &(), &params("", None, Some(5000))).unwrap();
    assert_eq!(big.entries.len(), 1000);
    assert!(big.next_cursor.is_some());
    assert_eq!(
        list(&ws, &(), &params("", None, None))
            .unwrap()
            .entries
            .len(),
        200
    );
}

#[test]
fn annotate_marks_managed_entries() {
    let (_dir, ws) = workspace(&["d/p.jpg", "d/notes.txt"]);
    let app = ManagesJpg { expect_dir: p("d") };
    let l = list(&ws, &app, &params("d", None, None)).unwrap();
    assert_eq!(names(&l), ["notes.txt", "p.jpg"]);
    assert!(l.entries[0].managed.is_none());
    assert_eq!(l.entries[1].managed.as_ref().unwrap().item_id, "p.jpg");
    let root = ManagesJpg {
        expect_dir: WsPath::root(),
    };
    list(&ws, &root, &params("", None, None)).unwrap();
}

#[test]
fn listing_a_file_is_invalid_path() {
    let (_dir, ws) = workspace(&["b.txt"]);
    assert!(matches!(
        list(&ws, &(), &params("b.txt", None, None)),
        Err(FsError::InvalidPath(_))
    ));
}

#[cfg(unix)]
#[test]
fn non_utf8_names_are_skipped() {
    use std::os::unix::ffi::OsStrExt;
    let (dir, ws) = workspace(&["ok.txt"]);
    let bad = std::ffi::OsStr::from_bytes(b"bad\xff");
    std::fs::write(dir.path().join(bad), b"").unwrap();
    let l = list(&ws, &(), &params("", None, None)).unwrap();
    assert_eq!(names(&l), ["ok.txt"]);
}
