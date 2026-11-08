//! The change method: [`FS_APPLY`], and the operations it takes.

use serde::{Deserialize, Serialize};

/// Change the workspace with a batch of operations: [`ApplyParams`] →
/// [`BatchRecords`](crate::BatchRecords), or [`DryRun`](crate::DryRun) when `dry_run` is set.
pub const FS_APPLY: &str = "fs.apply";

/// The most operations one [`FS_APPLY`] takes.
pub const MAX_APPLY_OPS: usize = 10_000;

/// What a caller saw of a file, to be checked before the file is changed.
///
/// The operation is skipped when the file is no longer so. A folder's expect checks only that
/// it is still a folder: its `size` and `modified` are ignored, as a folder's `modified` moves
/// whenever an entry is added to it or taken from it. A folder with a `hash` never passes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expect {
    /// The `size` an [`Entry`](crate::Entry) showed.
    pub size: u64,
    /// The `modified` an [`Entry`](crate::Entry) showed.
    pub modified: Option<i64>,
    /// The file's `file/1` hash, if the caller has it: `blake3:` and 64 lowercase hex digits
    /// of the BLAKE3 hash of the whole file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

/// One operation of an [`FS_APPLY`] batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum FsOp {
    /// Move or rename a file or folder.
    #[serde(rename = "fs.move")]
    Move {
        /// The source path.
        from: String,
        /// The destination path, which must not exist.
        to: String,
        /// What the caller saw of the source, to check.
        #[serde(default)]
        expect: Option<Expect>,
    },
    /// Make a folder.
    #[serde(rename = "fs.mkdir")]
    Mkdir {
        /// The folder's path.
        path: String,
    },
    /// Move a file or folder to the workspace's trash.
    #[serde(rename = "fs.trash")]
    Trash {
        /// The path to trash.
        path: String,
        /// What the caller saw of it, to check.
        #[serde(default)]
        expect: Option<Expect>,
    },
}

/// Parameters of [`FS_APPLY`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyParams {
    /// The operations, in order; at most [`MAX_APPLY_OPS`].
    pub ops: Vec<FsOp>,
    /// What the batch comes from; undo and redo work per origin.
    pub origin: String,
    /// A label for people to read.
    #[serde(default)]
    pub label: Option<String>,
    /// Check the operations and report, without changing anything.
    #[serde(default)]
    pub dry_run: bool,
}
