# A workspace's files over the app's server, with an undo log

Asked for on 2026-11-06, for asteria and the sapphire apps.

## Why

asteria and sapphire keep their data as files in a folder the person chose, not in a
database (see [file-previews.md](../../file-previews.md)). That folder also holds files the
app does not manage: a booklet scan beside an album, a PDF beside a trip's photos. That is
normal use, and the apps must show those files and let the person move, rename and trash
them.

Both apps have the same shape. A server owns the folder. Clients reach the server over
JSON-RPC 2.0. corundum already has the explorer (`explorer::FileExplorer`) and the
previews, which touch no files. What is missing is the same thing in both apps:

- **asteria** lists only its managed items. `folder.list` reads the cache, so unmanaged
  files are never listed. Moves and trashing go through `organize.apply`, keyed by item id,
  with a journal and undo (`asteria-common-core::journal`, `asteria-common-server::organize`).
- **sapphire** has `workspace.list_dir`, `read_file`, `write_file` and `delete_file`. They
  are keyed by path, carry text only, and have no move, rename or undo.

The transports differ, and stay different: asteria uses HTTP `POST /rpc`, sapphire uses
framed JSON-RPC over a local socket or named pipe (`sapphire-framework-ipc`). What can be
shared is the method names and types, the engine behind them, and the GUI that drives the
explorer through them.

A move made by mistake must be undoable. The undo is meant for **the mistake just made**,
not for history long past: an undo log kept too long invites rolling back something by
accident.

## Goals

- One set of `fs.*` and `journal.*` methods that both servers serve, over their own
  transports.
- The explorer and the previews work over those methods, with no code specific to either
  app.
- Every change is a batch in a log, and can be undone by a compensating batch.
- An app can add its own kinds of operation to the log, and can take over the operations on
  the files it manages, so asteria's cache, sidecars and playlists stay consistent.
- Ctrl+Z in a GUI undoes that GUI's last change, never an agent's work done in parallel
  over MCP. A history screen can undo other changes on purpose.
- Files the app keeps for itself (its hidden folder, its sidecars) are never shown and
  never touched through `fs.*`.

## Non-goals

- Writing or copying file contents. This round has move, rename, make-folder and trash.
- Change notifications (watching the folder). The GUI refetches after its own changes.
- Thumbnails of unmanaged images (`fs.thumbnail`). Later, over corundum-framework-image.
- All-or-nothing (atomic) batches. Batches are best effort; see "Undo".
- Resuming a batch a crash interrupted.
- Moving asteria's journal onto this log, and adopting it in sapphire. Each is its own
  piece of work once this exists.

## Crates

| Crate | What it is | Depends on |
|---|---|---|
| `corundum-framework-fs-api` | The `fs.*` and `journal.*` method names, their serde types, the error codes, the API version | serde |
| `corundum-framework-journal` | The log: the `Operation` trait, the registry, batches, undo and redo, history, the `JournalStore` trait and its stores. Knows nothing of files | serde, serde_json, grain-id |
| `corundum-framework-fs` | The workspace engine: paths, hiding, listing and reading, the built-in operations, the app's hook, `FsService::dispatch` | fs-api, journal |
| `corundum-framework-gui`, feature `fs-client` | `FsTransport`, `FsClient`, `ExplorerDriver`, `FsReader`, `HistoryPage` | fs-api |

