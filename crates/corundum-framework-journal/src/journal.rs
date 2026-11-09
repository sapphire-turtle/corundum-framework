//! The journal: it runs batches on one worker thread and records them.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::op::Unparsed;
use crate::undo::{self, Inverse};
use crate::{
    Actor, BatchBegin, BatchEnd, BatchId, BatchRecords, BatchSummary, JournalError, JournalStore,
    OpId, OpRecord, Origin, Outcome, Planned, Record, Registry, Skip,
};

/// How deep an operation's children can nest. The batch's own operations are at depth 0;
/// a child of an operation at this depth is skipped, with [`Skip::InvalidPlan`], and its
/// own children are not looked at.
pub const MAX_CHILD_DEPTH: usize = 16;

/// How far back undo reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoPolicy {
    /// How many of an origin's newest forward batches can be undone; undos and redos are
    /// not counted.
    pub max_batches: usize,
    /// How old a batch can be and still be undone.
    pub max_age: Duration,
}

impl Default for UndoPolicy {
    /// 20 batches, one hour.
    fn default() -> Self {
        Self {
            max_batches: 20,
            max_age: Duration::from_secs(60 * 60),
        }
    }
}

/// Whether an actor may undo a batch, given how it began.
pub type MayUndo = Arc<dyn Fn(&Actor, &BatchBegin) -> bool + Send + Sync>;

/// A clock, in milliseconds since the Unix epoch.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// How a journal behaves.
#[derive(Clone)]
pub struct JournalConfig {
    /// How far back undo reaches.
    pub policy: UndoPolicy,
    /// Whether an actor may undo a batch. The default allows the batch's own principal.
    ///
    /// It runs under the journal's lock: it must not call back into the journal.
    pub may_undo: MayUndo,
    /// The clock. The default is the system's.
    ///
    /// It runs under the journal's lock: it must not call back into the journal.
    pub now: Clock,
    /// How many jobs can wait for the worker before the journal is `Busy`. The default is
    /// 64; the journal takes 0 as 1, since with no room at all nearly every call would be
    /// `Busy`.
    pub queue: usize,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            policy: UndoPolicy::default(),
            may_undo: Arc::new(|actor, begin| actor.principal == begin.actor.principal),
            now: Arc::new(system_now),
            queue: 64,
        }
    }
}

fn system_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// An origin's undo and redo stacks, newest last.
#[derive(Debug, Default)]
struct Stacks {
    undo: Vec<BatchId>,
    redo: Vec<BatchId>,
}

/// What the journal guards with its lock.
struct State {
    store: Box<dyn JournalStore>,
    stacks: HashMap<Origin, Stacks>,
    /// Batches a compensation is queued or running for.
    reserved: HashSet<BatchId>,
    /// Batches queued whose begin could not be written: the worker drops them unrun.
    cancelled: HashSet<BatchId>,
    /// Batches that failed, with the first error: a record of theirs could not be written,
    /// or, for a compensation, its target could not be read or had left the store.
    failed: HashMap<BatchId, JournalError>,
}

impl State {
    /// Drops, from the bottom of every stack, the entries the store no longer has or that
    /// are older than `max_age_ms` at `now`; an origin left with both stacks empty is
    /// removed. A batch the store cannot be read for is kept.
    fn trim(&mut self, now: i64, max_age_ms: i64) {
        let store = &self.store;
        let gone = |id: &BatchId| match store.batch(*id) {
            Ok(Some(b)) => now.saturating_sub(b.summary.begin.at) > max_age_ms,
            Ok(None) => true,
            Err(_) => false,
        };
        self.stacks.retain(|_, s| {
            for stack in [&mut s.undo, &mut s.redo] {
                let keep_from = stack.iter().position(|id| !gone(id)).unwrap_or(stack.len());
                stack.drain(..keep_from);
            }
            !(s.undo.is_empty() && s.redo.is_empty())
        });
    }
}

