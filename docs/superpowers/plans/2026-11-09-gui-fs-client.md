# The gui's fs-client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The gui's file explorer works a workspace through the `fs.*` and `journal.*` API, with Ctrl+Z, file previews read over the API, and a history screen. It does this from any transport an app plugs in.

**Architecture:**
- **Client:** `corundum-framework-gui` gets a feature `fs-client`, with a module `fs_client`. It holds:
  - a one-method transport (`FsTransport`) and a typed client (`FsClient`) that answer with `corundum_framework_platform::pending::Pending`;
  - a `Read + Seek` reader over `fs.read` (`FsReader`);
  - a driver that turns `FileExplorer` events into calls (`ExplorerDriver`);
  - the history screen (`HistoryPage`).
- **In-process transport:** `fs-local` adds a transport that calls an in-process `FsService`.
- **Server prerequisites** come first (Tasks 1–2):
  - the API version exchange;
  - an `undoable` flag in the history;
  - paths given in the disk's spelling to the app's `route`/`follow`.

**Tech Stack:** Rust 2024 (toolchain 1.99.0), egui 0.37 (headless tests via `egui::Context::run_ui`), tokio (rt), serde_json, base64 0.23.

**Spec:** `docs/superpowers/specs/2026-11-06-workspace-fs-journal-design.md` ("`corundum-framework-gui`, feature `fs-client`", "Errors", "Testing", "Order of work" step 4)

## Decisions this plan takes (the spec left them open)

- **`api_mismatch`:**
  - A new method `fs.version`, with no params, answers `{"api": API_VERSION}`.
  - `FsClient::check_version` calls it. The driver and the history page call it first and send nothing else until it answers.
  - If the server's `api` differs, or the server answers `method_not_found` (a server older than this), every later call of that client fails with `FsError::ApiMismatch { server, client }`. A server without the method counts as `server: 0`.
  - `codes::API_MISMATCH = -32110` is the code `FsError::ApiMismatch` reports. No server sends it: the server always answers `fs.version`.
- **The disk's spelling for `route`/`follow`:**
  - Before routing, `fs.apply` rewrites each path an op names into the disk's spelling where the entry exists.
  - For a move's `to`, that means its folder's spelling plus the requested last name.
  - The hidden check before routing, `route`, `follow` and the recorded plan all see that spelling.
  - On Linux this stays the request's spelling until the step-3 known limitation (`canonicalize` returns the asked spelling there) is fixed. The docs from step 3 are updated to say so.
- **HistoryPage's Undo enablement:**
  - The spec's "enabled when the batch is in the undo window and `may_undo` allows it" needs the server's answer.
  - `journal.history` gains, per batch, `undoable: bool` for the asking actor. It is computed with the journal's own undo check.
  - The JSON stays additive: `HistoryBatch` flattens `BatchSummary` and adds `undoable`.

## Global Constraints

- **The gate:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass. CI also runs macOS, Windows and ubuntu-arm.
- **Windows:** check cfg-dependent code with `cargo clippy --target x86_64-pc-windows-gnu -p <crate> --all-targets -- -D warnings`. Wine does not work on this machine.
- **Commits:** `feat(<crate>)!:` for a breaking change. Every commit ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- **Features:** `fs-client` depends on `corundum-framework-fs-api` only, never on the engine. `fs-local = ["fs-client", "dep:corundum-framework-fs", "dep:tokio"]`.
- **Threads:** the UI thread never blocks. Every call is a `Pending` polled each frame. Only `FsReader` waits, and it runs on a preview's worker thread.
- **Text:** every user-visible string goes through `tr!`, with an id `corundum-fs-*`, in both `locales/en/corundum.ftl` and `locales/ja/corundum.ftl`. The i18n check tests require the same keys in both. The Japanese is natural, in the existing file's style.
- **Spec values verbatim:**
  - notice: "Undid 'Move 3 items' (1 skipped)";
  - confirmation: "This change was made by the agent at 14:02. Undo it?";
  - `fs.read` chunk at most `MAX_READ_LEN` (1 MiB);
  - list pages of `DEFAULT_LIST_LIMIT`.
