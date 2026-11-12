//! `JsonlStore`: what it writes, and what it reads back after reopening.

#![cfg(feature = "jsonl")]

use std::fs;
use std::path::Path;

use corundum_framework_journal::{
    Actor, BatchBegin, BatchEnd, BatchId, JournalStore, JsonlStore, OpId, OpRecord, Origin,
    Outcome, Record, Skip, StoreError,
};
use serde_json::json;

/// 2026-10-15T00:00:00Z.
const OCT: i64 = 1_792_022_400_000;
/// 2026-11-15T00:00:00Z.
const NOV: i64 = 1_794_700_800_000;

fn begin(at: i64) -> BatchBegin {
    BatchBegin {
        batch_id: BatchId::random(),
        origin: Origin::new("app"),
        actor: Actor::person("u1"),
        at,
        label: None,
        undo_of: None,
        redo_of: None,
    }
}

fn op(batch_id: BatchId, outcome: Outcome) -> OpRecord {
    OpRecord {
        op_id: OpId::random(),
        batch_id,
        parent: None,
        kind: "k".into(),
        plan: json!({}),
        outcome,
    }
}

fn end(batch_id: BatchId, done: u32, skipped: u32) -> Record {
    Record::End(BatchEnd {
        batch_id,
        done,
        skipped,
        interrupted: false,
    })
}

fn open(dir: &Path) -> JsonlStore {
    JsonlStore::open(dir).unwrap()
}

/// Begins a batch at `at` and gives its id.
fn add(s: &mut JsonlStore, b: BatchBegin) -> BatchId {
    let id = b.batch_id;
    s.append(&Record::Begin(b)).unwrap();
    id
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn records(path: &Path) -> Vec<Record> {
    lines(path)
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn ids(s: &JsonlStore, cursor: Option<BatchId>, limit: usize) -> Vec<BatchId> {
    s.batches_rev(cursor, limit)
        .unwrap()
        .iter()
        .map(|b| b.begin.batch_id)
        .collect()
}

#[test]
fn a_batch_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let b = begin(NOV);
    let id = b.batch_id;
    let o1 = op(id, Outcome::Done(json!(1)));
    let o2 = op(id, Outcome::Skipped(Skip::NotFound));
    let written = vec![
        Record::Begin(b),
        Record::Op(o1.clone()),
        Record::Op(o2.clone()),
        end(id, 1, 1),
    ];
    {
        let mut s = open(dir.path());
        for r in &written {
            s.append(r).unwrap();
        }
    }
    let s = open(dir.path());
    let got = s.batch(id).unwrap().unwrap();
    assert_eq!(got.ops, vec![o1, o2]);
    assert_eq!(got.summary.done, 1);
    assert_eq!(got.summary.skipped, 1);
    assert!(got.summary.complete);
    assert!(!got.summary.interrupted);
    assert_eq!(records(&dir.path().join("2026-11.jsonl")), written);
}

#[test]
fn batches_page_newest_first_across_months() {
    let dir = tempfile::tempdir().unwrap();
    let all: Vec<BatchId> = {
        let mut s = open(dir.path());
        [OCT, OCT, NOV, NOV, NOV]
            .into_iter()
            .map(|at| add(&mut s, begin(at)))
            .collect()
    };
    let s = open(dir.path());
    assert_eq!(ids(&s, None, 2), vec![all[4], all[3]]);
    assert_eq!(ids(&s, Some(all[2]), 10), vec![all[1], all[0]]);
    assert_eq!(
        ids(&s, None, 10),
        all.iter().rev().copied().collect::<Vec<_>>()
    );
    assert!(ids(&s, Some(BatchId::random()), 10).is_empty());
    assert!(dir.path().join("2026-10.jsonl").exists());
    assert!(dir.path().join("2026-11.jsonl").exists());
}

#[test]
fn an_op_goes_to_its_batch_month() {
    let dir = tempfile::tempdir().unwrap();
    let (a, o) = {
        let mut s = open(dir.path());
        let a = add(&mut s, begin(OCT));
        add(&mut s, begin(NOV));
        let o = op(a, Outcome::Done(json!(1)));
        s.append(&Record::Op(o.clone())).unwrap();
        s.append(&end(a, 1, 0)).unwrap();
        (a, o)
    };
    let s = open(dir.path());
    let got = s.batch(a).unwrap().unwrap();
    assert_eq!(got.ops, vec![o.clone()]);
    assert!(got.summary.complete);
    assert!(records(&dir.path().join("2026-10.jsonl")).contains(&Record::Op(o.clone())));
    assert!(!records(&dir.path().join("2026-11.jsonl")).contains(&Record::Op(o)));
}

#[test]
fn an_undo_marks_its_target_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = {
        let mut s = open(dir.path());
        let a = add(&mut s, begin(NOV));
        let mut undo = begin(NOV);
        undo.undo_of = Some(a);
        let b = add(&mut s, undo);
        assert_eq!(s.batch(a).unwrap().unwrap().summary.undone_by, Some(b));
        (a, b)
    };
    let s = open(dir.path());
    assert_eq!(s.batch(a).unwrap().unwrap().summary.undone_by, Some(b));
}

#[test]
fn months_never_go_backwards() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = {
        let mut s = open(dir.path());
        (add(&mut s, begin(NOV)), add(&mut s, begin(OCT)))
    };
    let s = open(dir.path());
    assert_eq!(ids(&s, None, 10), vec![b, a]);
    assert!(!dir.path().join("2026-10.jsonl").exists());
}

#[test]
fn a_second_open_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let first = open(dir.path());
    assert!(matches!(
        JsonlStore::open(dir.path()),
        Err(StoreError::Io(_))
    ));
    drop(first);
    open(dir.path());
}

#[test]
fn other_files_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let names = ["README", "notes.jsonl", "2026-13.jsonl", ".DS_Store"];
    for n in names {
        fs::write(dir.path().join(n), "not json\n").unwrap();
    }
    let s = open(dir.path());
    assert!(ids(&s, None, 10).is_empty());
    for n in names {
        assert_eq!(
            fs::read_to_string(dir.path().join(n)).unwrap(),
            "not json\n"
        );
    }
}

#[test]
fn a_duplicate_begin_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    let b = begin(NOV);
    s.append(&Record::Begin(b.clone())).unwrap();
    s.append(&Record::Begin(b)).unwrap();
    assert_eq!(lines(&dir.path().join("2026-11.jsonl")).len(), 1);
    assert_eq!(ids(&s, None, 10).len(), 1);
}

#[test]
fn an_op_of_an_unknown_batch_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(dir.path());
    s.append(&Record::Op(op(BatchId::random(), Outcome::Done(json!(1)))))
        .unwrap();
    s.append(&end(BatchId::random(), 0, 0)).unwrap();
    let months: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n.to_string_lossy().ends_with(".jsonl"))
        .collect();
    assert!(months.is_empty(), "{months:?}");
}
