//! `fs.move`.

mod common;

use std::fs;

use common::*;
use corundum_framework_fs::api::Expect;
use corundum_framework_fs::ops::{MoveOp, MovePlan};
use corundum_framework_fs::{Access, WsPath};
use corundum_framework_journal::{Outcome, Planned, Skip};

fn mv(from: &str, to: &str, expect: Option<Expect>) -> Planned {
    Planned::new::<MoveOp>(&MovePlan {
        from: p(from),
        to: p(to),
        expect,
    })
}

fn done(o: &Outcome) -> bool {
    matches!(o, Outcome::Done(_))
}

#[test]
fn a_move_renames_and_its_undo_moves_back() {
    let (dir, ws) = workspace(&["a.txt"]);
    let j = journal(ws, ());
    assert!(done(&run(&j, mv("a.txt", "b.txt", None))));
    assert_eq!(names(dir.path(), ""), ["b.txt"]);
    assert!(done(&undo(&j)));
    assert_eq!(names(dir.path(), ""), ["a.txt"]);
    assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"a.txt");
}

#[test]
fn a_move_never_replaces() {
    let (dir, ws) = workspace(&["a.txt", "b.txt"]);
    let j = journal(ws, ());
    assert_eq!(
        run(&j, mv("a.txt", "b.txt", None)),
        Outcome::Skipped(Skip::Exists)
    );
    assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"a.txt");
    assert_eq!(fs::read(dir.path().join("b.txt")).unwrap(), b"b.txt");
}

#[test]
fn a_folder_moves_with_everything_in_it() {
    let (dir, ws) = workspace(&["d/x"]);
    let j = journal(ws, ());
    assert!(done(&run(&j, mv("d", "e", None))));
    assert_eq!(names(dir.path(), ""), ["e"]);
    assert_eq!(fs::read(dir.path().join("e/x")).unwrap(), b"d/x");
}

#[test]
fn a_folder_cannot_move_into_itself() {
    let (dir, ws) = workspace(&["d/sub/", "d/x"]);
    let j = journal(ws, ());
    assert_eq!(
        run(&j, mv("d", "d/sub", None)),
        Outcome::Skipped(Skip::InvalidPath)
    );
    assert_eq!(
        run(&j, mv("d", "d/sub/deeper", None)),
        Outcome::Skipped(Skip::InvalidPath)
    );
    assert_eq!(
        run(&j, mv("d", "d", None)),
        Outcome::Skipped(Skip::InvalidPath)
    );
    assert_eq!(
        run(&j, mv("", "z", None)),
        Outcome::Skipped(Skip::InvalidPath)
    );
    assert_eq!(names(dir.path(), "d"), ["sub", "x"]);
}

#[test]
fn a_changed_file_is_not_moved() {
    let (dir, ws) = workspace(&["a.txt"]);
    let j = journal(ws, ());
    let expect = Expect {
        size: 99,
        modified: None,
        hash: None,
    };
    assert_eq!(
        run(&j, mv("a.txt", "b.txt", Some(expect))),
        Outcome::Skipped(Skip::PreconditionFailed)
    );
    assert_eq!(names(dir.path(), ""), ["a.txt"]);
}

fn expect_of(dir: &std::path::Path, rel: &str, hash: Option<String>) -> Expect {
    let meta = fs::symlink_metadata(dir.join(rel)).unwrap();
    let modified = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    Expect {
        size: meta.len(),
        modified: Some(modified),
        hash,
    }
}

#[test]
fn a_hash_that_differs_is_not_moved() {
    let (dir, ws) = workspace(&[]);
    write(dir.path(), "a.txt", b"hello");
    let j = journal(ws, ());
    let hash = format!("blake3:{}", blake3::hash(b"jello").to_hex());
    let expect = expect_of(dir.path(), "a.txt", Some(hash));
    assert_eq!(
        run(&j, mv("a.txt", "b.txt", Some(expect))),
        Outcome::Skipped(Skip::PreconditionFailed)
    );
    assert_eq!(names(dir.path(), ""), ["a.txt"]);
}

#[test]
fn a_matching_hash_moves() {
    let (dir, ws) = workspace(&[]);
    write(dir.path(), "a.txt", b"hello");
    let j = journal(ws, ());
    let hash = format!("blake3:{}", blake3::hash(b"hello").to_hex());
    let expect = expect_of(dir.path(), "a.txt", Some(hash));
    assert!(done(&run(&j, mv("a.txt", "b.txt", Some(expect)))));
    assert_eq!(names(dir.path(), ""), ["b.txt"]);
}

#[test]
fn a_hash_on_a_folder_is_not_moved() {
    let (dir, ws) = workspace(&["d/"]);
    let j = journal(ws, ());
    let hash = format!("blake3:{}", blake3::hash(b"").to_hex());
    let expect = expect_of(dir.path(), "d", Some(hash));
    let expect = Expect { size: 0, ..expect };
    assert_eq!(
        run(&j, mv("d", "e", Some(expect))),
        Outcome::Skipped(Skip::PreconditionFailed)
    );
    assert_eq!(names(dir.path(), ""), ["d"]);
}

