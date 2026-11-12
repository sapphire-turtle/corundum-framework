# JsonlStore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `JsonlStore`, a `JournalStore` behind corundum-framework-journal's `jsonl` feature. It keeps the journal in one `YYYY-MM.jsonl` file per month (UTC), so an app can keep a long history to audit.

**Architecture:**
- **What lives on disk:** the records, one JSON `Record` per line.
- **What lives in memory:** the index, built on open:
  - every batch's `BatchSummary`, in append order;
  - for each batch, its file and the byte span of each of its op lines.
- **How `batch()` works:** it reads a batch's op lines back by their spans.
- **Where a batch goes:** wholly into one month file, chosen by its begin's `at`. Months never go backwards, so replaying the files in month order gives the append order.
- **On open:**
  - a torn last line is cut off;
  - a line that does not parse is skipped;
  - a batch without an end gets `BatchEnd { interrupted: true }`.

**Tech Stack:** Rust 2024 (toolchain 1.99.0, so `File::try_lock` is there), serde_json, tracing, tempfile 3 (dev).

**Spec:** `docs/superpowers/specs/2026-11-06-workspace-fs-journal-design.md`. The relevant parts are the "Stores" section and "Testing" (journal: "`JsonlStore`'s round trip and interrupted batches"). This plan is "Order of work" step 5, on the branch `jsonl-store` from main at a9cf78b.

## Decisions this plan makes (the spec leaves them open)

1. **Month of a batch.**
   - A batch's records all go into the file of the UTC month of its `BatchBegin::at`.
   - If that month is older than the newest file's month (the clock went back), the batch goes into the newest file instead.
   - So files never go backwards, and reading them in month order gives the append order back.
   - An op or an end is written to its batch's file, even when that file is no longer the newest one (a batch that ran over midnight at the end of a month).
2. **Torn last line.**
   - On open, any file that does not end in `\n` is truncated to just after its last `\n`, with a `tracing::warn!`.
   - A complete line that does not parse as a `Record` (corruption, or a record kind from a later version) is skipped, with a `warn`. Open does not fail because of it, so the server still starts.
   - Empty lines are skipped silently.
3. **Retention.**
   - `JsonlStore` never deletes anything; the spec wants a long history.
   - An app that wants less removes old `YYYY-MM.jsonl` files while the store is closed.
   - On open, an op or an end whose batch is not found is ignored, and so is an `undo_of`/`redo_of` that names a missing batch. The trait already allows dropped batches.
   - The README says so, and says that memory grows with the history, by about one summary per batch.
4. **Paging `batches_rev` across months.**
   - The index is one `Vec<BatchId>` in append order, plus a `HashMap<BatchId, usize>` from id to position.
   - A cursor is found in O(1), and paging does not depend on files at all.
   - An unknown cursor gives an empty page, as in `MemoryStore`.
5. **Durability.**
   - Each record is serialized whole, with its `\n`, and written with one `write_all`.
   - If the write fails, the file is cut back to its length before the write (best effort), the index is left unchanged, and `StoreError::Io` is returned.
   - After an `End` record, `sync_data()` is called, so a finished batch survives a power loss. There is no fsync per op.
6. **One writer.**
   - `open` creates `<dir>/journal.lock` and holds `File::try_lock()` on it for the store's life.
   - A second open of the same directory fails with `StoreError::Io("the journal in <dir> is open elsewhere")`.
7. **Counts.**
   - As in `MemoryStore`, a summary's `done` and `skipped` are counted from its op records.
   - An end sets only `complete` and `interrupted`.
   - A duplicate begin is dropped without being written.

## Global Constraints

