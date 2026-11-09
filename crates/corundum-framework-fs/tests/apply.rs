mod common;

use std::fs;
use std::path::Path;

use common::{names, workspace, write};
use corundum_framework_fs::api::{BatchRecords, DryRun, History, Outcome, codes};
use corundum_framework_fs::journal::{Actor, BatchId};
use corundum_framework_fs::{FsError, FsService};
use serde_json::{Value, json};

fn actor() -> Actor {
    Actor::person("p")
}

fn service(files: &[&str]) -> (tempfile::TempDir, FsService<()>) {
    let (dir, ws) = workspace(files);
    (dir, FsService::new(ws, ()))
}

async fn call(s: &FsService<()>, method: &str, params: Value) -> Result<Value, FsError> {
    s.dispatch(method, params, &actor()).await
}

async fn records(s: &FsService<()>, method: &str, params: Value) -> BatchRecords {
    serde_json::from_value(call(s, method, params).await.unwrap()).unwrap()
}

async fn apply(s: &FsService<()>, origin: &str, ops: Value) -> BatchRecords {
    records(s, "fs.apply", json!({"ops": ops, "origin": origin})).await
}

async fn undo(s: &FsService<()>, origin: &str) -> BatchRecords {
    records(s, "journal.undo", json!({"origin": origin})).await
}

fn mv(from: &str, to: &str) -> Value {
    json!({"kind": "fs.move", "from": from, "to": to})
}

fn read(root: &Path, rel: &str) -> Vec<u8> {
    fs::read(root.join(rel)).unwrap()
}

#[tokio::test]
async fn apply_moves_and_answers_each_ops_result() {
    let (dir, s) = service(&["a"]);
    let b = apply(&s, "gui", json!([mv("a", "b")])).await;
    assert_eq!(b.ops.len(), 1);
    assert!(matches!(b.ops[0].outcome, Outcome::Done(_)), "{b:?}");
    assert_eq!(b.ops[0].kind, "fs.move");
    assert!(b.summary.complete);
    assert_eq!(b.summary.done, 1);
    assert_eq!(b.summary.begin.origin, "gui");
    assert_eq!(b.summary.begin.actor.principal, "p");
    assert_eq!(names(dir.path(), ""), ["b"]);
}

#[tokio::test]
async fn one_apply_is_one_undo() {
    let (dir, s) = service(&["a"]);
    apply(
        &s,
        "gui",
        json!([mv("a", "b"), {"kind": "fs.mkdir", "path": "d"}]),
    )
    .await;
    assert_eq!(names(dir.path(), ""), ["b", "d"]);
    let u = undo(&s, "gui").await;
    assert_eq!(u.ops.len(), 2);
    assert!(
        u.ops.iter().all(|o| matches!(o.outcome, Outcome::Done(_))),
        "{u:?}"
    );
    assert!(u.summary.begin.undo_of.is_some());
    assert_eq!(names(dir.path(), ""), ["a"]);
}

#[tokio::test]
async fn undo_after_the_place_was_taken_again_replaces_nothing() {
    let (dir, s) = service(&["a"]);
    apply(&s, "gui", json!([mv("a", "b")])).await;
    write(dir.path(), "a", b"new");
    let u = undo(&s, "gui").await;
    assert_eq!(u.ops.len(), 1);
    match &u.ops[0].outcome {
        Outcome::Skipped(skip) => assert_eq!(skip.reason, "exists"),
        other => panic!("{other:?}"),
    }
    assert_eq!(read(dir.path(), "a"), b"new");
    assert_eq!(read(dir.path(), "b"), b"a");
}

#[tokio::test]
async fn undo_is_per_origin() {
    let (dir, s) = service(&["a", "c"]);
    apply(&s, "gui", json!([mv("a", "b")])).await;
    apply(&s, "mcp", json!([mv("c", "d")])).await;
    undo(&s, "gui").await;
    assert_eq!(names(dir.path(), ""), ["a", "d"]);
}