- **Origins:** an `Origin` is made once per `FsClient`, as `format!("gui:{}", grain_id::GrainId::random())`.

## Review Focus

1. **An answer for a folder the person has already left.** Navigating `a` then `b` before `a`'s listing arrives shows `b`'s entries, never `a`'s. Test: `a_late_listing_for_a_left_folder_is_dropped` (Task 5).
2. **Two Ctrl+Z in a row while the first undo is still in flight.** Each press undoes one more batch, in order: none is dropped or sent twice. Test: `two_quick_undos_undo_two_batches` (Task 6).
3. **Ctrl+Z while a rename field is open, or while the explorer is not focused.** It is the text field's own undo, or nothing: no `journal.undo` is sent. Test: `ctrl_z_in_the_rename_field_sends_no_undo` (Task 6).
4. **A file changed on disk between the listing and a rename.** The change is skipped `precondition_failed`, a notice says so, and the folder is listed again. Test: `a_rename_of_a_changed_file_is_skipped_and_said` (Task 5).
5. **The transport fails mid-read in a preview.** The preview shows the failure with Retry, nothing panics, and the UI thread never waits. Test: `a_read_that_fails_reaches_the_preview_as_failed` (Task 4).

---

### Task 1: Server: `fs.version`, and `undoable` in the history

**Files:**
- Modify: `crates/corundum-framework-fs-api/src/{lib.rs,journal.rs}`
- Modify: `crates/corundum-framework-journal/src/journal.rs`
- Modify: `crates/corundum-framework-fs/src/{service.rs,wire.rs}`
- Test: `crates/corundum-framework-journal/tests/undo.rs`, `crates/corundum-framework-fs/tests/apply.rs`, `crates/corundum-framework-fs/tests/dispatch.rs`

**Interfaces:**
- Produces in fs-api:
  - `pub const FS_VERSION: &str = "fs.version";`
  - `pub struct VersionInfo { pub api: u32 }`
  - `codes::API_MISMATCH: i32 = -32110`
  - `pub struct HistoryBatch { #[serde(flatten)] pub summary: BatchSummary, pub undoable: bool }`
  - `History.batches: Vec<HistoryBatch>`. This breaks the Rust API but adds to the JSON, so `undoable` gets `#[serde(default)]`.
- Produces in journal: `pub fn check_undo(&self, id: BatchId, actor: &Actor) -> Result<(), JournalError>`. It runs the same check `undo_batch` runs (`undo::check`, with `missing = NotFound(id)`) under the lock, without changing anything.
- Behavior:
  - `FsService::dispatch("fs.version", _, _)` answers `{"api": API_VERSION}` whatever the params.
  - `journal.history` sets `undoable = journal.check_undo(id, actor).is_ok()` for each batch, with the caller's actor.

- [ ] **Step 1: Write the failing tests**

```rust
// journal tests/undo.rs
#[test] fn check_undo_agrees_with_undo_batch()
    // a done batch: check_undo Ok; undo_batch it; check_undo → AlreadyUndone; random id → NotFound;
    // an actor may_undo refuses → Forbidden; nothing is changed by check_undo (history unchanged)
// fs tests/dispatch.rs
#[tokio::test] async fn fs_version_answers_the_api_version()    // {"api": 1}, with params null and {}
// fs tests/apply.rs
#[tokio::test] async fn history_says_which_batches_the_caller_may_undo()
    // person "p" applies; history for "p": undoable true; for Actor::person("q"): false (default may_undo);
    // after undo_batch: the batch's undoable false; JSON has "undoable" beside the summary's fields, not nested
```

- [ ] **Step 2: Run them**
  - Run: `cargo test -p corundum-framework-journal --test undo check_undo && cargo test -p corundum-framework-fs --test dispatch --test apply`
  - Expected: fails to compile (`check_undo`, `FS_VERSION`, `HistoryBatch` missing).

