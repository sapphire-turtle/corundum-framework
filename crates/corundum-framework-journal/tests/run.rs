//! Running batches.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use common::*;
use corundum_framework_journal::{
    BatchId, BatchRecords, BatchSummary, Journal, JournalConfig, JournalError, JournalStore,
    MemoryStore, OpKind, Outcome, Planned, Record, Registry, Skip, StoreError,
};
use serde_json::json;

fn submit(j: &Journal<Shared>, ops: Vec<Planned>) -> corundum_framework_journal::BatchId {
    j.submit(ops, &gui(), &u1(), None).unwrap()
}

#[test]
fn a_batch_runs_in_order() {
    let (j, ctx) = journal();
    let id = submit(&j, vec![set("a", 1), set("a", 2)]);
    let b = j.wait(id).unwrap();
    assert_eq!(ctx.value("a"), Some(2));
    assert_eq!(b.ops.len(), 2);
    assert_eq!(
        b.ops[0].outcome,
        Outcome::Done(json!({"key": "a", "from": null, "to": 1}))
    );
    assert_eq!(
        b.ops[1].outcome,
        Outcome::Done(json!({"key": "a", "from": 1, "to": 2}))
    );
    assert!(b.ops.iter().all(|o| o.batch_id == id && o.parent.is_none()));
    assert_eq!(b.summary.done, 2);
    assert_eq!(b.summary.skipped, 0);
    assert!(b.summary.complete);
    assert_eq!(j.status(id).unwrap(), b);
}

#[test]
fn a_skip_does_not_stop_the_batch() {
    let (j, ctx) = journal();
    let id = submit(&j, vec![set_expect("a", 1, Some(5)), set("b", 1)]);
    let b = j.wait(id).unwrap();
    assert_eq!(b.ops[0].outcome, Outcome::Skipped(Skip::PreconditionFailed));
    assert!(matches!(b.ops[1].outcome, Outcome::Done(_)));
    assert_eq!(ctx.value("a"), None);
    assert_eq!(ctx.value("b"), Some(1));
    assert_eq!((b.summary.done, b.summary.skipped), (1, 1));
}

#[test]
fn children_run_after_their_parent() {
    let (j, ctx) = journal();
    let id = submit(&j, vec![with_child("x"), set("y", 1)]);
    let b = j.wait(id).unwrap();
    assert_eq!(b.ops.len(), 3);
    let (parent, child, next) = (&b.ops[0], &b.ops[1], &b.ops[2]);
    assert_eq!(parent.kind, WithChild::KIND);
    assert_eq!(parent.parent, None);
    assert_eq!(child.kind, Set::KIND);
    assert_eq!(child.parent, Some(parent.op_id));
    assert_eq!(child.plan["key"], "x.side");
    assert_eq!(next.parent, None);
    assert_eq!(ctx.value("x"), Some(1));
    assert_eq!(ctx.value("x.side"), Some(1));
    assert_eq!(b.summary.done, 3);
}

#[test]
fn a_panicking_op_is_skipped_and_the_worker_survives() {
    let (j, ctx) = journal();
    let id = submit(&j, vec![Planned::new::<Boom>(&()), set("a", 1)]);
    let b = j.wait(id).unwrap();
    assert_eq!(
        b.ops[0].outcome,
        Outcome::Skipped(Skip::Io("the operation panicked".into()))
    );
    assert_eq!(ctx.value("a"), Some(1));
    let id2 = submit(&j, vec![set("b", 1)]);
    assert!(j.wait(id2).unwrap().summary.complete);
    assert_eq!(ctx.value("b"), Some(1));
}

#[test]
fn bad_ops_reject_the_whole_batch() {
    let (j, ctx) = journal();
    let nope = Planned {
        kind: "nope".into(),
        plan: json!(null),
    };
    match j.submit(vec![set("ok", 1), nope], &gui(), &u1(), None) {
        Err(JournalError::UnknownKind { index: 1, kind }) => assert_eq!(kind, "nope"),
        other => panic!("unexpected: {other:?}"),
    }
    let bad = Planned {
        kind: Set::KIND.into(),
        plan: json!("x"),
    };
    assert!(matches!(
        j.submit(vec![set("ok", 1), bad], &gui(), &u1(), None),
        Err(JournalError::InvalidPlan { index: 1, .. })
    ));
    assert!(j.history(None, 10).unwrap().is_empty());
    assert_eq!(ctx.value("ok"), None);
}

#[test]
fn dry_run_checks_without_applying() {
    let (j, ctx) = journal();
    let got = j
        .dry_run(&[set_expect("a", 1, Some(5)), set("b", 1)])
        .unwrap();
    assert_eq!(got, vec![Err(Skip::PreconditionFailed), Ok(())]);
    assert_eq!(ctx.value("a"), None);
    assert_eq!(ctx.value("b"), None);
    assert!(j.history(None, 10).unwrap().is_empty());
    let nope = Planned {
        kind: "nope".into(),
        plan: json!(null),
    };
    assert!(matches!(
        j.dry_run(&[nope]),
        Err(JournalError::UnknownKind { index: 0, .. })
    ));
}

#[test]
fn history_is_newest_first() {
    let (j, _ctx) = journal();
    let ids: Vec<_> = (0..3).map(|i| submit(&j, vec![set("a", i)])).collect();
    for id in &ids {
        j.wait(*id).unwrap();
    }
    let got: Vec<_> = j
        .history(None, 10)
        .unwrap()
        .iter()
        .map(|s| s.begin.batch_id)
        .collect();
    assert_eq!(got, vec![ids[2], ids[1], ids[0]]);
    let page: Vec<_> = j
        .history(Some(ids[2]), 1)
        .unwrap()
        .iter()
        .map(|s| s.begin.batch_id)
        .collect();
    assert_eq!(page, vec![ids[1]]);
}

