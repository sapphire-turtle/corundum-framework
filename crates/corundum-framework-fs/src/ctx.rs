//! The journal's context for the workspace's operations.

use std::sync::Arc;

use crate::{Workspace, WorkspaceApp};

/// What the workspace's operations run against: the workspace and the app's hook.
///
/// The journal's context for the built-in kinds (`fs.*`) and for the app's own kinds.
pub struct FsCtx<A: WorkspaceApp> {
    ws: Arc<Workspace>,
    app: Arc<A>,
}

impl<A: WorkspaceApp> FsCtx<A> {
    /// The context over `ws`, for `app`.
    pub fn new(ws: Arc<Workspace>, app: Arc<A>) -> FsCtx<A> {
        FsCtx { ws, app }
    }

    /// The workspace.
    pub fn workspace(&self) -> &Workspace {
        &self.ws
    }

    /// The app's hook.
    pub fn app(&self) -> &A {
        &self.app
    }
}
