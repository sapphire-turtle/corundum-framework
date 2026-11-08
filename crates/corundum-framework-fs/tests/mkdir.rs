//! `fs.mkdir`.

mod common;

use common::*;
use corundum_framework_fs::ops::{MkdirOp, MkdirPlan};
use corundum_framework_journal::{Outcome, Planned, Skip};

fn make(path: &str) -> Planned {
    Planned::new::<MkdirOp>(&MkdirPlan::Make { path: p(path) })
}

#[test]
fn mkdir_makes_and_its_undo_removes_an_empty_folder() {
    let (dir, ws) = workspace(&["a/"]);
    let j = journal(ws, ());
    assert!(matches!(run(&j, make("a/b")), Outcome::Done(_)));
    assert!(dir.path().join("a/b").is_dir());
    assert!(matches!(undo(&j), Outcome::Done(_)));
    assert!(names(dir.path(), "a").is_empty());
}

#[test]
fn mkdir_undo_keeps_a_folder_that_is_no_longer_empty() {
    let (dir, ws) = workspace(&[]);
    let j = journal(ws, ());
    assert!(matches!(run(&j, make("b")), Outcome::Done(_)));
    write(dir.path(), "b/f", b"f");
    assert_eq!(undo(&j), Outcome::Skipped(Skip::PreconditionFailed));
    assert_eq!(names(dir.path(), "b"), ["f"]);
}

#[test]
fn mkdir_on_an_existing_entry_is_exists() {
    let (dir, ws) = workspace(&["f", "d/"]);
    let j = journal(ws, ());
    assert_eq!(run(&j, make("f")), Outcome::Skipped(Skip::Exists));
    assert_eq!(run(&j, make("d")), Outcome::Skipped(Skip::Exists));
    assert_eq!(names(dir.path(), ""), ["d", "f"]);
}

#[test]
fn mkdir_refuses_hidden_and_missing_parents() {
    let (dir, ws) = workspace(&[]);
    let j = journal(ws, ());
    assert_eq!(run(&j, make(".git")), Outcome::Skipped(Skip::InvalidPath));
    assert_eq!(run(&j, make("no/pe")), Outcome::Skipped(Skip::NotFound));
    assert!(names(dir.path(), "").is_empty());
}

#[test]
fn mkdir_plans_are_tagged() {
    let v = serde_json::to_value(MkdirPlan::Remove { path: p("a") }).unwrap();
    assert_eq!(v, serde_json::json!({"do": "remove", "path": "a"}));
}
