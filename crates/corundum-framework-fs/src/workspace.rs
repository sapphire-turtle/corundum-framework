//! The workspace's root and the resolution of paths under it.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::ambient_authority;
use cap_std::fs::{Dir, File, Metadata, OpenOptions};

use crate::{FsError, WorkspaceApp, WsPath};

/// A folder served over the `fs.*` methods.
#[derive(Debug)]
pub struct Workspace {
    root: PathBuf,
    dir: Dir,
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
        let dir = Dir::open_ambient_dir(&root, ambient_authority()).map_err(|e| FsError::Io {
            path: shown.clone(),
            source: e,
        })?;
        Ok(Workspace { root, dir })
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

    /// Resolves `path` by walking directory handles from the root, never following a symlink.
    ///
    /// Refuses a hidden path and any path through a symlink with [`FsError::InvalidPath`]. The
    /// result keeps the handle of the last segment's folder, so opening it later cannot be
    /// redirected by a symlink swapped into the path in between.
    pub fn resolve(&self, app: &dyn WorkspaceApp, path: &WsPath) -> Result<Resolved, FsError> {
        self.resolve_with(app, path, true)
    }

    /// [`Workspace::resolve`], judging hiding only if `hide`: without it a hidden entry is
    /// reached, still never through a symlink.
    pub(crate) fn resolve_with(
        &self,
        app: &dyn WorkspaceApp,
        path: &WsPath,
        hide: bool,
    ) -> Result<Resolved, FsError> {
        if hide && self.is_hidden(app, path) {
            return Err(FsError::InvalidPath(format!("{path:?} is hidden")));
        }
        let (parent, name) = self.walk_parent(path)?;
        let meta = match &name {
            Some(name) => parent.symlink_metadata(name).map_err(|e| map_io(e, path))?,
            None => parent.dir_metadata().map_err(|e| map_io(e, path))?,
        };
        let resolved = Resolved {
            path: path.clone(),
            parent,
            name,
            meta,
        };
        // On a file system that ignores case (or Unicode normalization), `P.JPG.XMP` reaches
        // `p.jpg.xmp`: judge hiding on the names on disk too, not only on those asked for.
        if hide && !path.is_root() && self.is_hidden(app, &self.on_disk(&resolved)?) {
            return Err(FsError::InvalidPath(format!("{path:?} is hidden")));
        }
        Ok(resolved)
    }

    /// `path` as the disk spells it, whether hidden or not: an entry that exists by the disk's
    /// spelling, one that does not by its nearest existing folder's followed by the rest as
    /// asked. The root, and a path that cannot be told (through a symlink, say), are as asked.
    pub(crate) fn spelled(&self, path: &WsPath) -> WsPath {
        let mut rest = Vec::new();
        let mut at = path.clone();
        loop {
            if at.is_root() {
                return path.clone();
            }
            match self.resolve_with(&(), &at, false) {
                Ok(r) if rest.is_empty() || r.meta.is_dir() => {
                    let spelled = self.on_disk(&r).and_then(|on_disk| {
                        rest.iter()
                            .rev()
                            .try_fold(on_disk, |p: WsPath, name: &String| p.join(name))
                    });
                    return spelled.unwrap_or_else(|_| path.clone());
                }
                Err(FsError::NotFound(_)) => {}
                _ => return path.clone(),
            }
            let (Some(parent), Some(name)) = (at.parent(), at.name()) else {
                return path.clone();
            };
            rest.push(name.to_owned());
            at = parent;
        }
    }

    /// Opens the folder holding the last segment of `path`, one no-follow open per segment,
    /// and returns it with that segment's name (`None` for the root, whose folder is the root
    /// itself).
    ///
    /// A symlink segment is [`FsError::InvalidPath`]; a missing or non-folder one
    /// [`FsError::NotFound`].
    pub(crate) fn walk_parent(&self, path: &WsPath) -> Result<(Dir, Option<String>), FsError> {
        let mut segments = path.segments().peekable();
        let mut dir = self.dir.try_clone().map_err(|e| map_io(e, path))?;
        while let Some(segment) = segments.next() {
            if segments.peek().is_none() {
                return Ok((dir, Some(segment.to_owned())));
            }
            dir = match dir.open_dir_nofollow(segment) {
                Ok(next) => next,
                Err(e) => {
                    // Told apart by the entry itself: a symlink, or not a folder.
                    return Err(match dir.symlink_metadata(segment) {
                        Ok(m) if m.file_type().is_symlink() => {
                            FsError::InvalidPath(format!("{path:?} passes a symlink"))
                        }
                        Ok(m) if !m.is_dir() => FsError::NotFound(path.to_string()),
                        _ => map_io(e, path),
                    });
                }
            };
        }
        Ok((dir, None))
    }

