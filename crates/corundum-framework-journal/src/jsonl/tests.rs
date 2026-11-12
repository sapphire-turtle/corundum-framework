//! A `JsonlStore` whose writes fail.

use serde_json::json;

use super::*;
use crate::{Actor, OpId, Origin};

/// 2026-11-15T00:00:00Z.
const NOV: i64 = 1_794_700_800_000;

fn begin() -> BatchBegin {
    BatchBegin {
        batch_id: BatchId::random(),
        origin: Origin::new("app"),
        actor: Actor::person("u1"),
        at: NOV,
        label: None,
        undo_of: None,
        redo_of: None,
    }
}

/// A complete batch with one done op.
fn complete(s: &mut JsonlStore) -> BatchId {
    let b = begin();
    let id = b.batch_id;
    s.append(&Record::Begin(b)).unwrap();
    s.append(&Record::Op(OpRecord {
        op_id: OpId::random(),
        batch_id: id,
        parent: None,
        kind: "k".into(),
        plan: json!({}),
        outcome: Outcome::Done(json!(1)),
    }))
    .unwrap();
    s.append(&Record::End(BatchEnd {
        batch_id: id,
        done: 1,
        skipped: 0,
        interrupted: false,
    }))
    .unwrap();
    id
}

fn ids(s: &JsonlStore) -> Vec<BatchId> {
    s.batches_rev(None, 10)
        .unwrap()
        .iter()
        .map(|b| b.begin.batch_id)
        .collect()
}

/// Every line of November's file parses.
fn all_lines_parse(dir: &Path) {
    let text = std::fs::read_to_string(dir.join("2026-11.jsonl")).unwrap();
    for line in text.lines() {
        serde_json::from_str::<Record>(line).unwrap_or_else(|e| panic!("{e}: {line}"));
    }
}

/// Writes half of a begin, fails, then cuts back or not; the next batch must still be
/// whole, on lines of its own, before and after reopening.
fn after_a_torn_write(failing_cut: bool) {
    let dir = tempfile::tempdir().unwrap();
    let mut s = JsonlStore::open(dir.path()).unwrap();
    let a = complete(&mut s);
    s.faults = Faults {
        torn_write: true,
        failing_cut,
    };
    let b = begin();
    let lost = b.batch_id;
    assert!(matches!(
        s.append(&Record::Begin(b)),
        Err(StoreError::Io(_))
    ));
    assert_eq!(ids(&s), vec![a]);
    let c = complete(&mut s);
    assert_eq!(s.batch(c).unwrap().unwrap().ops.len(), 1);
    drop(s);
    all_lines_parse(dir.path());
    let s = JsonlStore::open(dir.path()).unwrap();
    assert_eq!(ids(&s), vec![c, a]);
    assert!(s.batch(lost).unwrap().is_none());
    assert_eq!(s.batch(c).unwrap().unwrap().ops.len(), 1);
}

#[test]
fn a_failed_write_is_cut_back() {
    after_a_torn_write(false);
}

#[test]
fn a_failed_cut_is_made_good_by_the_next_write() {
    after_a_torn_write(true);
}