- Rust 2024, toolchain 1.99.0; the crate keeps `#![warn(missing_docs)]`.
- The `jsonl` feature adds no dependency (std only); `tempfile = "3"` is a dev-dependency.
- No tokio. The journal calls the store under its lock, and the store does blocking std I/O.
- The default build of corundum-framework-journal still "uses no file system": everything is under `#[cfg(feature = "jsonl")]`.
- File names are exactly `YYYY-MM.jsonl` (4-digit year, 2-digit month `01`–`12`), UTC. Other files in the directory are ignored. The lock file is `journal.lock`.
- The directory is the app's state directory, passed in by the app; `open` creates it if it is missing.
- Before each commit, run all of these:
  - `cargo fmt --all`
  - `cargo clippy -p corundum-framework-journal --all-targets --features jsonl --locked -- -D warnings`
  - the same without `--features jsonl`
  - `cargo clippy -p corundum-framework-journal --all-targets --features jsonl --target x86_64-pc-windows-gnu -- -D warnings` (Wine hangs on this machine, so Windows is only linted)
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **A crash in the middle of a write** leaves a torn last line. Open must cut it off, and the next record must start on a line of its own. (Task 2: `a_torn_last_line_is_cut_off`)
2. **The clock goes back across a month boundary** (a begin dated October after one dated November). After reopening, `batches_rev` must still give the append order. (Task 1: `months_never_go_backwards`)
3. **A second server opens the same state directory.** It must be refused, not allowed to interleave its writes. (Task 1: `a_second_open_is_refused`)
4. **The app deleted an old month file.** Open must still succeed, ignoring the orphan ops and ends and the undos of missing batches. (Task 2: `a_deleted_month_is_forgotten`)
5. **Stray files in the directory** (`README`, `2026-13.jsonl`, `notes.jsonl`, `.DS_Store`) must be ignored, not parsed. (Task 1: `other_files_are_ignored`)

---

### Task 1: `JsonlStore`: write, reopen, read back

**Files:**
- Modify: `crates/corundum-framework-journal/Cargo.toml`: `[features] jsonl = []`, `[dev-dependencies] tempfile = "3"`.
- Modify: `crates/corundum-framework-journal/src/lib.rs`: `#[cfg(feature = "jsonl")] mod jsonl;` and `#[cfg(feature = "jsonl")] pub use jsonl::JsonlStore;`.
- Create: `crates/corundum-framework-journal/src/jsonl/mod.rs`: `JsonlStore`, its `JournalStore` impl, writing and reading spans.
- Create: `crates/corundum-framework-journal/src/jsonl/month.rs`: `Month`, with unit tests.
- Create: `crates/corundum-framework-journal/src/jsonl/open.rs`: listing the month files and replaying them into the index.
- Test: `crates/corundum-framework-journal/tests/jsonl.rs`, starting with `#![cfg(feature = "jsonl")]`.

**Interfaces:**
- Produces:
  - `pub struct JsonlStore` (`Debug`).
  - `JsonlStore::open(dir: impl AsRef<Path>) -> Result<JsonlStore, StoreError>`.
  - `JsonlStore::dir(&self) -> &Path`.
  - `impl JournalStore for JsonlStore`.
- Produces, crate-private for Task 2:
  - `month::Month { year: i32, month: u32 }`, with `Ord`.
  - `Month::of(at_ms: i64) -> Month`.
  - `Month::file_name(&self) -> String`, which gives `"2026-11.jsonl"`.
  - `Month::parse(file_name: &str) -> Option<Month>`.
  - In `mod.rs`, a fn `write_line(&mut self, file: usize, record: &Record) -> Result<Span, StoreError>`, which does the cut-back on failure. Task 2 reuses it to write the interrupted ends.

- [ ] **Step 1: Write the month unit tests** in `month.rs`.

