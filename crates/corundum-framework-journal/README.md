# corundum-framework-journal

An undo log for an app server's changes. Work is recorded in batches of operations, each
with its plan and its outcome, so a batch can later be undone, and the undo redone.

This crate holds the ids and the records the journal writes.
