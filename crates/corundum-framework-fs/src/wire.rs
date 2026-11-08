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

    /// Pins the journal's JSON, every field set, to the mirror's: this fails if either
    /// drifts, an optional field's name included.
    #[test]
    fn the_journals_records_parse_as_the_mirrors() {
        let (batch_id, redo_of, undone_by) =
            (BatchId::random(), BatchId::random(), BatchId::random());
        let (op1, op2) = (OpId::random(), OpId::random());
        let b = journal::BatchRecords {
            summary: BatchSummary {
                begin: BatchBegin {
                    batch_id,
                    origin: Origin::new("gui"),
                    actor: Actor::agent("p"),
                    at: 7,
                    label: Some("l".into()),
                    undo_of: None,
                    redo_of: Some(redo_of),
                },
                done: 1,
                skipped: 1,
                complete: true,
                interrupted: true,
                undone_by: Some(undone_by),
            },
            ops: vec![
                OpRecord {
                    op_id: op1,
                    batch_id,
                    parent: None,
                    kind: "fs.move".into(),
                    plan: json!({"from": "a", "to": "b"}),
                    outcome: Outcome::Done(json!({"x": 1})),
                },
                OpRecord {
                    op_id: op2,
                    batch_id,
                    parent: Some(op1),
                    kind: "fs.trash".into(),
                    plan: json!({"do": "trash", "path": "c"}),
                    outcome: Outcome::Skipped(journal::Skip::Io("disk full".into())),
                },
            ],
        };
        let s = |id: &dyn std::fmt::Display| id.to_string();
        assert_eq!(
            serde_json::to_value(&b).unwrap(),
            json!({
                "summary": {
                    "begin": {
                        "batch_id": s(&batch_id),
                        "origin": "gui",
                        "actor": {"principal": "p", "agent": true},
                        "at": 7,
                        "label": "l",
                        "undo_of": null,
                        "redo_of": s(&redo_of),
                    },
                    "done": 1,
                    "skipped": 1,
                    "complete": true,
                    "interrupted": true,
                    "undone_by": s(&undone_by),
                },
                "ops": [
                    {
                        "op_id": s(&op1),
                        "batch_id": s(&batch_id),
                        "parent": null,
                        "kind": "fs.move",
                        "plan": {"from": "a", "to": "b"},
                        "outcome": {"outcome": "done", "value": {"x": 1}},
                    },
                    {
                        "op_id": s(&op2),
                        "batch_id": s(&batch_id),
                        "parent": s(&op1),
                        "kind": "fs.trash",
                        "plan": {"do": "trash", "path": "c"},
                        "outcome": {
                            "outcome": "skipped",
                            "value": {"reason": "io", "message": "disk full"},
                        },
                    },
                ],
            })
        );
        let summary_mirror = api::BatchSummary {
            begin: api::BatchBegin {
                batch_id: s(&batch_id),
                origin: "gui".into(),
                actor: api::Actor {
                    principal: "p".into(),
                    agent: true,
                },
                at: 7,
                label: Some("l".into()),
                undo_of: None,
                redo_of: Some(s(&redo_of)),
            },
            done: 1,
            skipped: 1,
            complete: true,
            interrupted: true,
            undone_by: Some(s(&undone_by)),
        };
        let expected = api::BatchRecords {
            summary: summary_mirror.clone(),
            ops: vec![
                api::OpRecord {
                    op_id: s(&op1),
                    batch_id: s(&batch_id),
                    parent: None,
                    kind: "fs.move".into(),
                    plan: json!({"from": "a", "to": "b"}),
                    outcome: api::Outcome::Done(json!({"x": 1})),
                },
                api::OpRecord {
                    op_id: s(&op2),
                    batch_id: s(&batch_id),
                    parent: Some(s(&op1)),
                    kind: "fs.trash".into(),
                    plan: json!({"do": "trash", "path": "c"}),
                    outcome: api::Outcome::Skipped(api::Skip {
                        reason: "io".into(),
                        message: Some("disk full".into()),
                    }),
                },
            ],
        };
        assert_eq!(records(&b).unwrap(), expected);
        assert_eq!(summary(&b.summary).unwrap(), summary_mirror);
        assert_eq!(
            skip(&journal::Skip::Exists).unwrap(),
            api::Skip {
                reason: "exists".into(),
                message: None
            }
        );
    }

    /// `undo_of` set too (the batch above leaves it `None`), through the mirror.
    #[test]
    fn an_undo_of_reaches_the_mirror() {
        let undo_of = BatchId::random();
        let begin = BatchBegin {
            batch_id: BatchId::random(),
            origin: Origin::new("gui"),
            actor: Actor::person("p"),
            at: 1,
            label: None,
            undo_of: Some(undo_of),
            redo_of: None,
        };
        assert_eq!(
            serde_json::to_value(&begin).unwrap()["undo_of"],
            json!(undo_of.to_string())
        );
        let b = summary(&BatchSummary {
            begin,
            done: 0,
            skipped: 0,
            complete: false,
            interrupted: false,
            undone_by: None,
        })
        .unwrap();
        assert_eq!(b.begin.undo_of, Some(undo_of.to_string()));
    }
}