- [ ] **Step 3: Implement.** Also:
  - Update the wire pin test in `wire.rs` for `HistoryBatch`. The journal's `BatchSummary` JSON must still parse as the mirror's flattened fields.
  - Update the fs README's method list.

- [ ] **Step 4: Run the gate** (Global Constraints). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(corundum-framework-fs-api)!: fs.version, and which history batches the caller may undo"
```

---

### Task 2: Server: route and follow see the disk's spelling

**Files:**
- Modify: `crates/corundum-framework-fs/src/{workspace.rs,service.rs,app.rs,lib.rs}`, `crates/corundum-framework-fs/README.md`
- Test: `crates/corundum-framework-fs/tests/hooks.rs`

**Interfaces:**
- Produces: `pub(crate) fn Workspace::spelled(&self, path: &WsPath) -> WsPath`, the disk's spelling of `path`:
  - an existing entry's spelling via the walk and `on_disk`, judged without hiding;
  - otherwise the spelling of its nearest existing folder followed by the rest as requested;
  - the root stays the root;
  - any error leaves `path` unchanged.
- Behavior: in `fs.apply`, after parsing and before the hidden check and `route`, each `FsOp` path is replaced by `ws.spelled(..)`:
  - `Move.from` and `Move.to`;
  - `Mkdir.path`;
  - `Trash.path`.

  The built-in plans are built from the rewritten op.
- Docs:
  - On `route` and `follow`: paths are as the disk spells them where the entry exists, and as requested for a name that does not exist yet.
  - Revise step 3's line saying they get the request's spelling.
  - The Linux known limitation also covers this: on Linux the spelling stays the request's.

- [ ] **Step 1: Write the failing tests** (`tests/hooks.rs`; each skips itself at runtime when a probe shows the temp dir is case-sensitive, as `a_restore_never_reaches_a_hidden_folder_spelled_otherwise` does)

```rust
#[tokio::test] async fn route_sees_the_disks_spelling()
    // disk has IMG_1.JPG; fs.move "img_1.jpg" → "b.jpg": the fake app's route was called with from == "IMG_1.JPG"
#[tokio::test] async fn a_new_name_keeps_the_requested_spelling()
    // disk has folder Photos; fs.mkdir "photos/New": route sees "Photos/New"
#[tokio::test] async fn a_path_hidden_on_disk_is_not_routed()
    // the app hides "meta"; fs.move "META/a.txt" → "x.txt": route not called; skipped invalid_path
// and, running on every file system:
#[tokio::test] async fn a_missing_path_is_routed_as_requested() // fs.trash "nope/x": route sees "nope/x"
```

- [ ] **Step 2: Run them.**
  - Run: `cargo test -p corundum-framework-fs --test hooks`
  - Expected: on a case-insensitive FS (macOS/Windows CI), the first three FAIL. On Linux they skip, and `a_missing_path_is_routed_as_requested` passes.
  - Rule from TDD: a test that cannot fail here must still be watched failing. Temporarily make `spelled` return a wrong spelling (for example upper-cased) and watch the fourth test fail, then revert.

- [ ] **Step 3: Implement** `spelled` and the rewrite, and update the docs.

- [ ] **Step 4: Run the gate.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(corundum-framework-fs): route and follow see the disk's spelling"
```

---

### Task 3: `Pending::wait`, the transport and the typed client

**Files:**
- Modify: `crates/corundum-framework-platform/src/pending.rs`
- Modify: `crates/corundum-framework-gui/Cargo.toml` (features `fs-client`, `fs-local`; deps `corundum-framework-fs-api`, `corundum-framework-fs` (optional), `base64`, `grain-id`; all optional, under `fs-client`), `src/lib.rs`
- Create: `crates/corundum-framework-gui/src/fs_client/{mod.rs,transport.rs,client.rs,local.rs}`
- Test: `crates/corundum-framework-gui/src/fs_client/tests.rs`, and tests in `pending.rs`

