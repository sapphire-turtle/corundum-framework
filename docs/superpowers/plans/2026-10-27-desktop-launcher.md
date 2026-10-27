# Desktop Launcher Entry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A corundum desktop app puts itself in the OS's app menu (Linux `.desktop`, Windows Start-menu `.lnk`, macOS `~/Applications/<name>.app`) on every start, with its own icon, matched to its window.

**Architecture:** A `launcher` module in `corundum-framework-platform` (no egui) builds every file from pure functions and writes them under a `Root`, so tests never touch the host. An `icon` module there renders the app's SVG to PNG, `.icns` and `.ico`. A new `corundum-framework-build` crate embeds the `.ico` in a Windows executable. `corundum-framework-gui`'s `window::run` calls `register` before opening the window and gives the window the same app id and icon; a Settings row removes or restores the entry.

**Tech Stack:** Rust 2024, resvg 0.45 (already in the lock through egui_extras), `windows` 0.62 (COM, Windows only; the version already in the lock), `winresource` 0.1 (build crate), eframe/egui 0.37.

**Spec:** `docs/superpowers/specs/2026-10-27-desktop-launcher-design.md`

## Global Constraints

- No egui in `corundum-framework-platform`; the crates without a window build it.
- `register` never fails the app: `window::run` logs a failure with `tracing::warn!` and opens the window.
- Skipped (nothing written, `Registered::Skipped`): an executable whose path has a `target/debug` or `target/release` component pair; `CORUNDUM_NO_LAUNCHER` set (any value); iOS; Android.
- The entry points at `std::env::current_exe()`; nothing is copied.
- Idempotent: a file whose content is already right is not rewritten (`Registered::Unchanged`).
- Linux files: `$XDG_DATA_HOME` (else `~/.local/share`) `/applications/<id>.desktop` and `/icons/hicolor/scalable/apps/<id>.svg`.
- Windows file: `%APPDATA%\Microsoft\Windows\Start Menu\Programs\<name>.lnk`, `System.AppUserModel.ID` = `<bundle_prefix>.<id>`.
- macOS file: `~/Applications/<name>.app`, `CFBundleIdentifier` = `<bundle_prefix>.<id>`, `LSMinimumSystemVersion` 11.0.
- `.ico` sizes 16, 24, 32, 48, 64, 256; `.icns` sizes 16, 32, 64, 128, 256, 512, 1024 (PNG entries).
- New dependencies' licenses must pass the existing `deny.toml` (`cargo deny check licenses`).
- New crates join the `corundum` version group in `release-plz.toml`.
- Match the crates' prose style: doc comments say what a thing is, in full sentences; no "sapphire" in corundum code where any app is meant.

## Review Focus

- A path with spaces, `"`, `$`, `` ` `` or `\` in `Exec=`: quoted per the Desktop Entry spec, so the entry still starts the app. → test in Task 3.
- The executable moved (reinstalled elsewhere, `update` from another dir): the old entry is `Stale` and is rewritten, not duplicated. → test in Task 3.
- `XDG_DATA_HOME` set to a relative path: the XDG spec says ignore it; fall back to `~/.local/share`. → test in Task 3.
- A `corundum.toml` written before this change (no `launcher` key): reads as registered, not removed. → test in Task 8.
- An SVG that does not parse: `register` returns an error (logged), and no half-written bundle or icon is left. → test in Task 4 (icon) and Task 5 (bundle written only after the icon renders).

## File Structure

```
crates/corundum-framework-platform/
  Cargo.toml                       + resvg (desktop targets), windows (Windows)
  src/lib.rs                       + pub mod launcher; pub mod icon (desktop targets)
  src/icon.rs                      render_png, icns, ico
  src/launcher/mod.rs              LauncherSpec, Registered, Status, Root, register/unregister/status, skip rules
  src/launcher/linux.rs            desktop_entry, quote_exec, files
  src/launcher/macos.rs            info_plist, bundle files
  src/launcher/windows.rs          LinkFields, link_fields, write via COM
crates/corundum-framework-build/   new: embed_icon for build.rs
crates/corundum-framework-gui/
  src/window/mod.rs                WindowOptions::launcher, register before run, app id + icon
  src/store.rs                     Stored.launcher
  src/settings/launcher.rs         LauncherRow
