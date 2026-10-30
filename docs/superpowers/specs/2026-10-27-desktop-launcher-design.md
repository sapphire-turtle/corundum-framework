# The desktop launcher entry

## Goal

A desktop app installed by `install.sh`, `install.ps1` or `cargo binstall` lands in
`~/.local/bin` (or `$CARGO_HOME/bin`), where no OS shows it: today it is started from a
terminal. After this, the app puts itself in the OS's app menu the first time it runs, and
from then on it is started like any other app, by its icon.

Success means:

- after one run from a terminal, the app is in the app menu (GNOME, KDE), the Start menu
  (Windows) and `~/Applications` (macOS), with its own icon and name;
- the running window is matched to that entry: the taskbar or dock shows the app's icon,
  not a generic one;
- `update` replacing the executable in place keeps the entry working;
- every sapphire app and asteria get this by naming themselves once; none writes a platform
  file of its own.

## Decisions

1. **The app registers itself, on every start.** Not the install scripts, which `cargo
   binstall` bypasses. Registering is idempotent: nothing is written when the files already
   say the same.
2. **The executable stays where it was installed.** The entry points at
   `std::env::current_exe()`; nothing is copied. The install scripts and `update` are
   unchanged.
3. **No entry for a build in a cargo target directory.** An executable under a `target/debug`
   or `target/release` directory, or a start with `CORUNDUM_NO_LAUNCHER` set, registers
   nothing, so `cargo run` does not leave an entry behind.
4. **Removing is remembered.** The user can take the entry away from Settings; the app then
   stops registering until they put it back.