```rust
#[test]
fn months_are_utc() {
    assert_eq!(Month::of(0).file_name(), "1970-01.jsonl");
    assert_eq!(Month::of(-1).file_name(), "1969-12.jsonl");
    assert_eq!(Month::of(1_796_083_199_999).file_name(), "2026-11.jsonl");
    assert_eq!(Month::of(1_796_083_200_000).file_name(), "2026-12.jsonl");
    assert_eq!(Month::of(1_709_208_000_000).file_name(), "2024-02.jsonl");
    assert_eq!(Month::of(951_868_800_000).file_name(), "2000-03.jsonl");
}

#[test]
fn only_month_names_parse() {
    assert_eq!(Month::parse("2026-11.jsonl"), Some(Month { year: 2026, month: 11 }));
    for bad in ["2026-13.jsonl", "2026-00.jsonl", "2026-1.jsonl", "notes.jsonl",
                "2026-11.json", "2026-11.jsonl.tmp", "journal.lock", ".DS_Store"] {
        assert_eq!(Month::parse(bad), None, "{bad}");
    }
    assert!(Month::parse("2026-10.jsonl") < Month::parse("2026-11.jsonl"));
}
```

- [ ] **Step 2: Write the store tests** in `tests/jsonl.rs`. Each uses `tempfile::tempdir()`. A local `begin(at: i64) -> BatchBegin` helper uses origin `"app"`, actor `Actor::person("u1")` and a random id.
  - `a_batch_round_trips`:
    - Write Begin(at = 1_794_700_800_000), two ops (one `Done(json!(1))`, one `Skipped(Skip::NotFound)`) and End.
    - Drop the store and reopen it.
    - Then `batch(id)` has the same `ops` (`assert_eq!`), `done == 1`, `skipped == 1`, `complete`, and not `interrupted`.
    - Also `<dir>/2026-11.jsonl` exists, and its 4 lines each parse as a `Record` equal to what was appended.
  - `batches_page_newest_first_across_months`:
    - Write 5 begins, at `2026-10-15`, `2026-10-15`, `2026-11-15`, `2026-11-15`, `2026-11-15` (the ms values above), and reopen.
    - `batches_rev(None, 2)` gives the last two.
    - `batches_rev(Some(third), 10)` gives the second and the first.
    - `batches_rev(Some(BatchId::random()), 10)` is empty.
    - Both `2026-10.jsonl` and `2026-11.jsonl` exist.
  - `an_op_goes_to_its_batch_month`:
    - Begin A in October, begin B in November, then an op of A and an End of A; reopen.
    - `batch(A)` has the op and is `complete`.
    - The op line is in `2026-10.jsonl`, not `2026-11.jsonl`.
  - `an_undo_marks_its_target_after_reopen`: begin A, then begin B with `undo_of: Some(A)`; reopen; `batch(A).summary.undone_by == Some(B)`.
  - `months_never_go_backwards`:
    - Begin A at `2026-11-15`, then begin B at `2026-10-15`; reopen.
    - `batches_rev(None, 10)` gives `[B, A]`.
    - `2026-10.jsonl` does not exist.
  - `a_second_open_is_refused`: while one store is open, `JsonlStore::open(same dir)` is `Err(StoreError::Io(_))`. Once the first store is dropped, opening succeeds.
  - `other_files_are_ignored`:
    - Create `README`, `notes.jsonl`, `2026-13.jsonl` and `.DS_Store`, each holding `not json\n`.
    - Then open succeeds, `batches_rev(None, 10)` is empty, and those files are unchanged.
  - `a_duplicate_begin_is_not_written`: appending the same Begin twice leaves one line in the file and one batch.
  - `an_op_of_an_unknown_batch_is_dropped`: appending it returns `Ok(())`, and no month file is created.

- [ ] **Step 3: Run the tests.** `cargo test -p corundum-framework-journal --features jsonl`. Expected: compile errors (no `JsonlStore`, no `Month`).

- [ ] **Step 4: Implement `month.rs`.**
  - `Month::of` takes `days = at_ms.div_euclid(86_400_000)` and converts with Howard Hinnant's `civil_from_days`, taking the year and month and dropping the day.
  - `parse` accepts exactly `^\d{4}-\d{2}\.jsonl$` with the month in `1..=12`.

