//! An undo log for an app server's changes.
//!
//! The journal records work in batches. A batch opens with a [`BatchBegin`], holds one
//! [`OpRecord`] for each operation, with its plan and its [`Outcome`], and closes with a
//! [`BatchEnd`]. A batch made by an undo or a redo names the batch it reverses, so the log
//! tells the whole story. See the journal spec for the design.
//!
//! This crate holds the ids, the records, and the operations with their registry.

#![warn(missing_docs)]

mod ids;
mod op;
mod record;
mod store;

pub use ids::{Actor, BatchId, OpId, Origin};
pub use op::{Applied, OpKind, Operation, Planned, Registry};
pub use record::{
    BatchBegin, BatchEnd, BatchRecords, BatchSummary, OpRecord, Outcome, Record, Skip,
};
pub use store::{JournalStore, MemoryStore, StoreError};