#[test]
fn a_hidden_source_or_destination_is_refused() {
    let (dir, ws) = workspace(&[".x", "y"]);
    let j = journal(ws, ());
    assert_eq!(
        run(&j, mv(".x", "z", None)),
        Outcome::Skipped(Skip::InvalidPath)
    );
    assert_eq!(
        run(&j, mv("y", ".z", None)),
        Outcome::Skipped(Skip::InvalidPath)
    );
    assert_eq!(names(dir.path(), ""), [".x", "y"]);
}

#[test]
fn a_missing_source_or_destination_folder_is_not_found() {
    let (dir, ws) = workspace(&["a.txt", "f"]);
    let j = journal(ws, ());
    assert_eq!(
        run(&j, mv("a.txt", "nope/b.txt", None)),
        Outcome::Skipped(Skip::NotFound)
    );
    assert_eq!(
        run(&j, mv("a.txt", "f/b.txt", None)),
        Outcome::Skipped(Skip::NotFound)
    );
    assert_eq!(
        run(&j, mv("gone.txt", "b.txt", None)),
        Outcome::Skipped(Skip::NotFound)
    );
    assert_eq!(names(dir.path(), ""), ["a.txt", "f"]);
}

#[test]
fn a_case_only_rename_changes_the_case() {
    let (dir, ws) = workspace(&["A.txt"]);
    let j = journal(ws, ());
    assert!(done(&run(&j, mv("A.txt", "a.txt", None))));
    assert_eq!(names(dir.path(), ""), ["a.txt"]);
    assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"A.txt");
}

#[test]
fn a_case_only_rename_onto_a_separate_file_is_refused() {
    let (dir, ws) = workspace(&["A.txt"]);
    write(dir.path(), "a.txt", b"lower");
    if names(dir.path(), "").len() != 2 {
        eprintln!("skipped: this file system ignores case, so A.txt and a.txt are one file");
        return;
    }
    let j = journal(ws, ());
    assert_eq!(
        run(&j, mv("A.txt", "a.txt", None)),
        Outcome::Skipped(Skip::Exists)
    );
    assert_eq!(names(dir.path(), ""), ["A.txt", "a.txt"]);
    assert_eq!(fs::read(dir.path().join("A.txt")).unwrap(), b"A.txt");
    assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"lower");
}

#[cfg(unix)]
#[test]
fn a_symlink_moves_as_itself() {
    let (dir, ws) = workspace(&["t.txt"]);
    std::os::unix::fs::symlink("t.txt", dir.path().join("link")).unwrap();
    let j = journal(ws, ());
    assert!(done(&run(&j, mv("link", "link2", None))));
    let meta = fs::symlink_metadata(dir.path().join("link2")).unwrap();
    assert!(meta.file_type().is_symlink());
    assert_eq!(
        fs::read_link(dir.path().join("link2")).unwrap(),
        std::path::Path::new("t.txt")
    );
    assert_eq!(names(dir.path(), ""), ["link2", "t.txt"]);
}

#[cfg(unix)]
#[test]
fn a_move_through_a_symlink_is_refused() {
    let (dir, ws) = workspace(&["d/a.txt", "e/"]);
    std::os::unix::fs::symlink("d", dir.path().join("ld")).unwrap();
    std::os::unix::fs::symlink("e", dir.path().join("le")).unwrap();
    let j = journal(ws, ());
    assert_eq!(
        run(&j, mv("ld/a.txt", "b.txt", None)),
        Outcome::Skipped(Skip::InvalidPath)
    );
    assert_eq!(
        run(&j, mv("d/a.txt", "le/a.txt", None)),
        Outcome::Skipped(Skip::InvalidPath)
    );
    assert_eq!(names(dir.path(), "d"), ["a.txt"]);
    assert!(names(dir.path(), "e").is_empty());
}

#[test]
fn the_app_reaches_hidden_entries_with_app_access() {
    let (dir, ws) = workspace(&["p.jpg", "p.jpg.xmp"]);
    let (from, to) = (p("p.jpg.xmp"), p("q.jpg.xmp"));
    assert_eq!(
        ws.rename(&HidesXmp, &from, &to, Access::Visible),
        Err(Skip::InvalidPath)
    );
    ws.rename(&HidesXmp, &from, &to, Access::App).unwrap();
    assert_eq!(names(dir.path(), ""), ["p.jpg", "q.jpg.xmp"]);
    let dot: WsPath = p(".hidden");
    ws.make_dir(&HidesXmp, &dot, Access::App).unwrap();
    assert!(ws.entry(&HidesXmp, &dot, Access::App).is_ok());
    assert!(ws.entry(&HidesXmp, &dot, Access::Visible).is_err());
    ws.remove_empty_dir(&HidesXmp, &dot, Access::App).unwrap();
    assert_eq!(names(dir.path(), ""), ["p.jpg", "q.jpg.xmp"]);
}

#[test]
fn plans_round_trip_as_json() {
    let plan = MovePlan {
        from: p("a"),
        to: p("b/c"),
        expect: None,
    };
    let v = serde_json::to_value(&plan).unwrap();
    assert_eq!(v, serde_json::json!({"from": "a", "to": "b/c"}));
    assert_eq!(serde_json::from_value::<MovePlan>(v).unwrap(), plan);
    assert!(serde_json::from_value::<WsPath>(serde_json::json!("../x")).is_err());
}
