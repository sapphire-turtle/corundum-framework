//! The journal's context for the workspace's operations.

use std::sync::Arc;

use crate::{Workspace, WorkspaceApp, WsPath};

/// What the workspace's operations run against: the workspace and the app's hook.
///
/// The journal's context for the built-in kinds (`fs.*`) and for the app's own kinds.
pub struct FsCtx<A: WorkspaceApp> {
    ws: Arc<Workspace>,
    app: Arc<A>,
}

impl<A: WorkspaceApp> FsCtx<A> {
    /// The context over `ws`, for `app`.
    ///
    /// # Panics
    ///
    /// Panics if the app's [`WorkspaceApp::own_folder`] is not a single dot-name segment.
    pub fn new(ws: Arc<Workspace>, app: Arc<A>) -> FsCtx<A> {
        let own = app.own_folder();
        assert!(
            own.starts_with('.') && WsPath::root().join(own).is_ok(),
            "the app's own folder {own:?} is not a single dot-name"
        );
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

    /// The app's own folder, [`WorkspaceApp::own_folder`], checked by [`FsCtx::new`].
    pub(crate) fn own_folder(&self) -> WsPath {
        WsPath::root()
            .join(self.app.own_folder())
            .expect("checked by FsCtx::new")
    }
}
