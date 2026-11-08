//! `fs.trash`.

mod common;

use std::fs;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Arc;

use common::*;
use corundum_framework_fs::api::{Expect, ListParams};
use corundum_framework_fs::ops::{TrashDone, TrashOp, TrashPlan};
use corundum_framework_fs::{FsCtx, Workspace, WorkspaceApp, list};
use corundum_framework_journal::{Journal, Outcome, Planned, Skip};

fn trash(path: &str, expect: Option<Expect>) -> Planned {
    Planned::new::<TrashOp>(&TrashPlan::Trash {
        path: p(path),
        expect,
    })
}

fn restore(path: &str, trash_id: &str) -> Planned {
    Planned::new::<TrashOp>(&TrashPlan::Restore {
        path: p(path),
        trash_id: trash_id.into(),
    })
}

/// The trash id of a `Trashed` outcome.
fn trashed_id(o: &Outcome) -> String {
    let Outcome::Done(v) = o else {
        panic!("not done: {o:?}");
    };
    match serde_json::from_value(v.clone()).unwrap() {
        TrashDone::Trashed { trash_id, .. } => trash_id,
        d => panic!("not trashed: {d:?}"),
    }
}

/// Redoes the newest undo, and gives the outcome of its one operation.
fn redo<A: WorkspaceApp>(j: &Journal<FsCtx<A>>) -> Outcome {
    let id = j.redo(&origin(), &actor()).unwrap();
    let b = j.wait(id).unwrap();
    assert_eq!(b.ops.len(), 1);
    b.ops[0].outcome.clone()
}

fn listed(root: &Path, path: &str) -> Vec<String> {
    let ws = Workspace::open(root).unwrap();
    let params = ListParams {
        path: path.into(),
        cursor: None,
        limit: None,
    };
    let l = list(&ws, &(), &params).unwrap();
    l.entries.into_iter().map(|e| e.name).collect()
}

#[test]
fn trash_moves_the_entry_into_the_apps_folder() {
    let (dir, ws) = workspace(&["a/b.txt", "a/c.txt"]);
    let j = journal(ws, ());
    let id = trashed_id(&run(&j, trash("a/b.txt", None)));
    assert_eq!(listed(dir.path(), "a"), ["c.txt"]);
    let trashed = dir.path().join(format!(".corundum/trash/{id}/a/b.txt"));
    assert_eq!(fs::read(trashed).unwrap(), b"a/b.txt");
}

#[test]
fn trash_reports_the_entry_it_moved() {
    let (_dir, ws) = workspace(&["a.txt"]);
    let j = journal(ws, ());
    let Outcome::Done(v) = run(&j, trash("a.txt", None)) else {
        panic!("not done");
    };
    let TrashDone::Trashed {
        path,
        trash_id,
        size,
        modified,
    } = serde_json::from_value(v).unwrap()
    else {
        panic!("not trashed");
    };
    assert_eq!(path, p("a.txt"));
    assert!(trash_id.parse::<grain_id::GrainId>().is_ok());
    assert_eq!(size, 5);
    assert!(modified.is_some());
}

#[test]
fn the_undo_of_a_trash_puts_it_back() {
    let (dir, ws) = workspace(&["a/b/c.txt"]);
    let j = journal(ws, ());
    trashed_id(&run(&j, trash("a/b/c.txt", None)));
    assert!(matches!(undo(&j), Outcome::Done(_)));
    assert_eq!(
        fs::read(dir.path().join("a/b/c.txt")).unwrap(),
        b"a/b/c.txt"
    );
    // The emptied folders under the trash id are gone, the id's own folder too.
    assert_eq!(names(dir.path(), ".corundum/trash"), Vec::<String>::new());
}

#[test]
fn the_undo_of_a_trash_keeps_what_now_stands_there() {
    let (dir, ws) = workspace(&["a.txt"]);
    let j = journal(ws, ());
    let id = trashed_id(&run(&j, trash("a.txt", None)));
    write(dir.path(), "a.txt", b"new");
    assert_eq!(undo(&j), Outcome::Skipped(Skip::Exists));
    assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"new");
    let trashed = dir.path().join(format!(".corundum/trash/{id}/a.txt"));
    assert_eq!(fs::read(trashed).unwrap(), b"a.txt");
}

#[test]
fn the_redo_of_a_trash_trashes_it_again() {
    let (dir, ws) = workspace(&["a.txt"]);
    let j = journal(ws, ());
    let first = trashed_id(&run(&j, trash("a.txt", None)));
    assert!(matches!(undo(&j), Outcome::Done(_)));
    let again = trashed_id(&redo(&j));
    assert_ne!(first, again);
    assert!(names(dir.path(), "").iter().all(|n| n != "a.txt"));
    let trashed = dir.path().join(format!(".corundum/trash/{again}/a.txt"));
    assert_eq!(fs::read(trashed).unwrap(), b"a.txt");
}

#[test]
fn the_redo_of_a_trash_skips_a_changed_file() {
    let (dir, ws) = workspace(&["a.txt"]);
    let j = journal(ws, ());
    trashed_id(&run(&j, trash("a.txt", None)));
    assert!(matches!(undo(&j), Outcome::Done(_)));
    write(dir.path(), "a.txt", b"changed!");
    assert_eq!(redo(&j), Outcome::Skipped(Skip::PreconditionFailed));
    assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"changed!");
}

#[test]
fn a_changed_file_is_not_trashed() {
    let (dir, ws) = workspace(&["a.txt"]);
    let j = journal(ws, ());
    let expect = Expect {
        size: 99,
        modified: None,
        hash: None,
    };
    assert_eq!(
        run(&j, trash("a.txt", Some(expect))),
        Outcome::Skipped(Skip::PreconditionFailed)
    );
    assert_eq!(names(dir.path(), ""), ["a.txt"]);
}