#[tokio::test]
async fn redo_after_undo() {
    let (dir, s) = service(&["a"]);
    apply(&s, "gui", json!([mv("a", "b")])).await;
    undo(&s, "gui").await;
    assert_eq!(names(dir.path(), ""), ["a"]);
    let r = records(&s, "journal.redo", json!({"origin": "gui"})).await;
    assert!(r.summary.begin.redo_of.is_some());
    assert!(matches!(r.ops[0].outcome, Outcome::Done(_)), "{r:?}");
    assert_eq!(names(dir.path(), ""), ["b"]);
}

#[tokio::test]
async fn undo_batch_from_the_history_screen() {
    let (dir, s) = service(&["a", "c"]);
    apply(&s, "gui", json!([mv("a", "b")])).await;
    let m = apply(&s, "mcp", json!([mv("c", "d")])).await;
    let u = records(
        &s,
        "journal.undo_batch",
        json!({"batch_id": m.summary.begin.batch_id, "origin": "history"}),
    )
    .await;
    assert_eq!(u.summary.begin.origin, "history");
    assert_eq!(
        u.summary.begin.undo_of.as_deref(),
        Some(m.summary.begin.batch_id.as_str())
    );
    assert_eq!(names(dir.path(), ""), ["b", "c"]);
}

#[tokio::test]
async fn an_empty_or_huge_apply_is_refused_and_not_recorded() {
    let (dir, s) = service(&[]);
    let e = call(&s, "fs.apply", json!({"ops": [], "origin": "gui"}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), -32602);
    let ops: Vec<Value> = (0..10_001)
        .map(|i| json!({"kind": "fs.mkdir", "path": format!("d{i}")}))
        .collect();
    let e = call(&s, "fs.apply", json!({"ops": ops, "origin": "gui"}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), codes::TOO_LARGE);
    let e = call(
        &s,
        "fs.apply",
        json!({"ops": [mv("a", "../b")], "origin": "gui"}),
    )
    .await
    .unwrap_err();
    assert_eq!(e.code(), codes::INVALID_PATH);
    let e = call(&s, "journal.undo", json!({"origin": "gui"}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), codes::NOTHING_TO_UNDO);
    assert!(names(dir.path(), "").is_empty());
}

