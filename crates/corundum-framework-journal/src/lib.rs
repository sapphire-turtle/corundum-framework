//! An undo log for an app server's changes.
//!
//! The journal records work in batches. A batch opens with a [`BatchBegin`], holds one
//! [`OpRecord`] for each operation, with its plan and its [`Outcome`], and closes with a
//! [`BatchEnd`]. A batch made by an undo or a redo names the batch it reverses, so the log
//! tells the whole story. See the journal spec for the design.
//!
//! This module set holds the ids and the records.

#![warn(missing_docs)]

mod ids;
mod record;

pub use ids::{Actor, BatchId, OpId, Origin};
pub use record::{
    BatchBegin, BatchEnd, BatchRecords, BatchSummary, OpRecord, Outcome, Record, Skip,
};
