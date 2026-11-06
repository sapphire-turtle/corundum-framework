//! Where the journal keeps its records.

use std::collections::{HashMap, VecDeque};

use crate::{BatchBegin, BatchId, BatchRecords, BatchSummary, OpRecord, Outcome, Record};

/// Why a store failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    /// An I/O error, with its message.
    #[error("i/o error: {0}")]
    Io(String),
    /// The stored data was not valid, with the reason.
    #[error("corrupt journal: {0}")]
    Corrupt(String),
}

/// A place to keep the journal's records and read them back.
pub trait JournalStore: Send {
    /// Keeps a record.
    fn append(&mut self, record: &Record) -> Result<(), StoreError>;

    /// Newest first, the batches begun before `cursor` (all of them when `None`).
    fn batches_rev(
        &self,
        cursor: Option<BatchId>,
        limit: usize,
    ) -> Result<Vec<BatchSummary>, StoreError>;

    /// A batch with its operations, if the store has it.
    fn batch(&self, id: BatchId) -> Result<Option<BatchRecords>, StoreError>;
}

/// A store that keeps the newest batches in memory.
#[derive(Debug)]
pub struct MemoryStore {
    capacity: usize,
    order: VecDeque<BatchId>,
    batches: HashMap<BatchId, BatchRecords>,
}

impl MemoryStore {
    /// A store that keeps up to `capacity` batches (at least 1).
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            order: VecDeque::new(),
            batches: HashMap::new(),
        }
    }

    fn begin(&mut self, begin: &BatchBegin) {
        let id = begin.batch_id;
        if self.batches.contains_key(&id) {
            tracing::debug!(batch = ?id, "duplicate begin dropped");
            return;
        }
        if let Some(target) = begin.undo_of.or(begin.redo_of)
            && let Some(t) = self.batches.get_mut(&target)
        {
            t.summary.undone_by = Some(id);
        }
        self.order.push_back(id);
        self.batches.insert(
            id,
            BatchRecords {
                summary: BatchSummary {
                    begin: begin.clone(),
                    done: 0,
                    skipped: 0,
                    complete: false,
                    undone_by: None,
                },
                ops: Vec::new(),
            },
        );
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.batches.remove(&old);
            }
        }
    }

    fn op(&mut self, op: &OpRecord) {
        let Some(b) = self.batches.get_mut(&op.batch_id) else {
            tracing::debug!(batch = ?op.batch_id, "op for an unknown batch dropped");
            return;
        };
        match op.outcome {
            Outcome::Done(_) => b.summary.done += 1,
            Outcome::Skipped(_) => b.summary.skipped += 1,
        }
        b.ops.push(op.clone());
    }
}

impl Default for MemoryStore {
    fn default() -> Self {
        Self::new(200)
    }
}

impl JournalStore for MemoryStore {
    fn append(&mut self, record: &Record) -> Result<(), StoreError> {
        match record {
            Record::Begin(b) => self.begin(b),
            Record::Op(o) => self.op(o),
            Record::End(e) => match self.batches.get_mut(&e.batch_id) {
                Some(b) => b.summary.complete = true,
                None => tracing::debug!(batch = ?e.batch_id, "end for an unknown batch dropped"),
            },
        }
        Ok(())
    }

    fn batches_rev(
        &self,
        cursor: Option<BatchId>,
        limit: usize,
    ) -> Result<Vec<BatchSummary>, StoreError> {
        let end = match cursor {
            None => self.order.len(),
            Some(c) => match self.order.iter().position(|id| *id == c) {
                Some(i) => i,
                None => return Ok(Vec::new()),
            },
        };
        Ok(self
            .order
            .iter()
            .take(end)
            .rev()
            .take(limit)
            .filter_map(|id| self.batches.get(id))
            .map(|b| b.summary.clone())
            .collect())
    }

    fn batch(&self, id: BatchId) -> Result<Option<BatchRecords>, StoreError> {
        Ok(self.batches.get(&id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Actor, BatchEnd, OpId, Origin, Skip};
    use serde_json::json;

    fn begin(origin: &str) -> BatchBegin {
        BatchBegin {
            batch_id: BatchId::random(),
            origin: Origin::new(origin),
            actor: Actor::person("u1"),
            at: 1,
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

    fn ids(v: &[BatchSummary]) -> Vec<BatchId> {
        v.iter().map(|s| s.begin.batch_id).collect()
    }

    #[test]
    fn a_batch_is_kept_with_its_ops() {
        let mut s = MemoryStore::default();
        let b = begin("app");
        let id = b.batch_id;
        let o1 = op(id, Outcome::Done(json!(1)));
        let o2 = op(id, Outcome::Skipped(Skip::NotFound));
        s.append(&Record::Begin(b)).unwrap();
        s.append(&Record::Op(o1.clone())).unwrap();
        s.append(&Record::Op(o2.clone())).unwrap();
        s.append(&Record::End(BatchEnd {
            batch_id: id,
            done: 1,
            skipped: 1,
            interrupted: false,
        }))
        .unwrap();
        let got = s.batch(id).unwrap().unwrap();
        assert_eq!(got.ops, vec![o1, o2]);
        assert_eq!(got.summary.done, 1);
        assert_eq!(got.summary.skipped, 1);
        assert!(got.summary.complete);
    }

    #[test]
    fn batches_come_newest_first_with_a_cursor() {
        let mut s = MemoryStore::default();
        let mut all = Vec::new();
        for _ in 0..3 {
            let b = begin("app");
            all.push(b.batch_id);
            s.append(&Record::Begin(b)).unwrap();
        }
        let (b1, b2, b3) = (all[0], all[1], all[2]);
        assert_eq!(ids(&s.batches_rev(None, 10).unwrap()), vec![b3, b2, b1]);
        assert_eq!(ids(&s.batches_rev(Some(b2), 10).unwrap()), vec![b1]);
        assert_eq!(ids(&s.batches_rev(None, 2).unwrap()), vec![b3, b2]);
    }

    #[test]
    fn an_undo_marks_its_target() {
        let mut s = MemoryStore::default();
        let b1 = begin("app");
        let id1 = b1.batch_id;
        s.append(&Record::Begin(b1)).unwrap();
        let mut b2 = begin("app");
        b2.undo_of = Some(id1);
        let id2 = b2.batch_id;
        s.append(&Record::Begin(b2)).unwrap();
        let got = s.batch(id1).unwrap().unwrap();
        assert_eq!(got.summary.undone_by, Some(id2));
    }

    #[test]
    fn the_oldest_batch_is_dropped_past_capacity() {
        let mut s = MemoryStore::new(2);
        let b1 = begin("app");
        let id1 = b1.batch_id;
        s.append(&Record::Begin(b1)).unwrap();
        s.append(&Record::Begin(begin("app"))).unwrap();
        s.append(&Record::Begin(begin("app"))).unwrap();
        assert!(s.batch(id1).unwrap().is_none());
        s.append(&Record::Op(op(id1, Outcome::Done(json!(1)))))
            .unwrap();
        assert!(s.batch(id1).unwrap().is_none());
        assert_eq!(s.batches_rev(None, 10).unwrap().len(), 2);
    }

    #[test]
    fn the_default_keeps_200() {
        let mut s = MemoryStore::default();
        let first = begin("app");
        let first_id = first.batch_id;
        s.append(&Record::Begin(first)).unwrap();
        for _ in 0..200 {
            s.append(&Record::Begin(begin("app"))).unwrap();
        }
        assert!(s.batch(first_id).unwrap().is_none());
        assert_eq!(s.batches_rev(None, 1000).unwrap().len(), 200);
    }
}