/// Work for the worker.
enum Job {
    /// A batch of plans, its begin already written.
    Batch {
        batch_id: BatchId,
        ops: Vec<Planned>,
    },
    /// A compensation, its begin already written and its target reserved. Its plans are
    /// built when it runs, so a target still queued has run by then.
    Compensate { batch_id: BatchId, target: BatchId },
}

/// Which of an origin's stacks.
#[derive(Debug, Clone, Copy)]
enum Side {
    Undo,
    Redo,
}

impl Stacks {
    fn get(&mut self, side: Side) -> &mut Vec<BatchId> {
        match side {
            Side::Undo => &mut self.undo,
            Side::Redo => &mut self.redo,
        }
    }
}

/// What the handles and the worker share. It holds no sender, so the worker keeping it
/// does not keep the queue open.
struct Shared<C> {
    registry: Registry<C>,
    ctx: C,
    config: JournalConfig,
    state: Mutex<State>,
    /// Notified after each `BatchEnd`.
    ended: Condvar,
}

/// What the handles own; the queue closes when the last one is dropped.
struct Handle<C> {
    jobs: SyncSender<Job>,
    shared: Arc<Shared<C>>,
}

/// A journal that runs batches of operations against a context `C` and records them.
///
/// One worker thread runs the batches, one at a time, in the order they were submitted. A
/// `Journal` is a cheap handle: clones share the worker, which ends when the last handle
/// is dropped, once it has run the jobs already queued.
pub struct Journal<C: Send + Sync + 'static> {
    inner: Arc<Handle<C>>,
}

impl<C: Send + Sync + 'static> Clone for Journal<C> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

fn unparsed(index: usize, p: &Planned, u: Unparsed) -> JournalError {
    match u {
        Unparsed::UnknownKind => JournalError::UnknownKind {
            index,
            kind: p.kind.clone(),
        },
        Unparsed::InvalidPlan(message) => JournalError::InvalidPlan { index, message },
    }
}