demo/                              wiring, assets/app-icon.svg, windows_subsystem
release-plz.toml                   corundum-framework-build in the group
```

---

### Task 1: macOS spike (throwaway; needs a Mac)

**Files:** none kept. Work in `/tmp/launcher-spike`.

The answer decides Task 5's launcher kind, or stops macOS work (spec Decision 5b).

- [ ] **Step 1: Build the demo** on the Mac: `cargo build -p corundum-framework-demo --release`.
- [ ] **Step 2: Hand-make the bundle with a symlink launcher**

```
~/Applications/Corundum Demo.app/Contents/Info.plist   (keys from the spec's macOS section; CFBundleExecutable corundum-demo)
~/Applications/Corundum Demo.app/Contents/MacOS/corundum-demo -> <repo>/target/release/corundum-framework-demo
~/Applications/Corundum Demo.app/Contents/Resources/corundum-demo.icns   (iconutil from a 1024 PNG of demo's logo)
```

- [ ] **Step 3: Launch from Finder and Spotlight.** Record: Dock icon (bundle's or generic), menu-bar app name (`Corundum Demo` or the binary's), whether a second Dock icon appears, whether Gatekeeper complains.
- [ ] **Step 4: Replace the symlink with a script** `#!/bin/sh\nexec "<path>" "$@"` (mode 755) and repeat Step 3.
- [ ] **Step 5: Report** in the PR description: the first kind where the Dock icon and the menu name are the bundle's, or "neither". If neither, stop macOS work (Task 5) and raise Decision 5b with the user; Tasks 2–4 and 6–9 go on.

---

### Task 2: `launcher` skeleton: spec, results, root, skip rules

**Files:**
- Create: `crates/corundum-framework-platform/src/launcher/mod.rs`
- Modify: `crates/corundum-framework-platform/src/lib.rs` (module doc line + `pub mod launcher;`)

**Interfaces:**
- Produces:
  - `pub struct LauncherSpec { pub id: &'static str, pub name: &'static str, pub description: &'static str, pub category: &'static str, pub icon_svg: &'static [u8], pub bundle_prefix: &'static str }` (`Clone, Copy, Debug`)
  - `pub enum Registered { Written, Unchanged, Skipped }`, `pub enum Status { Registered, Stale, Absent, Unsupported }` (`Clone, Copy, Debug, PartialEq, Eq`)
  - `pub struct Root { pub home: PathBuf, pub xdg_data_home: Option<PathBuf>, pub appdata: Option<PathBuf> }` with `Root::detect() -> Option<Root>` (from `dirs::home_dir`, `$XDG_DATA_HOME`, `%APPDATA%`)
  - `pub fn register(spec: &LauncherSpec) -> io::Result<Registered>`, `pub fn unregister(spec: &LauncherSpec) -> io::Result<()>`, `pub fn status(spec: &LauncherSpec) -> Status` — call the `*_at` forms with `Root::detect()` and `current_exe()`
  - `pub fn register_at(spec, root: &Root, exe: &Path) -> io::Result<Registered>`, `unregister_at(spec, root) -> io::Result<()>`, `status_at(spec, root, exe) -> Status`: dispatch on `cfg(target_os)` to the per-OS modules (Tasks 3, 5, 6); `Skipped`/`Unsupported` elsewhere
  - `pub(crate) fn skipped(exe: &Path, env_set: bool) -> bool`
  - `pub(crate) fn write_if_changed(path: &Path, bytes: &[u8]) -> io::Result<bool>` (creates parent dirs; `true` when written)

- [ ] **Step 1: Write the failing tests** in `launcher/mod.rs`:

```rust
#[test]
fn a_cargo_build_is_skipped() {
    assert!(skipped(Path::new("/home/u/src/app/target/debug/app"), false));
    assert!(skipped(Path::new("/home/u/src/app/target/release/app"), false));
    assert!(skipped(Path::new("/home/u/src/app/target/x86_64-unknown-linux-gnu/release/app"), false));
    assert!(!skipped(Path::new("/home/u/.local/bin/app"), false));
    assert!(!skipped(Path::new("/home/u/target/app"), false)); // a dir merely named target
}
#[test]
fn the_variable_skips_any_path() {
    assert!(skipped(Path::new("/home/u/.local/bin/app"), true));
}
#[test]
fn an_unchanged_file_is_not_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a/b/file");
    assert!(write_if_changed(&path, b"x").unwrap());
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert!(!write_if_changed(&path, b"x").unwrap());
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert!(write_if_changed(&path, b"y").unwrap());
}
```

`skipped` rule: a `target` component followed by `debug` or `release`, possibly with one target-triple component between.

- [ ] **Step 2: Run** `cargo test -p corundum-framework-platform launcher` — FAIL (module missing).
- [ ] **Step 3: Implement** the items above; `tempfile` as a dev-dependency.
- [ ] **Step 4: Run** the same command — PASS.
- [ ] **Step 5: Commit** `feat(corundum-framework-platform): the launcher module's spec, results and skip rules`.

---

### Task 3: Linux entry

**Files:**
- Create: `crates/corundum-framework-platform/src/launcher/linux.rs`

**Interfaces:**
- Consumes: Task 2's `LauncherSpec`, `Root`, `write_if_changed`.
- Produces: `pub(crate) fn desktop_entry(spec: &LauncherSpec, exe: &Path) -> String`, `pub(crate) fn quote_exec(path: &Path) -> String`, `pub(crate) fn data_home(root: &Root) -> PathBuf`, and `register`/`unregister`/`status` for Linux (also used on the other Unixes but macOS).

- [ ] **Step 1: Write the failing tests**

```rust
const SPEC: LauncherSpec = LauncherSpec { id: "corundum-demo", name: "Corundum Demo",
    description: "Every piece corundum shares", category: "Development",
    icon_svg: b"<svg/>", bundle_prefix: "net.fireturtle.corundum" };

#[test]
fn the_entry_names_the_app_and_its_window() {
    assert_eq!(desktop_entry(&SPEC, Path::new("/home/u/.local/bin/corundum-demo")),
"[Desktop Entry]
Type=Application
Name=Corundum Demo
Comment=Every piece corundum shares
Exec=\"/home/u/.local/bin/corundum-demo\" %U
Icon=corundum-demo
Categories=Development;
StartupWMClass=corundum-demo
Terminal=false
");
}
#[test]
fn reserved_characters_are_escaped() {
    assert_eq!(quote_exec(Path::new("/a b/$x`y\"z\\w")), r#""/a b/\\$x\\`y\\"z\\\\w""#);
}
#[test]
fn a_relative_xdg_data_home_is_ignored() {
    let root = Root { home: "/h".into(), xdg_data_home: Some("rel".into()), appdata: None };
    assert_eq!(data_home(&root), PathBuf::from("/h/.local/share"));
}
#[test]
fn register_writes_once_then_rewrites_a_moved_exe() {
    let dir = tempfile::tempdir().unwrap();
    let root = Root { home: dir.path().into(), xdg_data_home: None, appdata: None };
    let a = Path::new("/opt/a/corundum-demo");
    assert_eq!(status_at(&SPEC, &root, a), Status::Absent);
    assert_eq!(register_at(&SPEC, &root, a).unwrap(), Registered::Written);
    assert_eq!(register_at(&SPEC, &root, a).unwrap(), Registered::Unchanged);
    assert_eq!(status_at(&SPEC, &root, a), Status::Registered);
    let b = Path::new("/opt/b/corundum-demo");
    assert_eq!(status_at(&SPEC, &root, b), Status::Stale);
    assert_eq!(register_at(&SPEC, &root, b).unwrap(), Registered::Written);
    let apps = dir.path().join(".local/share/applications");
    assert_eq!(std::fs::read_dir(&apps).unwrap().count(), 1);
    assert!(dir.path().join(".local/share/icons/hicolor/scalable/apps/corundum-demo.svg").is_file());
    unregister_at(&SPEC, &root).unwrap();
    assert_eq!(status_at(&SPEC, &root, b), Status::Absent);
    assert!(!dir.path().join(".local/share/icons/hicolor/scalable/apps/corundum-demo.svg").exists());
}
```

The escaping: inside the double quotes, `"`, `` ` ``, `$` and `\` are each preceded by `\`; then, as the value is a Desktop Entry string, every `\` is doubled. The expected literal above is that result.

