# Workspace changes on the journal (step 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `corundum-framework-fs` moves, renames, makes folders and trashes entries through `fs.apply`, records each request as one batch in `corundum-framework-journal`, and serves `journal.undo`, `redo`, `undo_batch`, `history` and `status`, with the app's `route`, `follow` and `may_undo` hooks.

**Architecture:** Task 1 first replaces path-based resolution with a walk of directory handles (cap-std), so no write is built on a path a symlink swap can redirect. The built-in kinds (`fs.move`, `fs.mkdir`, `fs.trash`, `fs.deny`) are `Operation<FsCtx<A>>`s in the journal's registry, next to the app's own kinds. `FsService` routes each requested `FsOp` through the app, submits the batch, waits for it, and answers with the batch's records converted to `fs-api`'s mirror types.

**Tech Stack:** Rust 2024 (toolchain 1.99.0), cap-std 4 / cap-fs-ext 4, rustix 1 (unix), blake3 1, serde, tokio `spawn_blocking`.

**Spec:** `docs/superpowers/specs/2026-11-06-workspace-fs-journal-design.md` ("Changes (logged)", "The app's hook", "Mounting", "Errors", "Testing"; "Order of work" step 3).

## Global Constraints

- CI must stay green on ubuntu, ubuntu-arm, macOS and Windows: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `cargo test --workspace`. Tests that need symlinks or FIFOs are `#[cfg(unix)]`; a test that a file system may refuse (non-UTF-8 names, case) skips itself rather than failing.
- Symlinks are listed and never followed. A path through a symlink, out of the root, or naming a hidden entry is `invalid_path`.
- Every dot-name and every path `WorkspaceApp::hidden` names is hidden, also when judged on the name as the disk spells it. `fs.*` never reaches a hidden entry; the app's own kinds do (through `Access::App`, Task 4).
- A move never replaces an existing entry (`exists`). A move to another device is skipped with `io`.
- An operation inside a batch is never a request error: it is done or skipped with its reason.
- Error codes of corundum are in `-32199..=-32100`; JSON-RPC's own (`-32601`, `-32602`, `-32603`) keep their meaning.
- `fs.apply` takes at most 10 000 operations (`too_large` beyond).
- New dependencies, and only these: `cap-std = "4"`, `cap-fs-ext = "4"`, `blake3 = "1"`, `grain-id = "0.15"` (already the journal's) on `corundum-framework-fs`; `rustix = { version = "1", features = ["fs"] }` for `cfg(unix)`. All are under licenses `deny.toml` already allows.
- Crates are versioned in release-plz's `corundum` group (0.11.0, path dependencies). A breaking change to a public API is a `feat(<crate>)!:` or `fix(<crate>)!:` commit.
- Commit messages: conventional, scoped by crate (`feat(corundum-framework-fs): …`), ending with the attribution line the session gives.
- Doc comments on every public item (`#![warn(missing_docs)]`), in the crate's plain style.

## Review Focus

1. **A folder moved into itself or its own subfolder** (`a` → `a/b`): skipped with `invalid_path`, nothing moved. Test in Task 4.
2. **Undo after the entry changed again** (`a` moved to `b`, then a new `a` made by someone else): the inverse move is skipped with `exists`, and neither `a` nor `b` is overwritten. Test in Task 6.
3. **Undo of a trash when something now stands at the path**: the restore is skipped with `exists`, and the trashed entry is still in the trash, not lost. Test in Task 5.
4. **Case-only rename where the other spelling is a separate file** (a case-sensitive file system with both `A.txt` and `a.txt`): skipped with `exists`, `A.txt` keeps its name, and no temporary name is left behind. Test in Task 4.
5. **An empty `fs.apply`, or one over 10 000 operations**: refused (`invalid_params`, `too_large`) with nothing recorded, so Ctrl+Z still undoes the person's last real change. Test in Task 6.

---

### Task 1: Resolve paths by walking directory handles

The step 1 code resolves a path by `symlink_metadata` on each prefix, then opens the full path: a folder in the middle swapped for a symlink in between redirects the open. Walking `Dir` handles with no-follow opens closes that for every later use of the handle.

**Files:**
- Modify: `crates/corundum-framework-fs/Cargo.toml` (add `cap-std`, `cap-fs-ext`)
- Modify: `crates/corundum-framework-fs/src/workspace.rs`
- Modify: `crates/corundum-framework-fs/src/list.rs`, `src/read.rs`
- Modify: `crates/corundum-framework-fs/src/lib.rs` (export `Resolved`)
- Test: `crates/corundum-framework-fs/tests/resolve.rs`, `tests/read.rs`

**Interfaces:**
- Produces:
  - `Workspace` holds the root as a `cap_std::fs::Dir` (opened once, with `ambient_authority()`) besides the canonical `PathBuf`.
  - `pub struct Resolved` with `pub fn path(&self) -> &WsPath`, `pub fn metadata(&self) -> &cap_std::fs::Metadata` (the last segment's own), `pub fn open_dir(&self) -> Result<cap_std::fs::Dir, FsError>` (no-follow; `invalid_path` if not a folder), `pub fn open_file(&self) -> Result<cap_std::fs::File, FsError>` (no-follow, non-blocking, `invalid_path` unless a regular file), and `pub(crate) fn parent(&self) -> &Dir` / `pub(crate) fn name(&self) -> Option<&str>` (`None` for the root, whose `parent()` is the root itself).
  - `Workspace::resolve(&self, app: &dyn WorkspaceApp, path: &WsPath) -> Result<Resolved, FsError>` replaces the `(PathBuf, Metadata)` form. Same refusals as now.
  - `pub(crate) fn walk_parent(&self, path: &WsPath) -> Result<(Dir, Option<String>), FsError>`: opens each segment but the last with `cap_fs_ext::DirExt::open_dir_nofollow`; a symlink segment is `invalid_path`, a missing or non-folder one `not_found`. Task 4 builds on it.
  - `entry_for(name, &cap_std::fs::Metadata)` (the metadata type changes).

- [ ] **Step 1: Write the failing tests** in `tests/resolve.rs` (unix only):

```rust
#[cfg(unix)]
#[test]
fn a_middle_folder_swapped_for_a_symlink_after_resolve_is_not_followed() {
    let (dir, ws) = workspace(&["a/b/f"]);           // f holds "inside" (see common::workspace)
    std::fs::write(dir.path().join("a/b/f"), b"inside").unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(outside.path().join("b")).unwrap();
    std::fs::write(outside.path().join("b/f"), b"secret").unwrap();
    let resolved = ws.resolve(&(), &WsPath::parse("a/b/f").unwrap()).unwrap();
    std::fs::rename(dir.path().join("a"), dir.path().join("a_old")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("a")).unwrap();
    let mut got = String::new();
    std::io::Read::read_to_string(&mut resolved.open_file().unwrap(), &mut got).unwrap();
    assert_eq!(got, "inside");
}

#[cfg(unix)]
#[test]
fn a_symlinked_middle_folder_is_refused() {
    // a -> outside dir; resolve("a/f") is InvalidPath, as today.
}
```

and in `tests/read.rs` keep every existing test passing unchanged (they pin the refusals).

- [ ] **Step 2: Run them to see the first fail**

Run: `cargo test -p corundum-framework-fs --test resolve`
Expected: FAIL to compile (`resolve` returns a tuple; no `open_file`).

- [ ] **Step 3: Implement** `Resolved`, the handle walk, and move `list`, `stat`, `read` onto it: `list` reads `resolved.open_dir()?.entries()` and stats each page entry with `dir.symlink_metadata(name)`; `read` uses `resolved.open_file()` (cap-std's `OpenOptions` with `cap_fs_ext::OpenOptionsFollowExt::follow(FollowSymlinks::No)`, plus `O_NONBLOCK` on unix). Keep the on-disk hiding check (`on_disk`) as it is: it judges names, and the walk refuses any symlink it would meet. Delete `open_regular` and its unix tests once `open_file` carries the same two tests (symlink in the last segment, FIFO). Drop `libc` if nothing uses it any more.

- [ ] **Step 4: Run the crate's tests**

Run: `cargo test -p corundum-framework-fs`
Expected: PASS, all of step 1's tests too.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-fs Cargo.lock
git commit -m "feat(corundum-framework-fs)!: resolve paths by walking directory handles, so a swapped-in symlink is never followed"
```

---

### Task 2: Journal: a child depth limit, trimmed stacks, and a gone history cursor

Three deferred findings from step 2 that step 3 makes reachable: `follow` returns children (unbounded nesting overflows the worker's stack), every GUI session adds an origin whose stacks never shrink, and `journal.history` paging cannot tell its end from a cursor the store dropped.

**Files:**
- Modify: `crates/corundum-framework-journal/src/journal.rs`, `src/lib.rs`
- Test: `crates/corundum-framework-journal/tests/run.rs`, `tests/undo.rs`, `tests/history.rs` (create)

**Interfaces:**
- Produces:
  - `pub const MAX_CHILD_DEPTH: usize = 16;` exported from the crate root. An operation at depth 16 (the batch's own operations are depth 0) that returns children: each such child is recorded as `Skipped(Skip::InvalidPlan("children nest deeper than 16"))`, and its children are not looked at.
  - `Journal::history(cursor, limit)`: when `cursor` names a batch the store no longer has, `Err(JournalError::NotFound(cursor))`. A cursor at the oldest kept batch still gives `Ok(vec![])`.
  - Stacks are trimmed on every `submit`, `undo`, `redo`: for every origin, entries from the bottom of each stack are dropped while the store no longer has them or they are older than `UndoPolicy::max_age`; an origin with both stacks empty is removed from the map.

- [ ] **Step 1: Write the failing tests**

```rust
// tests/run.rs — a fixture kind whose apply returns one child of itself, forever.
#[test]
fn children_past_the_depth_limit_are_skipped() {
    let j = journal_with(Recurse);                 // common: register Recurse
    let b = j.submit(vec![recurse()], &origin("o"), &person(), None).unwrap();
    let r = j.wait(b).unwrap();
    assert_eq!(r.summary.done as usize, MAX_CHILD_DEPTH + 1);
    assert_eq!(r.summary.skipped, 1);
    assert!(matches!(&r.ops.last().unwrap().outcome,
        Outcome::Skipped(Skip::InvalidPlan(m)) if m.contains("16")));
}

// tests/history.rs
#[test]
fn a_cursor_the_store_dropped_is_not_found() {
    let j = journal_with_store(MemoryStore::new(2));
    let first = submit_and_wait(&j, "o");
    submit_and_wait(&j, "o");
    submit_and_wait(&j, "o");                       // `first` is dropped
    assert!(matches!(j.history(Some(first), 10), Err(JournalError::NotFound(id)) if id == first));
}

#[test]
fn the_oldest_kept_cursor_is_the_end() {
    let j = journal_with_store(MemoryStore::new(2));
    submit_and_wait(&j, "o");
    let page = j.history(None, 10).unwrap();
    assert_eq!(j.history(Some(page.last().unwrap().begin.batch_id), 10).unwrap(), vec![]);
}

// tests/undo.rs
#[test]
fn an_origin_whose_batches_left_the_store_no_longer_holds_them() {
    // MemoryStore::new(1): submit on "a", then on "b"; "a"'s batch is dropped.
    // undo("a") is NothingToUndo (not Expired), because the trimmed stack is empty.
}
```

- [ ] **Step 2: Run them**

Run: `cargo test -p corundum-framework-journal`
Expected: the three history and undo tests FAIL on their assertions. `children_past_the_depth_limit_are_skipped` aborts the `run` test binary with a stack overflow: that abort is its failure, so run it alone (`cargo test -p corundum-framework-journal --test run children_past`) to see it.

- [ ] **Step 3: Implement**: `run_op` takes a `depth: usize`; `history` checks `store.batch(cursor)?` only when the page comes back empty; a private `State::trim(&mut self, now: i64, max_age_ms: i64)` called under the lock in `submit` and `compensate_top`.

- [ ] **Step 4: Run the crate's tests**

Run: `cargo test -p corundum-framework-journal`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-journal
git commit -m "fix(corundum-framework-journal): limit child depth, trim the undo stacks, and refuse a history cursor that was dropped"
```

---

### Task 3: fs-api: the change and journal methods

**Files:**
- Modify: `crates/corundum-framework-fs-api/src/lib.rs` (split into `src/lib.rs`, `src/read.rs`, `src/change.rs`, `src/journal.rs` if it passes ~400 lines)
- Modify: `crates/corundum-framework-fs-api/Cargo.toml` (`serde_json` moves to `[dependencies]`: plans and done values are JSON)

**Interfaces:**
- Produces (all `Debug, Clone, PartialEq, Serialize, Deserialize`):
  - Method names: `FS_APPLY = "fs.apply"`, `JOURNAL_UNDO = "journal.undo"`, `JOURNAL_REDO = "journal.redo"`, `JOURNAL_UNDO_BATCH = "journal.undo_batch"`, `JOURNAL_HISTORY = "journal.history"`, `JOURNAL_STATUS = "journal.status"`.
  - `pub const MAX_APPLY_OPS: usize = 10_000;` and `pub const DEFAULT_HISTORY_LIMIT: u32 = 50;`, `pub const MAX_HISTORY_LIMIT: u32 = 200;`.
  - `pub struct Expect { pub size: u64, pub modified: Option<i64>, #[serde(default, skip_serializing_if = "Option::is_none")] pub hash: Option<String> }` — the `size` and `modified` an `Entry` showed, and optionally the file's `file/1` hash, `blake3:` and 64 lowercase hex digits of BLAKE3 over the whole file.
  - `#[serde(tag = "kind")] pub enum FsOp { #[serde(rename = "fs.move")] Move { from: String, to: String, #[serde(default)] expect: Option<Expect> }, #[serde(rename = "fs.mkdir")] Mkdir { path: String }, #[serde(rename = "fs.trash")] Trash { path: String, #[serde(default)] expect: Option<Expect> } }`.
  - `pub struct ApplyParams { pub ops: Vec<FsOp>, pub origin: String, #[serde(default)] pub label: Option<String>, #[serde(default)] pub dry_run: bool }`. (`origin` is not in the spec's `fs.apply { ops, label, dry_run }`; it is needed, because undo is per origin.)
  - `pub struct OriginParams { pub origin: String }` (undo, redo), `pub struct UndoBatchParams { pub batch_id: String, pub origin: String }`, `pub struct HistoryParams { #[serde(default)] pub cursor: Option<String>, #[serde(default)] pub limit: Option<u32> }`, `pub struct StatusParams { pub batch_id: String }`.
  - Mirrors of the journal's JSON, field for field: `Actor { principal, agent }`, `BatchBegin { batch_id, origin, actor, at, label, undo_of, redo_of }` (ids as `String`), `BatchSummary { begin, done, skipped, complete, interrupted, undone_by }`, `Skip { reason: String, #[serde(default)] message: Option<String> }`, `#[serde(tag = "outcome", content = "value", rename_all = "snake_case")] enum Outcome { Done(Value), Skipped(Skip) }`, `OpRecord { op_id, batch_id, parent, kind, plan: Value, outcome }`, `BatchRecords { summary, ops }`. `Skip.reason` stays a string so a client tolerates a reason it does not know.
  - Results: `fs.apply` → `BatchRecords`, or with `dry_run` → `DryRun { checks: Vec<Check> }` with `Check { kind: String, plan: Value, #[serde(default)] skip: Option<Skip> }`; `journal.undo` / `redo` / `undo_batch` → `BatchRecords` of the compensating batch; `journal.history` → `History { batches: Vec<BatchSummary>, next_cursor: Option<String> }`; `journal.status` → `BatchRecords`.
  - Codes: `NOTHING_TO_UNDO = -32104`, `NOTHING_TO_REDO = -32105`, `ALREADY_UNDONE = -32106`, `EXPIRED = -32107`, `FORBIDDEN = -32108`, `BUSY = -32109`.

- [ ] **Step 1: Write the failing tests** (extend the crate's `tests` module):

```rust
#[test]
fn fs_ops_are_tagged_by_kind() {
    let op: FsOp = serde_json::from_value(json!({"kind": "fs.move", "from": "a", "to": "b"})).unwrap();
    assert_eq!(op, FsOp::Move { from: "a".into(), to: "b".into(), expect: None });
    assert_eq!(serde_json::to_value(FsOp::Mkdir { path: "d".into() }).unwrap(),
               json!({"kind": "fs.mkdir", "path": "d"}));
}

#[test]
fn an_unknown_skip_reason_still_parses() {
    let o: Outcome = serde_json::from_value(json!({"outcome": "skipped", "value": {"reason": "later_reason"}})).unwrap();
    assert_eq!(o, Outcome::Skipped(Skip { reason: "later_reason".into(), message: None }));
}
```

plus every new type in `every_type_round_trips`, the new names in `method_names_are_namespaced` (`fs.` or `journal.`), and the new codes in `codes_stay_in_their_range` with a check that all codes are distinct.

- [ ] **Step 2: Run them**

Run: `cargo test -p corundum-framework-fs-api`
Expected: FAIL to compile (no `FsOp`).

- [ ] **Step 3: Implement** the types and constants. `API_VERSION` stays 1: v1 was never served with writes, and nothing is removed.

- [ ] **Step 4: Run them**

Run: `cargo test -p corundum-framework-fs-api`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-fs-api Cargo.lock
git commit -m "feat(corundum-framework-fs-api): fs.apply and the journal.* methods"
```

---

### Task 4: The engine's write primitives, `fs.mkdir` and `fs.move`

**Files:**
- Modify: `crates/corundum-framework-fs/Cargo.toml` (add `corundum-framework-journal` path dependency, `blake3`, unix `rustix`)
- Create: `crates/corundum-framework-fs/src/ctx.rs` (`FsCtx`), `src/access.rs` (`Access`, the engine's resolve-for-write), `src/rename.rs` (no-replace rename), `src/expect.rs`, `src/ops/mod.rs`, `src/ops/mkdir.rs`, `src/ops/move_.rs`
- Modify: `crates/corundum-framework-fs/src/lib.rs`, `src/path.rs` (serde for `WsPath`)
- Test: `crates/corundum-framework-fs/tests/move.rs`, `tests/mkdir.rs` (create), `tests/common/mod.rs` (a `journal(ws, app)` helper running the built-in kinds over a `MemoryStore`)

**Interfaces:**
- Consumes: Task 1's `Workspace::walk_parent`, `Resolved`; the journal's `Operation`, `OpKind`, `Applied`, `Skip`, `Registry`.
- Produces:
  - `pub struct FsCtx<A: WorkspaceApp> { /* Arc<Workspace>, Arc<A> */ }` with `pub fn new(ws: Arc<Workspace>, app: Arc<A>) -> FsCtx<A>`, `pub fn workspace(&self) -> &Workspace` and `pub fn app(&self) -> &A`. The journal's context for the built-in kinds and for the app's own kinds.
  - `pub enum Access { Visible, App }`: `Visible` refuses hidden paths (what `fs.*` uses); `App` reaches hidden entries, still inside the root and never through a symlink (what the app's kinds use, to move a sidecar).
  - On `Workspace`: `pub fn entry(&self, app: &dyn WorkspaceApp, path: &WsPath, access: Access) -> Result<Resolved, Skip>`; `pub fn rename(&self, app: &dyn WorkspaceApp, from: &WsPath, to: &WsPath, access: Access) -> Result<(), Skip>`; `pub fn make_dir(&self, app, path, access) -> Result<(), Skip>`; `pub fn remove_empty_dir(&self, app, path, access) -> Result<(), Skip>`. These are the primitives the built-in kinds and an app's kinds share.
  - `WsPath: Serialize + Deserialize` (as its string; deserializing runs `WsPath::parse`).
  - `pub fn register_builtin<A: WorkspaceApp>(registry: &mut Registry<FsCtx<A>>)` registering `fs.move` and `fs.mkdir` here, `fs.trash` (Task 5) and `fs.deny` (Task 7) later.
  - Plans and results (`pub`, in `ops`): `MovePlan { from: WsPath, to: WsPath, expect: Option<Expect> }`, `MoveDone { from: WsPath, to: WsPath, size: u64, modified: Option<i64> }` (the entry's state after the move, at `to`); `#[serde(tag = "do", rename_all = "snake_case")] enum MkdirPlan { Make { path }, Remove { path } }` and `enum MkdirDone { Made { path }, Removed { path } }`.
  - `pub enum FsDone { Moved { from: WsPath, to: WsPath }, Made { path: WsPath }, Trashed { path: WsPath, trash_id: String } }` (Task 7's `follow` receives it; Task 5 adds the `Trashed` value).

Rules each kind enforces in `apply` (and `check`, which does everything but the change):

| Condition | Skip |
|---|---|
| `from` is the root, or `to` is `from` or under it (`a` → `a/b`) | `invalid_path` |
| either path hidden (with `Access::Visible`), or through a symlink | `invalid_path` |
| `from` missing; `to`'s parent missing or not a folder | `not_found` |
| `expect` given and the entry's `size` or `modified` differs, or `hash` given and the entry is not a file or its `file/1` hash differs | `precondition_failed` |
| anything at `to` (checked atomically, see `rename.rs`) | `exists` |
| `to` on another device (`io::ErrorKind::CrossesDevices`) | `io("another device")` |
| `Make` where anything exists | `exists` |
| `Remove` where the folder is not empty | `precondition_failed` |

- `rename.rs`: `pub(crate) fn rename_noreplace(from_dir: &Dir, from: &str, to_dir: &Dir, to: &str) -> io::Result<()>`. On Linux and Apple, `rustix::fs::renameat_with(…, RenameFlags::NOREPLACE)`; on Linux, an `EINVAL` (a file system without the flag) falls back. The fallback, and the only path on Windows and other unixes: `to_dir.symlink_metadata(to)` must be `NotFound` (else `AlreadyExists`), then `from_dir.rename(from, to_dir, to)`. Document the fallback's window.
- A case-only rename (same parent, `from.name().to_lowercase() == to.name().to_lowercase()`, names differ) goes through a temporary dot-name in that folder, `.<to name>.corundum-<GrainId::random()>`: `from` → temp, temp → `to` with no-replace; if the second step fails, temp goes back to `from`, and the skip is the second step's. This is the same on every platform, so it needs no detection of case-insensitivity.
- `MoveOp::inverse(done)` = `MovePlan { from: done.to, to: done.from, expect: Some(Expect { size: done.size, modified: done.modified, hash: None }) }`. `MkdirOp::inverse`: `Made` → `Remove`, `Removed` → `Make`.
- `apply` of `Move` and `Make` calls no app hook yet (Task 7 adds `follow`).

- [ ] **Step 1: Write the failing tests** (`tests/move.rs`, `tests/mkdir.rs`, through a `Journal<FsCtx<()>>` from `common::journal`):

```rust
#[test] fn a_move_renames_and_its_undo_moves_back()           // a.txt → b.txt; undo → a.txt again, contents kept
#[test] fn a_move_never_replaces()                            // b.txt exists → skipped Exists; both files unchanged
#[test] fn a_folder_moves_with_everything_in_it()             // d/x → e/x after d → e
#[test] fn a_folder_cannot_move_into_itself()                 // Review Focus 1: d → d/sub skipped InvalidPath; d intact
#[test] fn a_changed_file_is_not_moved()                      // expect.size wrong → PreconditionFailed
#[test] fn a_hash_that_differs_is_not_moved()                 // expect.hash = blake3 of other bytes → PreconditionFailed
#[test] fn a_matching_hash_moves()                            // expect.hash = "blake3:" + blake3::hash(b"hello").to_hex()
#[test] fn a_hidden_source_or_destination_is_refused()        // ".x" → "y" and "y" → ".x": InvalidPath
#[test] fn a_missing_destination_folder_is_not_found()        // a.txt → nope/b.txt
#[test] fn a_case_only_rename_changes_the_case()              // A.txt → a.txt: listing shows "a.txt" only
#[test] fn a_case_only_rename_onto_a_separate_file_is_refused()
    // Review Focus 4: needs A.txt and a.txt to coexist; skip the test where the file system refuses the second.
    // Expect Exists; listing is exactly ["A.txt", "a.txt"]; no ".a.txt.corundum-*" left (read_dir the raw folder).
#[cfg(unix)] #[test] fn a_symlink_moves_as_itself()           // link → link2; link2 is still a symlink to the same target
#[test] fn mkdir_makes_and_its_undo_removes_an_empty_folder()
#[test] fn mkdir_undo_keeps_a_folder_that_is_no_longer_empty() // skipped PreconditionFailed; folder and file stay
#[test] fn mkdir_on_an_existing_entry_is_exists()
#[test] fn the_app_reaches_hidden_entries_with_app_access()   // Workspace::rename(.., Access::App) moves "p.jpg.xmp" for an app that hides *.xmp
```

Each asserts the op's `Outcome` (`Skipped(Skip::…)` or `Done`) and the folder's state on disk.

- [ ] **Step 2: Run them**

Run: `cargo test -p corundum-framework-fs --test move --test mkdir`
Expected: FAIL to compile (no `FsCtx`, no `register_builtin`).

- [ ] **Step 3: Implement** the files above.

- [ ] **Step 4: Run the crate's tests**

Run: `cargo test -p corundum-framework-fs`
Expected: PASS (a case test that skipped itself prints why).

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-fs Cargo.lock
git commit -m "feat(corundum-framework-fs): fs.move and fs.mkdir, undoable, never replacing"
```

---

### Task 5: `fs.trash`

**Files:**
- Create: `crates/corundum-framework-fs/src/ops/trash.rs`
- Modify: `crates/corundum-framework-fs/src/app.rs` (`own_folder`), `src/ops/mod.rs`
- Test: `crates/corundum-framework-fs/tests/trash.rs` (create)

**Interfaces:**
- Consumes: Task 4's primitives (`Access::App` for the trash side), `FsCtx`, `Expect`.
- Produces:
  - `WorkspaceApp::own_folder(&self) -> &str`, default `".corundum"`: the app's hidden folder at the root (asteria returns `".asteria"`). `FsCtx::new` panics if it is not a single dot-name segment.
  - `#[serde(tag = "do", rename_all = "snake_case")] enum TrashPlan { Trash { path: WsPath, expect: Option<Expect> }, Restore { path: WsPath, trash_id: String } }`; `enum TrashDone { Trashed { path, trash_id, size, modified }, Restored { path, trash_id, size, modified } }`.
  - `Trash` makes a fresh trash id (`grain_id::GrainId::random()` as a string), creates `<own>/trash/<trash_id>/<parent of path>` without following symlinks, and moves the entry to `<own>/trash/<trash_id>/<path>`. The id goes in `Trashed`, because `apply` is not told its op id.
  - `Restore` moves it back with no-replace (`exists` if anything is at `path`; `not_found` if `path`'s parent is gone), then removes the emptied folders under `<own>/trash/<trash_id>` (best effort).
  - `inverse`: `Trashed` → `Restore { path, trash_id }`; `Restored` → `Trash { path, expect: Some(after-state) }`.
  - Trashing the root or a hidden path is `invalid_path`. Nothing empties the trash in this step.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn trash_moves_the_entry_into_the_apps_folder()      // a/b.txt gone from the listing; .corundum/trash/<id>/a/b.txt exists with its bytes
#[test] fn the_undo_of_a_trash_puts_it_back()
#[test] fn the_undo_of_a_trash_keeps_what_now_stands_there()
    // Review Focus 3: trash a.txt, write a new a.txt, undo → skipped Exists;
    // the new a.txt is unchanged and the trashed copy is still under .corundum/trash/<id>/a.txt
#[test] fn the_redo_of_a_trash_trashes_it_again()
#[test] fn a_changed_file_is_not_trashed()                    // expect mismatch → PreconditionFailed
#[test] fn an_apps_own_folder_holds_its_trash()               // an app with own_folder ".asteria"
#[test] fn the_trash_is_never_listed()                        // fs.list "" after a trash shows no ".corundum"
```

- [ ] **Step 2: Run them**

Run: `cargo test -p corundum-framework-fs --test trash`
Expected: FAIL to compile.

- [ ] **Step 3: Implement** `ops/trash.rs` and `own_folder`; register `fs.trash` in `register_builtin`.

- [ ] **Step 4: Run the crate's tests**

Run: `cargo test -p corundum-framework-fs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-fs Cargo.lock
git commit -m "feat(corundum-framework-fs): fs.trash, into the app's hidden folder, and its undo"
```

---

### Task 6: `FsService` with the journal: `fs.apply` and `journal.*`

**Files:**
- Modify: `crates/corundum-framework-fs/src/service.rs`, `src/error.rs`, `src/lib.rs`
- Create: `crates/corundum-framework-fs/src/wire.rs` (journal types → `fs-api` mirrors)
- Modify: `crates/corundum-framework-fs/README.md`, `Cargo.toml` `description` (it now changes files too); the facade's README rows for `fs` in `README.md` and `README.ja.md`
- Test: `crates/corundum-framework-fs/tests/dispatch.rs`, `tests/apply.rs` (create)

**Interfaces:**
- Consumes: Tasks 3–5; `Journal`, `JournalConfig`, `JournalStore`, `MemoryStore`, `Actor`, `Origin`.
- Produces:
  - `FsService<A>` holds the workspace, the app and a `Journal<FsCtx<A>>`; clones share all three.
  - `FsService::new(ws: Workspace, app: A) -> FsService<A>`: built-in kinds only, `MemoryStore::default()`, `JournalConfig::default()`.
  - `FsService::with_journal(ws: Workspace, app: A, kinds: Registry<FsCtx<A>>, store: impl JournalStore + 'static, config: JournalConfig) -> FsService<A>`: the app's kinds, to which the service adds the built-in ones (an app kind named `fs.*` panics, as any double registration does).
  - `pub fn journal(&self) -> &Journal<FsCtx<A>>`, for an app's own batches (asteria's tag writes).
  - `pub async fn dispatch(&self, method: &str, params: Value, actor: &Actor) -> Result<Value, FsError>` (breaking: the actor is new; the read methods ignore it).
  - `pub use corundum_framework_journal as journal;` at the crate root.
  - `FsError::Journal(JournalError)` and `FsError::code` mapping: `NothingToUndo` → `NOTHING_TO_UNDO`, `NothingToRedo` → `NOTHING_TO_REDO`, `AlreadyUndone` → `ALREADY_UNDONE`, `Expired` → `EXPIRED`, `Forbidden` → `FORBIDDEN`, `Busy` → `BUSY`, `NotFound` → `NOT_FOUND`, and `UnknownKind`, `InvalidPlan`, `Stopped`, `Store` (and any later variant) → `INTERNAL_ERROR`.
- Behavior:
  - `fs.apply`: refuses `ops` empty (`invalid_params`: an empty batch would take a Ctrl+Z) or longer than `MAX_APPLY_OPS` (`too_large`), and a path that does not parse as a `WsPath` (`invalid_path`), all before anything is recorded. Otherwise maps each `FsOp` to its built-in `Planned`, then on a blocking thread either `journal.dry_run` (→ `DryRun`) or `journal.submit` then `journal.wait` (→ `BatchRecords`).
  - `journal.undo` / `redo` / `undo_batch`: `Journal::undo(&Origin::new(origin), actor)` and so on, then `wait`, → the compensation's `BatchRecords`.
  - `journal.history`: `limit` defaults to `DEFAULT_HISTORY_LIMIT` and is clamped to `1..=MAX_HISTORY_LIMIT`; `next_cursor` is the last batch's id when the page is full.
  - `journal.status`: `Journal::status`.
  - A `batch_id` or `cursor` that does not parse is `invalid_params`.
  - `wire.rs` converts by serializing the journal's value and deserializing the `fs-api` mirror; a failure there is `Io` (internal).

- [ ] **Step 1: Write the failing tests** (`tests/apply.rs`, over `FsService<()>` on a temp dir, `Actor::person("p")`):

```rust
#[tokio::test] async fn apply_moves_and_answers_each_ops_result()        // BatchRecords: 1 op, Outcome::Done, summary.complete
#[tokio::test] async fn one_apply_is_one_undo()                         // apply [move a→b, mkdir d]; journal.undo → both reversed
#[tokio::test] async fn undo_after_the_place_was_taken_again_replaces_nothing()
    // Review Focus 2: move a→b; write a new "a"; journal.undo → op skipped {"reason":"exists"}; both files unchanged
#[tokio::test] async fn undo_is_per_origin()                            // apply on "gui" and on "mcp"; undo("gui") reverses gui's only
#[tokio::test] async fn redo_after_undo()                               // then journal.redo moves it again
#[tokio::test] async fn undo_batch_from_the_history_screen()           // undo_batch of mcp's batch, from origin "history"
#[tokio::test] async fn an_empty_or_huge_apply_is_refused_and_not_recorded()
    // Review Focus 5: ops [] → code -32602; 10_001 mkdirs → code TOO_LARGE; then journal.undo → code NOTHING_TO_UNDO
#[tokio::test] async fn a_dry_run_changes_nothing()                     // dry_run: move onto an existing name → checks[0].skip.reason "exists"; disk unchanged; history empty
#[tokio::test] async fn history_pages_newest_first()                   // 3 applies, limit 2 → 2 batches + next_cursor; next page → 1, no cursor
#[tokio::test] async fn journal_errors_have_their_codes()              // nothing_to_undo, nothing_to_redo, already_undone (undo_batch twice), not_found (status of a random id)
```

and in `tests/dispatch.rs`, pass `&Actor::person("p")` to every call; keep the `MethodNotFound` test.

- [ ] **Step 2: Run them**

Run: `cargo test -p corundum-framework-fs --test apply --test dispatch`
Expected: FAIL to compile (`dispatch` takes two arguments).

- [ ] **Step 3: Implement** the service, the error mapping and `wire.rs`; update the README and the description.

- [ ] **Step 4: Run the workspace's tests**

Run: `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-fs README.md README.ja.md Cargo.lock
git commit -m "feat(corundum-framework-fs)!: fs.apply and the journal.* methods, with the caller's actor"
```

---

### Task 7: The app's hooks: `route`, `follow`, `may_undo`, and `fs.deny`

**Files:**
- Modify: `crates/corundum-framework-fs/src/app.rs`, `src/service.rs` (routing), `src/ops/{mod,move_,mkdir,trash}.rs` (`follow`)
- Create: `crates/corundum-framework-fs/src/ops/deny.rs`
- Test: `crates/corundum-framework-fs/tests/hooks.rs` (create)

**Interfaces:**
- Consumes: Task 6's service; Task 4's `FsDone`, `Access::App`.
- Produces, on `WorkspaceApp` (all with defaults, so `()` still works):
  - `fn route(&self, op: &FsOp) -> Route { Route::Fs }` with `pub enum Route { Fs, Replace(Vec<Planned>), Deny(String) }`. For a folder, `op` names the folder.
  - `fn follow(&self, op: &FsOp, done: &FsDone) -> Vec<Planned> { Vec::new() }`.
  - `fn may_undo(&self, actor: &Actor, batch: &BatchBegin) -> bool { actor.principal == batch.actor.principal }`. The service sets `JournalConfig::may_undo` to call it, replacing whatever the config held (document this on `with_journal`).
  - Built-in kind `fs.deny` with `DenyPlan { op: FsOp, reason: String }`: `check` and `apply` skip with `Skip::Denied`; `inverse` is `None`. The reason is in the recorded plan, so the history shows it.
- Behavior:
  - `fs.apply` asks `route` once per requested op, before submitting, on the blocking thread: `Fs` → the built-in `Planned`; `Replace(v)` → `v`, in place, in order; `Deny(reason)` → `fs.deny`. An op on a hidden path is not routed: it goes to its built-in kind, which skips it with `invalid_path`.
  - Each built-in `apply` that changes something (`Move`, `Make`, `Trash`; not the inverse-only `Remove` and `Restore`) returns `ctx.app().follow(&op, &done)` as its children, `op` being the `FsOp` it carried out. In a compensation the journal drops them (spec, decision of 2026-11-06); the app's follow-up kinds must be undoable children whose inverse redoes their work.

- [ ] **Step 1: Write the failing tests** with a fake app in `tests/hooks.rs`: it hides `*.xmp`, registers a kind `test.move_sidecar` (plan `{ from, to }`, applying `Workspace::rename(.., Access::App)`, inverse the reverse move), follows each `fs.move` of `x.jpg` with a sidecar move of `x.jpg.xmp`, routes `fs.trash` of a `*.album` to `Replace` with one `test.archive { path }` (an app kind that moves the album under `archive/` with `Access::App`, its inverse moving it back), and denies anything under `locked/`.

```rust
#[tokio::test] async fn a_sidecar_moves_with_its_photo()           // move p.jpg → q.jpg: q.jpg.xmp exists, p.jpg.xmp gone; ops[1].parent == Some(ops[0].op_id)
#[tokio::test] async fn the_undo_moves_the_sidecar_back_too()      // journal.undo: p.jpg and p.jpg.xmp back; the compensation has 2 ops, no extra children
#[tokio::test] async fn a_replaced_op_runs_the_apps_kinds_instead() // ops[0].kind is the app's kind, not "fs.trash"
#[tokio::test] async fn a_denied_op_is_skipped_with_its_reason()   // locked/a: outcome skipped "denied"; plan.reason == the app's text; the rest of the batch runs
#[tokio::test] async fn a_hidden_path_is_never_routed()            // move p.jpg.xmp → z.xmp: InvalidPath, route not called (the fake counts calls)
#[tokio::test] async fn may_undo_decides_who_undoes()              // app forbids agents: Actor::agent("p") undo_batch → code FORBIDDEN; Actor::person("p") succeeds
#[tokio::test] async fn a_dry_run_reports_a_denial()               // dry_run → checks[0].skip.reason "denied"
```

- [ ] **Step 2: Run them**

Run: `cargo test -p corundum-framework-fs --test hooks`
Expected: FAIL to compile (no `route`).

- [ ] **Step 3: Implement** the hooks, routing and `fs.deny`.

- [ ] **Step 4: Run the gate**

Run: `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/corundum-framework-fs
git commit -m "feat(corundum-framework-fs): the app's route, follow and may_undo hooks"
```

---

## Not in this plan

- `api_mismatch` (the spec's error for a client whose API major differs): the client side, step 4, decides how the version is exchanged.
- Emptying the trash, and `JsonlStore` (step 5).
- Deferred journal minors not reached by this step: the `failed` map never shrinking, an `Expired` undo still adjusting the stacks, panics in user `Serialize`/`Drop`.
