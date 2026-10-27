# Updating from the GUI

## Goal

Today a sapphire app server learns of a newer release once a day and the GUI shows "update
available", but installing it is a terminal command (`<binary> update`), run once per
binary: the desktop app, its server, sapphire-bridge. After this, the desktop app's
Settings → About lists those three, each with its version, the newest release and an
Update button that downloads, replaces and restarts it. asteria's desktop app gets the same
panel for itself and its server.

Success means:

- one click updates a binary installed from a GitHub release, and restarts it;
- a binary installed by cargo, `cargo binstall`, a package manager or an app store is
  never replaced: the panel says how it is updated instead;
- a build for an app store contains no code that downloads or replaces an executable;
- sapphire and asteria share the code: release lookup, replacing, the panel.

## Decisions

1. **The release code moves to corundum.** `sapphire-framework-update` becomes
   `corundum-framework-update`; nothing in it is sapphire's. Tag prefixes and asset names
   already come from the caller (`ReleaseSource`), which covers asteria's names (no
   `-desktop` suffix, its own tag prefix).
2. **Only binaries on this machine.** The server and the bridge the GUI updates are the
   ones it already starts and installs as services here; a remote server is not updated.
3. **Manual only.** The GUI looks for releases when the About page opens and when "Check"
   is pressed, and installs only on a click. The daily look and the banner stay the app
   server's (`server.update`).
4. **The bridge is included.** It is one of the sapphire GUI's three targets; any sapphire
   app's GUI may update it.
