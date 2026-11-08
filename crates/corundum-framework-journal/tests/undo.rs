//! Undo, redo and the undo window.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use common::*;
use corundum_framework_journal::{
    Actor, BatchBegin, BatchEnd, BatchId, Journal, JournalConfig, JournalError, JournalStore,
    MemoryStore, OpId, OpKind, OpRecord, Origin, Outcome, Planned, Record, Skip, UndoPolicy,
};
use serde_json::json;

fn agent() -> Origin {
    Origin::new("agent")
}

fn submit(j: &Journal<Shared>, ops: Vec<Planned>) -> BatchId {
    j.submit(ops, &gui(), &u1(), None).unwrap()
}

fn submit_done(j: &Journal<Shared>, ops: Vec<Planned>) -> BatchId {
    let id = submit(j, ops);
    j.wait(id).unwrap();
    id
}

#[test]
fn undo_puts_values_back() {
    let (j, ctx) = journal();
    let first = submit_done(&j, vec![set("a", 1), set("b", 2)]);
    let comp = j.undo(&gui(), &u1()).unwrap();
    let c = j.wait(comp).unwrap();
    assert_eq!(ctx.value("a"), None);
    assert_eq!(ctx.value("b"), None);
    assert_eq!(c.summary.begin.undo_of, Some(first));
    assert_eq!(c.summary.begin.redo_of, None);
    assert_eq!(c.summary.begin.origin, gui());
    assert_eq!(c.summary.begin.actor, u1());
    assert_eq!(c.summary.done, 2);
    assert_eq!(j.status(first).unwrap().summary.undone_by, Some(comp));
}

#[test]
fn a_compensation_keeps_the_target_label() {
    let (j, _ctx) = journal();
    let first = j
        .submit(vec![set("a", 1)], &gui(), &u1(), Some("rename".into()))
        .unwrap();
    let comp = j.undo(&gui(), &u1()).unwrap();
    let c = j.wait(comp).unwrap();
    assert_eq!(c.summary.begin.label.as_deref(), Some("rename"));
    assert_eq!(c.summary.begin.undo_of, Some(first));
}

#[test]
fn undo_reverses_children_first() {
    let (j, _ctx) = journal();
    submit_done(&j, vec![with_child("x")]);
    let comp = j.undo(&gui(), &u1()).unwrap();
    let c = j.wait(comp).unwrap();
    assert_eq!(c.ops[0].kind, Set::KIND);
    assert_eq!(c.ops[0].plan["key"], "x.side");
    assert_eq!(c.ops[1].kind, WithChild::KIND);
    assert_eq!(c.ops[1].plan["key"], "x");
}

#[test]
fn redo_does_again() {
    let (j, ctx) = journal();
    submit_done(&j, vec![set("a", 1)]);
    let u = j.undo(&gui(), &u1()).unwrap();
    j.wait(u).unwrap();
    assert_eq!(ctx.value("a"), None);
    let r = j.redo(&gui(), &u1()).unwrap();
    let b = j.wait(r).unwrap();
    assert_eq!(ctx.value("a"), Some(1));
    assert_eq!(b.summary.begin.redo_of, Some(u));
    assert_eq!(b.summary.begin.undo_of, None);
    // The redo is on the undo stack again.
    let u2 = j.undo(&gui(), &u1()).unwrap();
    assert_eq!(j.wait(u2).unwrap().summary.begin.undo_of, Some(r));
    assert_eq!(ctx.value("a"), None);
}

#[test]
fn a_new_batch_clears_redo() {
    let (j, _ctx) = journal();
    submit_done(&j, vec![set("a", 1)]);
    let u = j.undo(&gui(), &u1()).unwrap();
    j.wait(u).unwrap();
    submit_done(&j, vec![set("b", 1)]);
    assert_eq!(j.redo(&gui(), &u1()), Err(JournalError::NothingToRedo));
}

#[test]
fn undo_is_per_origin() {
    let (j, ctx) = journal();
    submit_done(&j, vec![set("a", 1)]);
    let b = j.submit(vec![set("b", 1)], &agent(), &u1(), None).unwrap();
    j.wait(b).unwrap();
    let u = j.undo(&gui(), &u1()).unwrap();
    j.wait(u).unwrap();
    assert_eq!(ctx.value("a"), None);
    assert_eq!(ctx.value("b"), Some(1));
    assert_eq!(j.undo(&gui(), &u1()), Err(JournalError::NothingToUndo));
}