- [ ] **Step 2: Run** `cargo test -p corundum-framework-platform launcher::linux` — FAIL.
- [ ] **Step 3: Implement.** After writing, run `update-desktop-database <apps dir>` through `std::process::Command` only when the entry was written, ignoring every error (absent tool included). `Stale` = the entry exists and its `Exec=` line differs from `quote_exec(exe)`.
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(corundum-framework-platform): the Linux desktop entry`.

---

### Task 4: Icon rendering

**Files:**
- Create: `crates/corundum-framework-platform/src/icon.rs`
- Modify: `crates/corundum-framework-platform/Cargo.toml` — `resvg = { version = "0.45", default-features = false, features = ["text"] }` under `[target.'cfg(not(any(target_os = "ios", target_os = "android")))'.dependencies]` (match the version already in `Cargo.lock`); `lib.rs` gets `#[cfg(not(any(target_os = "ios", target_os = "android")))] pub mod icon;`

**Interfaces:**
- Produces:
  - `pub fn render_png(svg: &[u8], size: u32) -> io::Result<Vec<u8>>` (square, transparent background, `InvalidData` for an unparsable SVG)
  - `pub fn render_rgba(svg: &[u8], size: u32) -> io::Result<Vec<u8>>` (for the window icon, Task 8)
  - `pub fn icns(svg: &[u8]) -> io::Result<Vec<u8>>` — PNG entries `icp4` 16, `icp5` 32, `icp6` 64, `ic07` 128, `ic08` 256, `ic09` 512, `ic10` 1024
  - `pub fn ico(svg: &[u8]) -> io::Result<Vec<u8>>` — PNG entries 16, 24, 32, 48, 64, 256 (width/height byte 0 for 256)