5. **Off for store builds, at compile time.** A cargo feature `self-update` (on by default)
   holds the downloading and replacing. Without it the panel only shows versions. Store
   review (the Mac App Store's especially) objects to such code being present at all, so a
   runtime check alone is not enough.
6. **Stores are recognised at run time too.** `InstallMethod::Store` (Flatpak, Snap, MSIX,
   Mac App Store) behaves like a package manager: no button. This covers a store build that
   forgot to turn the feature off.

## corundum-framework-update

A new crate, the contents of `sapphire-framework-update` at its current version, with:

- **Kept as is**: `ReleaseSource`, `Release`, `Asset`, `latest_release`, `client`,
  `asset_name`, `platform`, `InstallMethod`, `BinstallSource`, `install_method`, `Error`.
  Doc comments and test data stop saying "sapphire" where they mean any app.
- **Behind `cli`** (off by default): `UpdateArgs` and `run`, the `update` subcommand, which
  need clap. sapphire's CLIs and servers turn it on.
- **Behind `self-update`** (on by default): `install` (replace the running executable,
  through `self-replace`), the download, and the new `replace_file`.
- **New `replace_file(path, bytes)`**, for an executable other than the running one (the
  server, the bridge), which may be running:
  - Unix: write a temporary file in `path`'s directory, mode 0755, `fsync`, rename over
    `path`. A running process keeps its old inode.
  - Windows: rename `path` to `path.old` (allowed while it runs), then move the new file
    to `path`. If that move fails, rename `path.old` back. A `path.old` left by an earlier
    update is removed first, ignoring a failure (it may still be running).
- **`InstallMethod::Store`**, detected before the others:
  - Flatpak: `/.flatpak-info` exists;
  - Snap: `$SNAP` is set;
  - MSIX: the process has a package identity (`GetCurrentPackageFullName` succeeds);
  - Mac App Store: the bundle has `Contents/_MASReceipt/receipt`.

`sapphire-framework-update` stays for one release as a crate that re-exports
`corundum-framework-update` with `cli` on, so the apps move at their own pace; it is
removed in the release after every app has moved.

## The panel (corundum-framework-gui, module `update`)

Behind a new feature `update` of `corundum-framework-gui` (off by default), which depends
on `corundum-framework-update` without default features; the gui's own `self-update`
feature turns on the update crate's.

```rust
pub struct UpdateTarget {
    /// What the row says: `Sapphire Journal`, `sapphire-journal server`, `sapphire-bridge`.
    pub label: String,
    /// The version running now, or `None` when it is not known (a server not running).
    pub current: Option<semver::Version>,
    pub source: ReleaseSource,
    /// The executable to replace.
    pub exe: PathBuf,
    pub after: AfterUpdate,
}

pub enum AfterUpdate {
    /// The app itself: start `exe` again with the same arguments, then close the window.
    Relaunch,
    /// A service: the app's own way to restart it, run on the panel's worker thread.
    Restart(Box<dyn Fn() -> Result<(), String> + Send + Sync>),
    /// Nothing the app can do: the row says to restart it by hand.
    Manual,
}

pub struct UpdatesPanel { /* rows and their state */ }
impl UpdatesPanel {
    pub fn new(targets: Vec<UpdateTarget>) -> Self;
    pub fn ui(&mut self, ui: &mut egui::Ui);
}
```

Each row: label, current version, newest release, and one of

| State | Shown |
|---|---|
| Checking | spinner |
| Up to date | "Up to date" |
| Newer release | version, link to its release notes, **Update** |
| Downloading / Restarting | spinner and what it is doing |
| Done | "Updated to x.y.z" |
| Restart by hand | "Updated; restart it to use x.y.z" |
| Not by this app | how it is updated, with the command to copy (`cargo binstall --git … <crate>`, "Updated by the store", "Updated by your package manager") |
| Failed | the error, **Retry** |

A "Check" button above the rows looks again. Without `self-update` there is no Update
button and the newer release reads "Download from the release page".

The work runs on one worker thread the panel owns, with a current-thread tokio runtime;
results come back over a channel and the panel asks for a repaint. One target is updated at
a time; the others' buttons wait.

The state machine (`RowState` and its transitions on each result) is a plain type with no
egui, tested on its own.

## sapphire-framework-gui

The About page gets an `UpdatesPanel` with three targets:

| Target | `exe` | `after` |
|---|---|---|
| The desktop app | `current_exe()` | `Relaunch` |
| The app server | `ClientConfig.service_exes.app` | `Restart`: `<exe> service restart`, then reconnect |
| sapphire-bridge | `ClientConfig.service_exes.bridge` | `Restart`: `sapphire-bridge service restart` |

The current versions are the ones the GUI already has (its own `CARGO_PKG_VERSION`, the
server's and the bridge's from their status). A service that is not installed as a service
(`service status` says so) gets `Manual`.

`sapphire-framework-service` gains `ServiceCommand::Restart`:

| Platform | Restart |
|---|---|
| Linux | `systemctl --user restart <app>.service` |
| macOS | `launchctl kickstart -k gui/<uid>/<label>` |
| Windows | `schtasks /end /tn <task>`, then `schtasks /run /tn <task>` |

`server.restart` (which reloads settings in the same process) is not used for this: it
does not load the new executable.

The bridge restarting drops every app's connection to it for a moment; the GUIs already
reconnect.

## asteria

asteria's desktop app builds two targets (itself, its server) the same way, with its own
`ReleaseSource`s and its own restart. That wiring is asteria's work, not part of this spec.

## Testing

- `replace_file`: on a temporary directory, replaces the content and keeps the mode; on
  Windows, the rename-aside path, the restore when the move fails, the removal of an old
  `.old`. The decisions are split into a function over a small file-system trait, so the
  Windows path is tested on every platform.
- `InstallMethod::Store`: each marker, on a temporary root.
- `latest_release`, the download: the existing tests against a local HTTP server, moved.
- The panel's state machine: every transition, including a failure while restarting and
  a target installed by cargo.
- `ServiceCommand::Restart`: the commands recorded by `RecordingManager`, per platform.
- A build without `self-update` does not link `self-replace` (`cargo tree -e features`
  check in CI).

## Out of scope

- Updating a server on another machine.
- Automatic updates, or updating at start.
- Signature checks beyond HTTPS from the GitHub release (as today).
- Store packaging, and a bridge shared by store-installed apps.