5. **macOS gets a generated `.app`.** The app writes a small bundle into `~/Applications`
   that starts the installed executable (see [macOS](#macos)). A spike settles first whether
   such a bundle shows the right Dock icon and name; if it cannot, macOS falls back to a
   bundle built in CI (Decision 5b below) and this spec is revised before that work starts.
6. **Failure never stops the app.** A launcher that cannot be written is a warning in the
   log.

## API

In `corundum-framework-platform`, a new module `launcher` (no egui, as the crate's other
modules):

```rust
pub struct LauncherSpec {
    /// The app's id: the file names, the Linux app id and `StartupWMClass`,
    /// the macOS bundle identifier's last part. `sapphire-journal`.
    pub id: &'static str,
    /// The name in the menu: `Sapphire Journal`.
    pub name: &'static str,
    /// One line for the menu's tooltip / comment.
    pub description: &'static str,
    /// freedesktop main category: `Office`, `Graphics`, `AudioVideo`, …
    pub category: &'static str,
    /// The app icon, an SVG (the series tile with the app's glyph), embedded with
    /// `include_bytes!`.
    pub icon_svg: &'static [u8],
    /// The macOS bundle identifier's prefix: `net.sapphire-turtle`.
    pub bundle_prefix: &'static str,
}

/// Write or refresh the entry for the running executable. `Ok(Registered::Skipped)` for a
/// build in a cargo target directory or with `CORUNDUM_NO_LAUNCHER` set; a no-op on iOS
/// and Android.
pub fn register(spec: &LauncherSpec) -> io::Result<Registered>;

/// Remove what `register` wrote.
pub fn unregister(spec: &LauncherSpec) -> io::Result<()>;

/// Whether an entry for `spec` exists and points at the running executable.
pub fn status(spec: &LauncherSpec) -> Status;

pub enum Registered { Written, Unchanged, Skipped }
pub enum Status { Registered, Stale, Absent, Unsupported }
```

The writing goes through a `Root` (the home directory, or a temporary one in tests), so no
test touches the host's menus. The file contents are built by pure functions
(`desktop_entry`, `info_plist`, …) that the tests compare against expected text.

In `corundum-framework-gui`:

- `WindowOptions::launcher(spec)`. `window::run` then, before opening the window:
  - calls `launcher::register` unless the store says the user removed the entry, logging a
    failure with `tracing::warn!`;
  - sets the viewport's app id (`with_app_id(spec.id)`) and its icon (the SVG rendered to
    RGBA with resvg), so the window matches the entry.
- `Stored` gains `launcher: bool` (default `true`; absent in an existing `corundum.toml`
  reads as `true`).
- `settings::LauncherRow`: a row for the app's About page, "In the app menu" with a
  Remove / Add back button, calling `unregister` / `register` and saving `launcher`.

## Per platform

### Linux

- `~/.local/share/applications/<id>.desktop` (`$XDG_DATA_HOME` when set):

  ```ini
  [Desktop Entry]
  Type=Application
  Name=<name>
  Comment=<description>
  Exec="<current_exe>" %U
  Icon=<id>
  Categories=<category>;
  StartupWMClass=<id>
  Terminal=false
  ```

  The path is quoted per the Desktop Entry spec (`"`, `` ` ``, `$` and `\` escaped).
- `~/.local/share/icons/hicolor/scalable/apps/<id>.svg`: the SVG as given.
- After writing, `update-desktop-database ~/.local/share/applications` when it is on
  `PATH`, ignoring its failure; GNOME and KDE watch the directory without it.

### Windows

- `%APPDATA%\Microsoft\Windows\Start Menu\Programs\<name>.lnk`, made through COM
  (`IShellLinkW` + `IPersistFile`, the `windows` crate), target `current_exe()`, working
  directory its folder, icon the executable's own (resource index 0), and the
  `System.AppUserModel.ID` property set to `<bundle_prefix>.<id>` so the taskbar groups the
  window with the shortcut.
- The executable carries the icon: a new crate `corundum-framework-build` offers
  `embed_icon(svg_path)` for an app's `build.rs`, which renders the SVG to a multi-size
  `.ico` (16, 24, 32, 48, 64, 256) and embeds it with `winresource`. Building on Linux for
  Windows embeds nothing and says so as a cargo warning.
- `windows_subsystem = "windows"` is the app's to set in its `main.rs`; the corundum
  README says so, since without it a console window opens beside the app.

### macOS

`~/Applications/<name>.app`:

```
Contents/
  Info.plist     CFBundleIdentifier <bundle_prefix>.<id>, CFBundleName <name>,
                 CFBundleExecutable <id>, CFBundleIconFile <id>,
                 CFBundlePackageType APPL, LSMinimumSystemVersion 11.0,
                 NSHighResolutionCapable true
  MacOS/<id>     the launcher (below)
  Resources/<id>.icns   rendered from the SVG (16 … 1024, PNG entries)
```

The spike tries two launchers, in this order, and keeps the first that shows the app's
icon and name in the Dock and the menu bar while the app runs:

1. a symbolic link to `current_exe()`;
2. a shell script `exec "<current_exe>" "$@"`.

If neither does: **Decision 5b** — CI builds the real `.app` (with the executable inside),
the install scripts unpack it into `~/Applications`, `update` replaces the executable
inside the bundle, and `register` on macOS returns `Skipped`. That is a separate
change to the release workflow and the scripts, specified when the spike says it is needed.

A bundle copied by hand elsewhere is not touched: `register` only writes its own path.

### iOS, Android

`register` returns `Skipped`; the store's bundle is the entry.

## The app's side

An app adds, once:

```rust
const LAUNCHER: LauncherSpec = LauncherSpec {
    id: "sapphire-journal",
    name: "Sapphire Journal",
    description: "Journal, tasks and notes",
    category: "Office",
    icon_svg: include_bytes!("../assets/app-icon.svg"),
    bundle_prefix: "net.sapphire-turtle",
};
window::run(WindowOptions::new("Sapphire Journal").launcher(LAUNCHER), create)
```

plus `corundum_framework_build::embed_icon("assets/app-icon.svg")` in `build.rs` and the
`windows_subsystem` attribute. The icon itself comes from the series' icon work
(`icon-drafts/`), and until it is chosen a placeholder tile is used.

## Testing

- The contents: `desktop_entry`, `info_plist` and the `.lnk` fields for a given spec and
  path, against expected text; quoting of a path with spaces and `$`.
- The flow, on a temporary `Root`: first register writes, second is `Unchanged`, a moved
  executable is `Stale` then rewritten, `unregister` removes every file written, a
  `target/debug` path is `Skipped`.
- The icon: the `.icns` and `.ico` writers produce files with the expected sizes, read back.
- The spike on macOS and a manual check on GNOME, KDE and Windows 11 before release, listed
  in the PR.

## Out of scope

- App store packages (Mac App Store, Microsoft Store, Flathub): their bundle is the entry.
- File associations and URL schemes.
- Signing and notarization.