- [ ] **Step 1: Write the failing tests**

```rust
const SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="#1f4fbf"/></svg>"#;

#[test] fn a_png_has_the_asked_size() { /* decode header: width/height at bytes 16..24 == 48 */ }
#[test] fn an_unparsable_svg_is_invalid_data() {
    assert_eq!(render_png(b"<nope", 16).unwrap_err().kind(), io::ErrorKind::InvalidData);
}
#[test] fn the_icns_lists_every_size() {
    let f = icns(SVG).unwrap();
    assert_eq!(&f[0..4], b"icns");
    assert_eq!(u32::from_be_bytes(f[4..8].try_into().unwrap()) as usize, f.len());
    // walk the entries: types are exactly icp4 icp5 icp6 ic07 ic08 ic09 ic10, each a PNG
}
#[test] fn the_ico_lists_every_size() {
    let f = ico(SVG).unwrap();
    assert_eq!(&f[0..4], &[0, 0, 1, 0]);
    assert_eq!(u16::from_le_bytes([f[4], f[5]]), 6);
    // directory widths: 16 24 32 48 64 0
}
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-platform icon` — FAIL.
- [ ] **Step 3: Implement** with `resvg::usvg::Tree::from_data`, a `tiny_skia::Pixmap` scaled to fit, `pixmap.encode_png()`; the two containers are written by hand (headers per Apple's icns and Microsoft's ICONDIR layouts).
- [ ] **Step 4: Run** — PASS. Also `cargo deny check licenses` — PASS.
- [ ] **Step 5: Commit** `feat(corundum-framework-platform): render an app icon to PNG, icns and ico`.

---

### Task 5: macOS bundle

Skip this task if Task 1 answered "neither".

**Files:**
- Create: `crates/corundum-framework-platform/src/launcher/macos.rs`

**Interfaces:**
- Consumes: Task 2 (`Root`, `write_if_changed`), Task 4 (`icon::icns`).
- Produces: `pub(crate) fn info_plist(spec: &LauncherSpec) -> String`, `pub(crate) fn bundle_dir(spec, root) -> PathBuf` (`<home>/Applications/<name>.app`), `register`/`unregister`/`status` for macOS. The launcher kind is the one Task 1 kept (symlink to `exe`, or a 0755 script `exec "<exe>" "$@"` with the path quoted for `sh`).

- [ ] **Step 1: Write the failing tests** (they run on every OS: the functions are pure or work under a temporary `Root`; gate the module with `#[cfg(any(target_os = "macos", test))]`)
  - `info_plist(&SPEC)` equals the expected XML: `CFBundleIdentifier` `net.fireturtle.corundum.corundum-demo`, `CFBundleName` `Corundum Demo`, `CFBundleExecutable` `corundum-demo`, `CFBundleIconFile` `corundum-demo`, `CFBundlePackageType` `APPL`, `LSMinimumSystemVersion` `11.0`, `NSHighResolutionCapable` `<true/>`; a name with `&` or `<` is XML-escaped.
  - `register_at` writes `Contents/Info.plist`, `Contents/MacOS/corundum-demo`, `Contents/Resources/corundum-demo.icns`; second call `Unchanged`; moved exe `Stale` then `Written`; `unregister_at` removes the `.app` directory.
  - An unparsable `icon_svg`: `register_at` errors and the `.app` directory does not exist afterwards (the icon is rendered before anything is written).
