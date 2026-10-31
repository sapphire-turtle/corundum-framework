# Background Service Registration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** corundum registers, stops, starts and unregisters an app's background services on Windows, macOS and Linux, asks about them in a first-run flow the app composes, and keeps them across updates.

**Architecture:** A new crate `corundum-framework-service` holds sapphire-framework-service's code, generalised, plus structured state, stop/start and an SMAppService backend. corundum-framework-package (feature `service`) stops running services before an update and starts them at the next start. corundum-framework-gui gets `first_run` (pages the app composes) and, with feature `service`, `ServicePage` and `ServiceRow`. The demo becomes a single binary with a `daemon` subcommand registered as its service.

**Tech Stack:** Rust 2024, egui, clap 4 (feature `cli`), objc2-service-management 0.3.2 (macOS), systemd user units, launchd, Task Scheduler, Velopack 1.2.161.

**Spec:** `docs/superpowers/specs/2026-10-31-service-registration-design.md`

## Global Constraints

- New crate `crates/corundum-framework-service`, version `0.4.0` (release-plz bumps the group to 0.5.0), in release-plz's `version_group = "corundum"`. Desktops only: on iOS and Android it builds empty, like corundum-framework-package.
- The service crate has no egui and no velopack. Dependencies run `platform ← service ← package (feature service) ← gui (feature service)`.
- Labels, unit names and task names stay what sapphire installs today: launchd label `net.sapphire-turtle.<app>`, unit `<app>.service`, task name `<app>`. The existing golden files stay byte-identical, except the unit's new `ConditionPathExists=` line.
- `ServiceSpec.legacy_labels: Vec<String>` replaces the hard-coded `net.fireturtle.sapphire.<app>`; corundum names no app.
- Copy is user-facing English plus Japanese in `crates/corundum-framework-gui/locales/{en,ja}/corundum.ftl`, ids prefixed `corundum-service-` and `corundum-first-run-`.
- Nothing in a test touches the host's service manager: every flow goes through `&dyn ServiceManager`, tests use `RecordingManager`.
- Commands run from a GUI must not open a console window on Windows (`CREATE_NO_WINDOW`).
- Code style: match the surrounding crates (doc comment on every pub item, `#![warn(missing_docs)]`, full sentences, no abbreviations in names).

## Rulings made while planning (deviations from or refinements of the spec)

