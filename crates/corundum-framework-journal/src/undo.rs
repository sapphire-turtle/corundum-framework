//! Undo and redo: choosing and checking targets, and building a compensation's plans.

use std::collections::HashSet;

use serde_json::Value;

use crate::journal::JournalConfig;
use crate::op::Unparsed;
use crate::{
    Actor, BatchBegin, BatchId, BatchRecords, JournalError, JournalStore, Origin, Outcome,
    Registry, Skip, StoreError,
};

/// How many summaries to read at a time while counting an origin's batches.
const PAGE: usize = 64;

/// Checks that `actor` may undo `target` now, and gives how the target began.
///
/// The checks, in order: the target is in the store (else `missing`); it is neither undone
/// nor reserved ([`JournalError::AlreadyUndone`]); `may_undo` allows it
/// ([`JournalError::Forbidden`]); it is inside the undo window ([`JournalError::Expired`]).
pub(crate) fn check(
    store: &dyn JournalStore,
    reserved: &HashSet<BatchId>,
    config: &JournalConfig,
    target: BatchId,
    actor: &Actor,
    missing: JournalError,
) -> Result<BatchBegin, JournalError> {
    let Some(records) = store.batch(target)? else {
        return Err(missing);
    };
    let summary = records.summary;
    if summary.undone_by.is_some() || reserved.contains(&target) {
        return Err(JournalError::AlreadyUndone(target));
    }
    let begin = summary.begin;
    if !(config.may_undo)(actor, &begin) {
        return Err(JournalError::Forbidden(target));
    }
    let max_age = i64::try_from(config.policy.max_age.as_millis()).unwrap_or(i64::MAX);
    let age = (config.now)().saturating_sub(begin.at);
    if age > max_age || !in_window(store, &begin, config.policy.max_batches)? {
        return Err(JournalError::Expired(target));
    }
    Ok(begin)
}

/// Whether the batch is among the newest `max` batches of its own origin, counting every
/// batch that began with that origin, compensations too.
fn in_window(store: &dyn JournalStore, begin: &BatchBegin, max: usize) -> Result<bool, StoreError> {
    let mut cursor = None;
    let mut newer = 0;
    loop {
        let page = store.batches_rev(cursor, PAGE)?;
        let Some(last) = page.last() else {
            return Ok(false);
        };
        cursor = Some(last.begin.batch_id);
        for s in &page {
            if s.begin.batch_id == begin.batch_id {
                return Ok(newer < max);
            }
            if s.begin.origin == begin.origin {
                newer += 1;
                if newer >= max {
                    return Ok(false);
                }
            }
        }
    }
}

/// The begin of a compensation for `target`: it redoes the target if the target undid a
/// batch, and undoes it otherwise.
pub(crate) fn begin_for(
    batch_id: BatchId,
    target: &BatchBegin,
    origin: &Origin,
    actor: &Actor,
    at: i64,
) -> BatchBegin {
    let (undo_of, redo_of) = if target.undo_of.is_some() {
        (None, Some(target.batch_id))
    } else {
        (Some(target.batch_id), None)
    };
    BatchBegin {
        batch_id,
        origin: origin.clone(),
        actor: actor.clone(),
        at,
        label: target.label.clone(),
        undo_of,
        redo_of,
    }
}

/// One operation of a compensation: the target operation's kind, and the inverse plan to
/// apply, or why there is none.
pub(crate) struct Inverse {
    pub(crate) kind: String,
    pub(crate) plan: Result<Value, Skip>,
}

/// The inverses of the target's done operations, in reverse record order, so children are
/// reversed before their parents.
pub(crate) fn inverses<C>(registry: &Registry<C>, target: &BatchRecords) -> Vec<Inverse> {
    target
        .ops
        .iter()
        .rev()
        .filter_map(|op| {
            let Outcome::Done(done) = &op.outcome else {
                return None;
            };
            let plan = match registry.inverse(&op.kind, done) {
                Err(Unparsed::UnknownKind) => Err(Skip::UnknownKind),
                Err(Unparsed::InvalidPlan(m)) => Err(Skip::InvalidPlan(m)),
                Ok(Err(skip)) => Err(skip),
                Ok(Ok(None)) => Err(Skip::NotUndoable),
                Ok(Ok(Some(plan))) => Ok(plan),
            };
            Some(Inverse {
                kind: op.kind.clone(),
                plan,
            })
        })
        .collect()
}
