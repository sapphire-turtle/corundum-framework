# Updating from the GUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The desktop app's Settings → About lists the app, its server and (for sapphire) sapphire-bridge, each with an Update button that downloads the GitHub release, replaces the executable and restarts it.

**Architecture:** `sapphire-framework-update` moves to corundum as `corundum-framework-update` (features `cli`, `self-update`), gains `replace_file` for an executable other than the running one and `InstallMethod::Store`. `corundum-framework-gui` gets an `update` module: a pure row state machine and an `UpdatesPanel` whose worker thread does the network and file work. sapphire-framework then re-exports the moved crate, adds `service restart`, and puts the panel with three targets on its About page.

**Tech Stack:** Rust 2024, reqwest, semver, self-replace 1.5, tempfile, tokio (current-thread runtime on the panel's worker), egui 0.37.

**Spec:** `docs/superpowers/specs/2026-10-27-gui-update-design.md`

**Repositories:** Part A is `sapphire-turtle/corundum-framework`. Part B is `sapphire-turtle/sapphire-framework` and starts after Part A is released to crates.io.

## Global Constraints

- Feature `self-update` (default on) holds every download and replacement; without it `self-replace` is not linked and no Update button exists.
- Feature `cli` (default off) holds `UpdateArgs` and `run` (clap).
- Never replace an executable whose `InstallMethod` is not `Release`; show the advice instead.
- Never downgrade: install only when the release's version is greater than the current one.
- Downloads keep the existing limits (`MAX_DOWNLOAD`, `DOWNLOAD_TIMEOUT`) and come only from the repository's own release over HTTPS.
- One target is updated at a time.
- The GUI checks for releases only when the panel is first shown and on "Check"; no check at start.
- `server.restart` is not used to apply an update (it does not load the new executable).
- Messages through Fluent, in `en` and `ja`, ids prefixed `corundum-update-` (gui) as the crate's other ids are.

**Refinement of the spec:** `AfterUpdate::Restart` returns a `RestartOutcome` (`Restarted`, `ByHand`, `Failed(String)`) instead of `Result<(), String>`, so a service found not installed when restarting shows "restart it by hand" rather than an error. sapphire's restart runs `service status` first and answers `ByHand` when it fails.

## Review Focus

- The newest release has no asset for this platform (e.g. a new OS/arch): the row says so and offers the release page, not a generic error. → test in Task 4.
- The server is not running, so its current version is unknown (`current: None`): the row still shows the newest release and offers Update. → test in Task 4.
- A `path.old` left by an earlier update on Windows is still locked (the old process lives): the update proceeds, the leftover stays, no error. → test in Task 2.
- The download fails half-way or is cut: nothing is replaced, the staging file is gone, the row offers Retry. → test in Task 2 (staging removed on error) and Task 4 (Failed → Retry).
- The app relaunches itself while a second instance cannot start (exe replaced by a broken file): the old window stays open with the error, rather than closing first. → test in Task 5 (spawn failure keeps the window).

---

## Part A: corundum-framework

### Task 1: Move the crate

**Files:**
- Create: `crates/corundum-framework-update/` from `sapphire-framework` `origin/main:crates/sapphire-framework-update/` (`Cargo.toml`, `src/lib.rs`, `src/install.rs`, `src/release.rs`)
- Modify: root `Cargo.toml` (workspace deps `reqwest`, `thiserror` as sapphire-framework pins them), `release-plz.toml` (`corundum` group, facade `changelog_include`), `crates/corundum-framework/Cargo.toml` (feature `update = ["dep:corundum-framework-update"]`, re-export `pub use corundum_framework_update as update` under it), READMEs (crate table row).

**Interfaces:**
- Produces (unchanged names, now from `corundum_framework_update`): `ReleaseSource`, `Release`, `Asset`, `latest_release`, `client`, `asset_name`, `platform`, `InstallMethod`, `BinstallSource`, `install_method`, `Error`, `Result`; behind `self-update`: `install`; behind `cli`: `UpdateArgs`, `run`.
- Newly public, for the panel: `pub async fn download_asset(client: &reqwest::Client, source: &ReleaseSource, release: &Release) -> Result<Vec<u8>>` (today's private `fetch`, taking the base from `source.asset_base()`, `Error::NotConfigured` when none) behind `self-update`; `pub fn advice(source: &ReleaseSource, release: &Release, method: &InstallMethod) -> String` (today's `manual_advice`); `pub fn cargo_home() -> Option<PathBuf>`.

- [ ] **Step 1: Copy** the files; rename the package; `[features] default = ["self-update"]`, `self-update = ["dep:self-replace", "dep:tempfile"]`, `cli = ["dep:clap"]`.
- [ ] **Step 2: Generalize the prose**: doc comments that say "sapphire binary" say "an app's binary"; test data keeps real sapphire names where it tests their tag layouts (they are examples of a layout, not assumptions).
- [ ] **Step 3: Gate** `install`, `download`, `download_asset`, `run` and their tests on the features; `advice` stays ungated.
- [ ] **Step 4: Run** `cargo test -p corundum-framework-update --all-features`, `cargo test -p corundum-framework-update --no-default-features`, `cargo clippy -p corundum-framework-update --all-features -- -D warnings`, `cargo deny check` — all PASS.
- [ ] **Step 5: Run** `cargo tree -p corundum-framework-update --no-default-features -e normal | grep -c self-replace` — prints `0`. Add the same check to `.github/workflows/ci.yml` as a step after the shared CI job (a small job on ubuntu).
- [ ] **Step 6: Commit** `feat(corundum-framework-update): move the release lookup and self-update from sapphire-framework`.

---

### Task 2: `replace_file`

**Files:**
- Create: `crates/corundum-framework-update/src/replace.rs` (behind `self-update`)

**Interfaces:**
- Produces:
  - `pub fn replace_file(path: &Path, bytes: &[u8]) -> Result<()>`
  - `pub(crate) trait Fs { fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>; fn rename(&self, from: &Path, to: &Path) -> io::Result<()>; fn remove(&self, path: &Path) -> io::Result<()>; fn exists(&self, path: &Path) -> bool; }` with `RealFs`
  - `pub(crate) fn replace_unix(fs: &dyn Fs, path: &Path, staged: &Path) -> io::Result<()>` and `pub(crate) fn replace_windows(fs: &dyn Fs, path: &Path, staged: &Path) -> io::Result<()>`; `replace_file` stages with `tempfile` in `path`'s directory (mode 0755 on Unix, `sync_all`) and calls the one for this OS. The staging file is removed on every error.

- [ ] **Step 1: Write the failing tests** (all OSes; a `RecordingFs` that records calls and fails on a chosen one)

```rust
#[test] fn unix_renames_over_the_target() {
    // calls == [rename(staged, path)]
}
#[test] fn windows_sets_the_running_exe_aside() {
    // path.old absent: calls == [rename(path, path.old), rename(staged, path)]
}
#[test] fn windows_removes_an_earlier_leftover_first() {
    // path.old exists: calls start with remove(path.old)
}
#[test] fn windows_goes_on_when_the_leftover_is_locked() {
    // remove(path.old) fails: result Ok; the target is renamed to path.old.1 instead
}
#[test] fn windows_puts_the_old_one_back_when_the_move_fails() {
    // rename(staged, path) fails: last call rename(path.old, path); result Err
}
#[test] fn the_real_thing_replaces_content_and_keeps_it_executable() {
    // tempdir: write "old" to f (0755 on unix), replace_file(f, b"new"), read == "new",
    // unix mode & 0o111 != 0, and the directory holds only f
}
#[test] fn a_failed_replace_leaves_no_staging_file() {
    // replace_file on a path whose parent does not exist → Err, nothing left in the tempdir
}
```

When `path.old` cannot be removed, use the first free `path.old.<n>`; the next update removes those too (`path.old*` in the directory).

- [ ] **Step 2: Run** `cargo test -p corundum-framework-update replace` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(corundum-framework-update): replace another program's executable`.

---

### Task 3: `InstallMethod::Store`

**Files:**
- Modify: `crates/corundum-framework-update/src/install.rs`

**Interfaces:**
- Produces: `InstallMethod::Store(StoreKind)`, `pub enum StoreKind { Flatpak, Snap, Msix, MacAppStore }`; `pub(crate) struct Probe { pub root: PathBuf, pub snap: bool, pub msix: bool }` with `Probe::detect()`; `pub fn install_method_with(exe: &Path, cargo_home: Option<&Path>, probe: &Probe) -> InstallMethod`; `install_method` calls it with `Probe::detect()`. `advice` for `Store(_)`: "installed from <store>; it updates there" (`Flathub`, `the Snap Store`, `the Microsoft Store`, `the Mac App Store`).

Detection order, first match wins: `<root>/.flatpak-info` exists → Flatpak; `snap` (`$SNAP` set) → Snap; `msix` (Windows: `GetCurrentPackageFullName` returns success; `false` elsewhere) → Msix; `exe` is `…/Contents/MacOS/<x>` and `…/Contents/_MASReceipt/receipt` exists → MacAppStore; then today's rules.

- [ ] **Step 1: Write the failing tests**: one per kind on a temporary root (the macOS one builds `App.app/Contents/{MacOS/app,_MASReceipt/receipt}`), and that a plain `~/.local/bin` path with no marker stays `Release`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-update install_method` — FAIL.
- [ ] **Step 3: Implement.** MSIX through `windows` 0.62 (`Win32_Storage_Packaging_Appx`), `cfg(windows)` only.
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(corundum-framework-update): recognise an app installed from a store`.

---

### Task 4: The row state machine

**Files:**
- Create: `crates/corundum-framework-gui/src/update/mod.rs`, `src/update/state.rs`
- Modify: `crates/corundum-framework-gui/Cargo.toml` (feature `update = ["dep:corundum-framework-update", "dep:semver", "dep:tokio"]`, `self-update = ["update", "corundum-framework-update/self-update"]`; the update dep with `default-features = false`; `tokio` with `rt`), `src/lib.rs` (`#[cfg(feature = "update")] pub mod update;`), facade features `update`/`self-update` passing through.

**Interfaces:**
- Produces:

```rust
pub enum RowState {
    Checking,
    UpToDate { latest: Version },
    Newer { release: Release },                 // Update offered (self-update only)
    NotHere { release: Release, advice: String },// installed by cargo, binstall, a package manager, a store; or no asset
    Working { release: Release, step: Step },   // Step::Downloading | Step::Restarting
    Done { version: Version },
    RestartByHand { version: Version },
    Failed { message: String, retry: Retry },   // Retry::Check | Retry::Update(Release)
}
pub(crate) enum Event { Checked(Result<Release, String>), Downloaded(Result<(), String>), Restarted(RestartOutcome) }
pub enum RestartOutcome { Restarted, ByHand, Failed(String) }
pub(crate) fn on_check(current: Option<&Version>, method: &InstallMethod, has_asset: bool, can_install: bool, source: &ReleaseSource, found: Result<Release, String>) -> RowState
pub(crate) fn on_event(state: RowState, event: Event) -> RowState
```

`can_install` is `cfg!(feature = "self-update")`. `has_asset` is whether `release.assets` holds `asset_name(base)`.

- [ ] **Step 1: Write the failing tests** in `state.rs`, one per transition:
  - newer release, `Release` method, asset present, can install → `Newer`
  - same version → `UpToDate`; older release → `UpToDate` (never a downgrade)
  - `current: None` with a release → `Newer`
  - `Cargo` / `Binstall{Git}` / `Store(Flatpak)` → `NotHere` with `advice(...)` text (`cargo binstall --git <url> <crate>` for the Git case)
  - newer release without this platform's asset → `NotHere` whose advice names the release page URL
  - `can_install == false` → `NotHere` with "download it from <url>"
  - check error → `Failed { retry: Retry::Check }`
  - `Working{Downloading}` + `Downloaded(Err)` → `Failed { retry: Retry::Update(release) }`
  - `Working{Restarting}` + `Restarted(Restarted)` → `Done`; `ByHand` → `RestartByHand`; `Failed(m)` → `Failed` with `m`
- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --features update update::state` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(corundum-framework-gui): the update panel's row states`.

---

### Task 5: `UpdatesPanel`

**Files:**
- Create: `crates/corundum-framework-gui/src/update/panel.rs`, `src/update/worker.rs`
- Modify: `crates/corundum-framework-gui/src/i18n/{en,ja}/*.ftl` (the crate's message files)

**Interfaces:**
- Consumes: Task 1 (`client`, `latest_release`, `download_asset`, `install`, `install_method`, `cargo_home`, `advice`), Task 2 (`replace_file`), Task 4.
- Produces: `pub struct UpdateTarget { pub label: String, pub current: Option<Version>, pub source: ReleaseSource, pub exe: PathBuf, pub after: AfterUpdate }`; `pub enum AfterUpdate { Relaunch, Restart(Box<dyn Fn() -> RestartOutcome + Send + Sync>), Manual }`; `pub struct UpdatesPanel`, `UpdatesPanel::new(targets: Vec<UpdateTarget>, user_agent: &str) -> Self`, `fn ui(&mut self, ui: &mut egui::Ui)`; `pub(crate) fn relaunch(exe: &Path) -> io::Result<()>` (spawn `exe` with `std::env::args_os().skip(1)`).

Worker: one `std::thread`, a current-thread tokio runtime, jobs over an `mpsc` channel (`Check(index)`, `Update(index, Release)`), results back as `(index, Event)`, then `ctx.request_repaint()`. The panel holds a clone of the `egui::Context` from its first `ui` call. Update of a target: `download_asset` → for `Relaunch` `install(bytes, exe, self_replace::self_replace)`, otherwise `replace_file(exe, bytes)` → `after`: `Relaunch` sends `Restarted(Restarted)` only after `relaunch` succeeded, and the panel then sends `egui::ViewportCommand::Close`; a spawn failure is `Restarted(Failed(..))` and the window stays; `Manual` is `ByHand`. While one target is `Working`, the other rows' buttons are disabled.

Rows: label · current version (or "not running") · state text · button (Update / Retry / Copy for `NotHere` advice containing a command) · release-notes link (`release.url`) when a release is known. A **Check** button above the rows re-sends `Check` for every row not `Working`.

- [ ] **Step 1: Write the failing tests**
  - `panel` renders (`test_support::render`) with two targets and a stub worker (a `pub(crate)` constructor taking a job sender and result receiver) and sends `Check(0)`, `Check(1)` on its first frame — and no job on the second.
  - Feeding `(0, Checked(Ok(newer)))` shows an enabled Update for row 0; clicking it sends `Update(0, release)` and disables row 1's button.
  - `relaunch` of a path that does not exist returns `Err` (the window is not closed: no `Close` command was queued).
- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --features self-update update` — FAIL.
- [ ] **Step 3: Implement**, messages in `en` and `ja`.
- [ ] **Step 4: Run** — PASS; `cargo test -p corundum-framework-gui --features update` (no self-update) PASS; `cargo clippy --workspace --all-features -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat(corundum-framework-gui): the updates panel`.

---

### Task 6: Demo and docs; release gate

**Files:**
- Modify: `demo/src/screens/settings.rs` (About shows an `UpdatesPanel` with one target: the demo itself, `ReleaseSource::github("sapphire-turtle", "corundum-framework").tag_prefix("v")` without an asset, so it shows the newest release and "download it from…" — the demo is not released as a binary), `demo/Cargo.toml` (features `update`).
- Modify: `README.md`, `README.ja.md` (the update crate, the gui features `update` / `self-update`, and that a store build turns `self-update` off).

- [ ] **Step 1: Run** `cargo run -p corundum-framework-demo`, open Settings → About: the row reaches "Up to date"/"Newer" with the newest tag, and the link opens the release. Record in the PR.
- [ ] **Step 2: Run** `cargo test --workspace --all-features` — PASS.
- [ ] **Step 3: Commit** `feat(demo): the updates panel on About`.
- [ ] **Step 4: Release gate.** Open the PR; after merge, merge the release-plz PR so `corundum-framework-update` and the gui's new features are on crates.io. Part B depends on that version.

---

## Part B: sapphire-framework

### Task 7: `sapphire-framework-update` re-exports

**Files:**
- Replace: `crates/sapphire-framework-update/src/{lib.rs,install.rs,release.rs}` with one `lib.rs`: `pub use corundum_framework_update::*;` and a doc comment saying the crate moved and will be removed in the next release.
- Modify: `crates/sapphire-framework-update/Cargo.toml` (dependency `corundum-framework-update = { workspace = true, features = ["cli", "self-update"] }`), root `Cargo.toml` (workspace dep at the Part A version).

- [ ] **Step 1: Run** `cargo test --workspace` — PASS (the server's `update.rs`, the bridge's `update` command and the CLIs compile unchanged).
- [ ] **Step 2: Commit** `refactor(update): re-export corundum-framework-update`.

---

### Task 8: `service restart`

**Files:**
- Modify: `crates/sapphire-framework-service/src/manager.rs` (`ServiceCommand::Restart`, `pub fn restart(spec, env, manager) -> Result<()>`, `fn restart_commands(spec, env) -> Vec<Vec<String>>`)

**Interfaces:**
- Produces: `restart` and the CLI verb `<app> service restart` (every app's and the bridge's CLI get it through `ServiceCommand`), printing `restarted the <app> service`.

| OS | Commands |
|---|---|
| Linux | `systemctl --user restart <app>.service` |
| macOS | `launchctl kickstart -k gui/<euid>/<label(app)>` |
| Windows | `schtasks /end /tn <app>`, then `schtasks /run /tn <app>` (a failing `/end`, as for a stopped task, is ignored) |

- [ ] **Step 1: Write the failing tests** with `RecordingManager`, one per OS, asserting the recorded commands exactly; and that on Windows a failing `/end` still runs `/run`.
- [ ] **Step 2: Run** `cargo test -p sapphire-framework-service restart` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(service): restart an installed service`.

---

### Task 9: The About page's updates

**Files:**
- Create: `crates/sapphire-framework-gui/src/updates.rs`
- Modify: `crates/sapphire-framework-gui/src/settings.rs` (`about_ui` shows the panel under the services), `Cargo.toml` (`corundum-framework-gui` features `update`; a `self-update` feature, default on, passing to `corundum-framework-gui/self-update`), the GUI's `ClientConfig`/app info if the release sources are not reachable there yet.

**Interfaces:**
- Consumes: Task 5 (`UpdatesPanel`, `UpdateTarget`, `AfterUpdate`, `RestartOutcome`), Task 8 (`<exe> service restart`), the existing `ClientConfig.service_exes` and the snapshot's versions.
- Produces: `pub(crate) fn targets(cfg: &ClientConfig, snapshot: &Snapshot) -> Vec<UpdateTarget>` — app (`current_exe()`, `CARGO_PKG_VERSION` of the app as `ClientConfig.app.version`, `Relaunch`), server (`service_exes.app`, server version from `Conn::Up`, `Restart`), bridge (`service_exes.bridge`, bridge version, `Restart`); the release sources come from the app's existing update configuration (the same `ReleaseSource`s its server and CLI use: tag prefixes `desktop-v` for the desktop app, `v` for the server, the bridge's from sapphire-framework).
- `pub(crate) fn restart_service(exe: &Path) -> RestartOutcome`: `<exe> service status` failing → `ByHand`; else `<exe> service restart` → `Restarted` or `Failed(stderr)`.

- [ ] **Step 1: Write the failing tests**: `targets` with a snapshot where the server is `Conn::Absent` gives the server `current: None`; labels and exes match `service_exes`; `restart_service` against a stub script (Unix test, a shell script in a tempdir that exits 1 for `status`) → `ByHand`.
- [ ] **Step 2: Run** `cargo test -p sapphire-framework-gui updates` — FAIL.
- [ ] **Step 3: Implement**; the panel is built once per Settings, its targets' versions refreshed when the snapshot changes.
- [ ] **Step 4: Run** `cargo test --workspace` PASS; manual: with sapphire-journal desktop from a release in `~/.local/bin`, About lists the three with their versions. Record in the PR.
- [ ] **Step 5: Commit** `feat(gui): update the app, its server and the bridge from About`.

asteria's wiring (its two targets) is asteria's own change after Part A, following Task 9.