    /// The workspace path of `resolved` as the file system spells it. No segment is a symlink
    /// but perhaps the last, so canonicalizing the folder follows nothing, and a final symlink
    /// keeps its name as asked.
    fn on_disk(&self, resolved: &Resolved) -> Result<WsPath, FsError> {
        let path = &resolved.path;
        let io = |source| FsError::Io {
            path: path.to_string(),
            source,
        };
        let mut full = self.root.clone();
        full.extend(path.segments());
        let real = if resolved.meta.file_type().is_symlink() {
            let parent = full.parent().unwrap_or(&self.root);
            let name = full.file_name().unwrap_or_default();
            fs::canonicalize(parent).map_err(io)?.join(name)
        } else {
            fs::canonicalize(&full).map_err(io)?
        };
        let rel = real
            .strip_prefix(&self.root)
            .map_err(|_| FsError::InvalidPath(format!("{path:?} leaves the workspace")))?;
        let mut on_disk = WsPath::root();
        for segment in rel.components() {
            let name = segment
                .as_os_str()
                .to_str()
                .ok_or_else(|| FsError::InvalidPath(format!("{path:?} is not UTF-8 on disk")))?;
            on_disk = on_disk.join(name)?;
        }
        Ok(on_disk)
    }
}

fn map_io(e: io::Error, path: &WsPath) -> FsError {
    match e.kind() {
        ErrorKind::NotFound | ErrorKind::NotADirectory => FsError::NotFound(path.to_string()),
        _ => FsError::Io {
            path: path.to_string(),
            source: e,
        },
    }
}

/// A path resolved under a [`Workspace`]: the handle of the folder holding its last segment,
/// and that segment's own metadata (a symlink's, not its target's).
#[derive(Debug)]
pub struct Resolved {
    path: WsPath,
    parent: Dir,
    name: Option<String>,
    meta: Metadata,
}

impl Resolved {
    /// The path that was resolved.
    pub fn path(&self) -> &WsPath {
        &self.path
    }

    /// The last segment's own metadata, as it was when resolved.
    pub fn metadata(&self) -> &Metadata {
        &self.meta
    }

    /// Opens the folder, without following a symlink. [`FsError::InvalidPath`] unless it is a
    /// folder.
    pub fn open_dir(&self) -> Result<Dir, FsError> {
        let not_a_folder = || FsError::InvalidPath(format!("{:?} is not a folder", self.path));
        let Some(name) = &self.name else {
            return self.parent.try_clone().map_err(|e| self.io(e));
        };
        if !self.meta.is_dir() {
            return Err(not_a_folder());
        }
        // Swapped since it was resolved: a symlink, or a file, is not a folder.
        self.parent
            .open_dir_nofollow(name)
            .map_err(|e| match self.parent.symlink_metadata(name) {
                Ok(m) if !m.is_dir() => not_a_folder(),
                _ => self.io(e),
            })
    }

    /// Opens the file for reading, without following a symlink and without blocking on a
    /// FIFO. [`FsError::InvalidPath`] unless it is a regular file: the entry may have been
    /// swapped since it was resolved, so what was opened is checked.
    pub fn open_file(&self) -> Result<File, FsError> {
        let not_a_file = || FsError::InvalidPath(format!("{:?} is not a file", self.path));
        let Some(name) = &self.name else {
            return Err(not_a_file());
        };
        if !self.meta.is_file() {
            return Err(not_a_file());
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        #[cfg(unix)]
        {
            use cap_std::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = self.parent.open_with(name, &options).map_err(|e| {
            // A symlink swapped in is refused by the no-follow open as a loop.
            match self.parent.symlink_metadata(name) {
                Ok(m) if !m.is_file() => not_a_file(),
                _ => self.io(e),
            }
        })?;
        let meta = file.metadata().map_err(|e| self.io(e))?;
        if !meta.is_file() {
            return Err(not_a_file());
        }
        Ok(file)
    }

    /// The folder holding the last segment (the root itself for the root).
    pub(crate) fn parent(&self) -> &Dir {
        &self.parent
    }

    /// The last segment's name, `None` for the root.
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    fn io(&self, source: io::Error) -> FsError {
        FsError::Io {
            path: self.path.to_string(),
            source,
        }
    }
}