impl<C: Send + Sync + 'static> Journal<C> {
    /// A journal over `store`, and its worker thread.
    ///
    /// # Panics
    ///
    /// Panics if the thread cannot be started.
    pub fn new(
        registry: Registry<C>,
        ctx: C,
        store: impl JournalStore + 'static,
        config: JournalConfig,
    ) -> Journal<C> {
        let (jobs, queue) = mpsc::sync_channel(config.queue.max(1));
        let shared = Arc::new(Shared {
            registry,
            ctx,
            config,
            state: Mutex::new(State {
                store: Box::new(store),
                stacks: HashMap::new(),
                reserved: HashSet::new(),
                cancelled: HashSet::new(),
                failed: HashMap::new(),
            }),
            ended: Condvar::new(),
        });
        let worker = Arc::clone(&shared);
        thread::Builder::new()
            .name("journal".into())
            .spawn(move || worker.run(queue))
            .expect("the journal's thread starts");
        Journal {
            inner: Arc::new(Handle { jobs, shared }),
        }
    }

    fn shared(&self) -> &Shared<C> {
        &self.inner.shared
    }

    /// Whether every operation's kind is registered and its plan parses.
    fn parse_all(&self, ops: &[Planned]) -> Result<(), JournalError> {
        for (index, p) in ops.iter().enumerate() {
            self.shared()
                .registry
                .parse(p)
                .map_err(|u| unparsed(index, p, u))?;
        }
        Ok(())
    }

    /// Queues a batch, and gives its id.
    ///
    /// The whole batch is refused, with nothing written, if an operation's kind is unknown
    /// ([`JournalError::UnknownKind`]) or its plan does not parse
    /// ([`JournalError::InvalidPlan`]), naming the first such operation; with
    /// [`JournalError::Busy`] if the queue is full; or with [`JournalError::Stopped`] if the
    /// worker is gone. Otherwise the batch's begin is written
    /// before this returns, and the batch goes on the origin's undo stack.
    pub fn submit(
        &self,
        ops: Vec<Planned>,
        origin: &Origin,
        actor: &Actor,
        label: Option<String>,
    ) -> Result<BatchId, JournalError> {
        self.parse_all(&ops)?;
        let shared = self.shared();
        let batch_id = BatchId::random();
        let begin = BatchBegin {
            batch_id,
            origin: origin.clone(),
            actor: actor.clone(),
            at: (shared.config.now)(),
            label,
            undo_of: None,
            redo_of: None,
        };
        // The worker writes only under this lock, so the begin goes in before the batch's
        // first record even if the worker takes the job at once.
        let mut state = shared.lock();
        self.queue(&mut state, Job::Batch { batch_id, ops }, begin)?;
        let stacks = state.stacks.entry(origin.clone()).or_default();
        stacks.undo.push(batch_id);
        stacks.redo.clear();
        self.trim(&mut state);
        Ok(batch_id)
    }

    /// Trims the stacks by the store and the policy; call it under the lock.
    fn trim(&self, state: &mut State) {
        let config = &self.shared().config;
        let max_age = i64::try_from(config.policy.max_age.as_millis()).unwrap_or(i64::MAX);
        state.trim((config.now)(), max_age);
    }

    /// Queues `job`, then writes its `begin`; call it under the lock. If the begin cannot be
    /// written, the job is dropped unrun.
    fn queue(&self, state: &mut State, job: Job, begin: BatchBegin) -> Result<(), JournalError> {
        match self.inner.jobs.try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(JournalError::Busy),
            Err(TrySendError::Disconnected(_)) => {
                tracing::error!("the journal's worker is gone");
                return Err(JournalError::Stopped);
            }
        }
        let batch_id = begin.batch_id;
        if let Err(e) = state.store.append(&Record::Begin(begin)) {
            // The job is queued already; without its begin it must not run.
            state.cancelled.insert(batch_id);
            return Err(e.into());
        }
        Ok(())
    }

    /// Undoes the newest batch on the origin's undo stack, and gives the compensating
    /// batch's id; [`Journal::wait`] works on it.
    ///
    /// Entries already undone, or with an undo under way, are dropped from the stack first,
    /// and so are batches that ended with nothing done (every operation skipped), so one
    /// undo reaches the newest batch that changed something; such a batch stays in the store
    /// and in the history. A batch still running is never passed over.
    /// [`JournalError::NothingToUndo`] if none is left. The target is refused, and stays on
    /// the stack, if the store no longer has it or it is outside the undo window
    /// ([`JournalError::Expired`]), or if `may_undo` does not allow the actor
    /// ([`JournalError::Forbidden`]). Otherwise the compensation's begin is written, with the
    /// caller's origin and actor, before this returns, and it goes on the redo stack.
    pub fn undo(&self, origin: &Origin, actor: &Actor) -> Result<BatchId, JournalError> {
        self.compensate_top(Side::Undo, origin, actor)
    }

    /// Redoes the newest undo on the origin's redo stack, and gives the new batch's id;
    /// [`Journal::wait`] works on it.
    ///
    /// The rules are [`Journal::undo`]'s, with [`JournalError::NothingToRedo`] for an empty
    /// stack: an undo that ended with nothing done is passed over too. The new batch goes on
    /// the undo stack.
    pub fn redo(&self, origin: &Origin, actor: &Actor) -> Result<BatchId, JournalError> {
        self.compensate_top(Side::Redo, origin, actor)
    }

    /// Undoes the batch `target`, whatever its origin, and gives the compensating batch's
    /// id; [`Journal::wait`] works on it. No stack changes.
    ///
    /// Refused with [`JournalError::NotFound`] if the store has no such batch,
    /// [`JournalError::AlreadyUndone`] if it was undone or an undo of it is under way, and
    /// otherwise as [`Journal::undo`] refuses. If the target undid a batch, the new batch
    /// redoes that batch (its `redo_of` is the target).
    pub fn undo_batch(
        &self,
        target: BatchId,
        origin: &Origin,
        actor: &Actor,
    ) -> Result<BatchId, JournalError> {
        let shared = self.shared();
        let mut state = shared.lock();
        let begin = undo::check(
            state.store.as_ref(),
            &state.reserved,
            &shared.config,
            target,
            actor,
            JournalError::NotFound(target),
        )?;
        self.compensate(&mut state, &begin, origin, actor)
    }

    /// Whether `actor` could undo the batch `id` now, by the checks of
    /// [`Journal::undo_batch`], which it runs under the same lock; nothing changes.
    ///
    /// `Ok` if it would be accepted, otherwise the error `undo_batch` would give
    /// ([`JournalError::NotFound`], [`JournalError::AlreadyUndone`], and the rest).
    pub fn check_undo(&self, id: BatchId, actor: &Actor) -> Result<(), JournalError> {
        let shared = self.shared();
        let state = shared.lock();
        undo::check(
            state.store.as_ref(),
            &state.reserved,
            &shared.config,
            id,
            actor,
            JournalError::NotFound(id),
        )
        .map(|_| ())
    }

    fn compensate_top(
        &self,
        side: Side,
        origin: &Origin,
        actor: &Actor,
    ) -> Result<BatchId, JournalError> {
        let shared = self.shared();
        let mut state = shared.lock();
        loop {
            let top = state
                .stacks
                .get_mut(origin)
                .and_then(|s| s.get(side).last().copied());
            let Some(target) = top else {
                return Err(match side {
                    Side::Undo => JournalError::NothingToUndo,
                    Side::Redo => JournalError::NothingToRedo,
                });
            };
            let checked = undo::check(
                state.store.as_ref(),
                &state.reserved,
                &shared.config,
                target,
                actor,
                JournalError::Expired(target),
            );
            // Passed over: already undone, being undone, or ended with nothing done.
            let pass_over = match &checked {
                Err(JournalError::AlreadyUndone(_)) => true,
                Err(_) => false,
                Ok(_) => undo::did_nothing(state.store.as_ref(), target),
            };
            if pass_over {
                state
                    .stacks
                    .entry(origin.clone())
                    .or_default()
                    .get(side)
                    .pop();
                continue;
            }
            let begin = checked?;
            let id = self.compensate(&mut state, &begin, origin, actor)?;
            let stacks = state.stacks.entry(origin.clone()).or_default();
            stacks.get(side).pop();
            let other = match side {
                Side::Undo => Side::Redo,
                Side::Redo => Side::Undo,
            };
            stacks.get(other).push(id);
            self.trim(&mut state);
            return Ok(id);
        }
    }

    /// Writes and queues a compensation for `target`, and reserves the target; call it
    /// under the lock, once the target is checked.
    fn compensate(
        &self,
        state: &mut State,
        target: &BatchBegin,
        origin: &Origin,
        actor: &Actor,
    ) -> Result<BatchId, JournalError> {
        let batch_id = BatchId::random();
        let begin = undo::begin_for(
            batch_id,
            target,
            origin,
            actor,
            (self.shared().config.now)(),
        );
        let job = Job::Compensate {
            batch_id,
            target: target.batch_id,
        };
        self.queue(state, job, begin)?;
        // Reserved only once its begin is written, so a failure leaves nothing to release.
        state.reserved.insert(target.batch_id);
        Ok(batch_id)
    }

    /// A batch with its operations so far, or [`JournalError::NotFound`].
    ///
    /// [`JournalError::Store`] if one of the batch's records could not be written. For an
    /// undo or a redo: [`JournalError::Expired`] if its target left the store before it
    /// ran, and [`JournalError::Store`] if the target could not be read; such a batch ends
    /// at once, with nothing done, and its end says it was interrupted.
    pub fn status(&self, id: BatchId) -> Result<BatchRecords, JournalError> {
        let state = self.shared().lock();
        if let Some(e) = state.failed.get(&id) {
            return Err(e.clone());
        }
        state.store.batch(id)?.ok_or(JournalError::NotFound(id))
    }

    /// Blocks until the batch has ended, and gives it with its operations.
    ///
    /// [`JournalError::NotFound`] if the store has no such batch, or drops it while waiting;
    /// [`JournalError::Store`] if one of the batch's records could not be written; and, for
    /// an undo or a redo, the errors [`Journal::status`] gives.
    pub fn wait(&self, id: BatchId) -> Result<BatchRecords, JournalError> {
        let shared = self.shared();
        let mut state = shared.lock();
        loop {
            if let Some(e) = state.failed.get(&id) {
                return Err(e.clone());
            }
            match state.store.batch(id)? {
                None => return Err(JournalError::NotFound(id)),
                Some(b) if b.summary.complete => return Ok(b),
                Some(_) => {
                    state = shared
                        .ended
                        .wait(state)
                        .unwrap_or_else(PoisonError::into_inner);
                }
            }
        }
    }

    /// Checks each operation against the context now, on the caller's thread, without
    /// applying or recording anything.
    ///
    /// Refuses the operations as [`Journal::submit`] would.
    pub fn dry_run(&self, ops: &[Planned]) -> Result<Vec<Result<(), Skip>>, JournalError> {
        self.parse_all(ops)?;
        let shared = self.shared();
        ops.iter()
            .enumerate()
            .map(|(index, p)| {
                shared
                    .registry
                    .check(&shared.ctx, p)
                    .map_err(|u| unparsed(index, p, u))
            })
            .collect()
    }

    /// Newest first, up to `limit` batches begun before `cursor` (from the newest when
    /// `None`).
    ///
    /// [`JournalError::NotFound`] if `cursor` names a batch the store no longer has, so a
    /// caller paging can tell the end of the history from a cursor that was dropped. A
    /// cursor at the oldest batch kept gives an empty page.
    pub fn history(
        &self,
        cursor: Option<BatchId>,
        limit: usize,
    ) -> Result<Vec<BatchSummary>, JournalError> {
        let state = self.shared().lock();
        let page = state.store.batches_rev(cursor, limit)?;
        if page.is_empty()
            && let Some(cursor) = cursor
            && state.store.batch(cursor)?.is_none()
        {
            return Err(JournalError::NotFound(cursor));
        }
        Ok(page)
    }
}