#[test]
fn undo_batch_reaches_another_origin() {
    let (j, ctx) = journal();
    let b = j
        .submit(vec![set("b", 1)], &agent(), &Actor::agent("u1"), None)
        .unwrap();
    j.wait(b).unwrap();
    let u = j.undo_batch(b, &gui(), &u1()).unwrap();
    let c = j.wait(u).unwrap();
    assert_eq!(c.summary.begin.undo_of, Some(b));
    assert_eq!(c.summary.begin.origin, gui());
    assert_eq!(ctx.value("b"), None);
    assert_eq!(
        j.undo(&agent(), &Actor::agent("u1")),
        Err(JournalError::NothingToUndo)
    );
}

#[test]
fn undoing_twice_is_already_undone() {
    let (j, _ctx) = journal();
    let a = submit_done(&j, vec![set("a", 1)]);
    let u = j.undo_batch(a, &gui(), &u1()).unwrap();
    j.wait(u).unwrap();
    assert_eq!(
        j.undo_batch(a, &gui(), &u1()),
        Err(JournalError::AlreadyUndone(a))
    );
}

#[test]
fn undoing_a_reserved_target_is_already_undone() {
    let (j, ctx) = journal();
    let gate = ctx.gate();
    let w = submit(&j, vec![Planned::new::<Wait>(&())]);
    gate.send(()).unwrap();
    let a = submit(&j, vec![set("a", 1)]);
    let u = j.undo_batch(a, &gui(), &u1()).unwrap();
    assert_eq!(
        j.undo_batch(a, &gui(), &u1()),
        Err(JournalError::AlreadyUndone(a))
    );
    drop(gate);
    j.wait(w).unwrap();
    j.wait(u).unwrap();
    assert_eq!(ctx.value("a"), None);
}

#[test]
fn an_unknown_target_is_not_found() {
    let (j, _ctx) = journal();
    let id = BatchId::random();
    assert_eq!(
        j.undo_batch(id, &gui(), &u1()),
        Err(JournalError::NotFound(id))
    );
}

#[test]
fn another_principal_is_forbidden() {
    let (j, ctx) = journal();
    let a = submit_done(&j, vec![set("a", 1)]);
    assert_eq!(
        j.undo_batch(a, &gui(), &Actor::person("u2")),
        Err(JournalError::Forbidden(a))
    );
    let u = j.undo_batch(a, &gui(), &Actor::agent("u1")).unwrap();
    j.wait(u).unwrap();
    assert_eq!(ctx.value("a"), None);
}

#[test]
fn a_refused_target_stays_on_its_stack() {
    let (j, ctx) = journal();
    let a = submit_done(&j, vec![set("a", 1)]);
    assert_eq!(
        j.undo(&gui(), &Actor::person("u2")),
        Err(JournalError::Forbidden(a))
    );
    let u = j.undo(&gui(), &u1()).unwrap();
    assert_eq!(j.wait(u).unwrap().summary.begin.undo_of, Some(a));
    assert_eq!(ctx.value("a"), None);
}

#[test]
fn age_expires_an_undo() {
    let clock = Arc::new(AtomicI64::new(1_000_000));
    let now = clock.clone();
    let (j, _ctx) = journal_with(JournalConfig {
        now: Arc::new(move || now.load(Ordering::SeqCst)),
        ..JournalConfig::default()
    });
    let a = submit_done(&j, vec![set("a", 1)]);
    clock.fetch_add(3_600_001, Ordering::SeqCst);
    assert_eq!(j.undo(&gui(), &u1()), Err(JournalError::Expired(a)));
}

#[test]
fn count_expires_an_undo() {
    let (j, ctx) = journal_with(JournalConfig {
        policy: UndoPolicy {
            max_batches: 2,
            ..UndoPolicy::default()
        },
        ..JournalConfig::default()
    });
    let first = submit_done(&j, vec![set("a", 1)]);
    submit_done(&j, vec![set("b", 1)]);
    submit_done(&j, vec![set("c", 1)]);
    assert_eq!(
        j.undo_batch(first, &gui(), &u1()),
        Err(JournalError::Expired(first))
    );
    let u = j.undo(&gui(), &u1()).unwrap();
    j.wait(u).unwrap();
    assert_eq!(ctx.value("c"), None);
    assert_eq!(ctx.value("a"), Some(1));
}