**Interfaces:**
- Produces in platform: `pub fn Pending::wait(self) -> T`.
  - It blocks until the answer comes. A sender dropped without sending gives `T::default()`, as `poll` does.
  - It is implemented with a `Condvar` beside the existing `Mutex`.
  - It is documented as "never on the UI thread".
- Produces in `gui::fs_client`:
  - `#[non_exhaustive] pub enum FsError`:
    - `Rpc { code: i32, message: String }` (a server's JSON-RPC error)
    - `Transport(String)`
    - `Decode(String)`
    - `ApiMismatch { server: u32, client: u32 }`
  - `FsError::code(&self) -> i32`:
    - `Rpc` gives its code;
    - `ApiMismatch` gives `API_MISMATCH`;
    - the others give `INTERNAL_ERROR`.
  - `impl Default for FsError`, which is `Transport("the request was lost")`, as `Pending`'s sender needs.
  - `pub trait FsTransport: Send + Sync + 'static { fn call(&self, method: &str, params: Value) -> Pending<Result<Value, FsError>>; }`
  - `#[derive(Clone)] pub struct FsClient` (an `Arc<dyn FsTransport>` and the origin):
    - `FsClient::new(transport: impl FsTransport) -> FsClient`
    - `origin(&self) -> &str`
    - `check_version(&self) -> Pending<Result<(), FsError>>`
    - `list(&self, path: &str, cursor: Option<String>) -> Pending<Result<Listing, FsError>>`
    - `stat(&self, path) -> Pending<Result<Entry, FsError>>`
    - `read(&self, path, offset: u64, len: u32) -> Pending<Result<Vec<u8>, FsError>>` (decodes base64; also gives `total` through `read_with_total -> (Vec<u8>, u64)`)
    - `apply(&self, ops: Vec<FsOp>, label: Option<String>) -> Pending<Result<BatchRecords, FsError>>`
    - `undo(&self)`, `redo(&self)`, `undo_batch(&self, batch_id: &str)` → `Pending<Result<BatchRecords, FsError>>`
    - `history(&self, cursor: Option<String>) -> Pending<Result<History, FsError>>`
    - `status(&self, batch_id: &str) -> Pending<Result<BatchRecords, FsError>>`

    Each call puts the client's origin where the params have one.
  - After `check_version` found a mismatch, every call answers `ApiMismatch` at once (`Pending::ready`).
  - `pub struct LocalTransport` (feature `fs-local`): `LocalTransport::new<A: WorkspaceApp>(service: FsService<A>, actor: Actor, runtime: tokio::runtime::Handle, wake: impl Fn() + Send + Sync + 'static) -> LocalTransport`.
    - It spawns `service.dispatch(method, params, &actor)` on `runtime`.
    - It maps the server's `corundum_framework_fs::FsError` to `FsError::Rpc { code: e.code(), message: e.to_string() }`.
    - It answers through `pending::channel(wake)`.

- [ ] **Step 1: Write the failing tests**

```rust
// platform pending.rs
#[test] fn wait_blocks_until_the_answer()            // sender on another thread after 20 ms; wait() == the value
#[test] fn wait_on_a_dropped_sender_is_the_default()
// gui fs_client/tests.rs (feature fs-local; a multi-thread runtime; an FsService<()> on a tempdir;
// a helper `done<T>(p: Pending<T>) -> T` polling with 5 ms sleeps up to 5 s)
#[test] fn a_listing_comes_back_typed()              // dir with a.txt, b/: entries names ["a.txt","b"], kinds File, Dir
#[test] fn apply_then_undo_through_the_client()      // move a.txt→c.txt; undo; a.txt back; origin starts with "gui:"
#[test] fn each_client_has_its_own_origin()          // two clients: undo on the second → Rpc{code: NOTHING_TO_UNDO}
#[test] fn a_server_error_keeps_its_code()           // list "nope" → Rpc{code: NOT_FOUND}
#[test] fn the_version_matches()                     // check_version → Ok
#[test] fn a_mismatched_version_fails_every_call()   // a fake transport answering {"api": 99}: check_version → ApiMismatch{server:99, client:1}; list → ApiMismatch
#[test] fn a_server_without_fs_version_is_a_mismatch() // fake transport answering Rpc{METHOD_NOT_FOUND}: ApiMismatch{server:0, ..}
#[test] fn read_decodes_the_bytes()                  // read "a.txt" 1..3 → b"bc", total 4
```

- [ ] **Step 2: Run them.**
  - Run: `cargo test -p corundum-framework-platform pending && cargo test -p corundum-framework-gui --features fs-local fs_client`
  - Expected: fails to compile.

- [ ] **Step 3: Implement.** Also:
  - Add a module doc on `fs_client` with a `no_run` example of an app's own transport.
  - Add the Cargo.toml feature comments in the existing style.

- [ ] **Step 4: Run the gate,** plus `cargo clippy -p corundum-framework-gui --features fs-client --all-targets -- -D warnings` (without `fs-local`). Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(corundum-framework-gui): fs-client's transport and typed client, and fs-local"
```

---

### Task 4: `FsReader`

**Files:**
- Create: `crates/corundum-framework-gui/src/fs_client/reader.rs`
- Test: in `fs_client/tests.rs`

**Interfaces:**
- Consumes: `FsClient::read_with_total`, `FsClient::stat`, `Pending::wait`.
- Produces:
  - `pub struct FsReader` (`Read + Seek + Send + Sync`).
  - `FsReader::open(client: FsClient, path: String) -> io::Result<FsReader>`, which stats the file, blocking, to learn its length.
  - Reads fetch `MAX_READ_LEN` chunks at the position and keep the last chunk.
  - `Seek` covers `Start`, `End` and `Current`; a seek before 0 is `InvalidInput`.
  - An `FsError` becomes an `io::Error`: `NOT_FOUND` → `NotFound`, the others → `Other`, with its message.
  - `FsClient::content(&self, path: &str, entry: &Entry) -> preview::Content`:
    - its key is `"{path}\0{size}\0{modified}"`;
    - its name is the entry's name;
    - its data is a reader opener making `FsReader::open(self.clone(), path)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn the_reader_reads_and_seeks()                 // 3 MiB file of a known pattern: read_to_end equals it; seek End(-10) then read 10 → the tail
#[test] fn a_read_that_fails_reaches_the_preview_as_failed()
    // Review Focus 5: a transport whose fs.read answers Transport("gone"): Content::open().read → Err(Other, "gone");
    // a FilePreview over a TextPreview-able "a.txt" drawn headless with that content shows the failure (State::Failed path),
    // and no frame takes longer than 100 ms (the read happens on the preview's worker)
#[test] fn a_missing_file_is_not_found()                // FsReader::open on "nope" → ErrorKind::NotFound
```

- [ ] **Step 2: Run them.**
  - Run: `cargo test -p corundum-framework-gui --features fs-local,text-preview fs_client`
  - Expected: fails to compile.

- [ ] **Step 3: Implement.**

- [ ] **Step 4: Run the gate.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(corundum-framework-gui): FsReader, a file over fs.read for the previews"
```

---

### Task 5: `ExplorerDriver`: listing and changes

**Files:**
- Create: `crates/corundum-framework-gui/src/fs_client/driver.rs`
- Modify: `crates/corundum-framework-gui/src/explorer/mod.rs` (`is_focused`), `locales/{en,ja}/corundum.ftl`
- Test: `crates/corundum-framework-gui/src/fs_client/tests_driver.rs`

**Interfaces:**
- Consumes: `FsClient`; `FileExplorer` (`set_entries`, `append_entries`, `set_failed`, `menu`, `movable`, `renamable`, `location`, `go_to`).
- Produces:
  - `pub fn FileExplorer::is_focused(&self, ctx: &egui::Context) -> bool`, which is true while its focus id has focus. It is not breaking.
  - `pub struct ExplorerDriver`:
    - `ExplorerDriver::new(client: FsClient, root_label: impl Into<String>) -> ExplorerDriver`. It makes its own `FileExplorer`: movable, renamable, with a menu item `"trash"` worded `tr!("corundum-fs-trash")`.
    - `.decorate(f: impl Fn(&mut explorer::Entry, &Managed) + 'static) -> ExplorerDriver`.
    - `ui(&mut self, ui: &mut egui::Ui) -> Vec<DriverEvent>`.
  - `#[non_exhaustive] pub enum DriverEvent { OpenManaged { item_id: String }, Opened { path: String } }`. `Opened` is for an unmanaged file.
- Behavior:
  - **First frame:** `check_version`. On failure, `set_failed` with the error's text. On success, list the root.
  - **Navigated / Refresh:** `fs.list` of the location joined by `/`, its first page.
    - The answer goes to `set_entries(location, ..)` for the location it was asked for, so an answer for a folder already left is dropped.
    - `more` is `next_cursor.is_some()`. On `LoadMore`, the next page goes to `append_entries`.
    - A failure goes to `set_failed`.
  - **Entries:** an fs-api `Entry` becomes an explorer `Entry`.
    - `Dir` becomes a folder, anything else a file.
    - A file has a size; `modified` milliseconds become a `SystemTime`.
    - A managed entry is then passed to `decorate`.
    - The driver keeps each listed entry by name, for its `expect`.
  - **Changes:** each becomes one `fs.apply`.
    - `Rename { name, to }` → `FsOp::Move` in the current folder, labelled `tr!("corundum-fs-label-rename", name)`.
    - `Move { names, to }` → one `FsOp::Move` per name, labelled `tr!("corundum-fs-label-move", count)`.
    - Menu `"trash"` → `FsOp::Trash` per name, labelled `tr!("corundum-fs-label-trash", count)`.
    - Each op carries `expect: Some(Expect { size, modified, hash: None })` from the listing.
    - After the answer, the current folder is listed again.
    - The driver owns a one-line notice strip above the explorer, drawn by `ui`, with a close button.
    - If any op was skipped, the notice says so: `tr!("corundum-fs-done", label)` plus Task 6's `tr!("corundum-fs-skipped", count)` (so this task adds that id too), for example "Move 3 items (1 skipped)".
    - An error is a notice with the error's text.
  - **Opening:** `Opened(name)` of a managed entry → `DriverEvent::OpenManaged { item_id }`; otherwise `DriverEvent::Opened { path }`.

- [ ] **Step 1: Write the failing tests** (headless frames over `LocalTransport`; a helper runs frames until no request is in flight; events are fed by calling the driver's internal `handle(ExplorerEvent)`, `pub(crate)`, as the explorer's own tests feed input)

```rust
#[test] fn the_root_is_listed_on_the_first_frame()         // tempdir a.txt, b/: explorer.entries names ["a.txt","b"]
#[test] fn a_late_listing_for_a_left_folder_is_dropped()   // Review Focus 1: a transport delaying "a"'s answer; go a then b; after both: entries are b's
#[test] fn paging_appends()                                // 250 files: first 200, LoadMore → 250
#[test] fn a_rename_applies_one_move_with_its_expect()     // rename a.txt→c.txt: disk has c.txt; the batch's plan has expect.size == 1; label "Rename “a.txt”"
#[test] fn a_rename_of_a_changed_file_is_skipped_and_said() // Review Focus 4: rewrite a.txt after listing; rename → disk unchanged; notice mentions "skipped"; folder listed again
#[test] fn a_drag_moves_each_name_in_one_batch()           // Move{names:[a,b], to:["d"]}: one batch, 2 ops
#[test] fn trash_from_the_menu()                           // Menu{id:"trash", names:["a.txt"]}: a.txt gone from the listing
#[test] fn a_managed_entry_is_decorated_and_opens_as_managed() // app annotate marks x.jpg managed {kind:"photo", item_id:"7"}; decorate sets icon; Opened("x.jpg") → OpenManaged{item_id:"7"}
#[test] fn a_version_mismatch_shows_as_failed()            // fake transport {"api": 99}: the explorer shows Failed
```

- [ ] **Step 2: Run them.**
  - Run: `cargo test -p corundum-framework-gui --features fs-local fs_client::tests_driver`
  - Expected: fails to compile.

- [ ] **Step 3: Implement,** with the ftl ids in en and ja.

- [ ] **Step 4: Run the gate.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(corundum-framework-gui): ExplorerDriver lists and changes a workspace over fs-client"
```

---

### Task 6: `ExplorerDriver`: Ctrl+Z, the notice, and the preview

**Files:**
- Modify: `crates/corundum-framework-gui/src/fs_client/driver.rs`, `locales/{en,ja}/corundum.ftl`
- Test: `crates/corundum-framework-gui/src/fs_client/tests_driver.rs`

**Interfaces:**
- Consumes: Task 5's driver, `FileExplorer::is_focused`, `FsClient::{undo, redo, content}`, `preview::FilePreview`.
- Produces:
  - `pub fn ExplorerDriver::detail_ui(&mut self, ui: &mut egui::Ui) -> PreviewResponse`:
    - with one unmanaged file picked, it draws `FilePreview` with `FileInfo` from the listing, and its content from `FsClient::content`;
    - with nothing picked it draws nothing;
    - Retry gets the content again (a new key).
  - `ExplorerDriver::with_preview(preview: FilePreview) -> ExplorerDriver`, for an app's player.
- Behavior:
  - **Keys:** Ctrl+Z (⌘Z) and Ctrl+Shift+Z (⌘⇧Z) are read from the input after `explorer.ui`, only while `explorer.is_focused(ctx)` and no rename field is open (the explorer keeps its rename state; add `pub(crate) fn is_renaming`).
  - **Queue:** presses are queued and sent one at a time, in order. Each press is one `journal.undo` / `journal.redo`.
  - **After each answer:** the folder is listed again, and the notice is set.
    - Notice text: `tr!("corundum-fs-undid", label)` or `corundum-fs-redid`, plus `tr!("corundum-fs-skipped", count)` when any op was skipped. English is exactly "Undid 'Move 3 items' (1 skipped)".
    - The label is the target batch's label, read from the compensation's `undo_of`/`redo_of` target through `FsClient::status`, or `tr!("corundum-fs-unnamed")` when it has none.
    - `NOTHING_TO_UNDO` / `NOTHING_TO_REDO` → `tr!("corundum-fs-nothing-to-undo" / "-redo")`.
  - **Notice:** Task 5's strip gains a way open. Clicking it opens a list of the skipped ops: kind, path from the plan, and reason (`tr!("corundum-fs-reason-<reason>")` for the spec's reasons, the raw reason otherwise).

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn ctrl_z_undoes_the_last_change()           // rename a→c; focus the explorer; press Ctrl+Z: a.txt back; notice "Undid 'Rename “a.txt”'"
#[test] fn ctrl_shift_z_redoes_it()
#[test] fn two_quick_undos_undo_two_batches()        // Review Focus 2: two renames; two Ctrl+Z in the same frame: both reverted, 2 undo batches in history
#[test] fn ctrl_z_in_the_rename_field_sends_no_undo() // Review Focus 3: F2 open, Ctrl+Z: history has no undo batch; also unfocused explorer: none
#[test] fn an_undo_that_skips_says_how_many()        // move a→b; write new a; Ctrl+Z → notice ends "(1 skipped)"; opening it lists reason text for "exists"
#[test] fn nothing_to_undo_is_said()
#[test] fn a_picked_file_is_previewed_over_fs_read() // feature text-preview: pick a.txt "hello"; detail_ui after frames shows "hello"
```

- [ ] **Step 2: Run them.**
  - Run: `cargo test -p corundum-framework-gui --features fs-local,text-preview fs_client::tests_driver`
  - Expected: FAIL (the methods are missing).

- [ ] **Step 3: Implement,** with the ftl ids in en and ja.

- [ ] **Step 4: Run the gate.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(corundum-framework-gui): Ctrl+Z in the explorer, its notice, and the preview over fs.read"
```

---

### Task 7: `HistoryPage`

**Files:**
- Create: `crates/corundum-framework-gui/src/fs_client/history.rs`
- Modify: `locales/{en,ja}/corundum.ftl`, the gui README's features table, `README.md` / `README.ja.md` rows for the gui if they list features
- Test: `crates/corundum-framework-gui/src/fs_client/tests_history.rs`

**Interfaces:**
- Consumes: `FsClient::{check_version, history, status, undo_batch, origin}`, `HistoryBatch.undoable`.
- Produces:
  - `pub struct HistoryPage`.
  - `HistoryPage::new(client: FsClient) -> HistoryPage`.
  - `.time_format(f: impl Fn(SystemTime) -> String + 'static)`, defaulting to `explorer::format_time`.
  - `ui(&mut self, ui: &mut egui::Ui)`.
  - `refresh(&mut self)`.
- Behavior:
  - **Loading:** on the first frame (and on `refresh`, and after an undo), the first `journal.history` page loads. "Load more" fetches `next_cursor`.
  - **Each row** shows:
    - the label, or `corundum-fs-unnamed`;
    - the time;
    - who: the principal, with a badge `tr!("corundum-fs-agent")` when `actor.agent`;
    - `tr!("corundum-fs-this-session")` when the batch's origin is the client's;
    - the done and skipped counts;
    - `tr!("corundum-fs-undone")` when `undone_by` is set.
  - **Expanding a row** loads `journal.status` and lists its ops (kind, path, outcome, reason).
  - **Undo** is enabled exactly when `undoable`.
    - For a batch whose origin is not the client's, it first asks in a modal: agent → `tr!("corundum-fs-confirm-agent", time)` ("This change was made by the agent at 14:02. Undo it?"); otherwise `tr!("corundum-fs-confirm-other", time)` ("This change was made in another window at 14:02. Undo it?").
    - The undo itself is `undo_batch(id)`. Its result is a notice like the driver's.
    - Then the page refreshes.
    - An error (for example `ALREADY_UNDONE` from a stale page) is a notice, and the page refreshes.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn rows_show_newest_first_with_their_counts()   // two applies: first row is the second's label; "1" done shown
#[test] fn an_agents_batch_has_a_badge_and_asks_first()
    // one FsService, cloned (clones share the journal) into two LocalTransports: the page's with Actor::person("p"),
    // an agent's with Actor::agent("p") (same principal, so the default may_undo allows it); the agent applies;
    // the page shows the agent badge; Undo opens the confirmation containing "by the agent at"; confirming undoes it
#[test] fn this_sessions_batch_undoes_without_asking()
#[test] fn undo_is_disabled_when_not_undoable()        // a batch of Actor::person("q") seen by "p": the Undo button is disabled
#[test] fn a_stale_undo_says_already_undone_and_refreshes()
#[test] fn load_more_fetches_the_next_page()           // 60 applies: 50 rows, Load more → 60
```

- [ ] **Step 2: Run them.**
  - Run: `cargo test -p corundum-framework-gui --features fs-local fs_client::tests_history`
  - Expected: fails to compile.

- [ ] **Step 3: Implement,** with the ftl ids in en and ja and the README rows.

- [ ] **Step 4: Run the gate.** Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(corundum-framework-gui): HistoryPage, the journal's batches with Undo"
```

---

## Not in this plan

- `JsonlStore` (step 5).
- asteria and sapphire serving these methods (after the spec).
- The Linux case-insensitive on-disk spelling fix (a tracked follow-up from step 3).
- Emptying the trash.
