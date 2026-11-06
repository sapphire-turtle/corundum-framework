//! Why the journal refused or failed a call.

use crate::{BatchId, StoreError};

/// Why the journal refused or failed a call.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JournalError {
    /// The origin has no batch left to undo.
    #[error("nothing to undo")]
    NothingToUndo,
    /// The origin has no batch left to redo.
    #[error("nothing to redo")]
    NothingToRedo,
    /// The batch was already undone, or an undo of it is under way.
    #[error("batch {0} was already undone")]
    AlreadyUndone(BatchId),
    /// The batch is outside the undo window.
    #[error("batch {0} is too old to undo")]
    Expired(BatchId),
    /// The actor may not undo the batch.
    #[error("batch {0} may not be undone by this actor")]
    Forbidden(BatchId),
    /// The store has no such batch.
    #[error("batch {0} not found")]
    NotFound(BatchId),
    /// The worker's queue is full.
    #[error("the journal is busy")]
    Busy,
    /// An operation's kind is not registered.
    #[error("operation {index}: unknown kind {kind:?}")]
    UnknownKind {
        /// The operation's index in the batch.
        index: usize,
        /// Its kind.
        kind: String,
    },
    /// An operation's plan did not parse.
    #[error("operation {index}: invalid plan: {message}")]
    InvalidPlan {
        /// The operation's index in the batch.
        index: usize,
        /// Why it did not parse.
        message: String,
    },
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}
