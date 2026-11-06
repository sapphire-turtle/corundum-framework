//! The workspace's root and the resolution of paths under it.

use std::fs::{self, Metadata};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::{FsError, WorkspaceApp, WsPath};

/// A folder served over the `fs.*` methods.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// The workspace at `root`, which must be a folder. It is canonicalized once, here.
    pub fn open(root: impl AsRef<Path>) -> Result<Workspace, FsError> {
        let given = root.as_ref();
        let shown = given.display().to_string();
        let root = fs::canonicalize(given).map_err(|e| match e.kind() {
            ErrorKind::NotFound => FsError::NotFound(shown.clone()),
            _ => FsError::Io {
                path: shown.clone(),
                source: e,
            },
        })?;
        if !root.is_dir() {
            return Err(FsError::InvalidPath(format!("{shown} is not a folder")));
        }
        Ok(Workspace { root })
    }

    /// The canonical root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether `path` is hidden: it has a dot segment, or the app hides it or a folder above
    /// it.
    pub fn is_hidden(&self, app: &dyn WorkspaceApp, path: &WsPath) -> bool {
        path.has_dot_segment() || path.ancestors_and_self().any(|p| app.hidden(&p))
    }

    /// The file system's path of `path`, and its last segment's own metadata (a symlink's,
    /// not its target's).
    ///
    /// Refuses a hidden path and any path through a symlink with [`FsError::InvalidPath`].
    pub fn resolve(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
    ) -> Result<(PathBuf, Metadata), FsError> {
        if self.is_hidden(app, path) {
            return Err(FsError::InvalidPath(format!("{path:?} is hidden")));
        }
        let mut full = self.root.clone();
        let mut meta = self.metadata(&full, path)?;
        for segment in path.segments() {
            if meta.file_type().is_symlink() {
                return Err(FsError::InvalidPath(format!("{path:?} passes a symlink")));
            }
            if !meta.is_dir() {
                return Err(FsError::NotFound(path.to_string()));
            }
            full.push(segment);
            meta = self.metadata(&full, path)?;
        }
        Ok((full, meta))
    }

    fn metadata(&self, full: &Path, path: &WsPath) -> Result<Metadata, FsError> {
        fs::symlink_metadata(full).map_err(|e| match e.kind() {
            ErrorKind::NotFound | ErrorKind::NotADirectory => FsError::NotFound(path.to_string()),
            _ => FsError::Io {
                path: path.to_string(),
                source: e,
            },
        })
    }
}
