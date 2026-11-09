//! `fs.deny`.

use corundum_framework_journal::{Applied, OpKind, Operation, Skip};
use serde::{Deserialize, Serialize};

use crate::api::FsOp;
use crate::{FsCtx, WorkspaceApp};

/// An operation the app refused ([`Route::Deny`]): checking or applying it skips it with
/// [`Skip::Denied`], and it cannot be undone. Its plan keeps the operation and the app's
/// reason, so the history shows them.
///
/// [`Route::Deny`]: crate::Route::Deny
#[derive(Debug, Clone, Copy, Default)]
pub struct DenyOp;

/// The plan of an `fs.deny`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenyPlan {
    /// The operation the caller asked for.
    pub op: FsOp,
    /// Why the app refused it.
    pub reason: String,
}

impl OpKind for DenyOp {
    const KIND: &'static str = "fs.deny";
    type Plan = DenyPlan;
    /// Never made: an `fs.deny` is always skipped.
    type Done = ();
}

impl<A: WorkspaceApp> Operation<FsCtx<A>> for DenyOp {
    fn check(&self, _ctx: &FsCtx<A>, _plan: &DenyPlan) -> Result<(), Skip> {
        Err(Skip::Denied)
    }

    fn apply(&self, _ctx: &FsCtx<A>, _plan: &DenyPlan) -> Result<Applied<()>, Skip> {
        Err(Skip::Denied)
    }

    fn inverse(&self, _done: &()) -> Option<DenyPlan> {
        None
    }
}