- [ ] **Step 5: Implement `JsonlStore` in `mod.rs` and `open.rs`.**
  - `open`:
    1. `create_dir_all`.
    2. Lock `journal.lock` with `try_lock`; `TryLockError::WouldBlock` becomes the "open elsewhere" `Io` error.
    3. List the month files, sorted by `Month`.
    4. Read each file line by line, keeping each line's byte offset, and replay it into the index.
    5. Open the newest file in append mode.
  - Index entries hold `summary`, `file: usize` and `ops: Vec<Span { offset: u64, len: u32 }>`.
  - On append:
    - A begin picks `max(Month::of(at), newest)` and creates that file if it is new.
    - An op or an end goes to its batch's file. A file other than the newest is opened in append mode for that one write.
    - The index changes only after the write succeeds.
  - `batch()` opens the batch's file and reads each span, with `seek` + `read_exact`. A span that does not parse as `Record::Op` becomes `StoreError::Corrupt`.
  - I/O errors become `StoreError::Io(e.to_string())`.
  - Keep each file's length in memory, for the cut-back.

- [ ] **Step 6: Run the tests.**
  - `cargo test -p corundum-framework-journal --features jsonl`: Expected PASS.
  - `cargo test -p corundum-framework-journal`, without the feature: Expected PASS, with `tests/jsonl.rs` compiled out.

- [ ] **Step 7: Run the Global Constraints checks, then commit.**

```bash
git add crates/corundum-framework-journal
git commit -m "feat(corundum-framework-journal): JsonlStore, the journal in a jsonl file per month"
```

### Task 2: Recovery on open

**Files:**
- Modify: `crates/corundum-framework-journal/src/jsonl/open.rs`.
- Test: `crates/corundum-framework-journal/tests/jsonl.rs`.

**Interfaces:**
- Consumes: Task 1's `Month`, its index and `write_line`.
- Produces: no new public API. `open` now repairs instead of failing.

- [ ] **Step 1: Write the failing tests** in `tests/jsonl.rs`.
  - `a_torn_last_line_is_cut_off`:
    - Write a complete batch A, close the store, and append `{"record":"begin","batch_` (no newline) to its file.
    - Reopen. `batches_rev(None, 10)` gives `[A]`, and the file ends in `\n`.
    - Append begin B and reopen again: both A and B are there, and every line of the file parses.
  - `a_line_that_does_not_parse_is_skipped`:
    - Between two complete batches, insert the line `{"record":"later_kind"}\n`, then `garbage\n`.
    - Reopen: both batches are there, in order. The op spans of the second batch still read back (`batch(second).ops.len()` is what was written).
  - `an_unfinished_batch_ends_interrupted`:
    - Write begin A and one `Done` op, with no end; drop and reopen.
    - `batch(A).summary` has `complete == true`, `interrupted == true` and `done == 1`.
    - The file's last line is `Record::End(BatchEnd { batch_id: A, done: 1, skipped: 0, interrupted: true })`.
    - Reopening once more adds no second end: the line count is unchanged.
  - `an_unfinished_batch_in_an_older_month_ends_in_its_month`: A is begun in October with no end, and B in November, complete. Reopen: the interrupted end of A is the last line of `2026-10.jsonl`.
  - `a_deleted_month_is_forgotten`:
    - Write A in October.
    - In November write: B with `undo_of: Some(A)`; an op of A; an end of A; and C, complete.
    - Close, delete `2026-10.jsonl`, and reopen. Open succeeds.
    - `batches_rev(None, 10)` gives `[C, B]`, and `batch(A)` is `None`.

- [ ] **Step 2: Run the tests.** `cargo test -p corundum-framework-journal --features jsonl --test jsonl`. Expected: the torn-line test and the not-parsing test fail with a `Corrupt` error from open (or a misparse). The interrupted tests fail on `complete == false`.

- [ ] **Step 3: Implement the recovery in `open.rs`.**
  - Before replaying a file, truncate it after its last `\n`, opening it read-write, with a `warn` naming the file and the bytes cut.
  - Skip a line that does not parse, with a `warn` naming the file and the line number.
  - After all files are replayed, call `write_line` with each incomplete batch's interrupted end, in append order, and update its summary.
  - Write one `warn` with the number of interrupted batches.

