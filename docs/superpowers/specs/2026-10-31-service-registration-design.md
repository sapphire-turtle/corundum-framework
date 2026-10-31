# Background services, registered by corundum

> 2026-10-31: SMAppService was dropped by the user's decision. macOS registers the
> LaunchAgent file everywhere, as Linux does its unit file. The SMAppService work is in git
> history (4981b1d..9e4929d).

Issue: corundum-framework#51, part 3. Part 2 (the app as a Velopack package) is in 0.4.0:
`docs/superpowers/specs/2026-10-29-velopack-runtime-design.md`. The spike behind both is
`docs/superpowers/spikes/2026-10-28-velopack.md`.

## Goal

An app installed by someone who is not a developer can keep part of itself running after
its window closes, such as sapphire's app server, which serves MCP clients and syncs. The
person agrees to that once, at the first start, and can change it later. Updates never
leave the old version running, and uninstalling leaves nothing behind that can be avoided.
corundum does all of this for every app. sapphire-framework has done it for itself until
now, in `sapphire-framework-service`.

Success means:

- an app names its services once, and corundum registers, stops, starts and unregisters
  them on Windows, macOS and Linux;
- at the first start, the person goes through the app's own setup pages and corundum's
  "keep running in the background?" page, in the order the app chooses;
- an About row shows each service's state and changes it, including "the program it starts
  is gone";
- an update stops the app's running services before it is applied and starts them again
  at the next start, whether that start is the GUI, the command line or a restart by the
  updater;
- sapphire-framework moves to this crate without changing what an installed user has.

## Decisions

1. **Scope: the library, the GUI pieces and the update integration.** A service that
   outlives the window has to be stopped and started around every update on every OS, and
   leaving that to each app is how one gets missed.
2. **A new crate, `corundum-framework-service`.** It has no egui and no velopack. The
   command-line build of an app, installed by script, needs services but not Velopack.
   The platform crate is used by phones too and should not carry ServiceManagement. The
   dependencies run `platform ← service ← package (feature) ← gui (feature)`.
3. **macOS: the LaunchAgent file.** `~/Library/LaunchAgents/<label>.plist`, loaded with
   `launchctl`, as sapphire does today and as Linux uses its unit file
   (see the note at the top).
