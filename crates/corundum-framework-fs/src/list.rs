//! `fs.list`: a folder, a page at a time.

use std::time::UNIX_EPOCH;

use cap_std::fs::Metadata;

use corundum_framework_fs_api::{
    DEFAULT_LIST_LIMIT, Entry, EntryKind, ListParams, Listing, MAX_LIST_LIMIT,
};

use crate::{FsError, Workspace, WorkspaceApp, WsPath};

/// One page of the folder `params.path`: the entries after `params.cursor`, in name order,
/// hidden ones left out, the app's managed ones marked.
///
/// The cursor is the last name of the previous page, so a page stays right when entries
/// come and go in between. Names that are not UTF-8 are skipped.
pub fn list(
    ws: &Workspace,
    app: &dyn WorkspaceApp,
    params: &ListParams,
) -> Result<Listing, FsError> {
    let dir = WsPath::parse(&params.path)?;
    let resolved = ws.resolve(app, &dir)?;
    let handle = resolved.open_dir()?;
    let io = |source| FsError::Io {
        path: dir.to_string(),
        source,
    };
    // Names first, without a stat each: only the page's entries are stat'ed, so a large
    // folder costs a sort of its names, not a stat of every entry.
    let mut names = Vec::new();
    for item in handle.entries().map_err(io)? {
        let item = item.map_err(io)?;
        let Ok(name) = item.file_name().into_string() else {
            tracing::debug!(folder = %dir, name = ?item.file_name(), "skipped a name that is not UTF-8");
            continue;
        };
        if params.cursor.as_deref().is_some_and(|c| name.as_str() <= c) {
            continue;
        }
        let Ok(child) = dir.join(&name) else {
            continue;
        };
        if ws.is_hidden(app, &child) {
            continue;
        }
        names.push(name);
    }
    names.sort_unstable();
    let limit = params
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT) as usize;
    let mut rest = names.into_iter();
    let mut entries: Vec<Entry> = Vec::with_capacity(limit.min(rest.len()));
    for name in rest.by_ref() {
        // Gone since `read_dir` saw it: leave it out, and take the next.
        let Ok(meta) = handle.symlink_metadata(&name) else {
            continue;
        };
        entries.push(entry_for(name, &meta));
        if entries.len() == limit {
            break;
        }
    }
    let more = rest.len() > 0;
    app.annotate(&dir, &mut entries);
    let next_cursor = more
        .then(|| entries.last().map(|e| e.name.clone()))
        .flatten();
    Ok(Listing {
        entries,
        next_cursor,
    })
}

/// The entry `name`, from its own metadata (a symlink's, not its target's).
pub(crate) fn entry_for(name: String, meta: &Metadata) -> Entry {
    let kind = if meta.file_type().is_symlink() {
        EntryKind::Symlink
    } else if meta.is_dir() {
        EntryKind::Dir
    } else {
        EntryKind::File
    };
    let size = if kind == EntryKind::File {
        meta.len()
    } else {
        0
    };
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.into_std().duration_since(UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_millis()).ok());
    Entry {
        name,
        kind,
        size,
        modified,
        managed: None,
    }
}