#[test]
fn another_origins_batches_do_not_expire_mine() {
    let (j, ctx) = journal();
    submit_done(&j, vec![set("a", 1)]);
    let mut last = None;
    for i in 0..25 {
        last = Some(j.submit(vec![set("b", i)], &agent(), &u1(), None).unwrap());
    }
    j.wait(last.unwrap()).unwrap();
    let u = j.undo(&gui(), &u1()).unwrap();
    j.wait(u).unwrap();
    assert_eq!(ctx.value("a"), None);
}

#[test]
fn a_skipped_inverse_is_reported() {
    let (j, ctx) = journal();
    submit_done(&j, vec![set("a", 1)]);
    ctx.values.lock().unwrap().insert("a".into(), 5);
    let u = j.undo(&gui(), &u1()).unwrap();
    let c = j.wait(u).unwrap();
    assert_eq!(c.ops.len(), 1);
    assert_eq!(c.ops[0].outcome, Outcome::Skipped(Skip::PreconditionFailed));
    assert_eq!(ctx.value("a"), Some(5));
}

#[test]
fn skipped_ops_are_not_undone() {
    let (j, _ctx) = journal();
    let a = submit_done(&j, vec![set_expect("a", 1, Some(5))]);
    let u = j.undo(&gui(), &u1()).unwrap();
    let c = j.wait(u).unwrap();
    assert_eq!(c.summary.begin.undo_of, Some(a));
    assert!(c.ops.is_empty());
    assert!(c.summary.complete);
}

#[test]
fn children_do_not_run_in_a_compensating_batch() {
    let (j, ctx) = journal();
    submit_done(&j, vec![with_child("x")]);
    assert_eq!(ctx.value("x.side"), Some(1));
    let u = j.undo(&gui(), &u1()).unwrap();
    let c = j.wait(u).unwrap();
    assert_eq!(c.ops.len(), 2);
    assert!(c.ops.iter().all(|o| o.parent.is_none()));
    assert_eq!(ctx.value("x"), None);
    assert_eq!(ctx.value("x.side"), None);
}

#[test]
fn two_quick_undos_take_two_batches() {
    let (j, ctx) = journal();
    let a = submit(&j, vec![set("a", 1)]);
    let b = submit(&j, vec![set("b", 1)]);
    let u1_ = j.undo(&gui(), &u1()).unwrap();
    let u2_ = j.undo(&gui(), &u1()).unwrap();
    let c1 = j.wait(u1_).unwrap();
    let c2 = j.wait(u2_).unwrap();
    assert_eq!(c1.summary.begin.undo_of, Some(b));
    assert_eq!(c2.summary.begin.undo_of, Some(a));
    assert_eq!(ctx.value("a"), None);
    assert_eq!(ctx.value("b"), None);
}

#[test]
fn undo_right_after_submit_waits_for_the_batch() {
    let (j, ctx) = journal();
    let gate = ctx.gate();
    submit(&j, vec![Planned::new::<Wait>(&()), set("a", 1)]);
    let u = j.undo(&gui(), &u1()).unwrap();
    drop(gate);
    let c = j.wait(u).unwrap();
    assert_eq!(ctx.value("a"), None);
    assert_eq!(c.ops.len(), 2);
    assert_eq!(c.ops[0].kind, Set::KIND);
    assert!(matches!(c.ops[0].outcome, Outcome::Done(_)));
    assert_eq!(c.ops[1].kind, Wait::KIND);
    assert_eq!(c.ops[1].outcome, Outcome::Skipped(Skip::NotUndoable));
    assert_eq!(c.ops[1].plan, serde_json::Value::Null);
}

