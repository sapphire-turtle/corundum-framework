//! `fs.stat` and `fs.read`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use corundum_framework_fs_api::{Entry, MAX_READ_LEN, ReadParams, ReadResult, StatParams};

use crate::list::entry_for;
use crate::{FsError, Workspace, WorkspaceApp, WsPath};

/// The entry at `params.path`, marked by the app if it manages it. The root is a folder
/// named `""`.
pub fn stat(ws: &Workspace, app: &dyn WorkspaceApp, params: &StatParams) -> Result<Entry, FsError> {
    let path = WsPath::parse(&params.path)?;
    let (_, meta) = ws.resolve(app, &path)?;
    let mut entry = [entry_for(path.name().unwrap_or("").to_owned(), &meta)];
    app.annotate(&path.parent().unwrap_or_else(WsPath::root), &mut entry);
    let [entry] = entry;
    Ok(entry)
}

/// Up to `params.len` bytes of the file at `params.path` from `params.offset`, and the
/// file's length. Past the end the bytes are empty. A folder or a symlink is
/// [`FsError::InvalidPath`].
pub fn read(
    ws: &Workspace,
    app: &dyn WorkspaceApp,
    params: &ReadParams,
) -> Result<ReadResult, FsError> {
    if params.len > MAX_READ_LEN {
        return Err(FsError::TooLarge(format!(
            "{} bytes asked; at most {MAX_READ_LEN} at a time",
            params.len
        )));
    }
    let path = WsPath::parse(&params.path)?;
    let (full, meta) = ws.resolve(app, &path)?;
    if !meta.is_file() {
        return Err(FsError::InvalidPath(format!("{path:?} is not a file")));
    }
    let io = |source| FsError::Io {
        path: path.to_string(),
        source,
    };
    let mut file = open_regular(&full, &path)?;
    let total = file.metadata().map_err(io)?.len();
    let left = total.saturating_sub(params.offset);
    let mut data = Vec::with_capacity(left.min(u64::from(params.len)) as usize);
    if params.offset < total {
        file.seek(SeekFrom::Start(params.offset)).map_err(io)?;
        file.take(u64::from(params.len))
            .read_to_end(&mut data)
            .map_err(io)?;
    }
    Ok(ReadResult {
        data: STANDARD.encode(&data),
        total,
    })
}

/// Opens the file at `full` (the resolution of `path`) for reading, without following a
/// symlink in its last segment and without blocking on a FIFO, and checks that what it opened
/// is a regular file: the entry may have been swapped since it was resolved.
///
/// A symlink swapped into a middle segment in between is still followed; opening relative to
/// directory handles closes that, before writes are built on `resolve`.
fn open_regular(full: &Path, path: &WsPath) -> Result<File, FsError> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: open a symlink or junction itself, not its target.
        options.custom_flags(0x0020_0000);
    }
    let not_a_file = || FsError::InvalidPath(format!("{path:?} is not a file"));
    let file = options.open(full).map_err(|source| {
        #[cfg(unix)]
        if source.raw_os_error() == Some(libc::ELOOP) {
            return not_a_file();
        }
        FsError::Io {
            path: path.to_string(),
            source,
        }
    })?;
    let meta = file.metadata().map_err(|source| FsError::Io {
        path: path.to_string(),
        source,
    })?;
    if !meta.is_file() {
        return Err(not_a_file());
    }
    Ok(file)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn a_symlink_swapped_in_after_resolve_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("secret"), b"secret").unwrap();
        std::os::unix::fs::symlink(dir.path().join("secret"), dir.path().join("f")).unwrap();
        let got = open_regular(&dir.path().join("f"), &WsPath::parse("f").unwrap());
        assert!(matches!(got, Err(FsError::InvalidPath(_))), "{got:?}");
    }

    #[test]
    fn a_fifo_swapped_in_after_resolve_does_not_block() {
        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("f");
        let c = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let got = open_regular(&fifo, &WsPath::parse("f").unwrap());
        assert!(matches!(got, Err(FsError::InvalidPath(_))), "{got:?}");
    }
}