- **macOS `start` is `launchctl kickstart -k gui/<uid>/<label>`** (spec: without `-k`). The agents' `KeepAlive { SuccessfulExit = false }` may restart a service killed by `stop` before the update swaps the files; `-k` replaces whatever runs with the new binary.
- **Windows `state` reads the running state from PowerShell's `(Get-ScheduledTask -TaskName <app>).State`** (unlocalised `Running`/`Ready`/`Disabled`), and the command from `schtasks /query /tn <app> /xml`. `schtasks /query /fo csv` prints a localised status a Japanese Windows does not spell `Running`.
- **`FirstRun` takes no `app_key`.** The store attached to the egui context is already the app's own (`corundum.toml` in its config directory). `FirstRun::new(app_name, pages)`, `FirstRun::done(ctx)`, `FirstRun::reset(ctx)`.
- **`first_run` is behind the gui crate's existing `serde` feature**, as it records completion in the store, which needs it. No new feature.
- **The cargo-build test moves to `platform::package::is_cargo_build(exe: &Path) -> bool`** (the launcher's private `skipped` rule, without the variable), so ServicePage can ask it.

## Review Focus

- A `services-to-restart` file naming a service since removed, or holding garbage: the start logs it, removes the file and goes on. The app always starts. (Task 4 test `a_marker_naming_an_unknown_service_is_dropped`.)
- An app whose store cannot be written: the first-run flow, once finished, is not shown again in this run, and the app is usable. (Task 5 test `a_finished_flow_is_held_even_when_the_store_cannot_save`.)
- A service installed before a version that dropped it from `PackageSpec.services`: an update neither stops nor restarts it, and nothing errors. (Task 4 test `only_the_spec_s_running_services_are_stopped`.)
- ServicePage after a failed install: **Try again** runs install again, **Continue without it** records "no" and moves on. (Task 6 test `a_failed_install_offers_try_again_and_continue`.)
- A macOS machine with both an old agent under a legacy label and the current one: install, uninstall and state all retire the legacy one; the service never runs twice. (Task 1 test `every_legacy_label_is_retired`.)

---

### Task 1: The crate, moved from sapphire-framework-service

**Files:**
- Create: `crates/corundum-framework-service/{Cargo.toml,README.md,CHANGELOG.md}`, `src/{lib.rs,error.rs,exe.rs,launchd.rs,manager.rs,scope.rs,systemd.rs,windows.rs}`, `tests/golden.rs`, `tests/golden/{launchagent.plist,task.xml,user.service}`
- Modify: `release-plz.toml` (add the package to the `corundum` group), `crates/corundum-framework/Cargo.toml` (feature `service`), `crates/corundum-framework/src/lib.rs` (re-export `service`), root `README.md` crate table

**Interfaces:**
- Consumes: `corundum_framework_platform::package::{current, PackageKind, PackageLayout}`.
- Produces: everything sapphire-framework-service exports (same names: `ServiceSpec`, `Environment`, `Os`, `InstallContext`, `PostInstall`, `ServiceManager`, `SystemManager`, `RecordingManager`, `Calls`, `install`, `uninstall`, `status`, `label`, `agent_path`, `relabel`, `render_launch_agent`, `render_unit`, `unit_path`, `activation`, `linger_hint`, `render_task`, `service_exe`, `Error`, `Result`), plus:
  - `ServiceSpec { app_name: &'static str, description: String, args: Vec<String>, legacy_labels: Vec<String>, post_install: Option<PostInstall> }`
  - `launchd::labelled_agent_path(label: &str, home: &Path) -> PathBuf` (public now)
  - `launchd::relabel(plist: &str, from_label: &str, app_name: &str) -> String`
  - `ServiceCommand` only with feature `cli`.
  - Removed: `legacy_label`, `legacy_agent_path`.

- [ ] **Step 1: Copy the source.** From `/home/fluo10/Documents/Dev/project-sapphire/sapphire-framework`, `git show origin/main:crates/sapphire-framework-service/<path>` for every file above (the shared checkout may be on another branch; never read its working tree). Rename the crate, `sapphire_framework_service` in tests, and doc text that says "sapphire application" to "app". Dependencies: `corundum-framework-platform = { version = "0.4.0", path = "../corundum-framework-platform", default-features = false }`, `thiserror`, `tracing` from the workspace, `clap` optional; `libc` under `cfg(unix)`; dev: `tempfile`. All under `[target.'cfg(not(any(target_os = "ios", target_os = "android")))'.dependencies]`, and `lib.rs` gates its modules the way corundum-framework-package's does. `[features] cli = ["dep:clap"]`.

- [ ] **Step 2: Run the moved tests.**
Run: `cargo test -p corundum-framework-service --features cli`
Expected: the moved tests pass (the legacy-label tests may need Step 3's renames to compile; fix compilation, not assertions).

- [ ] **Step 3: Write the failing tests for the generalised legacy labels** in `src/manager.rs`, replacing the four `legacy_*` tests' fixed label with `spec().legacy_labels = vec!["net.fireturtle.sapphire.sapphire-bridge".into()]`, and adding:

```rust
#[test]
fn every_legacy_label_is_retired() {
    let mut spec = spec();
    spec.legacy_labels = vec!["org.old.a".into(), "org.old.b".into()];
    let home = PathBuf::from("/Users/me");
    let manager = RecordingManager::new()
        .with_unit(labelled_agent_path("org.old.a", &home), &legacy_body())
        .with_unit(labelled_agent_path("org.old.b", &home), &legacy_body());
    uninstall(&spec, &macos(), &manager).unwrap();
    let calls = manager.calls();
    for old in ["org.old.a", "org.old.b"] {
        assert!(calls.commands.iter().any(|c| c.last().is_some_and(|w| w == old) && c.contains(&"bootout".to_string())));
        assert!(calls.removed.contains(&labelled_agent_path(old, &home)));
    }
}

#[test]
fn without_legacy_labels_no_old_agent_is_looked_for() {
    let manager = RecordingManager::new();
    let mut spec = spec();
    spec.legacy_labels.clear();
    install(&spec, &macos(), &manager).unwrap();
    assert!(manager.calls().read.is_empty());
}
```

`status` relabels the first legacy agent it finds to the current label, as today, with `relabel(&body, from_label, spec.app_name)`.

- [ ] **Step 4: Implement.** `retire_legacy_agent` loops over `spec.legacy_labels` (macOS only, as today) and returns the first body found. `relabel` replaces `from_label` with `label(app_name)`.

- [ ] **Step 5: `service_exe()` through the package layout.** Replace `exe.rs`'s `$APPIMAGE`/`$APPDIR` reading with `resolve(running: PathBuf, layout: Option<&PackageLayout>) -> PathBuf`: `AppImage` → `layout.root`; `Windows`, `MacBundle` → the running executable with links resolved (`std::fs::canonicalize`, falling back to the path as given); `None` → the running executable. Tests, replacing the old four:
  - `outside_a_package_it_is_the_running_executable`
  - `in_an_appimage_it_is_the_image` (`layout.root == /home/me/Apps/demo.AppImage`)
  - `in_a_windows_package_it_is_the_executable_in_current`

- [ ] **Step 6: `ConditionPathExists`.** `render_unit` writes `ConditionPathExists=<exe>` in `[Unit]`, after `Description=`. Update `tests/golden/user.service` to:

```
[Unit]
Description=Sapphire agent server
ConditionPathExists=/usr/bin/sapphire-agent

[Service]
...
```

(the rest unchanged). Add `legacy_labels: vec![]` to the golden test's spec.

- [ ] **Step 7: No console from a GUI on Windows.** `SystemManager::run` sets `creation_flags(0x0800_0000)` (`CREATE_NO_WINDOW`) under `cfg(windows)` via `std::os::windows::process::CommandExt`.

- [ ] **Step 8: Wire the crate in.** `release-plz.toml`: `[[package]] name = "corundum-framework-service" version_group = "corundum"`. Facade: `service = ["dep:corundum-framework-service", "platform"]`, `pub use corundum_framework_service as service;` under the feature (copy how `package` is done). README crate table: one row.

- [ ] **Step 9: Verify.**
Run: `cargo test -p corundum-framework-service --all-features && cargo clippy -p corundum-framework-service --all-features --all-targets -- -D warnings && cargo build -p corundum-framework --features service`
Expected: all pass, no warnings.

- [ ] **Step 10: Commit.** `git add crates/corundum-framework-service release-plz.toml crates/corundum-framework README.md && git commit -m "feat(service): move sapphire-framework-service into corundum"`

---

### Task 2: `state`, `stop` and `start` with the file backends

**Files:**
- Create: `crates/corundum-framework-service/src/state.rs`
- Modify: `src/manager.rs` (flows), `src/systemd.rs`, `src/launchd.rs`, `src/windows.rs` (parsers), `src/lib.rs` (exports)

**Interfaces:**
- Consumes: Task 1's `ServiceSpec`, `Environment`, `ServiceManager`.
- Produces:
  - `pub enum ServiceState { NotInstalled, Stopped, Running, NeedsApproval, Broken { reason: String } }` (`Clone, Debug, PartialEq, Eq`)
  - `pub fn state(spec: &ServiceSpec, env: &Environment, m: &dyn ServiceManager) -> Result<ServiceState>`
  - `pub fn stop(spec: &ServiceSpec, env: &Environment, m: &dyn ServiceManager) -> Result<()>`
  - `pub fn start(spec: &ServiceSpec, env: &Environment, m: &dyn ServiceManager) -> Result<()>`
  - pure parsers: `systemd::exec_of(unit: &str) -> Option<PathBuf>`, `systemd::is_running(is_active: &str) -> bool`, `launchd::program_of(plist: &str) -> Option<PathBuf>`, `launchd::is_running(print: &str) -> bool`, `windows::command_of(task_xml: &str) -> Option<PathBuf>`, `windows::is_running(state: &str) -> bool`.
  - `ServiceManager::exists(&self, path: &Path) -> bool` (default `path.exists()`; `RecordingManager` answers from a new `with_file(path)` list), so `Broken` is testable without touching `/usr/bin`.

- [ ] **Step 1: Write the failing parser tests** (tables in each module's tests):
  - `systemd::exec_of("[Service]\nExecStart=/opt/a.AppImage serve\n") == Some("/opt/a.AppImage")`; no `ExecStart=` → `None`.
  - `systemd::is_running("active\n")` true; `"inactive"`, `"failed"`, `"activating"` false.
  - `launchd::program_of(<the golden plist>) == Some("/usr/bin/sapphire-agent")`.
  - `launchd::is_running` true for a `launchctl print` body containing a line `\tstate = running`, false for `state = not running` and for empty.
  - `windows::command_of(<the golden task.xml>) == Some("/usr/bin/sapphire-agent")`.
  - `windows::is_running("Running\r\n")` true, `"Ready"` and `"Disabled"` false.

- [ ] **Step 2: Run them to see them fail.** `cargo test -p corundum-framework-service` → FAIL (functions missing).

- [ ] **Step 3: Implement the parsers.**

- [ ] **Step 4: Write the failing flow tests** in `src/manager.rs`, through `RecordingManager`:

| Test | Setup | Expected |
|---|---|---|
| `linux_without_a_unit_is_not_installed` | no unit | `NotInstalled`, no command run |
| `linux_with_a_missing_program_is_broken` | unit whose ExecStart file is not `with_file` | `Broken { reason }` with the path in `reason` |
| `linux_running` | unit, file exists, `returning("active")` | `Running`; ran `systemctl --user is-active <app>` |
| `linux_stopped_when_is_active_fails` | `failing_on("is-active")` | `Stopped` |
| `macos_without_a_plist_is_not_installed` | — | `NotInstalled` |
| `macos_running` | plist, file, `returning("\tstate = running\n")` | `Running`; ran `launchctl print gui/501/net.sapphire-turtle.<app>` |
| `macos_not_loaded_is_stopped` | plist, file, `failing_on("print")` | `Stopped` |
| `windows_without_a_task_is_not_installed` | `failing_on("/query")` | `NotInstalled` |
| `windows_running` | `/query` answers the golden XML (file exists), PowerShell answers `Running` | `Running` |
| `stop_and_start_run_each_backend_s_command` | each OS | Linux `systemctl --user stop <app>` / `start <app>`; macOS `launchctl kill SIGTERM gui/501/<label>` / `launchctl kickstart -k gui/501/<label>`; Windows `schtasks /end /tn <app>` / `schtasks /run /tn <app>` |
| `stopping_a_stopped_service_is_not_an_error` | `failing_on("stop")` / `"kill"` / `"/end"` | `Ok(())` |

`RecordingManager::returning` answers every command with one text; add `answering(needle: &str, text: &str) -> Self` (builder, may be chained) for per-command output, used by the Windows rows.

The Windows PowerShell command is exactly `["powershell", "-NoProfile", "-NonInteractive", "-Command", "(Get-ScheduledTask -TaskName '<app>').State"]`.

- [ ] **Step 5: Run them to see them fail.**

- [ ] **Step 6: Implement `state`, `stop`, `start`.** `state` calls `retire_legacy_agent` first, as `status` does. A `start` failure is an error (unlike `stop`).

- [ ] **Step 7: Verify.** `cargo test -p corundum-framework-service --all-features && cargo clippy -p corundum-framework-service --all-features --all-targets -- -D warnings` → pass.

- [ ] **Step 8: Commit.** `git commit -m "feat(service): structured state, stop and start"`

---

### Task 3: The SMAppService backend on macOS

**Files:**
- Create: `crates/corundum-framework-service/src/smappservice.rs` (the objc2 calls, `cfg(target_os = "macos")`), `tests/golden/bundled-agent.plist`
- Modify: `src/launchd.rs` (`bundled_agent_plist`), `src/manager.rs` (backend choice in install/uninstall/state/stop/start), `Cargo.toml`

**Interfaces:**
- Consumes: Task 2's flows; `platform::package::{PackageKind, PackageLayout}`.
- Produces:
  - `pub enum MacBackend { AppService { plist_name: String }, AgentFile }`
  - `pub fn mac_backend(layout: Option<&PackageLayout>, plist_present: impl Fn(&Path) -> bool, app_name: &str) -> MacBackend` (pure)
  - `pub fn launchd::bundled_agent_plist(spec: &ServiceSpec, exe_name: &str) -> String`
  - `ServiceManager` gains `fn app_service(&self, plist_name: &str, action: AppServiceAction) -> Result<AppServiceStatus>` with `AppServiceAction { Register, Unregister, Status }` and `AppServiceStatus { NotRegistered, Enabled, RequiresApproval, NotFound }`. `SystemManager` calls SMAppService on macOS and returns `Err(Error::Manager("SMAppService is macOS only"))` elsewhere; `RecordingManager` records the call (new `Calls.app_service: Vec<(String, AppServiceAction)>`) and answers a status set with `with_app_service(status)`.
  - `Environment` gains `package: Option<PackageLayout>`, filled by `detect()` from `platform::package::current()`; tests set it.

- [ ] **Step 1: Write the failing tests.**
  - `mac_backend` table: `None` → `AgentFile`; `MacBundle` with `<root>/Contents/Library/LaunchAgents/net.sapphire-turtle.demo.plist` present → `AppService { plist_name: "net.sapphire-turtle.demo.plist" }`; `MacBundle` without it → `AgentFile`; `AppImage` → `AgentFile`.
  - Golden `bundled-agent.plist` for the golden spec and `exe_name = "sapphire-agent"`: the agent plist with `<key>BundleProgram</key><string>Contents/MacOS/sapphire-agent</string>` before `ProgramArguments`, `ProgramArguments` = `[sapphire-agent, server, run]`, the same `RunAtLoad`, `KeepAlive`, `ThrottleInterval`.
  - Flows with `Environment { os: MacOs, package: Some(bundle with plist) }`:
    - `install_registers_with_smappservice`: records `Register`, writes no file, and boots out and removes `~/Library/LaunchAgents/net.sapphire-turtle.<app>.plist` when one is there (`with_unit`).
    - `uninstall_unregisters`.
    - `state_maps_smappservice`: `RequiresApproval` → `NeedsApproval`; `NotRegistered`, `NotFound` → `NotInstalled`; `Enabled` + `print` running → `Running`; `Enabled` + `print` failing → `Stopped`.
    - stop/start use the same `launchctl` commands as Task 2.

- [ ] **Step 2: Run them to see them fail.**

- [ ] **Step 3: Implement.** `Cargo.toml` under `cfg(target_os = "macos")`: `objc2 = "0.6.4"`, `objc2-foundation = { version = "0.3.2", default-features = false, features = ["std", "NSString", "NSError"] }`, `objc2-service-management = { version = "0.3.2", default-features = false, features = ["std", "SMAppService", "objc2", "objc2-foundation"] }`. `smappservice.rs`: `SMAppService::agentServiceWithPlistName(&NSString::from_str(name))`, then `registerAndReturnError` / `unregisterAndReturnError` / `status()`. Map an `NSError` to `Error::Manager(localizedDescription)`.

- [ ] **Step 4: Verify.** `cargo test -p corundum-framework-service --all-features`, `cargo clippy ... -D warnings`, and `cargo check -p corundum-framework-service --target aarch64-apple-darwin` when that target is installed (otherwise CI's macOS job covers it; say so in the report).

- [ ] **Step 5: Commit.** `git commit -m "feat(service): register with SMAppService inside a macOS package"`

---

### Task 4: Updates stop and restart the services (package, feature `service`)

**Files:**
- Create: `crates/corundum-framework-package/src/services.rs`
- Modify: `crates/corundum-framework-package/{Cargo.toml,src/lib.rs,src/start.rs,src/updater.rs}`, facade `Cargo.toml` (`package` + `service` → `corundum-framework-package/service`)

**Interfaces:**
- Consumes: `corundum_framework_service::{ServiceSpec, ServiceState, Environment, ServiceManager, SystemManager, state, stop, start, uninstall}`.
- Produces:
  - `PackageSpec.services: &'static [fn() -> ServiceSpec]` under `#[cfg(feature = "service")]`. Every `PackageSpec` literal in the crate's tests gets `#[cfg(feature = "service")] services: &[]`.
  - `pub(crate) const RESTART_MARKER: &str = "services-to-restart";`
  - `pub(crate) fn stop_running(spec: &PackageSpec, data: &Path, env: &Environment, m: &dyn ServiceManager) -> Result<Vec<&'static str>, String>` — stops each `Running` service, writes their `app_name`s one per line to `data/RESTART_MARKER`, returns them.
  - `pub(crate) fn restart_stopped(spec: &PackageSpec, data: &Path, env: &Environment, m: &dyn ServiceManager)` — starts each named service that is in `spec.services`, logs failures, removes the file.
  - `pub(crate) fn uninstall_all(spec: &PackageSpec, env: &Environment, m: &dyn ServiceManager)`.

- [ ] **Step 1: Write the failing tests** in `services.rs` (tempdir data, `RecordingManager`, `Environment` for Linux with a unit `with_unit` + `with_file` so `state` is `Running`):
  - `a_running_service_is_stopped_and_marked`: marker holds `demo\n`; `systemctl --user stop demo` ran.
  - `only_the_spec_s_running_services_are_stopped`: a `Stopped` one is not stopped nor marked; with no services, no marker file is written.
  - `the_marked_services_start_and_the_marker_goes`.
  - `a_marker_naming_an_unknown_service_is_dropped`: marker `gone\n\xff\n`; nothing started, file removed, no panic.
  - `a_failed_stop_is_an_error_naming_the_service`.
  - `uninstall_all_uninstalls_each_service`.

  In `updater.rs`, with the fake backend:
  - `apply_stops_services_after_before_apply`: order `before_apply`, stop, backend apply.
  - `a_failed_apply_starts_the_stopped_services_again`.
  - `a_failed_stop_applies_nothing` (`Error::Prepare`).

  The updater needs the manager and environment injected: `Updater::with_backend` gains a `services: Arc<dyn ServiceDriver>` (a small crate-private trait `ServiceDriver { fn stop_running(&self) -> Result<(), String>; fn restart_stopped(&self); }`, real one built from `SystemManager`, `Environment::detect()` and the app's data dir; tests pass a recording one). Without the feature it is a no-op.

- [ ] **Step 2: Run them to see them fail.**
Run: `cargo test -p corundum-framework-package --features service`

- [ ] **Step 3: Implement.** `Shared::prepare` runs `before_apply`, then `stop_running`; `apply` and `apply_pending` call `restart_stopped` when the backend's apply returns an error. `start()` calls `restart_stopped` on every start but `NotPackaged`'s early return is moved after it (a script install's CLI also restarts). Windows: the uninstall hook in `windows_hooks` calls `uninstall_all` before `on_uninstalling`. Feature: `service = ["dep:corundum-framework-service"]`.

- [ ] **Step 4: Verify.**
Run: `cargo test -p corundum-framework-package && cargo test -p corundum-framework-package --features service && cargo clippy -p corundum-framework-package --all-features --all-targets -- -D warnings`
Expected: pass both with and without the feature.

- [ ] **Step 5: Commit.** `git commit -m "feat(package): stop services before an update and start them after"`

---

### Task 5: The first-run flow (gui, `first_run`)

**Files:**
- Create: `crates/corundum-framework-gui/src/first_run.rs`
- Modify: `src/lib.rs` (`#[cfg(feature = "serde")] pub mod first_run;`), `src/store.rs` (`Stored.first_run: Option<bool>`, `Stored.services_asked: Option<bool>`), `locales/{en,ja}/corundum.ftl`

**Interfaces:**
- Consumes: `store::{state, update}`.
- Produces:

```rust
pub trait FirstRunPage {
    fn title(&self, ctx: &egui::Context) -> String;
    fn ui(&mut self, ui: &mut egui::Ui) -> PageAction;
    fn can_continue(&self) -> bool { true }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageAction { Stay, Next, Back }
pub struct FirstRun { /* pages, current index */ }
impl FirstRun {
    pub fn new(pages: Vec<Box<dyn FirstRunPage>>) -> FirstRun;
    pub fn done(ctx: &egui::Context) -> bool;
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool;
    pub fn reset(ctx: &egui::Context);
}
```

- Copy ids: `corundum-first-run-step = Step { $step } of { $count }`, `corundum-first-run-back = Back`, `corundum-first-run-next = Next`, `corundum-first-run-finish = Finish`; ja: `ステップ { $step } / { $count }`, `戻る`, `次へ`, `完了`.

- [ ] **Step 1: Write the failing tests** (drive with `egui::Context::default()` and a scripted test page whose `ui` returns a queued `PageAction`; add a `#[cfg(test)] fn press(&mut self, PageAction)` seam on `FirstRun` for the footer buttons, as `CliRow` records `shown`):
  - `pages_go_in_order_and_back`
  - `back_on_the_first_page_stays`
  - `next_is_held_while_the_page_cannot_continue`
  - `finishing_is_stored_and_not_shown_again`: after the last `Next`, `ui` returns true and `FirstRun::done(&ctx)` is true, and `store::state(&ctx).first_run == Some(true)`.
  - `reset_shows_it_again`
  - `an_unfinished_flow_starts_at_page_one`: a new `FirstRun` after leaving at page 2 is on page 1.
  - `a_finished_flow_is_held_even_when_the_store_cannot_save` (store at an unwritable path, as `an_unwritable_store_still_holds_the_change` does).
  - The i18n check test (`i18n/check.rs`) passes with the new ids in both locales.

- [ ] **Step 2: Run them to see them fail.** `cargo test -p corundum-framework-gui --features serde first_run`

- [ ] **Step 3: Implement.** `ui` fills the given `Ui` with a `CentralPanel`-like full area: header (title, step text), the page in a scroll area, footer (Back left unless page 0, Next/Finish right, `add_enabled(can_continue)`).

- [ ] **Step 4: Verify.** `cargo test -p corundum-framework-gui --all-features && cargo clippy -p corundum-framework-gui --all-features --all-targets -- -D warnings`

- [ ] **Step 5: Commit.** `git commit -m "feat(gui): a first-run flow the app composes from pages"`

---

### Task 6: `ServicePage` and `ServiceRow` (gui, feature `service`)

**Files:**
- Create: `crates/corundum-framework-gui/src/settings/service.rs` (`ServiceRow`), `src/first_run/service.rs` or `src/service_page.rs` (`ServicePage`; keep `first_run.rs` focused)
- Modify: gui `Cargo.toml` (`service = ["serde", "dep:corundum-framework-service"]`), `settings/mod.rs`, `lib.rs`, locales; `crates/corundum-framework-platform/src/package.rs` (`is_cargo_build`), `src/launcher/mod.rs` (`skipped` uses it); facade `gui-service = ["gui", "service", "serde", "corundum-framework-gui/service"]`

**Interfaces:**
- Consumes: Task 2's `state/install/uninstall/stop/start`, `ServiceState`; Task 5's `FirstRunPage`, `PageAction`, `store::update`.
- Produces:
  - `pub fn platform::package::is_cargo_build(exe: &Path) -> bool`
  - `pub struct ServicePage; impl ServicePage { pub fn new(services: Vec<fn() -> ServiceSpec>, question: String, reason: String) -> ServicePage }` implementing `FirstRunPage`.
  - `pub struct ServiceRow; impl ServiceRow { pub fn new(service: fn() -> ServiceSpec) -> ServiceRow; pub fn ui(&mut self, ui: &mut egui::Ui) }`
  - Both find state and act on a worker thread, through a crate-private `ServiceOps` seam (`state`, `install`, `uninstall`, `stop`, `start` as `fn` pointers taking the spec), real ones on `SystemManager` + `Environment::detect()`; tests pass fakes.

- Copy ids (en / ja):
  - `corundum-service-yes = Yes, keep it running` / `はい、常駐させる`
  - `corundum-service-no = Not now` / `今はしない`
  - `corundum-service-dev-build = This is a development build; nothing is registered.` / `開発ビルドのため登録しません。`
  - `corundum-service-failed = Could not register: { $error }` / `登録できませんでした: { $error }`
  - `corundum-service-try-again = Try again` / `再試行`
  - `corundum-service-continue-without = Continue without it` / `登録せずに続ける`
  - `corundum-service-row = Background: { $name }` / `バックグラウンド: { $name }`
  - `corundum-service-run = Keep running in the background` / `バックグラウンドで実行`
  - `corundum-service-running = Running` / `実行中`; `corundum-service-stopped = Stopped` / `停止中`
  - `corundum-service-stop = Stop` / `停止`; `corundum-service-start = Start` / `開始`; `corundum-service-remove = Remove` / `削除`
  - `corundum-service-needs-approval = Waiting for your approval in Login Items` / `ログイン項目で許可を待っています`
  - `corundum-service-open-login-items = Open Login Items` / `ログイン項目を開く`
  - `corundum-service-broken = Not working: { $reason }` / `動作していません: { $reason }`
  - `corundum-service-repair = Repair` / `修復`

- [ ] **Step 1: Write the failing tests.**
  - platform: `is_cargo_build` true for `/w/target/debug/app`, `/w/target/x86_64-unknown-linux-gnu/release/app`; false for `/opt/app`, `/w/target/app`. The launcher's existing tests still pass.
  - ServicePage: `yes_installs_each_service_and_moves_on` (records `services_asked = Some(true)`), `no_registers_nothing_and_moves_on` (`Some(false)`), `a_failed_install_offers_try_again_and_continue` (Try again calls install a second time; Continue records `Some(false)` and returns `Next`), `it_cannot_continue_until_answered`, `a_cargo_build_registers_nothing` (seam: `is_dev: bool` field set by the test).
  - ServiceRow, with a fake `state` per row, recording the offered buttons as `CliRow` does (`shown`): `NotInstalled` → run; `Running` → stop, remove; `Stopped` → start, remove; `NeedsApproval` → open-login-items; `Broken` → repair; and `state_is_not_asked_on_the_ui_thread` (the fake `state` blocks on a channel; `ui` returns at once and shows nothing until it answers).

- [ ] **Step 2: Run them to see them fail.**

- [ ] **Step 3: Implement.** **Open Login Items** opens `x-apple.systempreferences:com.apple.LoginItems-Settings.extension` with `open`. After every action the row asks `state` again.

- [ ] **Step 4: Verify.** `cargo test -p corundum-framework-platform -p corundum-framework-gui --all-features && cargo clippy --workspace --all-features --all-targets -- -D warnings && cargo build -p corundum-framework --features gui-service`

- [ ] **Step 5: Commit.** `git commit -m "feat(gui): the service page and the About row for a service"`

---

### Task 7: The demo as one binary with its service

**Files:**
- Delete: `demo/src/bin/daemon.rs`, the `[[bin]] corundum-framework-demo-daemon` entry, the pid-file code in `demo/src/velopack.rs` (`DAEMON*`, `DAEMON_CHILD`, `stop_child`, `stop_left_daemon`, `pid_path`, `is_daemon`, `terminate`, and the unix `libc` dependency if nothing else uses it)
- Modify: `demo/src/main.rs` (`daemon` subcommand), `demo/src/velopack.rs` (`PACKAGE.services`, `before_apply` only logs), `demo/src/lib.rs` (first-run flow before the shell), `demo/src/screens/settings.rs` (About: `ServiceRow`, "Run first-run setup again"), `demo/Cargo.toml` (`velopack = ["corundum-framework/gui-package", "corundum-framework/gui-service"]`), `demo/locales/{en,ja}`, `.github/workflows/demo-release.yml`, `demo/README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: `corundum_demo::velopack::service() -> ServiceSpec` = `ServiceSpec { app_name: "corundum-demo", description: "Corundum demo daemon".into(), args: vec!["daemon".into()], legacy_labels: vec![], post_install: None }`; `velopack::daemon(&heartbeat)` stays as the subcommand's body.

- [ ] **Step 1: Write the failing tests** in `demo/src/tests.rs`: `the_daemon_subcommand_is_recognised` (`command_line` takes the args as a parameter now: `command_line(args: impl IntoIterator<Item = OsString>) -> CommandLine` with `CommandLine::{Window, Done, Daemon}`), and `the_first_run_has_a_welcome_and_the_service_page` (2 pages).

- [ ] **Step 2: Run them to see them fail.** `cargo test -p corundum-framework-demo --features velopack`

- [ ] **Step 3: Implement.** `main`: `Daemon` runs before the single-instance lock (the daemon is not the GUI's instance). The window shows `FirstRun` until `FirstRun::done` (only with `velopack`; a plain `cargo run` of the demo shows it too, its ServicePage saying it is a development build). The welcome page is a short text page.

- [ ] **Step 4: The release workflow.** Stop copying the daemon binary. On macOS, after `cargo build`, render the plist with a small example `cargo run -p corundum-framework-demo --example agent-plist -- <out>` (calls `launchd::bundled_agent_plist(&service(), "corundum-framework-demo")`), build the `.app` with `vpk [osx] bundle` (check `vpk -h` on the runner's version; if `bundle` is absent, assemble `CorundumDemo.app/Contents/{MacOS,Info.plist,Resources}` by hand), copy the plist into `Contents/Library/LaunchAgents/net.sapphire-turtle.corundum-demo.plist`, then `vpk pack --packDir <the .app>`. Linux and Windows unchanged but for the daemon copy.

- [ ] **Step 5: Verify.**
Run: `cargo test --workspace --all-features && cargo clippy --workspace --all-features --all-targets -- -D warnings && cargo build -p corundum-framework-demo --features velopack --bins`
Expected: pass. Then on this Linux host: `cargo run -p corundum-framework-demo --features velopack` shows the first-run flow, the ServicePage says development build, About shows the row as `NotInstalled`.

- [ ] **Step 6: Commit.** `git commit -m "feat(demo): one binary whose daemon is a registered service"`

---

## After the tasks

- Machine checks (by the user): Linux and Windows from a `demo-v*` pre-release — register through the first run, update with the service running (stopped, then running again), uninstall (Windows: the task gone). macOS: the LaunchAgent-file path now; SMAppService once signing (part 1) exists.
- The sapphire-framework issue for the move (spec, "sapphire-framework").
