//! The `fs.*` methods an app's server serves over its workspace folder: their names, their
//! parameters, their results and their error codes.
//!
//! Plain serde data, kept apart from the engine (`corundum-framework-fs`) so that a client
//! links nothing heavier, and so that the API moves only with [`API_VERSION`]. The transport
//! is the app's: asteria serves these methods over HTTP, sapphire over its local socket.
//!
//! Paths are workspace-relative, with `/` between segments; the root is `""`.
//!
//! See `docs/superpowers/specs/2026-11-06-workspace-fs-journal-design.md`.

#![warn(missing_docs)]

/// The version of this API.
///
/// Bumped by hand on any breaking change to a method, a parameter or a result here. The
/// crate is versioned with the rest of corundum, so its version cannot stand for the API's.
pub const API_VERSION: u32 = 1;

/// The version of this API, whatever the params: `Value` → [`VersionInfo`].
pub const FS_VERSION: &str = "fs.version";

/// Result of [`FS_VERSION`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VersionInfo {
    /// The server's [`API_VERSION`].
    pub api: u32,
}

mod change;
mod journal;
mod read;

pub use change::*;
pub use journal::*;
pub use read::*;

/// The JSON-RPC error codes of these methods.
///
/// corundum's own are in `-32199..=-32100`, apart from the apps' (asteria's are
/// `-32001..=-32009`).
pub mod codes {
    /// The path is malformed, leaves the workspace, passes a symlink, or names a hidden entry.
    pub const INVALID_PATH: i32 = -32101;
    /// The path does not exist.
    pub const NOT_FOUND: i32 = -32102;
    /// The request asks for more than a limit allows.
    pub const TOO_LARGE: i32 = -32103;
    /// There is nothing of the origin's to undo.
    pub const NOTHING_TO_UNDO: i32 = -32104;
    /// There is nothing of the origin's to redo.
    pub const NOTHING_TO_REDO: i32 = -32105;
    /// The batch was undone already.
    pub const ALREADY_UNDONE: i32 = -32106;
    /// The batch is too old to undo, its journal having been pruned.
    pub const EXPIRED: i32 = -32107;
    /// The caller may not do it: the batch is another origin's, or the actor is not allowed.
    pub const FORBIDDEN: i32 = -32108;
    /// The workspace is busy with another change; try again.
    pub const BUSY: i32 = -32109;
    /// The client and the server speak different versions of the API.
    ///
    /// Not answered by the server: a client raises it on comparing `fs.version`'s answer
    /// with its own [`API_VERSION`](crate::API_VERSION).
    pub const API_MISMATCH: i32 = -32110;
    /// JSON-RPC's: no such method.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// JSON-RPC's: the parameters do not parse.
    pub const INVALID_PARAMS: i32 = -32602;
    /// JSON-RPC's: the server failed (an I/O error, say).
    pub const INTERNAL_ERROR: i32 = -32603;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use serde_json::json;

    fn round_trip<T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug>(v: &T) {
        let json = serde_json::to_string(v).unwrap();
        assert_eq!(&serde_json::from_str::<T>(&json).unwrap(), v, "{json}");
    }

    fn entry(managed: Option<Managed>) -> Entry {
        Entry {
            name: "p.jpg".into(),
            kind: EntryKind::File,
            size: 42,
            modified: Some(1_700_000_000_000),
            managed,
        }
    }

