//! Checking that an entry is still as the client saw it.

use corundum_framework_fs_api::{EntryKind, Expect};
use corundum_framework_journal::Skip;

use crate::Resolved;
use crate::access::{io_skip, skip_of};
use crate::list::entry_for;

/// The `size` and `modified` an entry shows now, as `fs.list` and `fs.stat` give them.
pub(crate) fn state_of(resolved: &Resolved) -> (u64, Option<i64>) {
    let entry = entry_for(String::new(), resolved.metadata());
    (entry.size, entry.modified)
}

/// [`Skip::PreconditionFailed`] unless the entry shows `expect`'s `size` and `modified`, and,
/// if `expect` has a hash, is a file with that `file/1` hash.
pub(crate) fn check(resolved: &Resolved, expect: &Expect) -> Result<(), Skip> {
    if state_of(resolved) != (expect.size, expect.modified) {
        return Err(Skip::PreconditionFailed);
    }
    let Some(hash) = &expect.hash else {
        return Ok(());
    };
    if entry_for(String::new(), resolved.metadata()).kind != EntryKind::File {
        return Err(Skip::PreconditionFailed);
    }
    let file = resolved.open_file().map_err(|e| match skip_of(e) {
        // Swapped for something that is not a file since it was resolved.
        Skip::InvalidPath => Skip::PreconditionFailed,
        s => s,
    })?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(file).map_err(io_skip)?;
    if format!("blake3:{}", hasher.finalize().to_hex()) == *hash {
        Ok(())
    } else {
        Err(Skip::PreconditionFailed)
    }
}
