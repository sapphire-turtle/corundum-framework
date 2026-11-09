//! The engine's writes: resolving a path for a change, and the changes the operations share.

use std::io::{self, ErrorKind};

use cap_std::fs::Dir;
use corundum_framework_journal::Skip;
use grain_id::GrainId;

use crate::api::Expect;
use crate::expect;
use crate::rename::rename_noreplace;
use crate::{FsError, Resolved, Workspace, WorkspaceApp, WsPath};

/// Which entries a change may reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Only visible ones: a hidden path is refused with [`Skip::InvalidPath`]. What the
    /// built-in `fs.*` kinds use.
    Visible,
    /// Hidden ones too, still inside the root and never through a symlink. What the app's
    /// own kinds use, to move a sidecar along with its photo, say.
    App,
}

impl Prepared {
    /// Whether anything is at `to` now.
    fn is_taken(&self) -> Result<bool, Skip> {
        if !self.case_only {
            return exists(&self.to_dir, &self.to_name);
        }
        // The file system may find `from` under `to`'s name: look for the exact name.
        let mut entries = self.to_dir.entries().map_err(io_skip)?;
        Ok(entries.any(|e| e.is_ok_and(|e| e.file_name().to_str() == Some(self.to_name.as_str()))))
    }
}

/// A move, checked and ready to be done.
struct Prepared {
    from: Resolved,
    to_dir: Dir,
    to_name: String,
    /// Only the case of the name changes, in the same folder.
    case_only: bool,
}

impl Workspace {
    /// Resolves `path` for a change, as [`Workspace::resolve`] does, judging hiding only for
    /// [`Access::Visible`].
    ///
    /// A hidden path, or one through a symlink, is [`Skip::InvalidPath`]; a missing one
    /// [`Skip::NotFound`].
    pub fn entry(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
        access: Access,
    ) -> Result<Resolved, Skip> {
        self.resolve_with(app, path, access == Access::Visible)
            .map_err(skip_of)
    }

    /// Moves the entry at `from` to `to`, never replacing anything there: a file, a folder
    /// with everything in it, or a symlink as itself.
    ///
    /// Skipped with [`Skip::InvalidPath`] if `from` is the root, `to` is `from` or under it,
    /// or either is hidden (for [`Access::Visible`]) or through a symlink;
    /// [`Skip::NotFound`] if `from` is missing or `to`'s folder is not a folder;
    /// [`Skip::Exists`] if anything is at `to`; and [`Skip::Io`] if `to` is on another
    /// device. A change of case only goes through a temporary name, on every file system.
    pub fn rename(
        &self,
        app: &dyn WorkspaceApp,
        from: &WsPath,
        to: &WsPath,
        access: Access,
    ) -> Result<(), Skip> {
        self.move_entry(app, from, to, access, None, false)
            .map(|_| ())
    }

    /// [`Workspace::rename`], first checking the entry against `expect`
    /// ([`Skip::PreconditionFailed`]); with `dry_run`, everything but the move. Gives the
    /// entry's `size` and `modified` from before the move.
    pub(crate) fn move_entry(
        &self,
        app: &dyn WorkspaceApp,
        from: &WsPath,
        to: &WsPath,
        access: Access,
        expect: Option<&Expect>,
        dry_run: bool,
    ) -> Result<(u64, Option<i64>), Skip> {
        let prepared = self.prepare_move(app, from, to, access)?;
        if let Some(expect) = expect {
            expect::check(&prepared.from, expect)?;
        }
        let state = expect::state_of(&prepared.from);
        if dry_run {
            return if prepared.is_taken()? {
                Err(Skip::Exists)
            } else {
                Ok(state)
            };
        }
        let from_dir = prepared.from.parent();
        let from_name = prepared.from.name().ok_or(Skip::InvalidPath)?;
        if !prepared.case_only {
            rename_noreplace(from_dir, from_name, &prepared.to_dir, &prepared.to_name)
                .map_err(move_skip)?;
            return Ok(state);
        }
        // Through a dot-name in the same folder, so that a file system ignoring case sees two
        // different names; and `to` must still be free once `from` is out of the way.
        let temp = format!(".{}.corundum-{}", prepared.to_name, GrainId::random());
        rename_noreplace(from_dir, from_name, from_dir, &temp).map_err(move_skip)?;
        if let Err(e) = rename_noreplace(from_dir, &temp, from_dir, &prepared.to_name) {
            if let Err(back) = rename_noreplace(from_dir, &temp, from_dir, from_name) {
                tracing::error!("{from:?} is left at {temp:?}: moving it back failed: {back}");
            }
            return Err(move_skip(e));
        }
        Ok(state)
    }

    /// Makes an empty folder at `path`.
    ///
    /// Skipped with [`Skip::Exists`] if anything is there (the root is), [`Skip::NotFound`]
    /// if its folder is missing or not a folder, and [`Skip::InvalidPath`] if it is hidden
    /// (for [`Access::Visible`]) or through a symlink.
    pub fn make_dir(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
        access: Access,
    ) -> Result<(), Skip> {
        let (dir, name) = self.prepare_make(app, path, access)?;
        dir.create_dir(&name).map_err(move_skip)
    }

    /// Whether [`Workspace::make_dir`] would make `path` now, without making it.
    pub(crate) fn check_make_dir(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
        access: Access,
    ) -> Result<(), Skip> {
        let (dir, name) = self.prepare_make(app, path, access)?;
        if exists(&dir, &name)? {
            Err(Skip::Exists)
        } else {
            Ok(())
        }
    }

