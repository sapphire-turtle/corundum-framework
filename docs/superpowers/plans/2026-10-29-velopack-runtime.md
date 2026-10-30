# The app as a Velopack package (runtime) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every corundum desktop app, installed as a Velopack package, its start and hooks, its own updates (found and downloaded automatically, applied on demand), its command line on `PATH`, a Windows console shim, a launcher that steps aside, and update panel rows for packages.

**Architecture:** A new crate `corundum-framework-package` holds everything that needs the `velopack` crate; `corundum-framework-platform` learns to tell a package from the files around the executable without it; `corundum-framework-update` gets the shared package state types; `corundum-framework-gui`'s panel takes three kinds of target. The demo replaces the spike's own code with these pieces.

**Tech Stack:** Rust 2024 (toolchain from `rust-toolchain.toml`), `velopack` 1.2.161, egui 0.37, `windows` 0.62 (registry, job objects, `SendMessageTimeoutW`).

**Spec:** `docs/superpowers/specs/2026-10-29-velopack-runtime-design.md` (and the spike, `docs/superpowers/spikes/2026-10-28-velopack.md`).

## Global Constraints

- `velopack = "1.2.161"`, the same version as `VPK_VERSION` in `.github/workflows/demo-release.yml`.
- `corundum-framework-package` is desktop only: its `velopack` dependency sits under `[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]`; the crate builds empty on phones.
- Without the facade's `package` / `gui-package` (and the gui crate's `package`), `velopack` is not in the dependency tree.
- `corundum-framework-update` and `corundum-framework-platform` never depend on `velopack`.
- New crate version `0.3.0`, `license.workspace`, `repository.workspace`, in `release-plz.toml` with `version_group = "corundum"` and in the facade's `changelog_include`.
- Velopack's own apply-at-start is off: `VelopackApp::build().set_auto_apply_on_startup(false)`.
- The kept version lives in `AppDirs::new(app_name)?.data().join("package-version")`.
- `PATH` on Windows: `HKCU\Environment\Path`, type kept (`REG_EXPAND_SZ` stays so), then `WM_SETTINGCHANGE` with `"Environment"`.
- macOS link: `/usr/local/bin/<command>`; Linux link: `~/.local/bin/<command>` → `$APPIMAGE`.
- A `Delegated` row's status is asked every 5 seconds while the panel is shown.
- `UpdatesPanel::new(targets: Vec<Target>, user_agent: &str)`; `UpdateTarget` and `AfterUpdate` keep their names and fields.
- Comments and doc comments follow the crates' style: plain sentences, no "we", explain why.
- Build in this worktree with `CARGO_TARGET_DIR=~/.cache/claude-cargo-targets/corundum-velopack` (/tmp is RAM).

## Review Focus

