//! The journal's values as the `fs-api` mirrors a client parses.
//!
//! Each is converted through its JSON: the journal's value serialized, the mirror
//! deserialized from it. The mirrors are kept field for field with the journal's JSON, so a
//! failure here is the server's (an internal error), never the caller's.

use corundum_framework_fs_api as api;
use corundum_framework_journal as journal;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::FsError;

/// An internal error, for a failure that is no path's.
pub(crate) fn internal(e: impl std::error::Error + Send + Sync + 'static) -> FsError {
    FsError::Io {
        path: String::new(),
        source: std::io::Error::other(e),
    }
}

/// `value` as the mirror `M`, through its JSON.
fn mirror<T: Serialize, M: DeserializeOwned>(value: &T) -> Result<M, FsError> {
    let json = serde_json::to_value(value).map_err(internal)?;
    serde_json::from_value(json).map_err(internal)
}

/// A batch with its operations.
pub(crate) fn records(b: &journal::BatchRecords) -> Result<api::BatchRecords, FsError> {
    mirror(b)
}

/// A batch in brief.
pub(crate) fn summary(s: &journal::BatchSummary) -> Result<api::BatchSummary, FsError> {
    mirror(s)
}

/// Why an operation was not done, or would not be.
pub(crate) fn skip(s: &journal::Skip) -> Result<api::Skip, FsError> {
    mirror(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use journal::{Actor, BatchBegin, BatchId, BatchSummary, OpId, OpRecord, Origin, Outcome};
    use serde_json::json;

    fn op(batch_id: BatchId, outcome: Outcome) -> OpRecord {
        OpRecord {
            op_id: OpId::random(),
            batch_id,
            parent: None,
            kind: "fs.move".into(),
            plan: json!({"from": "a", "to": "b"}),
            outcome,
        }
    }

    /// Pins the journal's JSON to the mirror's: this fails if either drifts.
    #[test]
    fn the_journals_records_parse_as_the_mirrors() {
        let batch_id = BatchId::random();
        let done = op(batch_id, Outcome::Done(json!({"x": 1})));
        let skipped = op(
            batch_id,
            Outcome::Skipped(journal::Skip::Io("disk full".into())),
        );
        assert_eq!(
            serde_json::to_value(&done.outcome).unwrap(),
            json!({"outcome": "done", "value": {"x": 1}})
        );
        assert_eq!(
            serde_json::to_value(&skipped.outcome).unwrap(),
            json!({"outcome": "skipped", "value": {"reason": "io", "message": "disk full"}})
        );
        let b = journal::BatchRecords {
            summary: BatchSummary {
                begin: BatchBegin {
                    batch_id,
                    origin: Origin::new("gui"),
                    actor: Actor::agent("p"),
                    at: 7,
                    label: Some("l".into()),
                    undo_of: Some(BatchId::random()),
                    redo_of: None,
                },
                done: 1,
                skipped: 1,
                complete: true,
                interrupted: false,
                undone_by: None,
            },
            ops: vec![done.clone(), skipped.clone()],
        };
        let m = records(&b).unwrap();
        let begin = &m.summary.begin;
        assert_eq!(begin.batch_id, batch_id.to_string());
        assert_eq!(begin.origin, "gui");
        assert_eq!(
            begin.actor,
            api::Actor {
                principal: "p".into(),
                agent: true
            }
        );
        assert_eq!(
            begin.undo_of,
            b.summary.begin.undo_of.map(|i| i.to_string())
        );
        assert_eq!((m.summary.done, m.summary.skipped), (1, 1));
        assert!(m.summary.complete);
        assert_eq!(m.ops[0].op_id, done.op_id.to_string());
        assert_eq!(m.ops[0].batch_id, batch_id.to_string());
        assert_eq!(m.ops[0].plan, done.plan);
        assert_eq!(m.ops[0].outcome, api::Outcome::Done(json!({"x": 1})));
        assert_eq!(
            m.ops[1].outcome,
            api::Outcome::Skipped(api::Skip {
                reason: "io".into(),
                message: Some("disk full".into())
            })
        );
        assert_eq!(summary(&b.summary).unwrap(), m.summary);
        assert_eq!(
            skip(&journal::Skip::Exists).unwrap(),
            api::Skip {
                reason: "exists".into(),
                message: None
            }
        );
    }
}
