//! `fs.mkdir`, and its undo.

use corundum_framework_journal::{Applied, OpKind, Operation, Skip};
use serde::{Deserialize, Serialize};

use crate::api::FsOp;
use crate::ops::FsDone;
use crate::{Access, FsCtx, WorkspaceApp, WsPath};

/// Makes a folder; its undo removes it if it is still empty.
#[derive(Debug, Clone, Copy, Default)]
pub struct MkdirOp;

/// The plan of an `fs.mkdir`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum MkdirPlan {
    /// Make an empty folder; skipped if anything is there.
    Make {
        /// The folder.
        path: WsPath,
    },
    /// Remove the folder; skipped unless it is empty.
    Remove {
        /// The folder.
        path: WsPath,
    },
}

/// What an `fs.mkdir` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum MkdirDone {
    /// The folder was made.
    Made {
        /// The folder.
        path: WsPath,
    },
    /// The folder was removed.
    Removed {
        /// The folder.
        path: WsPath,
    },
}

impl OpKind for MkdirOp {
    const KIND: &'static str = "fs.mkdir";
    type Plan = MkdirPlan;
    type Done = MkdirDone;
}

impl<A: WorkspaceApp> Operation<FsCtx<A>> for MkdirOp {
    fn check(&self, ctx: &FsCtx<A>, plan: &MkdirPlan) -> Result<(), Skip> {
        let (ws, app) = (ctx.workspace(), ctx.app());
        match plan {
            MkdirPlan::Make { path } => ws.check_make_dir(app, path, Access::Visible),
            MkdirPlan::Remove { path } => ws.check_remove_empty_dir(app, path, Access::Visible),
        }
    }

    fn apply(&self, ctx: &FsCtx<A>, plan: &MkdirPlan) -> Result<Applied<MkdirDone>, Skip> {
        let (ws, app) = (ctx.workspace(), ctx.app());
        Ok(match plan {
            MkdirPlan::Make { path } => {
                ws.make_dir(app, path, Access::Visible)?;
                let op = FsOp::Mkdir {
                    path: path.as_str().to_owned(),
                };
                let made = FsDone::Made { path: path.clone() };
                Applied {
                    done: MkdirDone::Made { path: path.clone() },
                    children: app.follow(&op, &made),
                }
            }
            // Only an undo's: no follow-ups.
            MkdirPlan::Remove { path } => {
                ws.remove_empty_dir(app, path, Access::Visible)?;
                Applied::new(MkdirDone::Removed { path: path.clone() })
            }
        })
    }

    fn inverse(&self, done: &MkdirDone) -> Option<MkdirPlan> {
        Some(match done {
            MkdirDone::Made { path } => MkdirPlan::Remove { path: path.clone() },
            MkdirDone::Removed { path } => MkdirPlan::Make { path: path.clone() },
        })
    }
}
