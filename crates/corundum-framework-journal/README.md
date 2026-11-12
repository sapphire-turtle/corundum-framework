# corundum-framework-journal

An undo log for an app server's changes. The server's work runs in batches of app-defined
operations on one writer thread, and each batch is recorded with its plan and its outcome.

Each UI session (origin) can undo and redo its own batches. An undo is a batch of
compensating operations, recorded like any other. By default an origin can undo its last 20
batches, within an hour; `may_undo` decides who may undo (by default, the same principal).

The records are kept behind the `JournalStore` trait. `MemoryStore` keeps the last 200
batches in memory. The crate uses no tokio, and by default no file system.

With the `jsonl` feature, `JsonlStore` keeps the journal for good, for an app that audits a
long history: one `YYYY-MM.jsonl` file per month (UTC) in a directory the app gives, its
server instance's state directory, never the workspace.

- It never deletes anything. To keep less, remove old month files while the store is
  closed; the store opens without them. Its index, kept in memory, grows by about one
  summary per batch.
- One store per directory: it holds a lock on `journal.lock`, and a second open fails.
- On open, a line torn by a crash is cut off, and a batch left without an end ends as
  interrupted; its finished operations stay undoable within the undo window.
