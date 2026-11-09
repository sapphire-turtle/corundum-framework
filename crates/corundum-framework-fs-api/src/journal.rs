//! The journal methods (`journal.*`) and the records they return.
//!
//! The records mirror, field for field, the JSON of `corundum-framework-journal`, with ids as
//! strings, so that a client parses them without linking the journal.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Undo the origin's latest batch: [`OriginParams`] → [`BatchRecords`].
pub const JOURNAL_UNDO: &str = "journal.undo";
/// Redo the origin's latest undone batch: [`OriginParams`] → [`BatchRecords`].
pub const JOURNAL_REDO: &str = "journal.redo";
/// Undo a given batch: [`UndoBatchParams`] → [`BatchRecords`].
pub const JOURNAL_UNDO_BATCH: &str = "journal.undo_batch";
/// List batches, newest first, a page at a time: [`HistoryParams`] → [`History`].
pub const JOURNAL_HISTORY: &str = "journal.history";
/// One batch with its operations: [`StatusParams`] → [`BatchRecords`].
pub const JOURNAL_STATUS: &str = "journal.status";

/// The batches of a page when [`HistoryParams::limit`] is not given.
pub const DEFAULT_HISTORY_LIMIT: u32 = 50;
/// The most batches of a page.
pub const MAX_HISTORY_LIMIT: u32 = 200;

/// Parameters of [`JOURNAL_UNDO`] and [`JOURNAL_REDO`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginParams {
    /// The origin whose batch to undo or redo.
    pub origin: String,
}

/// Parameters of [`JOURNAL_UNDO_BATCH`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoBatchParams {
    /// The batch to undo.
    pub batch_id: String,
    /// The origin asking, recorded as the undo's origin; the batch may be of any origin.
    pub origin: String,
}

/// Parameters of [`JOURNAL_HISTORY`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryParams {
    /// The previous page's [`History::next_cursor`]; `None` for the first page.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Batches per page: [`DEFAULT_HISTORY_LIMIT`] when `None`, at most [`MAX_HISTORY_LIMIT`].
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Parameters of [`JOURNAL_STATUS`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusParams {
    /// The batch.
    pub batch_id: String,
}

/// Who made a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    /// The principal the batch was made for.
    pub principal: String,
    /// Whether an agent acted for the principal, rather than the person themselves.
    pub agent: bool,
}

/// The opening of a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchBegin {
    /// The batch.
    pub batch_id: String,
    /// What the batch came from.
    pub origin: String,
    /// Who made it.
    pub actor: Actor,
    /// When it began, in milliseconds since the Unix epoch.
    pub at: i64,
    /// A label for people to read.
    pub label: Option<String>,
    /// The batch this one undoes.
    pub undo_of: Option<String>,
    /// The batch this one redoes.
    pub redo_of: Option<String>,
}

/// A batch in brief.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchSummary {
    /// How the batch began.
    pub begin: BatchBegin,
    /// How many operations were done.
    pub done: u32,
    /// How many were skipped.
    pub skipped: u32,
    /// Whether the batch ended.
    pub complete: bool,
    /// Whether the batch ended cut short.
    #[serde(default)]
    pub interrupted: bool,
    /// The batch that undid this one, if any.
    pub undone_by: Option<String>,
}

/// Why an operation was not done.
///
/// `reason` stays a string, so that a client tolerates a reason it does not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skip {
    /// The reason, such as `"precondition_failed"` or `"exists"`.
    pub reason: String,
    /// More about it, for some reasons.
    #[serde(default)]
    pub message: Option<String>,
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

/// One operation in a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpRecord {
    /// The operation.
    pub op_id: String,
    /// The batch it belongs to.
    pub batch_id: String,
    /// The operation that led to this one.
    pub parent: Option<String>,
    /// The kind of operation.
    pub kind: String,
    /// What it was to do.
    pub plan: Value,
    /// What came of it.
    pub outcome: Outcome,
}

/// A batch with its operations: the result of [`FS_APPLY`](crate::FS_APPLY), the undo and
/// redo methods, and [`JOURNAL_STATUS`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchRecords {
    /// The batch in brief.
    pub summary: BatchSummary,
    /// Its operations, in order.
    pub ops: Vec<OpRecord>,
}

/// What a dry run found of one operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    /// The kind of operation.
    pub kind: String,
    /// What it would do.
    pub plan: Value,
    /// Why it would be skipped; `None` when it would be done.
    #[serde(default)]
    pub skip: Option<Skip>,
}

/// Result of [`FS_APPLY`](crate::FS_APPLY) with `dry_run`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DryRun {
    /// One check per operation, in order.
    pub checks: Vec<Check>,
}

/// A batch of the history: the batch in brief, and whether the caller may undo it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryBatch {
    /// The batch in brief; its fields sit beside `undoable` in the JSON.
    #[serde(flatten)]
    pub summary: BatchSummary,
    /// Whether [`JOURNAL_UNDO_BATCH`] would be accepted from the caller now: not undone
    /// already, not expired, and the caller allowed. `false` when a server does not say.
    #[serde(default)]
    pub undoable: bool,
}

/// Result of [`JOURNAL_HISTORY`]: batches, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct History {
    /// This page's batches.
    pub batches: Vec<HistoryBatch>,
    /// The cursor of the next page; `None` when this one is the last.
    pub next_cursor: Option<String>,
}
