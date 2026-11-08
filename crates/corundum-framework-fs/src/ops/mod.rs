//! The built-in kinds the journal runs: `fs.move`, `fs.mkdir` and `fs.trash`, and `fs.deny`
//! for an operation the app refused.

mod deny;
mod mkdir;
mod move_;
mod trash;

use corundum_framework_journal::Registry;
use serde::{Deserialize, Serialize};

use crate::{FsCtx, WorkspaceApp, WsPath};

pub use deny::{DenyOp, DenyPlan};
pub use mkdir::{MkdirDone, MkdirOp, MkdirPlan};
pub use move_::{MoveDone, MoveOp, MovePlan};
pub use trash::{TrashDone, TrashOp, TrashPlan};

/// Registers the built-in kinds in `registry`.
pub fn register_builtin<A: WorkspaceApp>(registry: &mut Registry<FsCtx<A>>) {
    registry
        .register(MoveOp)
        .register(MkdirOp)
        .register(TrashOp)
        .register(DenyOp);
}

/// What a built-in operation did, as the app's follow-up hook is told.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "done", rename_all = "snake_case")]
pub enum FsDone {
    /// An entry was moved.
    Moved {
        /// Where it was.
        from: WsPath,
        /// Where it is now.
        to: WsPath,
    },
    /// A folder was made.
    Made {
        /// The folder.
        path: WsPath,
    },
    /// An entry was moved to the trash.
    Trashed {
        /// Where it was.
        path: WsPath,
        /// Its trash id.
        trash_id: String,
    },
}