- [ ] **Step 2: Run** `cargo test -p corundum-framework-platform launcher::macos` — FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `feat(corundum-framework-platform): the macOS app bundle`.

---

### Task 6: Windows Start-menu shortcut

**Files:**
- Create: `crates/corundum-framework-platform/src/launcher/windows.rs`
- Modify: `crates/corundum-framework-platform/Cargo.toml` — `windows = { version = "0.62", features = ["Win32_System_Com", "Win32_UI_Shell", "Win32_UI_Shell_PropertiesSystem", "Win32_System_Com_StructuredStorage", "Win32_Storage_EnhancedStorage", "Win32_Foundation"] }` under `[target.'cfg(windows)'.dependencies]`

**Interfaces:**
- Consumes: Task 2.
- Produces: `pub(crate) struct LinkFields { pub target: PathBuf, pub working_dir: PathBuf, pub icon: (PathBuf, i32), pub app_id: String, pub description: String }`, `pub(crate) fn link_fields(spec, exe) -> LinkFields`, `pub(crate) fn link_path(spec, root) -> Option<PathBuf>` (`None` without `appdata`); Windows `register`/`unregister`/`status` (COM `IShellLinkW`, `IPropertyStore` with `PKEY_AppUserModel_ID`, `IPersistFile::Save`; `status` loads the link and compares its target).

- [ ] **Step 1: Write the failing tests**
  - (all OSes) `link_fields(&SPEC, Path::new(r"C:\Users\u\.local\bin\corundum-demo.exe"))`: target that path, working dir its folder, icon `(that path, 0)`, app id `net.fireturtle.corundum.corundum-demo`.
  - (all OSes) `link_path` is `<appdata>\Microsoft\Windows\Start Menu\Programs\Corundum Demo.lnk`.
  - (`#[cfg(windows)]`) register under a temporary `appdata`: `Written`, then `Unchanged`, `status_at` `Registered`, `unregister_at` removes the file.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-platform launcher::windows` — FAIL.
- [ ] **Step 3: Implement.** `CoInitializeEx(COINIT_APARTMENTTHREADED)` per call, tolerating `RPC_E_CHANGED_MODE`. `Unchanged` when the existing link's target and app id already match.
- [ ] **Step 4: Run** — PASS here; the `cfg(windows)` test runs in CI's Windows job.
- [ ] **Step 5: Commit** `feat(corundum-framework-platform): the Windows Start-menu shortcut`.

---

### Task 7: `corundum-framework-build`

**Files:**
- Create: `crates/corundum-framework-build/{Cargo.toml,README.md,src/lib.rs}`
- Modify: `release-plz.toml` (a `[[package]]` in `version_group = "corundum"`, and the facade's `changelog_include`)

**Interfaces:**
- Consumes: Task 4's `icon::ico` (dependency on `corundum-framework-platform`).
- Produces: `pub fn embed_icon(svg_path: impl AsRef<Path>)` for `build.rs`: prints `cargo:rerun-if-changed=<svg_path>`; when `CARGO_CFG_TARGET_OS == "windows"`, writes `$OUT_DIR/app.ico` and compiles it with `winresource::WindowsResource::new().set_icon(..).compile()`; on a Windows target built from another host where that fails, prints `cargo:warning=` and returns. On other targets it does nothing more.

- [ ] **Step 1: Write the failing test**: `pub(crate) fn wants_icon(target_os: &str) -> bool` — `true` only for `"windows"`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-build` — FAIL.
- [ ] **Step 3: Implement** `wants_icon` and `embed_icon`. Cargo.toml: `winresource = "0.1"`, license `MIT OR Apache-2.0`, description "Build-time helpers for corundum apps: the app icon embedded in a Windows executable".
- [ ] **Step 4: Run** — PASS; `cargo deny check licenses` — PASS.
- [ ] **Step 5: Commit** `feat(corundum-framework-build): embed the app icon in a Windows executable`.

---

