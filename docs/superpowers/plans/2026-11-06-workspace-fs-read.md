# Workspace fs, reading only — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the read-only first step of the spec: the `fs.list`, `fs.stat` and `fs.read`
methods, their types, and an engine that serves them over a workspace folder. The engine
hides dot-names and the app's own files, and lets the app mark its managed entries.

**Architecture:**
- **`corundum-framework-fs-api`**: serde types, method names, limits and error codes only.
- **`corundum-framework-fs`**:
  - `WsPath`: a checked workspace-relative path.
  - `Workspace`: the canonical root, and resolution that refuses symlinks and escapes.
  - `WorkspaceApp`: the hook, for now only `hidden` and `annotate`.
  - The operations `list`, `stat` and `read`.
  - `FsService::dispatch`, which a server mounts on its own JSON-RPC router. It runs the
    blocking work through `tokio::task::spawn_blocking`.
- **The facade** gains the features `fs-api` and `fs`.

There is no journal, no `Actor` and no changes yet. `dispatch` takes no actor in this step;
step 3 of the spec adds it with the journal, which is an expected change of signature.

**Tech Stack:** Rust 2024 (toolchain 1.99.0), serde / serde_json, base64 0.23, thiserror 2,
tokio 1 (`rt`), tempfile 3 (tests).

**Spec:** `docs/superpowers/specs/2026-11-06-workspace-fs-journal-design.md`. This plan is
its "Order of work" step 1, on the branch `workspace-fs-journal`.

## Global Constraints

- **Names:**
  - The crates are `corundum-framework-fs-api` and `corundum-framework-fs`, version
    `0.11.0`, `license = "MIT OR Apache-2.0"`.
  - Both go in release-plz's `version_group = "corundum"`, as `[[package]]` entries like the
    others in `release-plz.toml`.
- **Dependencies:**
  - fs-api depends on serde alone (serde_json as a dev-dependency).
  - fs adds fs-api, serde, serde_json, base64 0.23, thiserror, tracing, and tokio with `rt`.
