# corundum-framework-journal

An undo log for an app server's changes. The server's work runs in batches of app-defined
operations on one writer thread, and each batch is recorded with its plan and its outcome.

Each UI session (origin) can undo and redo its own batches. An undo is a batch of
compensating operations, recorded like any other. By default an origin can undo its last 20
batches, within an hour; `may_undo` decides who may undo (by default, the same principal).

The records are kept behind the `JournalStore` trait. `MemoryStore` keeps the last 200
batches in memory. The crate uses no file system and no tokio.
