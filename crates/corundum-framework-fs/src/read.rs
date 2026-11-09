//! `fs.stat` and `fs.read`.

use std::io::{Read, Seek, SeekFrom};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use corundum_framework_fs_api::{Entry, MAX_READ_LEN, ReadParams, ReadResult, StatParams};

use crate::list::entry_for;
use crate::{FsError, Workspace, WorkspaceApp, WsPath};

/// The entry at `params.path`, marked by the app if it manages it. The root is a folder
/// named `""`.
pub fn stat(ws: &Workspace, app: &dyn WorkspaceApp, params: &StatParams) -> Result<Entry, FsError> {
    let path = WsPath::parse(&params.path)?;
    let resolved = ws.resolve(app, &path)?;
    let mut entry = [entry_for(
        path.name().unwrap_or("").to_owned(),
        resolved.metadata(),
    )];
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
    let mut file = ws.resolve(app, &path)?.open_file()?;
    let io = |source| FsError::Io {
        path: path.to_string(),
        source,
    };
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