- A Windows `Path` that already holds the directory spelled differently (`C:\…\CURRENT\`, a trailing `\`, quoted) gets no second copy, and removing it keeps every other entry, `%VAR%` references unexpanded (Task 5).
- On macOS the launcher removes only a bundle it generated, never the bundle the app runs from, even when both have the app's name (Task 7).
- `before_apply` failing applies nothing and leaves the row `Failed` with the error, so the app's services are not half stopped and then replaced (Task 4).
- A `package-version` file that is empty, garbage or not valid UTF-8 counts as a first start and never panics (Task 3).
- A downloaded package that is not newer than the running version (a leftover after a manual reinstall) is not applied at start (Task 4).

---

### Task 1: Telling a Velopack package from its files (`platform::package`)

**Files:**
- Create: `crates/corundum-framework-platform/src/package.rs`
- Modify: `crates/corundum-framework-platform/src/lib.rs` (add `pub mod package;`, under `#[cfg(not(any(target_os = "ios", target_os = "android")))]`, and a line in the module doc)

**Interfaces:**
- Produces:
  - `pub enum PackageKind { Windows, MacBundle, AppImage }` (`Clone, Copy, Debug, PartialEq, Eq`)
  - `pub struct PackageLayout { pub kind: PackageKind, pub root: PathBuf, pub bin_dir: PathBuf }` (`Clone, Debug, PartialEq, Eq`)
  - `pub fn current() -> Option<PackageLayout>`
  - `pub fn current_with(exe: &Path, appimage: Option<&Path>) -> Option<PackageLayout>` — every OS's rule is checked on every OS, so the tests run anywhere

Rules (Velopack 1.2.161's locator): Windows — `exe`'s parent is named `current` and its parent holds `Update.exe` (`root` = that parent, `bin_dir` = `current`); macOS — the path holds `X.app/` and `X.app/Contents/MacOS/UpdateMac` exists (`root` = the `.app`, `bin_dir` = `Contents/MacOS`); AppImage — `appimage` is `Some` and `exe`'s directory ends in `usr/bin` and holds `UpdateNix` (`root` = the AppImage path, `bin_dir` = that directory). `current()` passes `std::env::current_exe()` and `$APPIMAGE`.

- [ ] **Step 1: Write the failing tests** in `package.rs` (`#[cfg(test)] mod tests`), each laying out files in a `tempfile::tempdir()`:
  - `a_windows_install_is_current_beside_update_exe`: `<d>/Update.exe`, exe `<d>/current/app.exe` → `Some(PackageLayout { kind: Windows, root: d, bin_dir: d/current })`
  - `current_without_update_exe_is_not_a_package`: no `Update.exe` → `None`
  - `a_mac_bundle_holds_update_mac`: `<d>/Demo.app/Contents/MacOS/UpdateMac`, exe `.../MacOS/demo` → `MacBundle`, root `<d>/Demo.app`
  - `an_app_without_update_mac_is_not_a_package`: a `.app` without `UpdateMac` → `None`
  - `an_appimage_needs_the_variable_and_update_nix`: `<m>/usr/bin/UpdateNix`, exe `<m>/usr/bin/demo`, `appimage = Some("/home/u/Demo.AppImage")` → `AppImage`, root the AppImage path; the same with `appimage = None` → `None`
  - `a_cargo_build_is_not_a_package`: exe `<d>/target/debug/demo` → `None`
- [ ] **Step 2: Run** `cargo test -p corundum-framework-platform package::` — expected FAIL (module missing).
- [ ] **Step 3: Implement** `current_with` and `current` as above; plain `Path` checks (`exists`, `file_name`), no canonicalisation.
- [ ] **Step 4: Run** `cargo test -p corundum-framework-platform package::` — expected PASS; `cargo clippy -p corundum-framework-platform --all-targets -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat(corundum-framework-platform): tell a Velopack package from its files (#51)`

### Task 2: The shared package state and `InstallMethod::Velopack` (`corundum-framework-update`)

**Files:**
- Create: `crates/corundum-framework-update/src/package.rs`
- Modify: `crates/corundum-framework-update/src/lib.rs` (re-export), `src/install.rs` (`InstallMethod`, `install_method_with`, `advice`, `advice_command`), `Cargo.toml` (add `corundum-framework-platform = { version = "0.3.0", path = "../corundum-framework-platform" }`), `crates/corundum-framework-gui/src/update/state.rs` (`on_check` for the new variant)

**Interfaces:**
- Consumes: `corundum_framework_platform::package::current_with` (Task 1).
- Produces:
  - `pub struct PackageStatus { pub current: Option<semver::Version>, pub state: PackageState }`
  - `pub enum PackageState { Idle, Checking, UpToDate, Downloading { percent: u8 }, Ready { version: semver::Version }, Failed(String) }`
  - both `Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize` (sapphire's bridge API carries them)
  - `InstallMethod::Velopack`
  - `Probe` gains `appimage: Option<PathBuf>` (`$APPIMAGE` in `detect`)

`install_method_with` returns `Velopack` when `current_with(exe, probe.appimage.as_deref())` is `Some`, checked after the stores and before cargo. `advice` for `Velopack`: "Updated by the app itself: Settings › About." `advice_command`: `None`.

- [ ] **Step 1: Write the failing tests** in `install.rs`'s tests: `a_velopack_package_is_the_package_s_to_update` (a Windows layout in a tempdir → `InstallMethod::Velopack`, and `advice_command(..)` is `None`); in `package.rs`: `the_state_round_trips_through_json` (`Ready { version: 1.2.3 }` and `Downloading { percent: 40 }` through `serde_json`).
- [ ] **Step 2: Run** `cargo test -p corundum-framework-update` — expected FAIL.
- [ ] **Step 3: Implement** the types and the detection.
- [ ] **Step 4: Run** `cargo test -p corundum-framework-update` and `cargo clippy -p corundum-framework-update --all-targets --no-default-features -- -D warnings` — PASS / clean. Also `cargo test -p corundum-framework-gui --features update` still passes (the gui's `on_check` handles a new variant: treat `Velopack` like `PackageManager`, a `NotHere` row with the advice).
- [ ] **Step 5: Commit** `feat(corundum-framework-update): package state types and InstallMethod::Velopack (#51)`

### Task 3: The new crate and `start` (`corundum-framework-package`)

**Files:**
- Create: `crates/corundum-framework-package/Cargo.toml`, `README.md`, `src/lib.rs`, `src/start.rs`, `src/error.rs`
- Modify: `release-plz.toml` (entry + `changelog_include`), `crates/corundum-framework/Cargo.toml` and `src/lib.rs` (feature `package = ["dep:corundum-framework-package", "platform"]`, `pub use corundum_framework_package as package;`, the module table), `README.md` / `README.ja.md` crate tables

**Interfaces:**
- Consumes: `platform::package::current` (Task 1), `platform::dirs::AppDirs`.
- Produces:
  - `pub struct PackageSpec { pub app_name: &'static str, pub on_installed: Option<fn()>, pub on_uninstalling: Option<fn()>, pub before_apply: Option<fn() -> Result<(), String>>, pub cli: Option<CliSpec> }` (`CliSpec` from Task 5; until then declare `pub struct CliSpec { pub command: &'static str }` in `cli_path.rs` with only that field)
  - `pub enum Start { NotPackaged, FirstStart { version: Version }, Updated { from: Version, to: Version }, Same { version: Version } }`
  - `pub fn start(spec: &PackageSpec) -> Start`
  - `pub(crate) fn classify(kept: Option<&str>, firstrun: bool, now: &Version) -> Start`
  - `pub enum Error { NotPackaged, Network(String), Velopack(String), Prepare(String), Io(std::io::Error) }` with `thiserror`

`start`: build `VelopackApp` with `set_auto_apply_on_startup(false)`; on Windows `on_after_install_fast_callback` runs `spec.on_installed` (and Task 5 adds `cli_path::install`), `on_before_uninstall_fast_callback` runs `spec.on_uninstalling` (Task 5: `cli_path::uninstall`), the update callbacks do nothing; `run()`. Then `NotPackaged` when `platform::package::current()` is `None`; else read the kept version, `classify` with `VELOPACK_FIRSTRUN` set or not (read before `run()`, which removes it), write the package's version, and on macOS and Linux run `on_installed` for `FirstStart`. The package's version: `velopack::locator::auto_locate_app_manifest(LocationContext::FromCurrentExe)?.get_manifest_version()`.

- [ ] **Step 1: Write the failing tests** for `classify` (table, `start.rs`): kept `None` → `FirstStart`; kept `"1.0.0"`, now 1.0.0 → `Same`; kept `"1.0.0"`, now 1.1.0 → `Updated { from: 1.0.0, to: 1.1.0 }`; kept `"1.1.0"`, now 1.0.0 → `Updated` (a downgrade is still a change); kept `"1.0.0"`, now 1.0.0, `firstrun` → `FirstStart`; kept `""`, `"garbage"`, `" 1.0.0\n"` (trimmed → `Same`) — the first two `FirstStart`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-package` — expected FAIL.
- [ ] **Step 3: Implement** the crate, `classify`, `start`, `Error`; the kept file is read with `fs::read` + `String::from_utf8_lossy` so invalid UTF-8 is just garbage.
- [ ] **Step 4: Run** `cargo test -p corundum-framework-package`, `cargo clippy -p corundum-framework-package --all-targets -- -D warnings`, `cargo build -p corundum-framework --features package` — PASS / clean.
- [ ] **Step 5: Commit** `feat(corundum-framework-package): the app's start in a Velopack package (#51)`

### Task 4: Updates (`package::Updater`)

**Files:**
- Create: `crates/corundum-framework-package/src/updater.rs`, `src/backend.rs`

**Interfaces:**
- Consumes: `PackageSpec`, `Error` (Task 3); `PackageStatus`, `PackageState` (Task 2).
- Produces:
  - `pub enum Source { Github { repo: String, prerelease: bool }, Http(String) }`
  - `pub enum After { RestartApp, RestartWith(Vec<OsString>), Exit }`
  - `#[derive(Clone)] pub struct Updater` with `new(spec: &'static PackageSpec, source: Source) -> Result<Updater, Error>`, `on_change(&self, f: impl Fn() + Send + Sync + 'static)`, `status(&self) -> PackageStatus`, `check_now(&self)`, `run_in_background(&self, every: Duration)`, `apply(&self, after: After) -> Result<Infallible, Error>`, `apply_pending_at_start(&self) -> Result<(), Error>`
  - `pub(crate) trait Backend: Send + Sync { fn current(&self) -> Version; fn check(&self) -> Result<Option<UpdateInfo>, Error>; fn download(&self, info: &UpdateInfo, progress: &dyn Fn(u8)) -> Result<(), Error>; fn pending(&self) -> Option<Version>; fn apply(&self, info_or_pending: Applied, after: &After) -> Result<Infallible, Error>; }` with `pub(crate) enum Applied { Downloaded(UpdateInfo), Pending }`; the real one in `backend.rs` over `velopack::UpdateManager` (`GithubSource` / `HttpSource`; `apply` → `wait_exit_then_apply_updates(asset, false, restart, args)` then `std::process::exit(0)`; `pending` → `get_update_pending_restart()`), a fake in the tests that records calls and never exits (its `apply` returns `Err(Error::Velopack("fake applied".into()))` after recording)

The state lives behind `Arc<Mutex<…>>`; a single worker thread (spawned on first `check_now` / `run_in_background`) takes requests over a channel, so two checks never run at once. Check → `Checking` → `UpToDate`, or `Downloading { percent }` … → `Ready { version }`; errors → `Failed(msg)`. `apply`: not `Ready` → `Err(Error::Velopack("nothing downloaded"))`; else `spec.before_apply` → on `Err(e)` set `Failed(e.clone())` and return `Err(Error::Prepare(e))` without touching the backend; then `backend.apply`. `apply_pending_at_start`: `pending()` greater than `current()` → `before_apply`, then apply with `After::RestartApp`; otherwise `Ok(())`.

- [ ] **Step 1: Write the failing tests** (`updater.rs`, with the fake): `a_found_update_is_downloaded_and_ready`; `nothing_found_is_up_to_date`; `a_check_error_is_failed_and_a_later_check_recovers`; `apply_runs_before_apply_first` (the fake sees `before_apply` recorded before `apply`); `a_failing_before_apply_applies_nothing` (fake's `apply` never called, state `Failed("stop failed")`, `Err(Error::Prepare(_))`); `apply_without_a_download_is_an_error`; `a_pending_update_newer_than_current_is_applied_at_start`; `a_pending_package_not_newer_is_left_alone`; `on_change_is_called_on_every_state_change`. Tests wait on state with a short poll loop bounded by 5 s.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-package updater::` — expected FAIL.
- [ ] **Step 3: Implement** `Updater`, `Backend`, the velopack backend.
- [ ] **Step 4: Run** the tests and clippy — PASS / clean.
- [ ] **Step 5: Commit** `feat(corundum-framework-package): updates found and downloaded, applied on demand (#51)`

### Task 5: The command line on `PATH` (`package::cli_path`)

**Files:**
- Create: `crates/corundum-framework-package/src/cli_path.rs`, `src/cli_path/windows.rs`, `src/cli_path/unix.rs`
- Modify: `src/start.rs` (Windows install / uninstall hooks call `install` / `uninstall` when `spec.cli` is set), `Cargo.toml` (`windows` 0.62 under `cfg(windows)` with `Win32_System_Registry`, `Win32_UI_WindowsAndMessaging`, `Win32_Foundation`)

**Interfaces:**
- Consumes: `platform::package::current` (Task 1).
- Produces:
  - `pub struct CliSpec { pub command: &'static str }` (`Clone, Copy, Debug`)
  - `pub enum CliStatus { Installed, Missing, Broken, Unsupported }` (`Clone, Copy, Debug, PartialEq, Eq`)
  - `pub fn status(spec: &CliSpec) -> CliStatus`, `pub fn install(spec: &CliSpec) -> Result<(), Error>`, `pub fn uninstall(spec: &CliSpec) -> Result<(), Error>`
  - `pub fn on_path(dir: &Path) -> bool` (for `CliRow`'s note on Linux)
  - pure: `pub(crate) fn path_with(value: &str, dir: &str) -> Option<String>` (`None` when already there) and `pub(crate) fn path_without(value: &str, dir: &str) -> Option<String>` (`None` when absent)

Entries compare case-insensitively, with surrounding quotes and one trailing `\` ignored; the other entries, their order, their spelling and their `%VAR%`s are kept; empty entries are kept as they are. Windows status: `Installed` when the `current` directory is in the user `Path`, else `Missing`. Unix: `Installed` when the link points at the package's binary (macOS: `<.app>/Contents/MacOS/<exe file name>`; Linux: `$APPIMAGE`), `Broken` when it is a link whose target is gone, `Missing` otherwise; outside a package `Unsupported`. macOS install / uninstall run `osascript -e 'do shell script "…" with administrator privileges'` with `mkdir -p /usr/local/bin && ln -sf '<target>' '/usr/local/bin/<command>'` / `rm -f`, paths single-quoted with `'` escaped as `'\''`.

- [ ] **Step 1: Write the failing tests:** `path_with` / `path_without` tables — `("C:\\a;C:\\b", "C:\\x\\current")` → `"C:\\a;C:\\b;C:\\x\\current"`; already there as `C:\X\CURRENT\` → `None`; quoted `"C:\x\current"` → `None`; `"%USERPROFILE%\\bin;C:\\x\\current;C:\\b"` without → `"%USERPROFILE%\\bin;C:\\b"`; `""` with → `"C:\\x\\current"`; trailing `;` kept. Unix link states in a tempdir via a `status_at(link: &Path, target: &Path) -> CliStatus` helper: no file → `Missing`; link to target → `Installed`; link to a removed file → `Broken`; link elsewhere → `Missing`. `on_path` with a made-up `PATH` through `on_path_in(dir, path_var: &OsStr)`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-package cli_path::` — FAIL.
- [ ] **Step 3: Implement**; the registry write uses `RegSetValueExW` with the type read back by `RegQueryValueExW` (default `REG_EXPAND_SZ` when the value is absent), then `SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, "Environment", SMTO_ABORTIFHUNG, 5000)`.
- [ ] **Step 4: Run** tests and clippy (on Linux; the Windows code is compiled by CI's Windows job) — PASS / clean.
- [ ] **Step 5: Commit** `feat(corundum-framework-package): the command line on PATH (#51)`

### Task 6: The Windows console shim (`package::console`)

**Files:**
- Create: `crates/corundum-framework-package/src/console.rs`, `crates/corundum-framework-package/tests/console.rs`, `crates/corundum-framework-package/examples/console_child.rs`
- Modify: `Cargo.toml` (`windows` features `Win32_System_JobObjects`, `Win32_System_Threading`)

**Interfaces:**
- Produces: `#[cfg(windows)] pub fn forward(gui_exe: &str) -> !` and the testable `pub fn forward_to(exe: &Path, args: impl IntoIterator<Item = OsString>) -> i32` (all OSes; the job object only on Windows)

`forward` = `forward_to(current_exe().parent().join(gui_exe), args_os().skip(1))` then `exit`. `forward_to`: spawn with inherited stdio; on Windows create a job with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and assign the child; wait; return its code (a signal or no code → 1). A missing `exe`: print `cannot start <path>: <error>` to stderr, return 1.

- [ ] **Step 1: Write the failing integration tests** (`tests/console.rs`). The child is `examples/console_child.rs`: it writes its arguments, one per line, to the file named by `CONSOLE_CHILD_OUT` and exits with code 3. The test builds it with `std::process::Command::new(env!("CARGO")).args(["build", "-p", "corundum-framework-package", "--example", "console_child"])` and finds it in `target/<profile>/examples/` from `current_exe()` (`deps`' parent). `the_child_s_arguments_and_exit_code_come_through`: `forward_to(child, ["a", "b c"]) == 3` and the file reads `"a\nb c\n"`. `a_missing_target_exits_1`: `forward_to("<tempdir>/nope", []) == 1`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-package --test console` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS on Linux; the job object path is exercised on CI's Windows job.
- [ ] **Step 5: Commit** `feat(corundum-framework-package): the console shim for a Windows GUI binary (#51)`

### Task 7: The launcher in a package (`platform::launcher`, `gui::settings::LauncherRow`)

**Files:**
- Modify: `crates/corundum-framework-platform/src/launcher/mod.rs` (`register`, `status`, a new `pub fn in_package(layout: Option<&PackageLayout>) -> Placement`), `launcher/macos.rs` (`is_generated(bundle: &Path) -> bool`, `remove_generated`), `launcher/windows.rs` (remove its own `.lnk`), `launcher/linux.rs` (`Exec` from the given exe, unchanged API); `crates/corundum-framework-gui/src/settings/launcher.rs` (hide in a Windows / macOS package)

**Interfaces:**
- Consumes: `platform::package::{current, PackageLayout, PackageKind}` (Task 1).
- Produces: `pub enum Placement { Register(PathBuf), StepAside }` — `Register(exe)` outside a package (`running_exe()`) and in an AppImage (`layout.root`, the AppImage path); `StepAside` in a Windows or macOS package. `register` on `StepAside` removes the launcher's leftovers (Windows: its own `.lnk` from `unregister_at`; macOS: `remove_generated` for `~/Applications/<name>.app` only when `is_generated` and it is not the running `.app`) and returns `Registered::Skipped`; `status` on `StepAside` → `Status::Unsupported`. `LauncherRow::ui` draws nothing when the status is `Unsupported` (add the early return if it does not have one), so it disappears in a Windows or macOS package.

`is_generated(bundle)`: `Contents/MacOS/<spec.id>` is a symlink and `Contents/MacOS/sq.version` and `Contents/Resources/sq.version` are both absent.

- [ ] **Step 1: Write the failing tests:** `placement_by_layout` (none → `Register(exe)`; AppImage layout → `Register(appimage path)`; Windows / MacBundle → `StepAside`); macOS (`#[cfg(unix)]` as the macos module's tests are): `a_generated_bundle_is_told_from_a_package` (a bundle made by `register_at` → `true`; a dir with `Contents/MacOS/UpdateMac` and `sq.version` → `false`); `stepping_aside_never_removes_the_running_bundle` (the running `.app` path equals `~/Applications/<name>.app` and looks generated → still there afterwards); linux: `an_appimage_entry_execs_the_appimage` (`register_at` with the AppImage path → the `.desktop`'s `Exec` names it).
- [ ] **Step 2: Run** `cargo test -p corundum-framework-platform launcher::` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** platform and gui tests, clippy — PASS / clean.
- [ ] **Step 5: Commit** `feat(corundum-framework-platform): the launcher steps aside in a Velopack package (#51)`

### Task 8: The panel's three kinds of target (`gui::update`)

**Files:**
- Create: `crates/corundum-framework-gui/src/update/target.rs`, `src/update/package_row.rs`
- Modify: `src/update/panel.rs` (`targets: Arc<[Target]>`, rows per kind), `src/update/worker.rs` (jobs for package and delegated rows), `src/update/mod.rs` (exports), `Cargo.toml` (feature `package = ["update", "dep:corundum-framework-package"]`), `assets/locales/{en,ja}` message files of the gui crate (the row texts), facade `Cargo.toml` (`gui-package = ["gui-update", "package", "corundum-framework-gui/package"]`), demo call site (`.into()`)

**Interfaces:**
- Consumes: `PackageStatus`, `PackageState` (Task 2); `Updater`, `After` (Task 4); `platform::package::current` (Task 1).
- Produces:
  - `pub enum Target { Binary(UpdateTarget), #[cfg(feature = "package")] Package(PackageTarget), Delegated(Box<dyn DelegatedTarget>) }`, `impl From<UpdateTarget> for Target`
  - `#[cfg(feature = "package")] pub struct PackageTarget { pub label: String, pub updater: Updater }`
  - `#[cfg(feature = "package")] impl Target { pub fn this_app(label: String, updater: Option<Updater>, binary: UpdateTarget) -> Target }` (`Package` when `updater` is `Some` and `platform::package::current()` is `Some`)
  - `pub trait DelegatedTarget: Send + Sync + 'static { fn label(&self) -> String; fn status(&self) -> Result<PackageStatus, String>; fn check(&self) -> Result<(), String>; fn apply(&self) -> Result<(), String>; }`
  - `pub(crate) fn package_row(status: &PackageStatus) -> RowView` with `pub(crate) struct RowView { pub text: (&'static str /* message id */, Vec<(&'static str, String)> /* args */), pub button: Option<RowButton>, pub progress: Option<f32>, pub spinner: bool }` and `pub(crate) enum RowButton { RestartToUpdate, Retry }`

Message ids (en / ja): `corundum-update-package-uptodate` "Up to date ({ $version })" / 「最新です（{ $version }）」; `corundum-update-package-ready` "{ $version } is ready" / 「{ $version } の準備ができました」; `corundum-update-package-restart` "Restart to update" / 「再起動して更新」; `corundum-update-package-retry` reuses the panel's existing retry id if there is one, else "Retry" / 「再試行」. A `Package` row's button calls `updater.apply(After::RestartApp)` on the worker; a `Delegated` row's calls `apply()`; Check calls `check_now()` / `check()`; `Delegated` rows are polled every 5 s while `ui` runs (track the last poll time in the panel).

- [ ] **Step 1: Write the failing tests:** `package_row` table — `Checking` → spinner, no button; `UpToDate` with current 1.0.0 → uptodate text with `version=1.0.0`; `Downloading { percent: 40 }` → `progress == Some(0.4)`; `Ready { version: 1.1.0 }` → ready text and `RestartToUpdate`; `Failed("x")` → the error and `Retry`; `Idle` → the current version, no button. Panel: `a_delegated_row_shows_its_status_and_applies` (a fake `DelegatedTarget` returning `Ready`; clicking the row's button calls its `apply` once — through the existing `with_worker` test seam extended for delegated jobs); `an_update_target_still_converts` (`UpdatesPanel::new(vec![binary.into()], "t")` compiles and shows its row as before — the existing tests, changed only by `.into()`).
- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --features package update::` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p corundum-framework-gui --features package`, `--features update`, `--features self-update`; clippy for each, plus `--no-default-features --features update` — PASS / clean.
- [ ] **Step 5: Commit** `feat(corundum-framework-gui): update rows for packages (#51)`

### Task 9: `ReadyBadge` and `CliRow`

**Files:**
- Create: `crates/corundum-framework-gui/src/update/badge.rs`, `src/settings/cli.rs`
- Modify: `src/update/mod.rs`, `src/settings/mod.rs`, the gui crate's message files

**Interfaces:**
- Consumes: `Updater`, `After`, `cli_path::{status, install, uninstall, on_path, CliSpec, CliStatus}` (Tasks 4, 5).
- Produces:
  - `#[cfg(feature = "package")] pub struct ReadyBadge` with `new(updater: Updater) -> ReadyBadge` and `ui(&mut self, ui: &mut egui::Ui)` — nothing unless `Ready`; then an icon button (`icons::download()` or the nearest Lucide icon the crate bundles) with tooltip `corundum-update-badge` "Restart to update to { $version }" / 「再起動して { $version } に更新」; a click calls `apply(After::RestartApp)`, an error shown as the tooltip
  - `#[cfg(feature = "package")] pub struct CliRow` with `new(spec: CliSpec) -> CliRow` and `ui(&mut self, ui: &mut egui::Ui)`, a `status_of: fn(&CliSpec) -> CliStatus` seam like `LauncherRow`'s; macOS / Linux: label `corundum-cli-row` "Command line: { $command }" / 「コマンドライン: { $command }」, buttons `corundum-cli-install` "Install" / 「インストール」 (Missing), `corundum-cli-remove` "Remove" / 「削除」 (Installed), `corundum-cli-repair` "Repair" / 「修復」 (Broken); Linux adds `corundum-cli-not-on-path` "~/.local/bin is not on PATH" / 「~/.local/bin が PATH にありません」 when `!on_path`; Windows: label and status text only; `Unsupported`: nothing

- [ ] **Step 1: Write the failing tests:** `CliRow` shows the right button for each status (a stand-in `status_of`, read through a `#[cfg(test)] shown` field as `LauncherRow`'s tests do); `ReadyBadge` draws nothing for `UpToDate` and an enabled button for `Ready { version: 1.1.0 }`. The badge's `Updater` comes from `#[doc(hidden)] pub fn Updater::for_tests(state: PackageState) -> Updater`, added to the package crate behind a `test-support` feature (a fake backend that never applies), turned on in the gui crate's `[dev-dependencies]`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --features package` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** tests and clippy — PASS / clean.
- [ ] **Step 5: Commit** `feat(corundum-framework-gui): the update badge and the command line row (#51)`

### Task 10: The demo on the real pieces, and CI

**Files:**
- Modify: `demo/Cargo.toml` (drop the direct `velopack` dependency; the `velopack` feature becomes `["corundum-framework/gui-package"]`; a `[[bin]] corundum-framework-demo-console` from `src/bin/console.rs`, `required-features = ["velopack"]`), `demo/src/main.rs` (`package::start(&PACKAGE)` first under the feature), `demo/src/velopack.rs` (keep `daemon`, `log`, `log_path`; replace `startup`, `compare_seen_version`, `Page` by a `pub static PACKAGE: PackageSpec` — `app_name: APP_NAME`, `on_installed` / `on_uninstalling` logging, `before_apply` killing the daemon it started (pid kept in a `Mutex<Option<u32>>`), `cli: Some(CliSpec { command: "corundum-demo" })` — and a small page with Start daemon and the log), `demo/src/screens/settings.rs` (About: `UpdatesPanel::new(vec![Target::this_app(…)])`, `CliRow`), `demo/src/lib.rs` (status bar: `ReadyBadge`), `demo/src/bin/console.rs` (`forward("corundum-framework-demo.exe")` on Windows; elsewhere a `main` that says it is Windows only), `.github/workflows/demo-release.yml` (copy the console binary as `corundum-demo.exe` on Windows), `.github/workflows/ci.yml` (`post-test`: `cargo build -p corundum-framework-demo --features velopack --bins --locked`; a tree check that `velopack` is absent for `-p corundum-framework --features gui-update` and present for `--features gui-package`), `demo/README.md`, `docs/superpowers/spikes/2026-10-28-velopack.md` (one line: the spike's page is replaced by the real pieces, see the spec)

**Interfaces:**
- Consumes: everything above.

- [ ] **Step 1: Build and test:** `cargo test -p corundum-framework-demo --features velopack` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` — PASS / clean (the demo's existing tests are the failing-first check: update their `UpdatesPanel::new` call first).
- [ ] **Step 2: Implement** the changes above.
- [ ] **Step 3: Run** the full workspace tests, `cargo fmt --all -- --check`, and the tree checks locally (`cargo tree --locked --target all -p corundum-framework --features gui-update -e normal | grep -c velopack` → `0`).
- [ ] **Step 4: Commit** `feat(demo): the demo on corundum-framework-package (#51)`
- [ ] **Step 5: Package and try:** push tags `demo-v0.0.3` and `demo-v0.0.4`; on each OS install 0.0.3, start the daemon, see the badge after the background download, restart to update, confirm the daemon was stopped by `before_apply` (`pgrep` / Task Manager), the command line (`corundum-demo --help` in a new terminal; macOS and Linux after the row's Install), the launcher's absence (Windows, macOS) or its `$APPIMAGE` entry (Linux), and uninstall. Record the results in the spike document's findings.
