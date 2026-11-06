//! The app's hook into the engine.

use corundum_framework_fs_api::Entry;

use crate::WsPath;

/// What the app tells the engine about its workspace.
///
/// `()` is an app that hides nothing beyond the dot-names and manages nothing.
pub trait WorkspaceApp: Send + Sync + 'static {
    /// Whether the app keeps `path` for itself (a sidecar beside its photo, say). A hidden
    /// folder hides everything in it. Dot-names are always hidden, whatever this says.
    fn hidden(&self, path: &WsPath) -> bool {
        let _ = path;
        false
    }

    /// Fills in [`Entry::managed`] for the entries the app manages, among `entries` of the
    /// folder `dir` (one page of a listing, or the single entry of a stat).
    fn annotate(&self, dir: &WsPath, entries: &mut [Entry]) {
        let _ = (dir, entries);
    }
}

impl WorkspaceApp for () {}