- **Method names:** `fs.list`, `fs.stat`, `fs.read`.
- **Limits:**
  - `fs.read`: `len` is at most 1 MiB (`MAX_READ_LEN = 1_048_576`).
  - `fs.list`: `limit` defaults to 200 and is clamped to 1..=1000 (`DEFAULT_LIST_LIMIT`,
    `MAX_LIST_LIMIT`, the same as asteria's `folder.list`).
- **Error codes:**
  - corundum's fs codes are in **-32100..=-32199**, apart from asteria's -32001..-32009:
    `INVALID_PATH = -32101`, `NOT_FOUND = -32102`, `TOO_LARGE = -32103`.
  - The JSON-RPC standard codes are `METHOD_NOT_FOUND = -32601`, `INVALID_PARAMS = -32602`
    and `INTERNAL_ERROR = -32603`.
- **API version:** `API_VERSION: u32 = 1`, a constant bumped by hand on any breaking change
  to fs-api. The crate's version is the `corundum` group's, so it cannot be derived from
  it.
- **Hiding:**
  - Any path with a segment starting with `.` is hidden.
  - So is any path the app's `hidden` matches, itself or any of its ancestors.
  - Hidden entries are never listed. `stat` and `read` of a hidden path answer
    `invalid_path`.
- **Symlinks:**
  - Listed with kind `symlink`, and never followed.
  - A path with a symlink anywhere in it, or `read` of a symlink, answers `invalid_path`.
- **Listing order and cursor:**
  - Entries come in `str` order of their names.
  - `next_cursor` is the last name returned. The next page has the names after it, so a
    page stays correct when entries are added or removed in between.
- **Wire form:**
  - `modified` is milliseconds since the Unix epoch, or `null`.
  - `data` is standard base64 with padding.
  - `EntryKind` serializes as `"file"` / `"dir"` / `"symlink"`.
- **Gate:** CI runs `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --check`, and every task leaves both clean. Public items carry doc comments
  (`#![warn(missing_docs)]`).

## Review Focus

1. **A non-UTF-8 file name in a listed folder** (a file copied from an old Windows or Linux
   disk): that entry is skipped (`tracing::debug!`), and the rest of the listing still
   comes. → test `non_utf8_names_are_skipped` (unix) in Task 4.
2. **An entry deleted between two pages, the cursor's own name included**: the next page
   continues after the cursor's name, without error or repetition.
   → test `a_page_after_a_removed_cursor_continues` in Task 4.
3. **A symlinked folder pointing out of the root, used as a middle segment**
   (`link/secret.txt`): `invalid_path`, never the outside file's contents.
   → test `a_symlink_in_the_middle_is_refused` (unix) in Task 3.
4. **A read past the end of a file** (a preview asks for the next chunk of a file that
   shrank): empty `data` with the true `total`, not an error.
   → test `reading_past_the_end_is_empty` in Task 5.
5. **`limit: 0` or a huge `limit`**: clamped to 1 and 1000, never an empty page that loops
   forever and never an unbounded one. → test `limit_is_clamped` in Task 4.

---

### Task 1: `corundum-framework-fs-api`

**Files:**
- Create: `crates/corundum-framework-fs-api/Cargo.toml`, `README.md`, `src/lib.rs`
- Modify: `release-plz.toml` (add a `[[package]]` with `version_group = "corundum"`)

**Interfaces:**
- Produces (all `pub`, in `corundum_framework_fs_api`):
  - Constants:
    - `API_VERSION: u32 = 1`
    - `FS_LIST`, `FS_STAT`, `FS_READ: &str`
    - `MAX_READ_LEN: u32 = 1_048_576`, `DEFAULT_LIST_LIMIT: u32 = 200`,
      `MAX_LIST_LIMIT: u32 = 1000`
  - `mod codes { INVALID_PATH, NOT_FOUND, TOO_LARGE, METHOD_NOT_FOUND, INVALID_PARAMS, INTERNAL_ERROR: i32 }`
  - `enum EntryKind { File, Dir, Symlink }`, with `#[serde(rename_all = "snake_case")]`
  - `struct Managed { kind: String, item_id: String }`
  - `struct Entry { name: String, kind: EntryKind, size: u64, modified: Option<i64>, managed: Option<Managed> }`
    - `managed` is `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  - `struct ListParams { path: String, cursor: Option<String>, limit: Option<u32> }`
    - `cursor` and `limit` have `#[serde(default)]`.
  - `struct Listing { entries: Vec<Entry>, next_cursor: Option<String> }`
  - `struct StatParams { path: String }`
  - `struct ReadParams { path: String, offset: u64, len: u32 }`
  - `struct ReadResult { data: String, total: u64 }`
  - Every type derives `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.

- [ ] **Step 1: Write the failing tests** (in `src/lib.rs`, `#[cfg(test)] mod tests`)
  - `every_type_round_trips`: an `Entry` with `managed: Some(..)` and one without, plus a
    `Listing`, `ListParams`, `StatParams`, `ReadParams` and `ReadResult`, each through
    `serde_json` and back, equal.
  - `entry_kinds_are_lowercase`:
    - `serde_json::to_string(&EntryKind::Symlink) == "\"symlink\""`, and likewise `"file"`
      and `"dir"`.
  - `missing_optional_params_default`:
    - `{"path":"a"}` parses as `ListParams { path: "a", cursor: None, limit: None }`.
    - An `Entry` with `managed: None` serializes without a `managed` key.
  - `method_names_are_namespaced`: each of `FS_LIST`, `FS_STAT` and `FS_READ` starts with
    `"fs."`.
  - `codes_stay_in_their_range`: `INVALID_PATH`, `NOT_FOUND` and `TOO_LARGE` are each in
    `-32199..=-32100`.

- [ ] **Step 2: Run them to see them fail**
  - Run: `cargo test -p corundum-framework-fs-api`
  - Expected: it fails to compile (the items are missing).

- [ ] **Step 3: Implement the items above**
  - The crate doc says what the crate is: the fs methods, kept apart so that a client
    links nothing heavier, and their version moving only with `API_VERSION`.
  - Write `README.md` in the style of `crates/corundum-framework-keys/README.md`, short.

- [ ] **Step 4: Run the tests**
  - Run: `cargo test -p corundum-framework-fs-api && cargo clippy -p corundum-framework-fs-api --all-targets -- -D warnings`
  - Expected: PASS, no warnings.

- [ ] **Step 5: Commit**
  - `git add crates/corundum-framework-fs-api release-plz.toml && git commit -m "feat(corundum-framework-fs-api): the fs.* read methods and their types"`

### Task 2: `WsPath` and `FsError`

**Files:**
- Create: `crates/corundum-framework-fs/Cargo.toml`, `README.md`, `src/lib.rs`,
  `src/path.rs`, `src/error.rs`
- Modify: `release-plz.toml` (add a `[[package]]` with `version_group = "corundum"`)

**Interfaces:**
- Consumes: `corundum_framework_fs_api::codes`.
- Produces: `WsPath`.
  - The type: `#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)] pub struct WsPath(String)`.
  - `WsPath::ROOT` (`""`), or `WsPath::root()`.
  - `WsPath::parse(s: &str) -> Result<WsPath, FsError>`.
  - `as_str(&self) -> &str`, `is_root(&self) -> bool`, `segments(&self) -> impl Iterator<Item = &str>`.
  - `name(&self) -> Option<&str>`, `parent(&self) -> Option<WsPath>`.
  - `join(&self, name: &str) -> Result<WsPath, FsError>`.
  - `ancestors_and_self(&self) -> impl Iterator<Item = WsPath>`: `a`, `a/b`, `a/b/c` for
    `a/b/c`, and nothing for the root.
  - `has_dot_segment(&self) -> bool`.
- Produces: `FsError`.
  - `#[derive(Debug, thiserror::Error)] pub enum FsError`, with the variants:
    - `InvalidPath(String)`
    - `NotFound(String)`
    - `TooLarge(String)`
    - `InvalidParams(String)`
    - `MethodNotFound(String)`
    - `Io { path: String, source: std::io::Error }`
  - `FsError::code(&self) -> i32`, which maps the variants in order to `INVALID_PATH`,
    `NOT_FOUND`, `TOO_LARGE`, `INVALID_PARAMS`, `METHOD_NOT_FOUND` and `INTERNAL_ERROR`.

- [ ] **Step 1: Write the failing tests** (`src/path.rs`, `#[cfg(test)]`)
  - `accepts_normal_paths`:
    - `parse("")` is the root.
    - `parse("a/b c/d.txt")` gives the segments `["a", "b c", "d.txt"]`, the name
      `"d.txt"` and the parent `"a/b c"`.
  - `refuses_bad_paths`: each of these is `Err(FsError::InvalidPath(_))`:
    - `"/a"`, `"a/"`, `"a//b"`, `"."`, `"a/./b"`, `".."`, `"a/../b"`
    - `"a\\b"`, `"a\0b"`
    - on `cfg(windows)` also `"C:"` and `"a:b"`
  - `join_checks_the_name`:
    - `root().join("a")` is `"a"`.
    - `parse("a").join("b")` is `"a/b"`.
    - `join("x/y")`, `join("..")` and `join("")` are each `InvalidPath`.
  - `dot_segments_are_found`:
    - `parse(".asteria/trash").has_dot_segment()` and `parse("a/.git").has_dot_segment()`
      are true.
    - `parse("a.b/c").has_dot_segment()` is false.
  - `ancestors_and_self_go_down`: for `"a/b/c"` it gives `["a", "a/b", "a/b/c"]`.
  - `error_codes_match_the_api`: `FsError::NotFound("x".into()).code() == codes::NOT_FOUND`,
    and likewise for each variant.

- [ ] **Step 2: Run them to see them fail**
  - Run: `cargo test -p corundum-framework-fs`
  - Expected: it fails to compile.

- [ ] **Step 3: Implement `WsPath` and `FsError`**
  - `parse` splits on `/`, refuses the cases above, and keeps the string as given. A path
    that passes is already normalized.
  - The `InvalidPath` message names the path and what is wrong with it.
  - In `lib.rs`: `#![warn(missing_docs)]`, `mod path; mod error;`, and
    `pub use path::WsPath; pub use error::FsError;`.
  - Also in `lib.rs`: `pub use corundum_framework_fs_api as api;`, so a server needs one
    dependency.

- [ ] **Step 4: Run the tests**
  - Run: `cargo test -p corundum-framework-fs && cargo clippy -p corundum-framework-fs --all-targets -- -D warnings`
  - Expected: PASS.

- [ ] **Step 5: Commit**
  - `git commit -m "feat(corundum-framework-fs): workspace-relative paths and errors"`

### Task 3: `Workspace`, resolution and hiding

**Files:**
- Create: `crates/corundum-framework-fs/src/workspace.rs`, `src/app.rs`,
  `tests/common/mod.rs` (a helper that builds a temporary workspace),
  `tests/resolve.rs`

**Interfaces:**
- Consumes: `WsPath`, `FsError` (Task 2); `corundum_framework_fs_api::{Entry, Managed}`.
- Produces: the `WorkspaceApp` hook.
  ```rust
  pub trait WorkspaceApp: Send + Sync + 'static {
      fn hidden(&self, path: &WsPath) -> bool { false }
      fn annotate(&self, dir: &WsPath, entries: &mut [Entry]) {}
  }
  impl WorkspaceApp for () {}
  ```
- Produces: `Workspace`.
  - `pub struct Workspace`, with:
    - `Workspace::open(root: impl AsRef<Path>) -> Result<Workspace, FsError>`, which
      canonicalizes the root. A missing root is `NotFound`; a root that is not a folder is
      `InvalidPath`.
    - `root(&self) -> &Path`
    - `is_hidden(&self, app: &dyn WorkspaceApp, path: &WsPath) -> bool`: true when the
      path has a dot segment, or when `app.hidden` is true for any of `ancestors_and_self`.
    - `resolve(&self, app: &dyn WorkspaceApp, path: &WsPath) -> Result<(PathBuf, std::fs::Metadata), FsError>`.
      It refuses a hidden path (`InvalidPath`). It walks the segments with
      `symlink_metadata`:
      - a missing segment is `NotFound`;
      - a symlink before the last segment is `InvalidPath`;
      - otherwise it returns the path and the last segment's `symlink_metadata`. The last
        segment may itself be a symlink, which the callers decide on.

- [ ] **Step 1: Write the failing tests** (`tests/resolve.rs`)
  - `resolves_files_and_folders`:
    - In a temporary root holding `a/b.txt`, `resolve("a/b.txt")` is a file and
      `resolve("a")` is a folder.
    - `resolve("")` is the root.
  - `missing_is_not_found`: `resolve("nope")` is `NotFound`.
  - `dot_names_are_hidden`:
    - `.asteria/x` exists on disk.
    - `resolve(".asteria/x")` is `InvalidPath`, and `is_hidden` is true.
  - `the_apps_hidden_covers_descendants`: an app with `hidden = |p| p.as_str().ends_with(".xmp") || p.as_str() == "meta"`:
    - `resolve("p.jpg.xmp")` is `InvalidPath`;
    - `resolve("meta/inner.txt")` is `InvalidPath`;
    - `resolve("p.jpg")` succeeds.
  - `a_symlink_in_the_middle_is_refused` (`cfg(unix)`):
    - `link` points to a temporary folder outside the root that holds `secret.txt`.
    - `resolve("link/secret.txt")` is `InvalidPath`.
  - `a_final_symlink_is_returned_as_a_symlink` (`cfg(unix)`): `resolve("link")` returns
    metadata whose `file_type().is_symlink()` is true.
  - `opening_a_missing_root_fails`: `Workspace::open` of a missing path is `NotFound`.

- [ ] **Step 2: Run them to see them fail**
  - Run: `cargo test -p corundum-framework-fs --test resolve`
  - Expected: it fails to compile.

- [ ] **Step 3: Implement `WorkspaceApp`, `Workspace::open`, `is_hidden` and `resolve`**

- [ ] **Step 4: Run the tests**
  - Run: `cargo test -p corundum-framework-fs`
  - Expected: PASS.

- [ ] **Step 5: Commit**
  - `git commit -m "feat(corundum-framework-fs): resolve paths in a workspace, hiding the app's files"`

### Task 4: `list`

**Files:**
- Create: `crates/corundum-framework-fs/src/list.rs`, `tests/list.rs`

**Interfaces:**
- Consumes: `Workspace::resolve`, `Workspace::is_hidden`, `WorkspaceApp::annotate`
  (Task 3); `ListParams`, `Listing`, `Entry`, `EntryKind`, `DEFAULT_LIST_LIMIT` and
  `MAX_LIST_LIMIT` (Task 1).
- Produces:
  - `pub fn list(ws: &Workspace, app: &dyn WorkspaceApp, params: &ListParams) -> Result<Listing, FsError>`
  - `pub(crate) fn entry_for(name: String, meta: &std::fs::Metadata) -> Entry`, which Task 5
    reuses for `stat`.

- [ ] **Step 1: Write the failing tests** (`tests/list.rs`)
  - `lists_in_name_order_with_kinds`:
    - The root holds `b.txt` (3 bytes), `a/` and, on unix, the symlink `l`.
    - The names come as `["a", "b.txt", "l"]`, with the kinds `Dir`, `File` and
      `Symlink`.
    - `b.txt` has size 3 and a `modified` that is `Some(_)`.
  - `hidden_entries_are_not_listed`:
    - The root holds `.asteria/`, `.DS_Store`, `p.jpg` and `p.jpg.xmp`, and the app hides
      `*.xmp`.
    - Only `["p.jpg"]` is listed.
  - `pages_follow_the_cursor`:
    - The root holds 5 files named `f0` to `f4`.
    - With `limit: Some(2)`, the pages are `["f0", "f1"]` (cursor `"f1"`), then
      `["f2", "f3"]`, then `["f4"]` with `next_cursor: None`.
  - `a_page_after_a_removed_cursor_continues`:
    - After the first page (cursor `"f1"`), delete `f1` and `f2`.
    - The next page is `["f3", "f4"]`.
  - `limit_is_clamped`:
    - With 1005 files, `limit: Some(0)` gives 1 entry.
    - `limit: Some(5000)` gives 1000 entries, with a cursor.
    - `limit: None` gives 200.
  - `annotate_marks_managed_entries`:
    - The app sets `managed = Some(Managed { kind: "photo", item_id: name })` on names
      ending in `.jpg`, and asserts that `dir` is the listed folder.
    - `p.jpg` is managed; `notes.txt` is not.
  - `listing_a_file_is_invalid_path`: `list` of `b.txt` is `InvalidPath`.
  - `non_utf8_names_are_skipped` (`cfg(unix)`):
    - The root holds a file named with the bytes `b"bad\xff"`, and `ok.txt`.
    - The listing is `["ok.txt"]`, with no error.

- [ ] **Step 2: Run them to see them fail**
  - Run: `cargo test -p corundum-framework-fs --test list`
  - Expected: it fails to compile.

- [ ] **Step 3: Implement `list`**
  1. Resolve `params.path`. A folder is required; anything else is `InvalidPath`.
  2. `read_dir` the folder, and skip:
     - non-UTF-8 names (`tracing::debug!`);
     - entries whose `symlink_metadata` fails (gone in between);
     - hidden entries (`is_hidden` of `dir.join(name)`).
  3. Keep the names `> cursor`, sort them with `str::cmp`, and take
     `limit.unwrap_or(DEFAULT).clamp(1, MAX)` of them.
  4. Call `app.annotate(&dir, &mut page)` on the page only.
  5. `next_cursor` is the last name when more remain.

  In `entry_for`:
  - The size is the file's length; folders and symlinks have 0.
  - `modified` is `metadata.modified()` as milliseconds since the epoch, or `None` if it
    fails.
  - Other kinds (a fifo, a socket) count as `File`.

- [ ] **Step 4: Run the tests**
  - Run: `cargo test -p corundum-framework-fs`
  - Expected: PASS.

- [ ] **Step 5: Commit**
  - `git commit -m "feat(corundum-framework-fs): list a folder a page at a time"`

### Task 5: `stat` and `read`

**Files:**
- Create: `crates/corundum-framework-fs/src/read.rs`, `tests/read.rs`

**Interfaces:**
- Consumes: `Workspace::resolve`, `entry_for` (Tasks 3–4); `StatParams`, `ReadParams`,
  `ReadResult` and `MAX_READ_LEN` (Task 1).
- Produces:
  - `pub fn stat(ws: &Workspace, app: &dyn WorkspaceApp, params: &StatParams) -> Result<Entry, FsError>`
  - `pub fn read(ws: &Workspace, app: &dyn WorkspaceApp, params: &ReadParams) -> Result<ReadResult, FsError>`

- [ ] **Step 1: Write the failing tests** (`tests/read.rs`)
  - `stat_gives_one_entry`: `stat("a/b.txt")` has the name `"b.txt"`, the kind `File` and
    the right size.
  - `stat_is_annotated`: with the app from Task 4's annotate test, `stat("p.jpg")` has
    `managed.is_some()`, and its `dir` is `""`.
  - `stat_of_the_root`: the root is an entry with the name `""` and the kind `Dir`.
  - `reads_a_range`:
    - The file holds `"hello world"`.
    - `read { offset: 6, len: 5 }` decodes to `b"world"`, with `total: 11`.
  - `reading_past_the_end_is_empty`: `read { offset: 100, len: 10 }` has `data` that
    decodes to an empty slice, and `total: 11`.
  - `read_len_is_capped`:
    - `len: MAX_READ_LEN + 1` is `TooLarge`.
    - `len: MAX_READ_LEN` on a smaller file is fine.
  - `reading_a_folder_or_a_symlink_is_invalid_path`: reading `a` is `InvalidPath`; on unix,
    so is reading a symlink to `a/b.txt` inside the root.
  - `hidden_paths_are_invalid_path`: `stat` and `read` of `p.jpg.xmp`, with the app hiding
    `*.xmp`, are each `InvalidPath`.

- [ ] **Step 2: Run them to see them fail**
  - Run: `cargo test -p corundum-framework-fs --test read`
  - Expected: it fails to compile.

- [ ] **Step 3: Implement `stat` and `read`**
  - **`stat`**: `annotate` its parent folder (the root for the root) on a slice of one
    entry.
  - **`read`**: it needs a regular file.
    1. Check `len` before touching the disk.
    2. `seek` to `offset`, then `take(len)` and `read_to_end`.
    3. Encode with `base64::engine::general_purpose::STANDARD`.
    4. `total` is the metadata's length.

- [ ] **Step 4: Run the tests**
  - Run: `cargo test -p corundum-framework-fs`
  - Expected: PASS.

- [ ] **Step 5: Commit**
  - `git commit -m "feat(corundum-framework-fs): stat an entry and read a file's range"`

### Task 6: `FsService::dispatch`, the facade and the READMEs

**Files:**
- Create: `crates/corundum-framework-fs/src/service.rs`, `tests/dispatch.rs`
- Modify:
  - `crates/corundum-framework/Cargo.toml` (features `fs-api = ["dep:corundum-framework-fs-api"]`,
    `fs = ["dep:corundum-framework-fs", "fs-api"]`, and the two optional dependencies with
    `version` and `path`, as the others have)
  - `crates/corundum-framework/src/lib.rs` (the `pub use` lines, and rows in the doc table)
  - `README.md` and `README.ja.md` (the crate table's rows, and facade feature rows, as for
    `keys`)

**Interfaces:**
- Consumes: `list`, `stat`, `read`, `Workspace`, `WorkspaceApp` (Tasks 3–5).
- Produces: `FsService`.
  - `pub struct FsService<A: WorkspaceApp>`, with:
    - `FsService::new(ws: Workspace, app: A) -> FsService<A>`, holding both in an `Arc`.
    - `Clone`.
    - `pub async fn dispatch(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value, FsError>`
  - Later steps add the journal methods and an `actor` parameter here.

- [ ] **Step 1: Write the failing tests** (`tests/dispatch.rs`, `#[tokio::test]`)
  - `dispatches_each_method`:
    - `dispatch("fs.list", json!({"path": ""}))` parses as a `Listing` naming the
      temporary files.
    - `fs.stat` and `fs.read` likewise give an `Entry` and a `ReadResult`.
  - `unknown_methods_are_method_not_found`: `dispatch("folder.list", json!({}))` is
    `MethodNotFound`, with `code() == -32601`. A server tells "not mine" from a failure by
    this.
  - `bad_params_are_invalid_params`:
    - `dispatch("fs.read", json!({"path": 3}))` is `InvalidParams`, with
      `code() == -32602`.
    - `dispatch("fs.list", Value::Null)` is `InvalidParams` too.
  - `errors_keep_their_codes`: `dispatch("fs.stat", json!({"path": "../x"}))` has
    `code() == -32101`.

- [ ] **Step 2: Run them to see them fail**
  - Run: `cargo test -p corundum-framework-fs --test dispatch`
  - Expected: it fails to compile.

- [ ] **Step 3: Implement `dispatch` and wire the facade**
  - `dispatch`:
    1. Match on the method name.
    2. `serde_json::from_value` the params (an error is `InvalidParams` with serde's
       message).
    3. Run the operation in `tokio::task::spawn_blocking` (a join error is `Io` with the
       path `""`).
    4. `serde_json::to_value` the result.
  - Then the facade, the READMEs, and the crate docs in `lib.rs`. The crate doc points to
    the spec.

- [ ] **Step 4: Run the whole gate**
  - Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
  - Run: `cargo build -p corundum-framework --features fs`
  - Expected: all PASS; the facade builds with `fs`.

- [ ] **Step 5: Commit**
  - `git commit -m "feat(corundum-framework-fs): FsService::dispatch, and the facade's fs features"`