#[test]
fn an_unknown_kind_in_the_target_is_skipped() {
    // A batch some other registry wrote, of a kind this one does not know.
    let target = BatchId::random();
    let mut store = MemoryStore::default();
    let records = [
        Record::Begin(BatchBegin {
            batch_id: target,
            origin: gui(),
            actor: u1(),
            at: 5,
            label: None,
            undo_of: None,
            redo_of: None,
        }),
        Record::Op(OpRecord {
            op_id: OpId::random(),
            batch_id: target,
            parent: None,
            kind: "gone".into(),
            plan: json!({}),
            outcome: Outcome::Done(json!({})),
        }),
        Record::End(BatchEnd {
            batch_id: target,
            done: 1,
            skipped: 0,
            interrupted: false,
        }),
    ];
    for r in &records {
        store.append(r).unwrap();
    }
    let config = JournalConfig {
        now: Arc::new(|| 10),
        ..JournalConfig::default()
    };
    let j = Journal::new(registry(), Shared::default(), store, config);
    let u = j.undo_batch(target, &gui(), &u1()).unwrap();
    let c = j.wait(u).unwrap();
    assert_eq!(c.ops.len(), 1);
    assert_eq!(c.ops[0].kind, "gone");
    assert_eq!(c.ops[0].plan, serde_json::Value::Null);
    assert_eq!(c.ops[0].outcome, Outcome::Skipped(Skip::UnknownKind));
}

#[test]
fn twenty_undos_in_a_row_all_work() {
    let (j, ctx) = journal();
    let keys: Vec<String> = (0..20).map(|i| format!("k{i}")).collect();
    for k in &keys {
        submit_done(&j, vec![set(k, 1)]);
    }
    for _ in 0..20 {
        let u = j.undo(&gui(), &u1()).unwrap();
        j.wait(u).unwrap();
    }
    assert!(keys.iter().all(|k| ctx.value(k).is_none()));
}

#[test]
fn an_undo_whose_target_was_evicted_before_it_ran_is_an_error() {
    let ctx = Shared::default();
    let j = Journal::new(
        registry(),
        ctx.clone(),
        MemoryStore::new(5),
        JournalConfig::default(),
    );
    let gate = ctx.gate();
    submit(&j, vec![Planned::new::<Wait>(&())]);
    // A rendezvous: once this returns, the worker is inside the `Wait`.
    gate.send(()).unwrap();
    let a = submit(&j, vec![set("a", 1)]);
    let u = j.undo(&gui(), &u1()).unwrap();
    // Four more batches push the gated one and `a` out of a store of five.
    let mut last = None;
    for i in 0..4 {
        last = Some(submit(&j, vec![set("b", i)]));
    }
    assert!(j.status(a).is_err());
    drop(gate);
    assert_eq!(j.wait(u), Err(JournalError::Expired(a)));
    assert_eq!(j.status(u), Err(JournalError::Expired(a)));
    j.wait(last.unwrap()).unwrap();
    assert_eq!(ctx.value("a"), Some(1));
    // The compensation's end says it was cut short.
    let summary = j
        .history(None, 10)
        .unwrap()
        .into_iter()
        .find(|s| s.begin.batch_id == u)
        .unwrap();
    assert!(summary.complete);
    assert!(summary.interrupted);
    assert_eq!((summary.done, summary.skipped), (0, 0));
}

#[test]
fn an_origin_whose_batches_left_the_store_no_longer_holds_them() {
    let (j, _) = journal_with_store(MemoryStore::new(1), JournalConfig::default());
    let a = Origin::new("a");
    let b = Origin::new("b");
    let id = j.submit(vec![set("a", 1)], &a, &u1(), None).unwrap();
    j.wait(id).unwrap();
    let id = j.submit(vec![set("b", 1)], &b, &u1(), None).unwrap();
    j.wait(id).unwrap();
    assert_eq!(j.undo(&a, &u1()), Err(JournalError::NothingToUndo));
}

#[test]
fn an_origin_whose_batches_aged_out_no_longer_holds_them() {
    let clock = Arc::new(AtomicI64::new(0));
    let now = clock.clone();
    let config = JournalConfig {
        now: Arc::new(move || now.load(Ordering::SeqCst)),
        ..JournalConfig::default()
    };
    let (j, _) = journal_with(config);
    submit_done(&j, vec![set("a", 1)]);
    clock.store(2 * 60 * 60 * 1000, Ordering::SeqCst);
    submit_done(&j, vec![set("b", 1)]);
    // The first batch is trimmed; the second is the only one left to undo.
    let comp = j.undo(&gui(), &u1()).unwrap();
    j.wait(comp).unwrap();
    assert_eq!(j.undo(&gui(), &u1()), Err(JournalError::NothingToUndo));
}