#[test]
fn a_batch_begins_with_its_origin_actor_and_label() {
    let (j, _ctx) = journal();
    let id = j
        .submit(vec![set("a", 1)], &gui(), &u1(), Some("rename".into()))
        .unwrap();
    let b = j.wait(id).unwrap().summary.begin;
    assert_eq!(b.batch_id, id);
    assert_eq!(b.origin, gui());
    assert_eq!(b.actor, u1());
    assert_eq!(b.label.as_deref(), Some("rename"));
    assert_eq!((b.undo_of, b.redo_of), (None, None));
}

#[test]
fn an_unknown_batch_is_not_found() {
    let (j, _ctx) = journal();
    let id = corundum_framework_journal::BatchId::random();
    assert!(matches!(j.status(id), Err(JournalError::NotFound(x)) if x == id));
    assert!(matches!(j.wait(id), Err(JournalError::NotFound(x)) if x == id));
}

#[test]
fn a_full_queue_is_busy() {
    let (j, ctx) = journal_with(JournalConfig {
        queue: 2,
        ..JournalConfig::default()
    });
    let gate = ctx.gate();
    let w = submit(&j, vec![Planned::new::<Wait>(&())]);
    // A rendezvous: once this returns, the worker is inside the `Wait`, off the queue.
    gate.send(()).unwrap();
    let a = submit(&j, vec![set("a", 1)]);
    let b = submit(&j, vec![set("b", 1)]);
    assert!(matches!(
        j.submit(vec![set("c", 1)], &gui(), &u1(), None),
        Err(JournalError::Busy)
    ));
    assert_eq!(j.history(None, 10).unwrap().len(), 3);
    assert!(!j.status(a).unwrap().summary.complete);
    drop(gate);
    for id in [w, a, b] {
        assert!(j.wait(id).unwrap().summary.complete);
    }
    assert_eq!(ctx.value("c"), None);
}

#[test]
fn the_worker_ends_with_the_last_handle() {
    struct Probe(mpsc::Sender<()>);
    impl Drop for Probe {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }
    let (tx, rx) = mpsc::channel();
    let j = Journal::new(
        Registry::<Probe>::new(),
        Probe(tx),
        MemoryStore::default(),
        JournalConfig::default(),
    );
    let j2 = j.clone();
    drop(j);
    assert!(rx.try_recv().is_err(), "a handle is still alive");
    drop(j2);
    // The worker drops the context as it ends; the timeout only guards against a hang.
    rx.recv_timeout(Duration::from_secs(30))
        .expect("the worker ended");
}

/// A `MemoryStore` that fails on demand.
struct FlakyStore {
    inner: MemoryStore,
    /// Fail the next `Begin` append.
    fail_begin: Arc<AtomicBool>,
    /// Fail every `End` append.
    fail_end: Arc<AtomicBool>,
}

impl JournalStore for FlakyStore {
    fn append(&mut self, record: &Record) -> Result<(), StoreError> {
        let fail = match record {
            Record::Begin(_) => self.fail_begin.swap(false, Ordering::SeqCst),
            Record::End(_) => self.fail_end.load(Ordering::SeqCst),
            Record::Op(_) => false,
        };
        if fail {
            return Err(StoreError::Io("disk full".into()));
        }
        self.inner.append(record)
    }

    fn batches_rev(
        &self,
        cursor: Option<BatchId>,
        limit: usize,
    ) -> Result<Vec<BatchSummary>, StoreError> {
        self.inner.batches_rev(cursor, limit)
    }

    fn batch(&self, id: BatchId) -> Result<Option<BatchRecords>, StoreError> {
        self.inner.batch(id)
    }
}

fn flaky_journal() -> (Journal<Shared>, Shared, Arc<AtomicBool>, Arc<AtomicBool>) {
    let ctx = Shared::default();
    let fail_begin = Arc::new(AtomicBool::new(false));
    let fail_end = Arc::new(AtomicBool::new(false));
    let store = FlakyStore {
        inner: MemoryStore::default(),
        fail_begin: fail_begin.clone(),
        fail_end: fail_end.clone(),
    };
    let j = Journal::new(registry(), ctx.clone(), store, JournalConfig::default());
    (j, ctx, fail_begin, fail_end)
}

#[test]
fn a_batch_whose_begin_failed_does_not_run() {
    let (j, ctx, fail_begin, _) = flaky_journal();
    fail_begin.store(true, Ordering::SeqCst);
    assert!(matches!(
        j.submit(vec![set("a", 1)], &gui(), &u1(), None),
        Err(JournalError::Store(_))
    ));
    // The worker runs jobs in order, so once this one is done the first was handled.
    let id = submit(&j, vec![set("b", 1)]);
    assert!(j.wait(id).unwrap().summary.complete);
    assert_eq!(ctx.value("b"), Some(1));
    assert_eq!(ctx.value("a"), None);
    assert_eq!(j.history(None, 10).unwrap().len(), 1);
}

#[test]
fn a_failed_end_makes_wait_return_an_error() {
    let (j, _ctx, _, fail_end) = flaky_journal();
    fail_end.store(true, Ordering::SeqCst);
    let id = submit(&j, vec![set("a", 1)]);
    let (tx, rx) = mpsc::channel();
    let j2 = j.clone();
    thread::spawn(move || {
        let _ = tx.send(j2.wait(id));
    });
    // The timeout only detects a hang.
    let got = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("wait returned");
    assert!(matches!(got, Err(JournalError::Store(_))), "{got:?}");
    assert!(matches!(j.status(id), Err(JournalError::Store(_))));
}