    #[test]
    fn every_type_round_trips() {
        round_trip(&entry(None));
        round_trip(&entry(Some(Managed {
            kind: "photo".into(),
            item_id: "abc".into(),
        })));
        round_trip(&Listing {
            entries: vec![entry(None)],
            next_cursor: Some("p.jpg".into()),
        });
        round_trip(&ListParams {
            path: "a".into(),
            cursor: Some("b".into()),
            limit: Some(10),
        });
        round_trip(&StatParams { path: "a".into() });
        round_trip(&ReadParams {
            path: "a".into(),
            offset: 3,
            len: 5,
        });
        round_trip(&ReadResult {
            data: "aGVsbG8=".into(),
            total: 5,
        });
        let expect = Expect {
            size: 3,
            modified: Some(9),
            hash: Some(format!("blake3:{}", "0".repeat(64))),
        };
        let ops = vec![
            FsOp::Move {
                from: "a".into(),
                to: "b".into(),
                expect: Some(expect.clone()),
            },
            FsOp::Mkdir { path: "d".into() },
            FsOp::Trash {
                path: "c".into(),
                expect: None,
            },
        ];
        round_trip(&ApplyParams {
            ops: ops.clone(),
            origin: "app".into(),
            label: Some("l".into()),
            dry_run: true,
        });
        round_trip(&OriginParams {
            origin: "app".into(),
        });
        round_trip(&UndoBatchParams {
            batch_id: "b1".into(),
            origin: "app".into(),
        });
        round_trip(&HistoryParams {
            cursor: Some("c".into()),
            limit: Some(5),
        });
        round_trip(&StatusParams {
            batch_id: "b1".into(),
        });
        let begin = BatchBegin {
            batch_id: "b1".into(),
            origin: "app".into(),
            actor: Actor {
                principal: "u1".into(),
                agent: true,
            },
            at: 5,
            label: Some("l".into()),
            undo_of: Some("b0".into()),
            redo_of: None,
        };
        let summary = BatchSummary {
            begin,
            done: 1,
            skipped: 1,
            complete: true,
            interrupted: false,
            undone_by: Some("b2".into()),
        };
        let skip = Skip {
            reason: "io".into(),
            message: Some("disk".into()),
        };
        let records = BatchRecords {
            summary: summary.clone(),
            ops: vec![
                OpRecord {
                    op_id: "o1".into(),
                    batch_id: "b1".into(),
                    parent: Some("o0".into()),
                    kind: "fs.move".into(),
                    plan: json!({"from": "a"}),
                    outcome: Outcome::Done(json!({"x": 1})),
                },
                OpRecord {
                    op_id: "o2".into(),
                    batch_id: "b1".into(),
                    parent: None,
                    kind: "fs.trash".into(),
                    plan: json!({}),
                    outcome: Outcome::Skipped(skip.clone()),
                },
            ],
        };
        round_trip(&records);
        round_trip(&DryRun {
            checks: vec![Check {
                kind: "fs.mkdir".into(),
                plan: json!({}),
                skip: Some(skip),
            }],
        });
        round_trip(&History {
            batches: vec![HistoryBatch {
                summary,
                undoable: true,
            }],
            next_cursor: None,
        });
    }

    #[test]
    fn fs_ops_are_tagged_by_kind() {
        let op: FsOp =
            serde_json::from_value(json!({"kind": "fs.move", "from": "a", "to": "b"})).unwrap();
        assert_eq!(
            op,
            FsOp::Move {
                from: "a".into(),
                to: "b".into(),
                expect: None
            }
        );
        assert_eq!(
            serde_json::to_value(FsOp::Mkdir { path: "d".into() }).unwrap(),
            json!({"kind": "fs.mkdir", "path": "d"})
        );
    }

    #[test]
    fn an_unknown_skip_reason_still_parses() {
        let o: Outcome = serde_json::from_value(
            json!({"outcome": "skipped", "value": {"reason": "later_reason"}}),
        )
        .unwrap();
        assert_eq!(
            o,
            Outcome::Skipped(Skip {
                reason: "later_reason".into(),
                message: None
            })
        );
    }

    #[test]
    fn entry_kinds_are_lowercase() {
        assert_eq!(serde_json::to_string(&EntryKind::File).unwrap(), "\"file\"");
        assert_eq!(serde_json::to_string(&EntryKind::Dir).unwrap(), "\"dir\"");
        assert_eq!(
            serde_json::to_string(&EntryKind::Symlink).unwrap(),
            "\"symlink\""
        );
    }

    #[test]
    fn missing_optional_params_default() {
        let p: ListParams = serde_json::from_str(r#"{"path":"a"}"#).unwrap();
        assert_eq!(
            p,
            ListParams {
                path: "a".into(),
                cursor: None,
                limit: None
            }
        );
        let json = serde_json::to_value(entry(None)).unwrap();
        assert!(json.get("managed").is_none(), "{json}");
    }

    #[test]
    fn method_names_are_namespaced() {
        for name in [FS_LIST, FS_STAT, FS_READ, FS_APPLY] {
            assert!(name.starts_with("fs."), "{name}");
        }
        for name in [
            JOURNAL_UNDO,
            JOURNAL_REDO,
            JOURNAL_UNDO_BATCH,
            JOURNAL_HISTORY,
            JOURNAL_STATUS,
        ] {
            assert!(name.starts_with("journal."), "{name}");
        }
    }

    #[test]
    fn codes_stay_in_their_range() {
        let own = [
            codes::INVALID_PATH,
            codes::NOT_FOUND,
            codes::TOO_LARGE,
            codes::NOTHING_TO_UNDO,
            codes::NOTHING_TO_REDO,
            codes::ALREADY_UNDONE,
            codes::EXPIRED,
            codes::FORBIDDEN,
            codes::BUSY,
        ];
        for code in own {
            assert!((-32199..=-32100).contains(&code), "{code}");
        }
        let mut all = own.to_vec();
        all.extend([
            codes::METHOD_NOT_FOUND,
            codes::INVALID_PARAMS,
            codes::INTERNAL_ERROR,
        ]);
        let n = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), n, "codes must be distinct");
    }
}
