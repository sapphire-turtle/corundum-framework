//! The records the journal writes, and the summaries it reads back.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Actor, BatchId, OpId, Origin};

/// Why an operation was not done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(tag = "reason", content = "message", rename_all = "snake_case")]
pub enum Skip {
    /// What the operation expected was not there any more.
    #[error("precondition failed")]
    PreconditionFailed,
    /// The target already exists.
    #[error("already exists")]
    Exists,
    /// The target was not found.
    #[error("not found")]
    NotFound,
    /// The path was not valid.
    #[error("invalid path")]
    InvalidPath,
    /// The actor may not do it.
    #[error("denied")]
    Denied,
    /// An I/O error, with its message.
    #[error("i/o error: {0}")]
    Io(String),
    /// The operation's kind is not known.
    #[error("unknown operation kind")]
    UnknownKind,
    /// The operation cannot be undone.
    #[error("not undoable")]
    NotUndoable,
    /// The plan was not valid, with the reason.
    #[error("invalid plan: {0}")]
    InvalidPlan(String),
}

/// What came of an operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", content = "value", rename_all = "snake_case")]
pub enum Outcome {
    /// It was done, with the value it left for its undo.
    Done(Value),
    /// It was not done.
    Skipped(Skip),
}

/// The opening of a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchBegin {
    /// The batch.
    pub batch_id: BatchId,
    /// What the batch came from.
    pub origin: Origin,
    /// Who made it.
    pub actor: Actor,
    /// When it began, in milliseconds since the Unix epoch.
    pub at: i64,
    /// A label for people to read.
    pub label: Option<String>,
    /// The batch this one undoes.
    pub undo_of: Option<BatchId>,
    /// The batch this one redoes.
    pub redo_of: Option<BatchId>,
}

impl BatchBegin {
    /// Whether this batch undoes or redoes another.
    pub fn is_compensation(&self) -> bool {
        self.undo_of.is_some() || self.redo_of.is_some()
    }
}

/// One operation in a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpRecord {
    /// The operation.
    pub op_id: OpId,
    /// The batch it belongs to.
    pub batch_id: BatchId,
    /// The operation that led to this one.
    pub parent: Option<OpId>,
    /// The kind of operation.
    pub kind: String,
    /// What it was to do.
    pub plan: Value,
    /// What came of it.
    pub outcome: Outcome,
}

/// The closing of a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchEnd {
    /// The batch.
    pub batch_id: BatchId,
    /// How many operations were done.
    pub done: u32,
    /// How many were skipped.
    pub skipped: u32,
    /// Whether the batch was cut short.
    pub interrupted: bool,
}

/// A line of the journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum Record {
    /// A batch began.
    Begin(BatchBegin),
    /// An operation was recorded.
    Op(OpRecord),
    /// A batch ended.
    End(BatchEnd),
}

/// A batch in brief.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchSummary {
    /// How the batch began.
    pub begin: BatchBegin,
    /// How many operations were done.
    pub done: u32,
    /// How many were skipped.
    pub skipped: u32,
    /// Whether the batch ended.
    pub complete: bool,
    /// The batch that undid this one, if any.
    pub undone_by: Option<BatchId>,
}

/// A batch with its operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchRecords {
    /// The batch in brief.
    pub summary: BatchSummary,
    /// Its operations, in order.
    pub ops: Vec<OpRecord>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Actor, BatchId, OpId, Origin};
    use serde_json::json;

    fn begin(undo_of: Option<BatchId>, redo_of: Option<BatchId>) -> BatchBegin {
        BatchBegin {
            batch_id: BatchId::random(),
            origin: Origin::new("app"),
            actor: Actor::person("u1"),
            at: 5,
            label: Some("l".into()),
            undo_of,
            redo_of,
        }
    }

    #[test]
    fn records_round_trip() {
        let b = begin(None, None);
        let id = b.batch_id;
        let all = [
            Record::Begin(b),
            Record::Op(OpRecord {
                op_id: OpId::random(),
                batch_id: id,
                parent: Some(OpId::random()),
                kind: "k".into(),
                plan: json!({"a": 1}),
                outcome: Outcome::Skipped(Skip::Io("x".into())),
            }),
            Record::End(BatchEnd {
                batch_id: id,
                done: 1,
                skipped: 2,
                interrupted: false,
            }),
        ];
        for r in all {
            let s = serde_json::to_string(&r).unwrap();
            assert_eq!(serde_json::from_str::<Record>(&s).unwrap(), r);
        }
    }

    #[test]
    fn skips_have_stable_names() {
        assert_eq!(
            serde_json::to_value(Skip::PreconditionFailed).unwrap(),
            json!({"reason": "precondition_failed"})
        );
        assert_eq!(
            serde_json::to_value(Skip::Io("disk".into())).unwrap(),
            json!({"reason": "io", "message": "disk"})
        );
    }

    #[test]
    fn records_are_tagged() {
        let r = Record::End(BatchEnd {
            batch_id: BatchId::random(),
            done: 0,
            skipped: 0,
            interrupted: true,
        });
        assert_eq!(serde_json::to_value(r).unwrap()["record"], "end");
    }

    #[test]
    fn actors() {
        assert_eq!(
            Actor::agent("u1"),
            Actor {
                principal: "u1".into(),
                agent: true
            }
        );
        assert!(!Actor::person("u1").agent);
    }

    #[test]
    fn compensation_is_seen() {
        assert!(begin(Some(BatchId::random()), None).is_compensation());
        assert!(begin(None, Some(BatchId::random())).is_compensation());
        assert!(!begin(None, None).is_compensation());
    }
}
