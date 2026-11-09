# corundum-framework-fs

An app server's workspace folder over the `fs.*` and `journal.*` methods of
`corundum-framework-fs-api`. Paths are checked and never pass a symlink. Every dot-name and
every file the app keeps for itself (its sidecars) is hidden. Folders are listed a page at a
time, with the app's managed entries marked, and files are read in ranges.

`fs.apply` changes the workspace: it moves and renames, makes folders, and moves entries to
the trash in the app's hidden folder, never replacing an entry (on unix file systems without
an atomic no-replace rename, NFS and many FUSE mounts for example, the check before the
rename leaves a short window). Each apply is one batch of a
`corundum-framework-journal` journal, so one apply is one undo. `journal.undo` and
`journal.redo` reverse batches per origin, and `journal.undo_batch` a chosen batch; they
skip an operation whose place has changed since rather than overwrite anything.
`journal.history` and `journal.status` show what was done, and `journal.history` says per
batch whether the caller may undo it now (`undoable`). `fs.version` answers the API's
version, for a client to compare with its own. A dry run checks a batch without changing or
recording anything.

A server mounts `FsService::dispatch` on its own JSON-RPC router, over HTTP or a local
socket, and passes it the caller's actor. `FsService::new` keeps the journal in memory;
`FsService::with_journal` takes the app's own kinds, its store and its undo policy, and
`FsService::journal` runs the app's own batches.

The app's `WorkspaceApp` hooks shape each apply. `route` decides, per requested operation,
whether the built-in kind runs, the app's own kinds run instead, or the operation is denied
(recorded as an `fs.deny`, skipped with its reason); an operation on a hidden path is never
routed. `follow` adds the app's follow-ups after each built-in change, such as moving a
photo's sidecar with it, as undoable children. `may_undo` decides who may undo a batch.
`route` and `follow` see each path as the disk spells it where the entry exists, and as
requested for a name that does not exist yet.

## Known limitations

On Linux, hiding is judged on the name as the request spells it, and on the name
`canonicalize` returns, which on Linux is the spelling asked for. So on a mount that ignores
case (ext4 or f2fs with casefold, vfat, exFAT, CIFS/SMB), a name spelled differently from a
hidden entry can still reach it, FAT 8.3 short names such as `CORUND~1` for `.corundum` among
them. Clients should send only names they listed. macOS and Windows judge the disk's
spelling. For the same reason, on Linux `route` and `follow` see the request's spelling, not
the disk's.