- [ ] **Step 4: Run the tests.** `cargo test -p corundum-framework-journal --features jsonl`. Expected PASS.

- [ ] **Step 5: Run the Global Constraints checks, then commit.**

```bash
git commit -am "feat(corundum-framework-journal): JsonlStore repairs a torn line and ends interrupted batches on open"
```

### Task 3: The journal over `JsonlStore`, the facade, CI and the docs

**Files:**
- Test: `crates/corundum-framework-journal/tests/jsonl_journal.rs` (`#![cfg(feature = "jsonl")]`, `mod common;`).
- Modify: `crates/corundum-framework/Cargo.toml`: add `journal-jsonl = ["journal", "corundum-framework-journal/jsonl"]` under `journal`, with a comment in the style of its neighbours.
- Modify: `.github/workflows/ci.yml`: next to the per-feature loops, add `cargo clippy -p corundum-framework-journal --all-targets --locked -- -D warnings`, so the crate is linted with no features.
- Modify: `crates/corundum-framework-journal/README.md`, `crates/corundum-framework-journal/src/lib.rs` (crate doc), `README.md` and `README.ja.md`.
  - The journal row of the crate table now ends: "`MemoryStore` keeps the last 200 batches; with `jsonl`, `JsonlStore` keeps them all in a file per month in the server's state directory". Drop the row's "No file system".
  - Add a `journal-jsonl` row to the facade feature table, under `journal`.
  - The crate README states decisions 3 and 6 above.

**Interfaces:**
- Consumes:
  - `common::journal_with_store(...)`, which already exists in `tests/common/mod.rs`, with its `set` operation, `gui()` and `u1()`.
  - `JsonlStore::open`.

- [ ] **Step 1: Write the failing tests** in `tests/jsonl_journal.rs`.
  - `history_survives_a_restart`:
    - Build a journal over `JsonlStore::open(dir)`, submit `set("a", 1)` and `wait`.
    - Drop the journal. Wait for the store to be released by reopening it in a short retry loop: the worker thread owns it. Bound the loop at 2 s.
    - Build a journal again over the same dir. `history(None, 10)` has the batch with one `Done` op.
    - `undo_batch(id, &gui(), &u1())` then `wait` sets `a` back to `None` in the new ctx. Seed the new ctx with `a = 1` first, so the undo has something to undo.
  - `an_interrupted_batch_stays_undoable`:
    - Write by hand, through a `JsonlStore`: a begin dated `now`, with origin `gui()` and actor `u1()`, plus one `Done` op of kind `test.set`, with plan `{"key":"a","to":1}` (shaped like `SetPlan`) and value `json!({"key": "a", "from": null, "to": 1})`, which is what `tests/run.rs` shows `Set` leaving. Write no end.
    - Reopen under a journal, with the ctx seeded with the op's `to`.
    - `status(id).summary.interrupted` is true, and `undo_batch(id, …)` succeeds and restores the old value.

- [ ] **Step 2: Run the tests.** `cargo test -p corundum-framework-journal --features jsonl --test jsonl_journal`. Expected: both pass, unless the journal mishandles a reopened store. If they pass at once, there is no production change to make, and the tests are the deliverable. Say so in the report.

- [ ] **Step 3: Wire the facade, CI and the docs** as listed under Files.

- [ ] **Step 4: Verify.**
  - `cargo test --workspace --all-features` passes.
  - `cargo clippy -p corundum-framework --all-targets --features journal-jsonl --locked -- -D warnings` is clean.
  - The Global Constraints checks pass.
  - If the repo has an i18n check or a README-table check, run it too.

- [ ] **Step 5: Commit.**

```bash
git commit -am "feat(corundum-framework): journal-jsonl, and the journal over JsonlStore across a restart"
```
