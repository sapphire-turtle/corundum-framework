//! `fs.move`.

use corundum_framework_journal::{Applied, OpKind, Operation, Skip};
use serde::{Deserialize, Serialize};

use crate::api::Expect;
use crate::expect::state_of;
use crate::{Access, FsCtx, WorkspaceApp, WsPath};

/// Moves or renames an entry, never replacing one (see [`Workspace::rename`]).
///
/// [`Workspace::rename`]: crate::Workspace::rename
#[derive(Debug, Clone, Copy, Default)]
pub struct MoveOp;

/// The plan of an `fs.move`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovePlan {
    /// The entry to move.
    pub from: WsPath,
    /// Where it goes.
    pub to: WsPath,
    /// How the entry must be now; `None` checks nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<Expect>,
}

/// What an `fs.move` did: the entry's state after the move, at `to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveDone {
    /// Where it was.
    pub from: WsPath,
    /// Where it is now.
    pub to: WsPath,
    /// Its `size`, as `fs.stat` gives it.
    pub size: u64,
    /// Its `modified`, as `fs.stat` gives it.
    pub modified: Option<i64>,
}

impl OpKind for MoveOp {
    const KIND: &'static str = "fs.move";
    type Plan = MovePlan;
    type Done = MoveDone;
}

impl<A: WorkspaceApp> Operation<FsCtx<A>> for MoveOp {
    fn check(&self, ctx: &FsCtx<A>, plan: &MovePlan) -> Result<(), Skip> {
        let ws = ctx.workspace();
        ws.move_entry(
            ctx.app(),
            &plan.from,
            &plan.to,
            Access::Visible,
            plan.expect.as_ref(),
            true,
        )
        .map(|_| ())
    }

    fn apply(&self, ctx: &FsCtx<A>, plan: &MovePlan) -> Result<Applied<MoveDone>, Skip> {
        let ws = ctx.workspace();
        let before = ws.move_entry(
            ctx.app(),
            &plan.from,
            &plan.to,
            Access::Visible,
            plan.expect.as_ref(),
            false,
        )?;
        // A move keeps the entry's size and time; should the entry be gone again already,
        // they are what it had.
        let (size, modified) = ws
            .entry(ctx.app(), &plan.to, Access::Visible)
            .map(|r| state_of(&r))
            .unwrap_or(before);
        Ok(Applied::new(MoveDone {
            from: plan.from.clone(),
            to: plan.to.clone(),
            size,
            modified,
        }))
    }

    fn inverse(&self, done: &MoveDone) -> Option<MovePlan> {
        Some(MovePlan {
            from: done.to.clone(),
            to: done.from.clone(),
            expect: Some(Expect {
                size: done.size,
                modified: done.modified,
                hash: None,
            }),
        })
    }
}
