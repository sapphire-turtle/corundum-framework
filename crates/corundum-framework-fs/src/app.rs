//! The app's hook into the engine.

use corundum_framework_fs_api::{Entry, FsOp};
use corundum_framework_journal::{Actor, BatchBegin, Planned};

use crate::WsPath;
use crate::ops::FsDone;

/// What `fs.apply` runs for one requested operation, as [`WorkspaceApp::route`] decides.
#[derive(Debug, Clone, PartialEq)]
pub enum Route {
    /// The built-in kind (`fs.move`, `fs.mkdir` or `fs.trash`).
    Fs,
    /// The app's own operations instead, in place and in order; none drops the operation.
    ///
    /// Their kinds must be registered (see [`FsService::with_journal`]): an unregistered
    /// kind refuses the whole batch as an internal error. `MAX_APPLY_OPS` bounds the
    /// request, not the batch routing makes of it.
    ///
    /// [`FsService::with_journal`]: crate::FsService::with_journal
    Replace(Vec<Planned>),
    /// None: an `fs.deny` with this reason is recorded in its place, and skipped with
    /// `denied`.
    Deny(String),
}

/// What the app tells the engine about its workspace.
///
/// `()` is an app that hides nothing beyond the dot-names and manages nothing, and keeps its
/// own files in `.corundum`.
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

    /// The app's hidden folder at the root, one dot-name: `fs.trash` keeps the trash in it,
    /// under `trash`. Hidden, as every dot-name is.
    fn own_folder(&self) -> &str {
        ".corundum"
    }

    /// What `fs.apply` runs for `op`, one of the operations a caller asked for; for a
    /// folder, `op` names the folder. Asked once per operation, before the batch is
    /// submitted (and for a dry run too), off the async threads. The default is
    /// [`Route::Fs`].
    ///
    /// The paths in `op` are as the disk spells them where the entry exists, and as the
    /// request spelled them for a name that does not exist yet (a move's last `to` name is
    /// always as requested, so that a case-only rename keeps its new spelling). So on a file
    /// system that ignores case, an app can look up the entries it knows by the disk's
    /// spelling. On Linux they stay as requested (see the crate's known limitations).
    ///
    /// An operation that names a hidden path is never routed: it goes to its built-in kind,
    /// which skips it with `invalid_path`. The built-in kind still judges hiding again on
    /// disk when it runs.
    fn route(&self, op: &FsOp) -> Route {
        let _ = op;
        Route::Fs
    }

    /// The app's follow-ups to `op`, which a built-in kind carried out with the result
    /// `done` (moving a photo's sidecar with it, say): they run right after it, in the same
    /// batch, recorded as its children. The default is none.
    ///
    /// An undo or a redo drops what this returns, and undoes the original follow-ups by
    /// their own inverses instead: a follow-up's kind must be undoable, its inverse redoing
    /// its work.
    ///
    /// The paths in `op` and `done` are spelled as [`WorkspaceApp::route`] saw them: as the
    /// disk spells them where the entry existed, and as requested for a new name. On Linux
    /// they stay as requested.
    fn follow(&self, op: &FsOp, done: &FsDone) -> Vec<Planned> {
        let _ = (op, done);
        Vec::new()
    }

    /// Whether `actor` may undo `batch` (for a redo, `batch` is the undo it reverses). The
    /// default allows the batch's own principal. [`FsService`](crate::FsService) makes this the journal's
    /// [`JournalConfig::may_undo`](corundum_framework_journal::JournalConfig::may_undo).
    ///
    /// It runs under the journal's lock: it must not call into the journal.
    fn may_undo(&self, actor: &Actor, batch: &BatchBegin) -> bool {
        actor.principal == batch.actor.principal
    }
}

impl WorkspaceApp for () {}