impl<C: Send + Sync + 'static> Shared<C> {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes a record of the batch `batch_id`; a failure is kept for `wait` and `status`.
    fn append(&self, batch_id: BatchId, record: Record) {
        let mut state = self.lock();
        if let Err(e) = state.store.append(&record) {
            tracing::error!(error = %e, batch = %batch_id, "the journal could not keep a record");
            state.failed.entry(batch_id).or_insert(e.into());
        }
    }

    /// The worker's loop: it ends when the queue closes and is empty.
    fn run(&self, queue: Receiver<Job>) {
        while let Ok(job) = queue.recv() {
            match job {
                Job::Batch { batch_id, ops } => {
                    if self.lock().cancelled.remove(&batch_id) {
                        tracing::debug!(batch = %batch_id, "batch dropped: its begin was not written");
                        continue;
                    }
                    self.run_batch(batch_id, &ops);
                }
                Job::Compensate { batch_id, target } => {
                    if self.lock().cancelled.remove(&batch_id) {
                        tracing::debug!(batch = %batch_id, "compensation dropped: its begin was not written");
                        continue;
                    }
                    self.run_compensation(batch_id, target);
                }
            }
        }
    }

    fn run_batch(&self, batch_id: BatchId, ops: &[Planned]) {
        let mut end = BatchEnd {
            batch_id,
            done: 0,
            skipped: 0,
            interrupted: false,
        };
        for p in ops {
            self.run_op(&mut end, None, p, 0);
        }
        self.end_batch(end, None);
    }

    /// Writes the batch's end, clears the reservation of its target if it compensates
    /// one, and wakes the waiters.
    fn end_batch(&self, end: BatchEnd, target: Option<BatchId>) {
        let batch_id = end.batch_id;
        let mut state = self.lock();
        if let Err(e) = state.store.append(&Record::End(end)) {
            tracing::error!(error = %e, batch = %batch_id, "the journal could not keep a record");
            state.failed.entry(batch_id).or_insert(e.into());
        }
        if let Some(t) = target {
            state.reserved.remove(&t);
        }
        drop(state);
        self.ended.notify_all();
    }

    /// Runs a compensation: the inverses of the target's done operations, newest first.
    /// Their children are dropped.
    ///
    /// If the target cannot be read, or has left the store, the compensation ends at once,
    /// cut short, and `wait` and `status` give [`JournalError::Store`] or
    /// [`JournalError::Expired`].
    fn run_compensation(&self, batch_id: BatchId, target: BatchId) {
        let mut end = BatchEnd {
            batch_id,
            done: 0,
            skipped: 0,
            interrupted: false,
        };
        let records = self.lock().store.batch(target);
        let failure = match records {
            Ok(Some(records)) => Ok(undo::inverses(&self.registry, &records)),
            Ok(None) => {
                tracing::warn!(batch = %batch_id, target = %target, "the target left the store before its undo ran");
                Err(JournalError::Expired(target))
            }
            Err(e) => {
                tracing::error!(error = %e, batch = %batch_id, target = %target, "the target of an undo could not be read");
                Err(e.into())
            }
        };
        let inverses = match failure {
            Ok(inverses) => inverses,
            Err(e) => {
                self.lock().failed.entry(batch_id).or_insert(e);
                end.interrupted = true;
                self.end_batch(end, Some(target));
                return;
            }
        };
        for Inverse { kind, plan } in inverses {
            let p = match plan {
                Err(skip) => {
                    let p = Planned {
                        kind,
                        plan: Value::Null,
                    };
                    self.record(&mut end, None, &p, Outcome::Skipped(skip));
                    continue;
                }
                Ok(plan) => Planned { kind, plan },
            };
            // A compensation reverses exactly what was done: its children are dropped.
            let (outcome, _children) = self.outcome(&p);
            self.record(&mut end, None, &p, outcome);
        }
        self.end_batch(end, Some(target));
    }

    /// Applies one operation, at `depth`, outside the lock, records it, and then runs its
    /// children. Past [`MAX_CHILD_DEPTH`] the operation is recorded skipped, unrun.
    fn run_op(&self, end: &mut BatchEnd, parent: Option<OpId>, p: &Planned, depth: usize) {
        if depth > MAX_CHILD_DEPTH {
            let skip = Skip::InvalidPlan(format!("children nest deeper than {MAX_CHILD_DEPTH}"));
            self.record(end, parent, p, Outcome::Skipped(skip));
            return;
        }
        let (outcome, children) = self.outcome(p);
        let op_id = self.record(end, parent, p, outcome);
        for child in &children {
            self.run_op(end, Some(op_id), child, depth + 1);
        }
    }

    /// Applies one operation, outside the lock, and gives what came of it with its children.
    fn outcome(&self, p: &Planned) -> (Outcome, Vec<Planned>) {
        match self.registry.apply(&self.ctx, p) {
            Ok(Ok((done, children))) => (Outcome::Done(done), children),
            Ok(Err(skip)) => (Outcome::Skipped(skip), Vec::new()),
            Err(Unparsed::UnknownKind) => (Outcome::Skipped(Skip::UnknownKind), Vec::new()),
            Err(Unparsed::InvalidPlan(m)) => (Outcome::Skipped(Skip::InvalidPlan(m)), Vec::new()),
        }
    }

    /// Records an operation of the batch, counts it, and gives its id.
    fn record(
        &self,
        end: &mut BatchEnd,
        parent: Option<OpId>,
        p: &Planned,
        outcome: Outcome,
    ) -> OpId {
        match outcome {
            Outcome::Done(_) => end.done += 1,
            Outcome::Skipped(_) => end.skipped += 1,
        }
        let op_id = OpId::random();
        self.append(
            end.batch_id,
            Record::Op(OpRecord {
                op_id,
                batch_id: end.batch_id,
                parent,
                kind: p.kind.clone(),
                plan: p.plan.clone(),
                outcome,
            }),
        );
        op_id
    }
}