4. **The first start asks, with the app's reason.** A window that keeps something running
   after it closes, without saying so, costs a light user's trust. The question comes
   once, inside the app's first-run setup. Its words are the app's ("for AI tools and
   sync"), and the answer can be changed in About.
5. **First-run setup is a sequence of pages the app composes.** The service question is
   one page, `ServicePage`, among the app's own: sapphire's welcome, workspace and
   "connect to Claude" pages, and asteria's server page.
6. **Old labels are generic.** sapphire's earlier `net.fireturtle.sapphire.<app>` is passed
   in by sapphire as `ServiceSpec.legacy_labels`; corundum names no app.

## Crates

| Crate | Change |
|---|---|
| `corundum-framework-service` | New, desktops only. `sapphire-framework-service`'s contents, generalised, plus `state`, `stop`, `start`, `service_exe()` through `platform::package` |
| `corundum-framework-package` | `PackageSpec.services: Services` (always there); feature `service`: stop before apply, start after; unregister in the Windows uninstall hook |
| `corundum-framework-gui` | Feature `service` for `ServiceRow` and `ServicePage`; the new `first_run` module (behind the existing `serde` feature) |
| `corundum-framework` | Facade features `service` and `gui-service` |

## corundum-framework-service

```rust
pub struct ServiceSpec {
    /// The label's, unit's and task's name: `sapphire-journal`.
    pub app_name: &'static str,
    pub description: String,
    /// What the executable is started with: `["serve"]`.
    pub args: Vec<String>,
    /// Labels an earlier version registered under: taken off and their files removed by
    /// install, uninstall and state, so the service never runs twice.
    pub legacy_labels: Vec<String>,
    pub post_install: Option<PostInstall>,
}

#[non_exhaustive]
pub enum ServiceState {
    NotInstalled,
    Stopped,
    Running,
    /// Registered, but the program it starts is gone (the app deleted or moved).
    Broken { reason: String },
}

pub fn install(spec: &ServiceSpec, env: &Environment, m: &dyn ServiceManager) -> Result<InstallContext>;
pub fn uninstall(spec: &ServiceSpec, env: &Environment, m: &dyn ServiceManager) -> Result<()>;
pub fn state(spec: &ServiceSpec, env: &Environment, m: &dyn ServiceManager) -> Result<ServiceState>;
pub fn status(spec: &ServiceSpec, env: &Environment, m: &dyn ServiceManager) -> Result<String>;
pub fn stop(spec: &ServiceSpec, env: &Environment, m: &dyn ServiceManager) -> Result<()>;
pub fn start(spec: &ServiceSpec, env: &Environment, m: &dyn ServiceManager) -> Result<()>;
pub fn service_exe() -> std::io::Result<PathBuf>;

#[cfg(feature = "cli")]
pub enum ServiceCommand { Install, Uninstall, Status } // clap::Subcommand, as today
```

- **Kept from sapphire-framework-service, renamed only where it named sapphire:**
  - `Environment`, `Os`, `InstallContext` and `PostInstall`;
  - the `ServiceManager` trait, with `SystemManager` and `RecordingManager`;
  - the file renderers (`render_unit`, `render_launch_agent`, `render_task`);
  - `launchd::label` (`net.sapphire-turtle.<app>`) and the relabelling of an old agent;
  - `linger_hint`.
- **`status`** is the OS tool's own output, for the command line. **`state`** is structured,
  for the GUI. It is decided by pure functions over the tools' output and the files:
  - `systemctl --user is-enabled` and `is-active`;
  - `launchctl print gui/<uid>/<label>`;
  - `schtasks /query /fo csv`.
- **`stop` and `start`** leave the registration as it is:

  | Backend | `stop` | `start` |
  |---|---|---|
  | systemd | `systemctl --user stop` | `systemctl --user start` |
  | launchd | `launchctl bootout gui/<uid>/<label>` | `launchctl bootstrap gui/<uid> <plist>`, then `launchctl kickstart -k gui/<uid>/<label>` |
  | Task Scheduler | `schtasks /end` | `schtasks /run` |

  `stop` cannot fail: stopping something not running is not an error, and every command's
  error is ignored. On macOS a signal would not do: the agent's `KeepAlive` starts a killed
  agent again, so `stop` boots it out of launchd; the plist stays, so `state` reads
  `Stopped`, and the next login (or `start`) loads it again. `start`'s `bootstrap` fails
  when the agent is loaded already and its error is ignored; the `kickstart`'s is returned.
- **`service_exe()`** asks `platform::package::current()`:
  - in a Windows package, the GUI-subsystem binary in `current\`, which opens no console,
    as it is: not canonicalized, which would put a `\\?\` path in the task's XML;
  - in a macOS package, the binary inside the `.app`, with links resolved;
  - in an AppImage, `$APPIMAGE`;
  - outside a package, the running executable.

  It replaces sapphire's `$APPIMAGE`/`$APPDIR` reading, as that code's comment planned.
- `clap` is behind feature `cli`, so a GUI-only app does not build it.

## Per OS

### Linux: a systemd user unit
- `~/.config/systemd/user/<app>.service`, `ExecStart=` the service exe and the arguments, as
  today.
- New: `ConditionPathExists=<the service exe>`. A deleted or moved AppImage leaves the unit
  skipped rather than failing and restarting into the journal at every login.
- `state` is `Broken` when the unit's executable is missing.

### macOS: the LaunchAgent file
- **The LaunchAgent file:** `launchctl bootstrap` installs it, `bootout` removes it, and `state` comes from the plist and `launchctl print`. `state` is `Broken` when the plist's program is missing. A legacy-label agent is retired.
- **Install** boots out an agent an earlier install left loaded (`launchctl bootout
  gui/<uid>/<label>`, its error ignored) before `bootstrap`, which refuses a loaded one.
- **`stop` / `start`** go through `launchctl`, as in the table above.

### Windows: Task Scheduler
- A logon task through `schtasks`, as today, starting the service exe.
- Install runs `schtasks /create`, then `schtasks /run`, so the service runs now rather
  than at the next logon. A failed `/run` is an install error reported after the task
  exists: the task is kept, and starts at the next logon.
- Uninstall ends the task (`schtasks /end`, its error ignored) before `schtasks /delete`.
- `stop` is `schtasks /end`, `start` is `schtasks /run`. `state` reads the program from
  `schtasks /query /tn <app> /xml` and whether it runs from PowerShell's
  `(Get-ScheduledTask -TaskName <app>).State`, which is not localised.

## corundum-framework-package (feature `service`)

```rust
pub struct PackageSpec {
    // … as in 0.4.0, and:
    /// The app's background services. A required field, with or without the feature.
    pub services: Services,
}

impl Services {
    /// No background services.
    pub const NONE: Services;
    /// These services; each function builds one's spec, as a spec holds owned values.
    #[cfg(feature = "service")]
    pub const fn new(services: &'static [fn() -> ServiceSpec]) -> Services;
}
```

- **Before an update is applied:**
  - `Updater::apply` and `apply_pending_at_start` run the app's `before_apply`.
  - Then they `stop` each service whose `state` is `Running`.
  - They write the stopped services' names to `<data dir>/<app>/services-to-restart`,
    before stopping anything.
  - If the apply then fails, they `start` those services again and remove the file.
  - `stop` cannot fail, so only a `services-to-restart` that cannot be written refuses the
    update (a `Prepare` error: nothing is applied). A service whose `state` cannot be read
    may be running: it is stopped and noted all the same.
- **At every start:** `start()` reads `services-to-restart`, `start`s each named service, then removes the file. This covers the GUI, the command line and the updater's restart, so a service is never left stopped by an update.
- **Uninstall hook on Windows:** `uninstall`s each service (ending its task first), within the hook's 30 seconds.
- **macOS and Linux run nothing at uninstall,** so the docs tell the person to remove the service first (About › the service row › Remove):
  - A LaunchAgent file, or a systemd unit, left behind shows as `Broken`. On Linux it is skipped through `ConditionPathExists`.
  - A reinstall registers again from the app's first-run answer.

## corundum-framework-gui

### `first_run` (the gui's `serde` feature)

```rust
pub trait FirstRunPage {
    /// The page's heading, in the language installed in `ctx`.
    fn title(&self, ctx: &egui::Context) -> String;
    /// Draw the page's body. A page with buttons of its own (ServicePage's Yes and No)
    /// returns `Next` or `Back` from them; otherwise `Stay`.
    fn ui(&mut self, ui: &mut egui::Ui) -> PageAction;
    /// Whether the footer's Next is enabled: false until the page has what it needs.
    fn can_continue(&self) -> bool { true }
}
pub enum PageAction { Stay, Next, Back }

pub struct FirstRun { /* the app's name for the header, the pages, the current one */ }
impl FirstRun {
    pub fn new(app_name: impl Into<String>, pages: Vec<Box<dyn FirstRunPage>>) -> FirstRun;
    /// Whether it has been finished before (read from the store attached to `ctx`, which
    /// is already the app's own, so no key is needed).
    pub fn done(ctx: &egui::Context) -> bool;
    /// Draw the current page over the whole window; `true` once the last page is done,
    /// which is recorded in the store.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool;
    /// Forget that it was finished, so it shows at the next start (an About button).
    pub fn reset(ctx: &egui::Context);
}
```

- The app builds the pages in its own order. Until `done`, it draws the `FirstRun` in place of its shell.
- `FirstRun` draws a header (the app's name, the page's title, "step 2 of 4") and a footer with **Back** (not on the first page) and **Next** (**Finish** on the last), enabled by `can_continue`. A `PageAction` the page returns moves the same way.
- An app closed halfway starts from the first page again. Each page keeps its own answers, so finished steps are not applied twice.
- Completion is stored in `corundum.toml` (`store`), next to the look.

### `ServicePage` (feature `service`)
- A `FirstRunPage`. The app gives it the services and the words: a question and the reason.
- **Yes** `install`s each service and then reports the result. A failure shows the error and offers **Try again** or **Continue without it**.
- **No** registers nothing.
- Either answer is stored, so the About row knows it was asked.
- In a build that runs from a cargo target directory (as the launcher decides) it says so and offers only **Next**, registering nothing: a service must not point into `target/`. The rest of the flow is shown as usual.
- Its `can_continue` is false until it has been answered, so the footer's Next cannot skip the question.

### `ServiceRow` (feature `service`)
- An About-page row per service.
- Its state is found on a thread, so it never blocks a frame.

| State | Shown |
|---|---|
| NotInstalled | **Keep running in the background** |
| Running / Stopped | the state, **Stop** or **Start**, **Remove** |
| Broken | the reason, **Repair** (install again) |
| the state could not be found | the error, **Try again** |

- In a build that runs from a cargo target directory, as `ServicePage` decides: NotInstalled
  shows the development-build note instead of **Keep running in the background**, and
  Broken shows the reason without **Repair**, so nothing registers a program in `target/`.
  **Stop**, **Start** and **Remove** stay, so a registration an installed build made can
  still be removed.

## The demo

- **The daemon becomes a subcommand:** `corundum-demo daemon` replaces the separate `corundum-framework-demo-daemon` binary, which is the single-binary shape the sapphire apps take. It is registered as `ServiceSpec { app_name: "corundum-demo", args: ["daemon"], .. }`.
- **The pid-file stop goes:** the spike's code that stopped the daemon through a pid file is removed. The package's stop-and-restart replaces it.
- **First-run setup:** two pages, a sample welcome page and `ServicePage`. About has a `ServiceRow` and a "Run first-run setup again" button.

## sapphire-framework

This is a separate issue in sapphire-framework, not this plan's work:

1. `sapphire-framework-service` becomes a crate that re-exports `corundum-framework-service`. It passes `legacy_labels: vec![format!("net.fireturtle.sapphire.{app}")]`.
2. The app server and the bridge move to the corundum crate.
3. The re-exporting crate is removed one release later.

Labels, unit names and task names stay what an installed user has: `net.sapphire-turtle.<app>`, `<app>.service` and the task name.

## Tests

- **Moved as they are:** the golden tests of the unit, the plists and the task XML, and the install / uninstall / relabel flows against `RecordingManager`.
- **`state`:**
  - pure functions from each tool's output, tested by table;
  - `Broken` with temporary directories;
  - the systemd unit carrying `ConditionPathExists`.
- **`stop` / `start`:** the commands each backend runs, through `RecordingManager`. `stop` of a stopped service is not an error.
- **package:**
  - with a fake backend and `RecordingManager`, a running service is stopped before apply, written to `services-to-restart`, and started at the next start;
  - a failed apply starts it again at once;
  - a `services-to-restart` that cannot be written applies nothing; a state that cannot be read stops and notes the service.
- **gui:**
  - `FirstRun`: pages in order; Back and Next; finishing is stored and not shown again; `reset` shows it again; a restart halfway begins at page one.
  - `ServicePage`: yes, no, and a failure.
  - `ServiceRow`: the buttons for each state, through a `status_of` seam like `CliRow`'s.
- **On the machines:**
  - Linux and Windows: register, update with the service running (stopped, then running again), and uninstall (Windows: the task gone).
  - macOS: the LaunchAgent-file path.

## Not in this part
- Signing and the packaging rules (part 1).
- sapphire's move (a separate issue).
- System services (LaunchDaemons, Windows services): every service is the user's own, as today.
