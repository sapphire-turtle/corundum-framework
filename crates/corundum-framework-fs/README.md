# corundum-framework-fs

An app server's workspace folder over the `fs.*` and `journal.*` methods of
`corundum-framework-fs-api`. Paths are checked and never pass a symlink. Every dot-name and
every file the app keeps for itself (its sidecars) is hidden. Folders are listed a page at a
time, with the app's managed entries marked, and files are read in ranges.

`fs.apply` changes the workspace: it moves and renames, makes folders, and moves entries to
the trash in the app's hidden folder, never replacing an entry. Each apply is one batch of a
`corundum-framework-journal` journal, so one apply is one undo. `journal.undo` and
`journal.redo` reverse batches per origin, and `journal.undo_batch` a chosen batch; they
skip an operation whose place has changed since rather than overwrite anything.
`journal.history` and `journal.status` show what was done. A dry run checks a batch without changing or recording
anything.

A server mounts `FsService::dispatch` on its own JSON-RPC router, over HTTP or a local
socket, and passes it the caller's actor. `FsService::new` keeps the journal in memory;
`FsService::with_journal` takes the app's own kinds, its store and its undo policy, and
`FsService::journal` runs the app's own batches.

The app's `WorkspaceApp` hooks shape each apply. `route` decides, per requested operation,
whether the built-in kind runs, the app's own kinds run instead, or the operation is denied
(recorded as an `fs.deny`, skipped with its reason); an operation on a hidden path is never
routed. `follow` adds the app's follow-ups after each built-in change, such as moving a
photo's sidecar with it, as undoable children. `may_undo` decides who may undo a batch.