#[tokio::test]
async fn a_dry_run_changes_nothing() {
    let (dir, s) = service(&["a", "b"]);
    let d: DryRun = serde_json::from_value(
        call(
            &s,
            "fs.apply",
            json!({"ops": [mv("a", "b")], "origin": "gui", "dry_run": true}),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(d.checks.len(), 1);
    assert_eq!(d.checks[0].kind, "fs.move");
    assert_eq!(d.checks[0].skip.as_ref().unwrap().reason, "exists");
    assert_eq!(read(dir.path(), "a"), b"a");
    assert_eq!(read(dir.path(), "b"), b"b");
    let h: History =
        serde_json::from_value(call(&s, "journal.history", json!({})).await.unwrap()).unwrap();
    assert!(h.batches.is_empty());
    assert_eq!(h.next_cursor, None);
}

#[tokio::test]
async fn history_pages_newest_first() {
    let (_dir, s) = service(&[]);
    let mut ids = Vec::new();
    for i in 0..3 {
        let b = apply(
            &s,
            "gui",
            json!([{"kind": "fs.mkdir", "path": format!("d{i}")}]),
        )
        .await;
        ids.push(b.summary.begin.batch_id);
    }
    let h: History = serde_json::from_value(
        call(&s, "journal.history", json!({"limit": 2}))
            .await
            .unwrap(),
    )
    .unwrap();
    let got: Vec<_> = h.batches.iter().map(|b| b.begin.batch_id.clone()).collect();
    assert_eq!(got, [ids[2].clone(), ids[1].clone()]);
    assert_eq!(h.next_cursor.as_deref(), Some(ids[1].as_str()));
    let h: History = serde_json::from_value(
        call(
            &s,
            "journal.history",
            json!({"limit": 2, "cursor": h.next_cursor}),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let got: Vec<_> = h.batches.iter().map(|b| b.begin.batch_id.clone()).collect();
    assert_eq!(got, [ids[0].clone()]);
    assert_eq!(h.next_cursor, None);
}

#[tokio::test]
async fn journal_errors_have_their_codes() {
    let (_dir, s) = service(&["a"]);
    let code = |r: Result<Value, FsError>| r.unwrap_err().code();
    assert_eq!(
        code(call(&s, "journal.undo", json!({"origin": "gui"})).await),
        codes::NOTHING_TO_UNDO
    );
    assert_eq!(
        code(call(&s, "journal.redo", json!({"origin": "gui"})).await),
        codes::NOTHING_TO_REDO
    );
    let b = apply(&s, "gui", json!([mv("a", "b")])).await;
    let params = json!({"batch_id": b.summary.begin.batch_id, "origin": "history"});
    call(&s, "journal.undo_batch", params.clone())
        .await
        .unwrap();
    assert_eq!(
        code(call(&s, "journal.undo_batch", params).await),
        codes::ALREADY_UNDONE
    );
    assert_eq!(
        code(
            call(
                &s,
                "journal.status",
                json!({"batch_id": BatchId::random().to_string()})
            )
            .await
        ),
        codes::NOT_FOUND
    );
    // An id or a cursor that does not parse is the caller's mistake.
    assert_eq!(
        code(call(&s, "journal.status", json!({"batch_id": "not an id"})).await),
        codes::INVALID_PARAMS
    );
    assert_eq!(
        code(call(&s, "journal.history", json!({"cursor": "not an id"})).await),
        codes::INVALID_PARAMS
    );
}

#[tokio::test]
async fn status_gives_a_batch_and_history_clamps_its_limit() {
    let (_dir, s) = service(&["a"]);
    let b = apply(&s, "gui", json!([mv("a", "b")])).await;
    let st = records(
        &s,
        "journal.status",
        json!({"batch_id": b.summary.begin.batch_id}),
    )
    .await;
    assert_eq!(st, b);
    // A limit of 0 is clamped to 1, so the page is full and has a cursor.
    let h: History = serde_json::from_value(
        call(&s, "journal.history", json!({"limit": 0}))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(h.batches.len(), 1);
    assert_eq!(h.next_cursor, Some(b.summary.begin.batch_id));
}

#[tokio::test]
async fn a_folder_move_undoes_after_its_contents_changed() {
    let (dir, s) = service(&["a/"]);
    apply(&s, "gui", json!([mv("a", "b")])).await;
    // Past the mtime's resolution, so adding and removing `b/c` shows on `b`.
    std::thread::sleep(std::time::Duration::from_millis(20));
    apply(&s, "gui", json!([{"kind": "fs.mkdir", "path": "b/c"}])).await;
    let u = undo(&s, "gui").await;
    assert!(matches!(u.ops[0].outcome, Outcome::Done(_)), "{u:?}");
    assert_eq!(names(dir.path(), "b"), Vec::<String>::new());
    let u = undo(&s, "gui").await;
    assert!(matches!(u.ops[0].outcome, Outcome::Done(_)), "{u:?}");
    assert_eq!(names(dir.path(), ""), ["a"]);
}

#[tokio::test]
async fn undo_passes_over_a_batch_that_did_nothing() {
    let (dir, s) = service(&["a", "b", "c"]);
    apply(&s, "gui", json!([mv("c", "d")])).await;
    let skipped = apply(&s, "gui", json!([mv("a", "b")])).await;
    assert_eq!(skipped.summary.done, 0, "{skipped:?}");
    let u = undo(&s, "gui").await;
    assert!(matches!(u.ops[0].outcome, Outcome::Done(_)), "{u:?}");
    assert_eq!(names(dir.path(), ""), ["a", "b", "c"]);
}