    /// Removes the folder at `path` if it is empty.
    ///
    /// Skipped with [`Skip::PreconditionFailed`] if it is not empty or not a folder,
    /// [`Skip::NotFound`] if it is missing, and [`Skip::InvalidPath`] for the root, a hidden
    /// path (for [`Access::Visible`]) or one through a symlink.
    pub fn remove_empty_dir(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
        access: Access,
    ) -> Result<(), Skip> {
        let resolved = self.prepare_remove(app, path, access)?;
        let name = resolved.name().ok_or(Skip::InvalidPath)?;
        resolved
            .parent()
            .remove_dir(name)
            .map_err(|e| match e.kind() {
                ErrorKind::DirectoryNotEmpty | ErrorKind::AlreadyExists => Skip::PreconditionFailed,
                _ => io_skip(e),
            })
    }

    /// Whether [`Workspace::remove_empty_dir`] would remove `path` now, without removing it.
    pub(crate) fn check_remove_empty_dir(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
        access: Access,
    ) -> Result<(), Skip> {
        let resolved = self.prepare_remove(app, path, access)?;
        let mut entries = resolved
            .open_dir()
            .map_err(skip_of)?
            .entries()
            .map_err(io_skip)?;
        if entries.next().is_some() {
            Err(Skip::PreconditionFailed)
        } else {
            Ok(())
        }
    }

    /// Checks a move, and opens what it needs: `from`'s folder (kept in `from`, not `from`
    /// itself, which on Windows could not be renamed while open) and `to`'s folder.
    fn prepare_move(
        &self,
        app: &dyn WorkspaceApp,
        from: &WsPath,
        to: &WsPath,
        access: Access,
    ) -> Result<Prepared, Skip> {
        let (Some(_), Some(to_name)) = (from.name(), to.name()) else {
            return Err(Skip::InvalidPath);
        };
        if to == from || to.as_str().starts_with(&format!("{from}/")) {
            return Err(Skip::InvalidPath);
        }
        if access == Access::Visible && (self.is_hidden(app, from) || self.is_hidden(app, to)) {
            return Err(Skip::InvalidPath);
        }
        let resolved = self.entry(app, from, access)?;
        let to_parent = to.parent().ok_or(Skip::InvalidPath)?;
        let to_dir = self.open_folder(app, &to_parent, access)?;
        let from_name = from.name().unwrap_or_default();
        let case_only = from.parent() == Some(to_parent)
            && from_name != to_name
            && from_name.to_lowercase() == to_name.to_lowercase();
        Ok(Prepared {
            from: resolved,
            to_dir,
            to_name: to_name.to_owned(),
            case_only,
        })
    }

    /// Checks a folder to make, and opens the folder it goes in.
    fn prepare_make(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
        access: Access,
    ) -> Result<(Dir, String), Skip> {
        let (Some(parent), Some(name)) = (path.parent(), path.name()) else {
            return Err(Skip::Exists);
        };
        if access == Access::Visible && self.is_hidden(app, path) {
            return Err(Skip::InvalidPath);
        }
        Ok((self.open_folder(app, &parent, access)?, name.to_owned()))
    }

    /// Checks a folder to remove.
    fn prepare_remove(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
        access: Access,
    ) -> Result<Resolved, Skip> {
        if path.is_root() {
            return Err(Skip::InvalidPath);
        }
        let resolved = self.entry(app, path, access)?;
        let meta = resolved.metadata();
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(Skip::PreconditionFailed);
        }
        Ok(resolved)
    }

    /// Opens the folder at `path`: [`Skip::InvalidPath`] for a symlink, [`Skip::NotFound`]
    /// for anything else that is not a folder.
    fn open_folder(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
        access: Access,
    ) -> Result<Dir, Skip> {
        let resolved = self.entry(app, path, access)?;
        let meta = resolved.metadata();
        if meta.file_type().is_symlink() {
            return Err(Skip::InvalidPath);
        }
        if !meta.is_dir() {
            return Err(Skip::NotFound);
        }
        resolved.open_dir().map_err(|e| match e {
            // Swapped for something else since it was resolved.
            FsError::InvalidPath(_) => Skip::NotFound,
            e => skip_of(e),
        })
    }
}

/// Whether anything (a dangling symlink too) is at `name` in `dir`.
fn exists(dir: &Dir, name: &str) -> Result<bool, Skip> {
    match dir.symlink_metadata(name) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
        Err(e) => Err(io_skip(e)),
    }
}

/// The skip for an error resolving a path.
pub(crate) fn skip_of(e: FsError) -> Skip {
    match e {
        FsError::InvalidPath(_) => Skip::InvalidPath,
        FsError::NotFound(_) => Skip::NotFound,
        e => Skip::Io(e.to_string()),
    }
}

/// The skip for an error making or moving an entry.
fn move_skip(e: io::Error) -> Skip {
    match e.kind() {
        // A folder in the way may be reported as not empty, rather than as existing.
        ErrorKind::AlreadyExists | ErrorKind::DirectoryNotEmpty => Skip::Exists,
        ErrorKind::NotFound | ErrorKind::NotADirectory => Skip::NotFound,
        ErrorKind::CrossesDevices => Skip::Io("another device".into()),
        _ => io_skip(e),
    }
}

/// [`Skip::Io`] with the error's message.
pub(crate) fn io_skip(e: io::Error) -> Skip {
    Skip::Io(e.to_string())
}
