//! The journal: it runs batches on one worker thread and records them.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::op::Unparsed;
use crate::{
    Actor, BatchBegin, BatchEnd, BatchId, BatchRecords, BatchSummary, JournalError, JournalStore,
    OpId, OpRecord, Origin, Outcome, Planned, Record, Registry, Skip,
};

/// How far back undo reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UndoPolicy {
    /// How many of an origin's newest batches can be undone.
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
    pub may_undo: MayUndo,
    /// The clock. The default is the system's.
    pub now: Clock,
    /// How many jobs can wait for the worker before the journal is `Busy`. The default is 64.
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
    // Used by undo, Task 5.
    #[allow(dead_code)]
    reserved: HashSet<BatchId>,
}

/// Work for the worker.
enum Job {
    /// A batch of plans, its begin already written.
    Batch {
        batch_id: BatchId,
        ops: Vec<Planned>,
    },
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
        let (jobs, queue) = mpsc::sync_channel(config.queue);
        let shared = Arc::new(Shared {
            registry,
            ctx,
            config,
            state: Mutex::new(State {
                store: Box::new(store),
                stacks: HashMap::new(),
                reserved: HashSet::new(),
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
    /// ([`JournalError::InvalidPlan`]), naming the first such operation; or with
    /// [`JournalError::Busy`] if the queue is full. Otherwise the batch's begin is written
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
        match self.inner.jobs.try_send(Job::Batch { batch_id, ops }) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => return Err(JournalError::Busy),
            Err(TrySendError::Disconnected(_)) => {
                tracing::error!("the journal's worker is gone");
                return Err(JournalError::Busy);
            }
        }
        state.store.append(&Record::Begin(begin))?;
        let stacks = state.stacks.entry(origin.clone()).or_default();
        stacks.undo.push(batch_id);
        stacks.redo.clear();
        Ok(batch_id)
    }

    /// A batch with its operations so far, or [`JournalError::NotFound`].
    pub fn status(&self, id: BatchId) -> Result<BatchRecords, JournalError> {
        self.shared()
            .lock()
            .store
            .batch(id)?
            .ok_or(JournalError::NotFound(id))
    }

    /// Blocks until the batch has ended, and gives it with its operations.
    ///
    /// [`JournalError::NotFound`] if the store has no such batch, or drops it while waiting.
    pub fn wait(&self, id: BatchId) -> Result<BatchRecords, JournalError> {
        let shared = self.shared();
        let mut state = shared.lock();
        loop {
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
    pub fn history(
        &self,
        cursor: Option<BatchId>,
        limit: usize,
    ) -> Result<Vec<BatchSummary>, JournalError> {
        Ok(self.shared().lock().store.batches_rev(cursor, limit)?)
    }
}

impl<C: Send + Sync + 'static> Shared<C> {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn append(&self, record: Record) {
        if let Err(e) = self.lock().store.append(&record) {
            tracing::error!(error = %e, "the journal could not keep a record");
        }
    }

    /// The worker's loop: it ends when the queue closes and is empty.
    fn run(&self, queue: Receiver<Job>) {
        while let Ok(job) = queue.recv() {
            match job {
                Job::Batch { batch_id, ops } => self.run_batch(batch_id, &ops),
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
            self.run_op(&mut end, None, p);
        }
        self.append(Record::End(end));
        self.ended.notify_all();
    }

    /// Applies one operation, outside the lock, records it, and then runs its children.
    fn run_op(&self, end: &mut BatchEnd, parent: Option<OpId>, p: &Planned) {
        let (outcome, children) = match self.registry.apply(&self.ctx, p) {
            Ok(Ok((done, children))) => (Outcome::Done(done), children),
            Ok(Err(skip)) => (Outcome::Skipped(skip), Vec::new()),
            Err(Unparsed::UnknownKind) => (Outcome::Skipped(Skip::UnknownKind), Vec::new()),
            Err(Unparsed::InvalidPlan(m)) => (Outcome::Skipped(Skip::InvalidPlan(m)), Vec::new()),
        };
        match outcome {
            Outcome::Done(_) => end.done += 1,
            Outcome::Skipped(_) => end.skipped += 1,
        }
        let op_id = OpId::random();
        self.append(Record::Op(OpRecord {
            op_id,
            batch_id: end.batch_id,
            parent,
            kind: p.kind.clone(),
            plan: p.plan.clone(),
            outcome,
        }));
        for child in &children {
            self.run_op(end, Some(op_id), child);
        }
    }
}