### Task 8: The window and Settings

**Files:**
- Modify: `crates/corundum-framework-gui/src/window/mod.rs`, `src/store.rs`
- Create: `crates/corundum-framework-gui/src/settings/launcher.rs`; export from `settings/mod.rs`
- Modify: `crates/corundum-framework/src/lib.rs` only if `platform::launcher` needs a doc mention (it is reachable as `corundum_framework::platform::launcher`).

**Interfaces:**
- Consumes: `launcher::{LauncherSpec, register, unregister, status, Status}`, `icon::render_rgba`.
- Produces:
  - `WindowOptions::launcher(self, spec: LauncherSpec) -> WindowOptions`
  - `Stored.launcher: Option<bool>` (`None` = never set = registered; serde default)
  - `pub(crate) fn wants_launcher(stored: &Stored) -> bool` (`stored.launcher != Some(false)`)
  - `settings::LauncherRow::new(spec: LauncherSpec)` with `fn ui(&mut self, ui: &mut egui::Ui)`: a label "In the app menu" (`corundum-launcher-in-menu`), and **Remove** (`corundum-launcher-remove`) when `status` is `Registered`/`Stale`, **Add back** (`corundum-launcher-add`) when `Absent`; nothing when `Unsupported`. A click calls `unregister`/`register` and saves `launcher` through `store::update`; an error is shown under the row. Messages in `en` and `ja`.

- [ ] **Step 1: Write the failing tests**
  - `store`: `toml::from_str::<Stored>("")` → `wants_launcher` true; `launcher = false` → false. (A file written before this change has no key.)
  - `window`: `WindowOptions::new("x").launcher(SPEC)` keeps the spec (a `pub(crate)` getter).
  - `settings::launcher`: rendering the row (`test_support::render`) with a `status` of `Absent` shows the Add-back button — drive `LauncherRow` through a `pub(crate)` constructor taking a `status` function, so the test does not touch the host.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --features serde` — FAIL.
- [ ] **Step 3: Implement.** In `run`, before `open`: when a spec is set and (no store, or `wants_launcher(&store.load().unwrap_or_default())`), call `register` and `tracing::warn!` on `Err`; set `viewport.with_app_id(spec.id)` and, when `render_rgba(spec.icon_svg, 256)` succeeds, `with_icon(egui::IconData { rgba, width: 256, height: 256 })`.
- [ ] **Step 4: Run** — PASS; `cargo clippy --workspace --all-features` clean; `cargo test --workspace` PASS.
- [ ] **Step 5: Commit** `feat(corundum-framework-gui): register the launcher entry and match the window to it`.

---

### Task 9: The demo, and the docs

**Files:**
- Create: `demo/assets/app-icon.svg` — a 24-unit rounded tile (`rx="5.5"`, fill `#1d2433`) with `demo/assets/corundum.svg`'s two paths scaled into it (stroke `#ffffff`, width 1.75, inset 3 units).
- Create: `demo/build.rs` calling `corundum_framework_build::embed_icon("assets/app-icon.svg")`; `demo/Cargo.toml` build-dependency by path.
- Modify: `demo/src/lib.rs` (`WindowOptions::launcher` with id `corundum-demo`, name `Corundum Demo`, category `Development`, prefix `net.fireturtle.corundum`), `demo/src/main.rs` (`#![cfg_attr(windows, windows_subsystem = "windows")]`), demo's Settings About page gets a `LauncherRow`.
- Modify: `README.md` / `README.ja.md` (platform crate: the launcher; gui: `WindowOptions::launcher`; a note that an app sets `windows_subsystem`), `demo/README.md` (the entry appears after `cargo install --path demo`, not after `cargo run`, per the skip rule).

- [ ] **Step 1: Run** `cargo build -p corundum-framework-demo` and `cargo test --workspace` — PASS.
- [ ] **Step 2: Manual check** (Linux here): `cargo install --path demo --root /tmp/cdemo`, run `/tmp/cdemo/bin/corundum-framework-demo` once, confirm `~/.local/share/applications/corundum-demo.desktop` and that the app menu shows "Corundum Demo" with the logo; Settings → About → Remove deletes it and a restart does not bring it back; Add back restores it. Then remove it. Record the result in the PR description; list the macOS and Windows checks still to be done by hand there.
- [ ] **Step 3: Commit** `feat(demo): put the demo in the app menu`.
