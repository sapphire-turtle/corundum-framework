# The app as a Velopack package: runtime

Issue: corundum-framework#51, part 2 ("runtime"). Packaging in CI and signing are part 1;
registering background services with the OS is part 3. The spike behind this design is
`docs/superpowers/spikes/2026-10-28-velopack.md`.

## Goal

The sapphire apps and asteria are meant for people who are not developers: someone who
needs help to keep a diary or a household budget installs one like any other app. Today
an app is several binaries put in place by a script or by cargo. After this, a desktop
app is one Velopack package per app, on Windows, macOS and Linux, and corundum gives it
everything that does not depend on the app:

- knowing at start whether it was just installed or updated, and running the app's own
  install and uninstall steps;
- updating itself: found and downloaded on its own, applied when the user says so, with
  the app's background processes stopped first;
- its command line on `PATH`;
- a console for that command line on Windows, where the app's binary has none;
- the app menu entry left to the package, where the package makes one.

Success means:

- an app installed from its Velopack package updates from Settings › About or its status
  bar badge, and from its own command line, and comes back in the new version;
- the app's background processes never keep running the old version after an update;
- `<app> …` works in a terminal on every OS once the user asked for it (on Windows, once
  installed);
- an app installed by the scripts or cargo works as it does today;
- a daemon without a window (sapphire's bridge) uses the same update code as the GUI.

## Decisions

1. **One binary per app, on every OS.** The desktop app carries its server and its command
   line as subcommands; the command-line-only build stays for headless machines and is
   installed by script. A person installs one or the other. This is what lets Linux use
   Velopack's AppImage: the spike found that the AppImage gives the executables in it no
   lasting path, only `$APPIMAGE`, which starts the main binary. (The apps' side of this
   is sapphire-framework#357.)
2. **Windows adds a console shim.** The app's binary is built for the Windows GUI
   subsystem, which has no console. A second, console binary on `PATH` starts it with the
   same arguments and standard streams and waits for it.
3. **Updates: found and downloaded automatically, applied on demand.** A GUI checks at
   start and then every day and downloads what it finds; the user applies it with a
   button. Not applied when the GUI closes; applied at the next start of the GUI if the
   user never pressed it. Velopack's own apply-at-start is not used: it would also fire
   when the binary starts as a server or a command, and it cannot stop the app's
   background processes first.
4. **Every package updates itself.** A package without a window (sapphire's bridge)
   updates with the same code as a GUI, and is told to apply through its own API or
   command line. The GUI shows such a package as a row it delegates to, and never updates
   another package itself.
5. **The app owns its background processes on every OS.** The spike found that only
   Windows stops the processes in the package on update and runs a hook on uninstall. So
   before applying an update the app stops its processes itself, through one callback
   used by every way of applying.
6. **The app tells first start and update by itself.** It keeps the version it last ran;
   `VELOPACK_FIRSTRUN` adds an install to that (macOS reinstall over old data). Velopack's
   Windows hooks are used for install and uninstall only.
7. **`PATH`: automatic on Windows, a button on macOS and Linux.** Windows has install and
   uninstall hooks to add and remove it. On macOS nothing runs on uninstall, so a link put
   there unasked would be left dangling; the user adds and removes it.
8. **The launcher steps aside inside a package.** The spike found that its generated
   macOS bundle collides with the package's, that Windows gets Velopack's own shortcut,
   and that inside an AppImage `current_exe()` is a temporary mount.
9. **A new crate, `corundum-framework-package`, holds everything that needs Velopack.**
   `corundum-framework-update` keeps replacing binaries placed by hand and does not depend
   on Velopack, so only the apps that are packages pull in the `velopack` crate and its
   dependencies.

## Crates

| Crate | Change |
|---|---|
| `corundum-framework-package` | New, desktops only. Start and hooks (`start`), updates (`Updater`), `cli_path`, `console` |
| `corundum-framework-platform` | New `package` module: whether the process runs in a Velopack package, from the files around it, without the `velopack` crate. The launcher uses it |
| `corundum-framework-update` | `InstallMethod::Velopack`; the shared state types `PackageState` and `PackageStatus` |
| `corundum-framework-gui` | `UpdatesPanel` takes three kinds of target; `update::ReadyBadge`; `settings::CliRow`; `LauncherRow` hidden in a package |
| `corundum-framework` | Features `package` and `gui-package` |

```
platform ← update ← gui
platform ← update ← package ← gui (feature gui-package)
```

`package` does not depend on `gui`, so a daemon uses it without egui.

## corundum-framework-platform: `package`

```rust
pub struct PackageLayout {
    pub kind: PackageKind,
    /// Windows: `%LocalAppData%\<id>`; macOS: the `.app`; Linux: `$APPIMAGE`.
    pub root: PathBuf,
    /// The directory holding the package's executables: `<root>\current`, `<.app>/Contents/MacOS`,
    /// the AppImage's mount (valid for this run only).
    pub bin_dir: PathBuf,
}
pub enum PackageKind { Windows, MacBundle, AppImage }

/// The package the running executable is in, or `None`.
pub fn current() -> Option<PackageLayout>;
/// The running executable as `current` looks at it: links resolved on macOS and Linux.
pub fn current_exe() -> Option<PathBuf>;
```

Told from the files Velopack lays out, as its own locator does:

- Windows: the executable's directory is named `current` and its parent holds
  `Update.exe`;
- macOS: the path has a `.app/` with `Contents/MacOS/UpdateMac` in it;
- Linux: `$APPIMAGE` is set and the executable's directory holds `UpdateNix`.

The checks take the paths and the variable as arguments (`current_with`), so the tests
give made-up layouts. The executable's links are resolved first (but on Windows, whose
canonical paths are `\\?\` paths): on macOS a start through the command line's
`/usr/local/bin` link reports the link as the executable. The package crate hands
Velopack the same resolved executable (`LocationContext::FromSpecifiedAppExecutable`,
then `set_locator` and `UpdateManager::new` with the paths found), as Velopack's own
search looks at the unresolved path.

## corundum-framework-package

### Start

```rust
pub struct PackageSpec {
    /// The app's directory name, where the last version run is kept: `sapphire-journal`.
    pub app_name: &'static str,
    /// Run once per install: in the install hook on Windows, at the first start elsewhere.
    /// It must be safe to run again.
    pub on_installed: Option<fn()>,
    /// Run before uninstalling, on Windows only (no other OS runs anything): 30 seconds,
    /// no window.
    pub on_uninstalling: Option<fn()>,
    /// Run before any update is applied: stop the app's background processes. An error
    /// stops the update.
    pub before_apply: Option<fn() -> Result<(), String>>,
    /// The command line to put on `PATH`, if the app has one.
    pub cli: Option<CliSpec>,
}

pub enum Start {
    /// Not in a package (`cargo run`, the scripts, cargo).
    NotPackaged,
    FirstStart { version: Version },
    Updated { from: Version, to: Version },
    Same { version: Version },
}

/// Velopack's start, then what this start is. The first thing `main` runs but the app's
/// logging (a hook's failures are only logged): started for a Windows hook, it runs the
/// hook and exits.
pub fn start(spec: &PackageSpec) -> Start;
```

- The Windows install hook runs `on_installed` and adds the command line to `PATH` (when
  `cli` is set); the uninstall hook runs `on_uninstalling` and removes it. The update hooks
  (`--veloapp-obsolete`, `--veloapp-updated`) exit without doing anything.
- Then `VelopackApp::run`, with Velopack's apply-at-start turned off
  (`set_auto_apply_on_startup(false)`). Before it, `start` notes whether
  `VELOPACK_RESTART` is set (`run` removes it): Velopack's updater started the app.
- The last version run is kept in `<data dir>/<app_name>/package-version`. `FirstStart`
  when it is missing or `VELOPACK_FIRSTRUN` is set; `Updated` when it differs from the
  package's; `Same` otherwise. The file is written before `start` returns. On macOS and
  Linux, `FirstStart` runs `on_installed`.
- Choosing the `Start` is a pure function of the kept version, the variable and the
  package's version (`classify`).

### Updates

```rust
pub enum Source {
    Github { repo: String, prerelease: bool },
    Http(String),
}
pub enum After {
    /// Start the app again without arguments: the GUI. The caller ends the process.
    RestartApp,
    /// Start the app again with these arguments. The caller ends the process.
    RestartWith(Vec<OsString>),
    /// Exit; something else starts it again (a service manager), or nothing. `apply`
    /// exits the process.
    Exit,
}

impl Updater {
    /// `Error::NotPackaged` outside a package.
    pub fn new(spec: &'static PackageSpec, source: Source) -> Result<Updater, Error>;
    /// Called after every change of state, on the updater's thread or on the thread
    /// calling `apply` (when `before_apply` refuses): a GUI asks for a repaint here.
    pub fn on_change(&self, f: impl Fn() + Send + Sync + 'static);
    pub fn status(&self) -> PackageStatus;
    /// Look now (the Check button, `<app> update`), and download what is found.
    pub fn check_now(&self);
    /// Look at once and then every `every`, downloading what is found.
    pub fn run_in_background(&self, every: Duration);
    /// Run `before_apply`, then hand the downloaded update to Velopack's updater. Exits
    /// for `After::Exit`; returns `Ok` for a restart, and the caller ends the process.
    pub fn apply(&self, after: After) -> Result<(), Error>;
    /// For a GUI's start only, before its window opens: apply an update downloaded
    /// earlier and never applied, and exit. Nothing on a start by Velopack's updater.
    pub fn apply_pending_at_start(&self) -> Result<(), Error>;
}
```

In `corundum-framework-update`, so that a GUI row and an app's own API (sapphire's bridge)
use the same types:

```rust
pub struct PackageStatus { pub current: Option<Version>, pub state: PackageState }
pub enum PackageState {
    Idle,
    Checking,
    UpToDate,
    Downloading { percent: u8 },
    Ready { version: Version },
    Failed(String),
}
```

- One thread per `Updater` does the checking and downloading; the state is shared, and
  `status` never blocks.
- `apply` starts Velopack's updater, which waits for the process to exit (60 seconds at
  most), replaces the files, and with `RestartApp` or `RestartWith` starts the app again.
  Who ends the process: with `Exit` (a daemon, a command line) `apply` exits it itself;
  with a restart `apply` returns `Ok` and the caller ends it soon, a GUI by closing its
  window (`ViewportCommand::Close`), so eframe saves its state and the app's on-exit code
  runs, as the binary row does after relaunching.
- One apply at a time: a second `apply` while one runs (the badge and the panel clicked
  together) returns `Error::Applying` at once. Once an update is handed to Velopack, later
  calls return it too: the process is ending.
- `apply_pending_at_start` looks only at the packages already downloaded, not at the
  network, so a GUI start is not delayed. It runs before the window exists, so after a
  successful apply it exits the process itself. On a start by Velopack's updater
  (`VELOPACK_RESTART`, noted by `start`) it applies nothing, as Velopack's own
  apply-at-start does: an update the updater failed to apply would otherwise be tried
  again at every start, in a loop.
- **An app applies an update only from its single running GUI instance; apps that can run
  two GUI instances must prevent a second one (a single-instance lock) before using
  `apply_pending_at_start` or the apply button.**
- Errors: `NotPackaged`, `Network`, `Velopack` (anything else from Velopack), `Applying`
  (another apply runs), `Prepare` (`before_apply` failed; nothing is applied).
- Which `Source` an app uses (a GitHub channel per app and OS, or a feed of its own) is
  part 1's to decide; the spike found `GithubSource` reads only a repository's 10 newest
  releases and does not tell packages apart.
- The calls into Velopack go through a private trait, so the state handling is tested with
  a fake.

### Command line on `PATH`: `cli_path`

```rust
pub struct CliSpec {
    /// The name typed in a terminal: `sapphire-journal`. On Windows, the console shim's
    /// file name without `.exe`.
    pub command: &'static str,
}
pub enum CliStatus { Installed, Missing, Broken, Taken, Unsupported }
pub fn status(spec: &CliSpec) -> CliStatus;
pub fn install(spec: &CliSpec) -> Result<(), Error>;
pub fn uninstall(spec: &CliSpec) -> Result<(), Error>;
```

| OS | Where | When |
|---|---|---|
| Windows | `<root>\current` added to the user `Path` (`HKCU\Environment`, keeping its `REG_EXPAND_SZ` type), then `WM_SETTINGCHANGE` broadcast | The install hook; removed by the uninstall hook |
| macOS | `/usr/local/bin/<command>`, a link to the binary in the `.app`, made with administrator rights through `osascript` (`mkdir -p` and `ln -sfh`, so a link to a directory is replaced, not linked into) | `CliRow`'s button |
| Linux | `~/.local/bin/<command>`, a link to `$APPIMAGE` | `CliRow`'s button; it says so when `~/.local/bin` is not on `PATH` |

- `Broken`: the link is there and its target is not (the `.app` or the AppImage was
  deleted or moved); the row offers to make it again, replacing the dangling link.
- `Taken`: another program has the name: a file that is not a link (a command-line-only
  install of the same app at `~/.local/bin/<command>`), or a link to another file that
  exists (Homebrew's in `/usr/local/bin`, pipx's in `~/.local/bin`). `install` and
  `uninstall` refuse it, never replacing another program's command, as root least of
  all; the row says another `<command>` is installed there.
- `Unsupported`: outside a package.
- An update keeps the target's path (spike), so no link is made again after one.
- Adding to and removing from a Windows `Path` value is a pure function: no duplicate,
  case and a trailing `\` ignored, the other entries untouched.

### Console shim: `console` (Windows)

```rust
/// Start `gui_exe`, beside this executable, with this process's arguments and standard
/// streams; exit with its exit code.
pub fn forward(gui_exe: &str) -> !;
```

An app adds a console binary of three lines, named `<command>.exe`. The child is put in a
job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so it ends with the shim (Ctrl+C in
the terminal), and `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`, so what the child starts is not
in the job: Velopack starts `Update.exe` without leaving the job, and the GUI exits for
the update to be applied; in the job, `Update.exe` (and the app it restarts, and any
daemon) would be killed with the shim halfway through the update. A job that cannot be
made is reported on standard error, as the shim has no logging. Standard input stays connected: interactive prompts and the stdio MCP
transport work through it. The package's main executable (`--mainExe`) and its shortcuts
are the GUI binary; `PATH` finds the shim.

## corundum-framework-platform: the launcher in a package

`launcher::register`, `status` and `LauncherRow` look at `package::current()`:

| Where | What the launcher does |
|---|---|
| Not in a package | As today |
| Windows package | Writes nothing: Velopack makes the Start menu shortcut and the "Apps & features" entry. Removes its own `.lnk` left by an earlier install outside a package |
| macOS package | Writes nothing. Removes a bundle it generated earlier under the app's name (its executable a link, no `sq.version` in it), never the bundle it runs from |
| AppImage | Writes the entry with `Exec` and the status check on `$APPIMAGE`, not `current_exe()` |

`LauncherRow` is hidden in a Windows or macOS package, where the entry is the OS's and the
installer's.

## corundum-framework-gui

### The panel's targets

```rust
pub enum Target {
    /// A binary replaced from a GitHub release: today's row, unchanged.
    Binary(UpdateTarget),
    /// This app's package.
    Package(PackageTarget),
    /// Another package, updated by itself when told (sapphire's bridge).
    Delegated(Box<dyn DelegatedTarget>),
}
impl From<UpdateTarget> for Target { … }

impl Target {
    /// `Package` in a package, else `Binary(binary)`.
    pub fn this_app(label: String, updater: Option<Updater>, binary: UpdateTarget) -> Target;
}

/// Called on the panel's worker thread; each call may block.
pub trait DelegatedTarget: Send + Sync + 'static {
    fn label(&self) -> String;
    fn status(&self) -> Result<PackageStatus, String>;
    fn check(&self) -> Result<(), String>;
    fn apply(&self) -> Result<(), String>;
}

impl UpdatesPanel {
    pub fn new(targets: Vec<Target>, user_agent: &str) -> UpdatesPanel;
}
```

`Package` and `this_app` need the gui crate's `package` feature. `UpdatesPanel::new` taking
`Target` is a breaking change of the gui crate (the next minor version); callers add
`.into()`. `UpdateTarget` and `AfterUpdate` keep their names and fields.

A `Package` or `Delegated` row:

| State | Shown |
|---|---|
| Checking | spinner |
| UpToDate | "Up to date (x.y.z)" |
| Downloading | progress bar |
| Ready | "x.y.z is ready" and **Restart to update** (`Package`: `apply(After::RestartApp)`; `Delegated`: `apply()`) |
| Failed | the error and **Retry** |
| Idle | the current version |

The panel's Check button calls `check_now` and `DelegatedTarget::check`. While the panel is
shown, a `Delegated` row's status is asked every 5 seconds. The rows' text and buttons come
from a pure function of `PackageStatus`, tested by table.

### `update::ReadyBadge`

A status bar item: nothing until the app's `Updater` is `Ready`, then an icon whose tooltip
names the version and whose click applies it (`After::RestartApp`) and, once that
succeeds, closes the root window. A `Package` row of the panel does the same after its
apply succeeds; a `Delegated` row does not, as another package restarts, not this app.

### `settings::CliRow`

For an About page, in a macOS or Linux package: "Command line: `<command>`", its status,
and **Install**, **Remove** or **Repair**. In a Windows package it only shows the status
(installed by the installer); outside a package it shows nothing.

## The demo

The spike's own page and code give way to these pieces: `package::start` in `main`,
`Target::this_app`, `ReadyBadge`, `CliRow`, and on Windows a console shim binary. The
spike's daemon stays, stopped by the demo's `before_apply`, to show the update stopping it.
`demo-release.yml` stays as it is to try the work on the machines; the release CI proper
is part 1.

## Tests

- `platform::package::current_with`: each OS's layout, and the near misses (a `current`
  directory without `Update.exe`, a `.app` without `UpdateMac`); a link to the binary in a
  `.app` is in the package.
- `classify`: kept version missing or equal or older or newer, with and without
  `VELOPACK_FIRSTRUN`.
- `Updater`: state changes and `before_apply` ordering against a fake Velopack;
  `Prepare` applies nothing; a restart's `apply` returns `Ok`; a second apply while one
  runs is refused; a start by Velopack's updater applies no pending package.
- `cli_path`: the Windows `Path` edits by table; the link states in a temporary directory,
  a link to another program `Taken` and left alone.
- `console::forward`: on the Windows CI job, a test child's output and exit code come
  through.
- The launcher in a package, through the existing `Root` with made-up layouts; the bundle
  it generated is told apart from a package's.
- The panel: rows from `PackageStatus` by table; a `Delegated` row with a fake; the buttons'
  presence and state, as the panel's tests do today; a `Package` row's and the badge's
  successful apply closes the window, a failed one does not.
- CI: without `gui-package` or `package`, the `velopack` crate is not in the tree (as the
  `self-replace` check today).
- On the machines: the demo's packages, installed, updated with the daemon running, and
  uninstalled, on each OS.

## Not in this part

- Building, signing and publishing the packages (part 1), and the rule for update sources.
- Registering background processes with the OS (part 3); `before_apply` is where an app
  stops them.
- sapphire's side: the apps as one binary, their versions released together, the bridge's
  update command and API (sapphire-framework#357).
- Mac App Store and MSIX.

## To check on the way

- Whether the AppImage Velopack makes needs `libfuse2` (an older AppImage runtime) or not
  (the static type2 runtime): a light user on Ubuntu 24.04 or Arch has no `libfuse2`.
- On Windows, whether the launcher's Start menu entry doubled Velopack's in the spike, and
  whether the daemon outlived the uninstall.
