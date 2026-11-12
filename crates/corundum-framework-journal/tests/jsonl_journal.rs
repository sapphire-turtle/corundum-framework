//! The journal over a `JsonlStore`, across a restart of the server.

#![cfg(feature = "jsonl")]

mod common;

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use common::*;
use corundum_framework_journal::{
    BatchBegin, BatchId, JournalConfig, JournalStore, JsonlStore, OpId, OpRecord, Outcome, Record,
};
use serde_json::json;

/// Opens the store in `dir`, waiting up to 2 s for a dropped journal's worker to let it go.
fn reopen(dir: &Path) -> JsonlStore {
    let start = Instant::now();
    loop {
        match JsonlStore::open(dir) {
            Ok(s) => return s,
            Err(e) if start.elapsed() > Duration::from_secs(2) => panic!("{e}"),
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn now() -> i64 {
    let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    i64::try_from(since.as_millis()).unwrap()
}

#[test]
fn history_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let (j, _ctx) = journal_with_store(reopen(dir.path()), JournalConfig::default());
        let id = j.submit(vec![set("a", 1)], &gui(), &u1(), None).unwrap();
        j.wait(id).unwrap();
        id
    };
    let (j, ctx) = journal_with_store(reopen(dir.path()), JournalConfig::default());
    ctx.values.lock().unwrap().insert("a".into(), 1);
    let history = j.history(None, 10).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].begin.batch_id, id);
    assert_eq!(history[0].done, 1);
    let u = j.undo_batch(id, &gui(), &u1()).unwrap();
    let undo = j.wait(u).unwrap();
    assert_eq!(undo.summary.done, 1);
    assert_eq!(ctx.value("a"), None);
}

#[test]
fn an_interrupted_batch_stays_undoable() {
    let dir = tempfile::tempdir().unwrap();
    let id = BatchId::random();
    {
        let mut s = reopen(dir.path());
        s.append(&Record::Begin(BatchBegin {
            batch_id: id,
            origin: gui(),
            actor: u1(),
            at: now(),
            label: None,
            undo_of: None,
            redo_of: None,
        }))
        .unwrap();
        s.append(&Record::Op(OpRecord {
            op_id: OpId::random(),
            batch_id: id,
            parent: None,
            kind: "test.set".into(),
            plan: json!({"key": "a", "to": 1}),
            outcome: Outcome::Done(json!({"key": "a", "from": null, "to": 1})),
        }))
        .unwrap();
    }
    let (j, ctx) = journal_with_store(reopen(dir.path()), JournalConfig::default());
    ctx.values.lock().unwrap().insert("a".into(), 1);
    let status = j.status(id).unwrap().summary;
    assert!(status.complete);
    assert!(status.interrupted);
    let u = j.undo_batch(id, &gui(), &u1()).unwrap();
    assert_eq!(j.wait(u).unwrap().summary.done, 1);
    assert_eq!(ctx.value("a"), None);
}
