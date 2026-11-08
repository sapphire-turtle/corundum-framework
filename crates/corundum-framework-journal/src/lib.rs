//! An undo log for an app server's changes.
//!
//! The journal records work in batches. A batch opens with a [`BatchBegin`], holds one
//! [`OpRecord`] for each operation, with its plan and its [`Outcome`], and closes with a
//! [`BatchEnd`]. A batch made by an undo or a redo names the batch it reverses, so the log
//! tells the whole story. Undo and redo are kept per [`Origin`], and reach back as far as the
//! [`UndoPolicy`] allows. (In the repository, the design is in
//! `docs/superpowers/specs/2026-11-06-workspace-fs-journal-design.md`.)
//!
//! [`Journal`] runs the batches on one worker thread, over a [`JournalStore`].

#![warn(missing_docs)]

mod error;
mod ids;
mod journal;
mod op;
mod record;
mod store;
mod undo;

pub use error::JournalError;
pub use ids::{Actor, BatchId, OpId, Origin};
pub use journal::{Clock, Journal, JournalConfig, MAX_CHILD_DEPTH, MayUndo, UndoPolicy};
pub use op::{Applied, OpKind, Operation, Planned, Registry};
pub use record::{
    BatchBegin, BatchEnd, BatchRecords, BatchSummary, OpRecord, Outcome, Record, Skip,
};
pub use store::{JournalStore, MemoryStore, StoreError};