#[test]
fn the_root_and_hidden_paths_are_not_trashed() {
    let (dir, ws) = workspace(&[".git/x", "a.xmp"]);
    let j = journal(ws, HidesXmp);
    for path in ["", ".git", ".git/x", "a.xmp"] {
        assert_eq!(
            run(&j, trash(path, None)),
            Outcome::Skipped(Skip::InvalidPath),
            "{path:?}"
        );
    }
    assert_eq!(names(dir.path(), ""), [".git", "a.xmp"]);
}

#[test]
fn the_trash_itself_is_not_trashed() {
    let (dir, ws) = workspace(&["a.txt"]);
    let j = journal(ws, ());
    let id = trashed_id(&run(&j, trash("a.txt", None)));
    for path in [
        ".corundum",
        ".corundum/trash",
        &format!(".corundum/trash/{id}"),
    ] {
        assert_eq!(
            run(&j, trash(path, None)),
            Outcome::Skipped(Skip::InvalidPath),
            "{path:?}"
        );
    }
    assert_eq!(names(dir.path(), ".corundum/trash"), [id]);
}

#[test]
fn a_missing_entry_is_not_trashed_and_leaves_no_folders() {
    let (dir, ws) = workspace(&["a/"]);
    let j = journal(ws, ());
    assert_eq!(
        run(&j, trash("a/nope.txt", None)),
        Outcome::Skipped(Skip::NotFound)
    );
    assert_eq!(names(dir.path(), ""), ["a"]);
}

#[test]
fn a_restore_needs_a_real_trash_id_and_a_visible_path() {
    let (_dir, ws) = workspace(&[]);
    let j = journal(ws, ());
    for (path, id) in [
        ("a.txt", "../x"),
        ("a.txt", "not a grain id"),
        ("a.txt", ""),
        (".git/a.txt", "012atvw"),
        ("", "012atvw"),
    ] {
        assert_eq!(
            run(&j, restore(path, id)),
            Outcome::Skipped(Skip::InvalidPath),
            "{path:?} {id:?}"
        );
    }
    assert_eq!(
        run(&j, restore("a.txt", "012atvw")),
        Outcome::Skipped(Skip::NotFound)
    );
}

#[test]
fn a_restore_into_a_gone_folder_is_not_found() {
    let (dir, ws) = workspace(&["a/b.txt"]);
    let j = journal(ws, ());
    let id = trashed_id(&run(&j, trash("a/b.txt", None)));
    fs::remove_dir(dir.path().join("a")).unwrap();
    assert_eq!(undo(&j), Outcome::Skipped(Skip::NotFound));
    let trashed = dir.path().join(format!(".corundum/trash/{id}/a/b.txt"));
    assert!(trashed.exists());
}

/// An app that keeps its files in `.asteria`.
struct Asteria;

impl WorkspaceApp for Asteria {
    fn own_folder(&self) -> &str {
        ".asteria"
    }
}

#[test]
fn an_apps_own_folder_holds_its_trash() {
    let (dir, ws) = workspace(&["a.txt"]);
    let j = journal(ws, Asteria);
    let id = trashed_id(&run(&j, trash("a.txt", None)));
    let trashed = dir.path().join(format!(".asteria/trash/{id}/a.txt"));
    assert_eq!(fs::read(trashed).unwrap(), b"a.txt");
    assert_eq!(names(dir.path(), ""), [".asteria"]);
}

#[test]
fn the_trash_is_never_listed() {
    let (dir, ws) = workspace(&["a.txt", "b.txt"]);
    let j = journal(ws, ());
    trashed_id(&run(&j, trash("a.txt", None)));
    assert_eq!(names(dir.path(), ""), [".corundum", "b.txt"]);
    assert_eq!(listed(dir.path(), ""), ["b.txt"]);
}

#[test]
fn trash_plans_are_tagged() {
    let v = serde_json::to_value(TrashPlan::Restore {
        path: p("a"),
        trash_id: "012atvw".into(),
    })
    .unwrap();
    assert_eq!(
        v,
        serde_json::json!({"do": "restore", "path": "a", "trash_id": "012atvw"})
    );
}

/// An app whose own folder is `name`.
struct Own(&'static str);

impl WorkspaceApp for Own {
    fn own_folder(&self) -> &str {
        self.0
    }
}

#[test]
fn an_own_folder_must_be_one_dot_name() {
    for bad in ["corundum", ".a/.b", "", ".", "..", ".a/"] {
        let (_dir, ws) = workspace(&[]);
        let made = std::panic::catch_unwind(AssertUnwindSafe(|| {
            FsCtx::new(Arc::new(ws), Arc::new(Own(bad)))
        }));
        assert!(made.is_err(), "{bad:?}");
    }
    let (_dir, ws) = workspace(&[]);
    let _ = FsCtx::new(Arc::new(ws), Arc::new(Own(".mine")));
}

#[cfg(unix)]
#[test]
fn a_symlinked_trash_folder_is_never_followed() {
    let (dir, ws) = workspace(&["a/b.txt", "elsewhere/"]);
    std::os::unix::fs::symlink(dir.path().join("elsewhere"), dir.path().join(".corundum")).unwrap();
    let j = journal(ws, ());
    assert_eq!(
        run(&j, trash("a/b.txt", None)),
        Outcome::Skipped(Skip::InvalidPath)
    );
    assert_eq!(names(dir.path(), "a"), ["b.txt"]);
    assert!(names(dir.path(), "elsewhere").is_empty());
}