The API crate is apart so that its version moves only when the API does, as
`sapphire-framework-server-api` is. A client depends on it and not on the engine. The log
is apart from the engine so that an app can log operations that are not file operations
(asteria's tag writes, its uploads) without the engine.

The facade gets the features `fs-api`, `journal`, `fs` (each re-exporting its crate as a
module), `journal-jsonl`, and `gui-fs-client` (the gui crate's `fs-client`).

## `corundum-framework-journal`

### Operations

An operation is a kind with two serde types: what was asked, and what happened.

```rust
pub trait Operation<C>: Send + Sync + 'static {
    /// Stable, namespaced: "fs.move", "asteria.move", "asteria.write_sidecar".
    const KIND: &'static str;
    /// The request and its preconditions (the size, mtime and hash the client saw).
    type Plan: Serialize + DeserializeOwned + Send;
    /// What happened: enough to build the inverse.
    type Done: Serialize + DeserializeOwned + Send;

    /// The preconditions alone, without changing anything: what a dry run calls.
    fn check(&self, ctx: &C, plan: &Self::Plan) -> Result<(), Skip> { Ok(()) }
    fn apply(&self, ctx: &C, plan: &Self::Plan) -> Result<Applied<Self::Done>, Skip>;
    /// The plan that undoes `done`, its preconditions being the state `apply` left.
    /// `None` when the operation cannot be undone.
    fn inverse(&self, done: &Self::Done) -> Option<Self::Plan>;
}

pub struct Applied<D> {
    pub done: D,
    /// Run in the same batch, right after this one.
    pub children: Vec<Planned>,
}
```

- `C` is the app's context: the workspace root, asteria's cache, and so on.
- `Planned` is a kind name and its plan as JSON. `Planned::new::<O>(plan)` builds one from a
  typed plan.
- A `Registry<C>` maps kind names to operations. The app registers its kinds next to the
  engine's built-in ones. Registering one name twice panics.
- **Children** run after their parent, in order, and are recorded with a `parent` link.
  This is how a move takes its sidecar and how a playlist is rewritten after a move.
- `Skip` carries a reason: `precondition_failed`, `exists`, `not_found`, `invalid_path`,
  `denied`, `io` (with a message), `unknown_kind`, `not_undoable`. A skipped operation does
  not stop the batch, and its children do not run.

### Records

What the store keeps:

- `BatchBegin { batch_id, origin, actor, at, label, undo_of, redo_of }`
- `Op { op_id, batch_id, parent, kind, plan, outcome }`, where `outcome` is `Done(json)` or
  `Skipped(reason)`. It is written **after** the operation ran. A stop between the two loses
  only the undo of that one operation, as asteria's journal does now.
- `BatchEnd { batch_id, done, skipped, interrupted }`

The types:

- `BatchId` and `OpId` are grain ids.
- `Origin` is a string the client chooses, one per UI session: a GUI window, an MCP
  session. Undo and redo are kept per origin.
- `Actor { principal: String, agent: bool }`. The app chooses `principal`: asteria's user
  id; for sapphire, the user or the device. An agent acting for a person has that person's
  principal and `agent: true`.

### Running batches

`Journal<C>` has **one writer**. Batches run one at a time on a blocking worker, in the
order they were submitted.

- `submit(ops, origin, actor, label) -> BatchId`, then `status(batch_id)` for the progress
  and, at the end, each operation's result.
- `dry_run(ops)` runs no `apply`. It checks each kind is known and each plan parses, calls
  each kind's `check`, and returns what would be attempted. Children are not known before
  their parent runs, so a dry run does not list them.
- `history(cursor, limit)`, newest first, and `batch(batch_id)` with its operations.

### Undo

Undo appends a **compensating batch**. It never erases history.

- The operations of the batch being undone are taken in reverse order, children before
  their parents. Each `inverse` gives a plan, and the plans are submitted as a new batch
  with `undo_of`. An operation whose preconditions no longer hold is skipped and reported.
  So is one with no inverse, or a kind this build does not know.
- **`undo(origin)`** undoes the latest batch of that origin that is not undone yet: Ctrl+Z.
  **`redo(origin)`** undoes the latest undo of that origin. The redo stack of an origin is
  cleared when that origin submits a new batch.
- **`undo_batch(batch_id)`** undoes the named batch, whatever its origin: the history
  screen. If newer batches touched the same files, their operations are skipped.
- **Who may undo**: by default, only an actor with the batch's `principal`. A person can
  undo what an agent did for them, and in asteria no user undoes another's work. The app
  replaces the rule with `may_undo(&Actor, &BatchBegin) -> bool`.
- **The undo window**: `UndoPolicy { max_batches, max_age }`, by default the last 20
  batches and one hour. An older batch stays in history but its undo is refused with
  `expired`. Undoing a batch twice is refused with `already_undone`.

### Stores

```rust
pub trait JournalStore: Send {
    fn append(&mut self, record: &Record) -> Result<(), StoreError>;
    fn batches_rev(&self, cursor: Option<BatchId>, limit: usize) -> Result<Vec<BatchSummary>, StoreError>;
    fn batch(&self, id: BatchId) -> Result<Option<BatchRecords>, StoreError>;
}
```

- **`MemoryStore`** is the default. It keeps the last `capacity` batches (by default twice
  `UndoPolicy::max_batches`) and drops the oldest. A server restart forgets them, so there
  is nothing to recover.
- **`JsonlStore`**, behind the `jsonl` feature, is for an app that wants a long history to
  audit ("when did the agent move this?"). It writes one `YYYY-MM.jsonl` file per month
  (UTC), in the server instance's state directory, never in the workspace, and builds its
  batch index in memory when it opens. On open, a batch with no `BatchEnd` gets one with
  `interrupted: true`; its finished operations stay undoable within the window.
- A store in the workspace (one file per device, carried by sync) is left for later; the
  trait leaves room for it.

## `corundum-framework-fs`

### Paths

- `WsPath` is a workspace-relative path with `/` separators, normalized. It is refused on
  construction if it is absolute or holds `..`, an empty segment or NUL.
- The workspace root is canonicalized once. Symlinks are listed, as `symlink` entries, and
  never followed. A read or an operation that would leave the root is refused with
  `invalid_path`.

### Hiding

- Every entry whose name starts with `.` is hidden: `.asteria/`, `.sapphire/`, `.DS_Store`.
- The app adds `WorkspaceApp::hidden(&WsPath) -> bool` for the files it keeps under other
  names. asteria's sidecars are `photo.jpg.xmp` beside the photo, the name darktable and
  Lightroom read, so they are hidden this way and not renamed.
- A hidden entry is never listed, and `fs.*` refuses every operation on it with
  `invalid_path`. The app's own kinds (through `Replace` and `follow`) still reach it: that
  is how a sidecar moves with its photo.

### Reading (not logged)

- `fs.list { path, cursor, limit }` returns `{ entries, next_cursor }`, in name order. An
  entry has `name`, `kind` (`file`, `dir` or `symlink`), `size`, `modified`, and `managed`:
  `None`, or `{ kind, item_id }` from the app. The explorer sorts what it has loaded.
- `fs.stat { path }` returns one entry.
- `fs.read { path, offset, len }` returns `{ data, total }`, with `data` in base64 and `len`
  at most 1 MiB. It works the same over HTTP and over IPC. asteria can keep `/media` as its
  fast path for its own items.

### Changes (logged)

All changes come through `fs.apply { ops, label, dry_run }`, so one action of the person (a
drag of several entries) is one batch and one undo. Its result is the batch id and each
operation's result, as `journal.status` gives them.

The built-in kinds:

- **`fs.move { from, to, expect }`**. A rename is a move within one folder. It never
  replaces an existing entry (`exists`). A change of case only is done through a temporary
  name on file systems that ignore case. A move to another device is skipped with `io`.
  Moving a folder moves everything in it.
- **`fs.mkdir { path }`**. Its undo removes the folder if it is still empty, and is skipped
  otherwise.
- **`fs.trash { path, expect }`** moves the entry to `<the app's hidden folder>/trash/<op_id>/<path>`.
  Its undo moves it back, unless something is in its place by then.

`expect` is the `size` and `mtime` the client saw, and optionally a `file/1` hash. A file
that changed since is skipped with `precondition_failed`.

### The app's hook

```rust
pub trait WorkspaceApp: Send + Sync {
    /// Names under the root the app keeps for itself, besides every dot-name.
    fn hidden(&self, path: &WsPath) -> bool { false }
    /// Fills in `managed` for the entries of one listed folder.
    fn annotate(&self, dir: &WsPath, entries: &mut [Entry]) {}
    /// How a requested operation on a managed file is carried out.
    fn route(&self, op: &FsOp) -> Route { Route::Fs }
    /// Operations to run after a built-in one: rewrites, reindexing.
    fn follow(&self, op: &FsOp, done: &FsDone) -> Vec<Planned> { Vec::new() }
    /// Who may undo a batch (see "Undo").
    fn may_undo(&self, actor: &Actor, batch: &BatchBegin) -> bool { actor.principal == batch.actor.principal }
}

pub enum Route {
    /// Carry it out as a built-in operation.
    Fs,
    /// Carry out the app's own operations instead: `asteria.move` for a managed photo,
    /// which takes its sidecar and updates the cache.
    Replace(Vec<Planned>),
    /// Refuse it, with a reason for the person.
    Deny(String),
}
```

For a folder, `route` and `follow` get the folder's path. The app handles the managed files
under it itself (asteria updates the cache's paths under that prefix).

### Mounting

`FsService<C>` holds the engine, the registry and the journal.

```rust
impl<C> FsService<C> {
    pub async fn dispatch(&self, method: &str, params: Value, actor: &Actor) -> Result<Value, FsError>;
}
```

- It serves `fs.list`, `fs.stat`, `fs.read`, `fs.apply`, `journal.undo { origin }`,
  `journal.redo { origin }`, `journal.undo_batch { batch_id }`, `journal.history { cursor, limit }`
  and `journal.status { batch_id }`.
- It answers `FsError::MethodNotFound` for any other method, so a server tries it first or
  last next to its own methods.
- Each `FsError` maps to a code in `fs-api::codes`, which the server sends as the JSON-RPC
  error. The codes are in their own range, so they do not clash with asteria's
  (`-32001` to `-32009`).
- The blocking work (listing, reading, the writer) runs off the async threads.
- asteria calls it from `/rpc`, sapphire from its IPC router. Each passes the actor it
  authenticated: the API key's profile in asteria, the IPC peer in sapphire.

## `corundum-framework-gui`, feature `fs-client`

### Transport and client

```rust
pub trait FsTransport: Send + Sync {
    fn call(&self, method: &str, params: Value) -> Pending<Result<Value, FsError>>;
}
```

- The app implements this one method: asteria over HTTP `/rpc`, sapphire over its IPC
  client.
- `LocalTransport` (feature `fs-local`) calls an in-process `FsService::dispatch`.
- `FsClient` is the typed client over a transport: `list`, `stat`, `read`, `apply`, `undo`,
  `redo`, `undo_batch`, `history`, `status`. It makes the session's `Origin` once and puts it
  in every request.

### `ExplorerDriver`

It owns a `FileExplorer` and an `FsClient`, and turns the explorer's events into calls:

- **Going to a folder** calls `fs.list`. When the explorer asks for more, the next page is
  fetched and added with `append_entries`. A failure goes to `set_failed`.
- **Rename, a drag onto a folder, and Trash in the menu** each become one `fs.apply`, with
  the `size` and `mtime` from the listing as `expect`. The label ("Move 3 items") comes from
  `tr!`.
- **After a change**, the current folder is fetched again.
- **Ctrl+Z and Ctrl+Shift+Z**, while the explorer has focus, call `journal.undo` and
  `journal.redo` with the session's origin. The result is a one-line notice ("Undid 'Move 3
  items' (1 skipped)"), which opens to the skipped operations and their reasons.
- **A managed entry** is decorated by the app's `decorate(&mut Entry, &Managed)`, with its
  icon and thumbnail. Opening it returns `DriverEvent::OpenManaged { item_id }`, so the app
  opens its own view (asteria's photo viewer).
- **An unmanaged file, selected**, is shown by `FilePreview` in the detail pane, over
  `FsReader`.

### `FsReader`

A `Read + Seek` over `fs.read`, a chunk at a time, waiting on the preview's worker thread
(never on the UI thread). It is handed to the previews as their reader opener, so their
limits (64 MiB for a PDF and so on) apply as for local files.

### `HistoryPage`

The history screen. The app places it, as a screen or a settings page.

- The batches, newest first: label, time, who (with a badge when an agent did it), whether
  it came from this session, the done and skipped counts, and whether it was undone. A row
  opens to its operations.
- Each row has Undo, enabled when the batch is in the undo window and `may_undo` allows it.
  For a batch of another session (an agent's work), Undo first asks for confirmation:
  "This change was made by the agent at 14:02. Undo it?"

## Errors

| Error | When |
|---|---|
| `invalid_path` | The path is malformed, leaves the root, or names a hidden entry |
| `not_found` | The path does not exist |
| `too_large` | `fs.read`'s `len` is over 1 MiB, or `fs.apply` has more than 10 000 operations |
| `nothing_to_undo` / `nothing_to_redo` | The origin's stack is empty |
| `already_undone` | The batch was undone before |
| `expired` | The batch is outside the undo window |
| `forbidden` | `may_undo` refused |
| `busy` | The writer's queue is full |
| `api_mismatch` | The client's API major differs from the server's |

An operation inside a batch is never an error: it is done or skipped, with its reason.

## Testing

- **journal**: a test context with fake kinds. Covered: order, children, skips, the stacks
  per origin, redo cleared by a new batch, `undo_batch`, the window, `may_undo`,
  `already_undone`, unknown kinds, and `JsonlStore`'s round trip and interrupted batches.
- **fs**: on a temporary directory. Covered: path refusal (`..`, absolute, a symlink out of
  the root), hiding of dot-names and of the app's `hidden`, paging, range reads, no
  replacing on move, case-only renames, trash and its undo, preconditions, and `route` and
  `follow` with a fake app (a sidecar that moves with its file).
- **fs-api**: each type round-trips through JSON, method names are namespaced, and the API
  version is the crate's major.
- **gui**: `ExplorerDriver` and `HistoryPage` driven headless in egui over `LocalTransport`
  and an `FsService` on a temporary directory, as the explorer's tests are.

## Order of work

1. `fs-api` and `fs` with reading only (`list`, `stat`, `read`, the paths, the hiding,
   `annotate`).
2. `journal`, with `MemoryStore`.
3. The changes in `fs` (the built-in kinds, `fs.apply`, `route` and `follow`) on the
   journal.
4. The gui's `fs-client`: the client, `ExplorerDriver`, `FsReader`, `HistoryPage`.
5. `JsonlStore`.

Then, apart from this spec: asteria serves these methods and moves its `organize` kinds onto
the journal; sapphire serves them next to `workspace.*`.
