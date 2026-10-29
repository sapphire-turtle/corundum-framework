<img src="docs/logo.svg" alt="" width="72" height="72">

# corundum-framework

> Language: **English** | [日本語](README.ja.md)

The application framework shared by the sapphire apps and asteria: what an
[egui](https://github.com/emilk/egui) app is made of before any of its own screens, on the
desktops and on phones.

## Crates

| Crate | What it is | Status |
|---|---|---|
| [`corundum-framework-gui`](crates/corundum-framework-gui) | The look: the shell, theme, fonts, icons, translations and window | unreleased |
| [`corundum-framework-platform`](crates/corundum-framework-platform) | Without egui: the app's directories (mobile included), the system's languages, the JNI / Objective-C bridging | unreleased |
| [`corundum-framework-update`](crates/corundum-framework-update) | Newer GitHub releases of an app's binary: finding them, and replacing the binary with one | unreleased |
| [`corundum-framework-package`](crates/corundum-framework-package) | The app as a Velopack package on the desktops: its start, and the install and uninstall hooks | unreleased |
| [`corundum-framework-build`](crates/corundum-framework-build) | For an app's `build.rs`: the app icon embedded in a Windows executable | unreleased |
| [`corundum-framework`](crates/corundum-framework) | The facade over the crates above | unreleased |
| `corundum-framework-demo` | A demo app showing every component | planned |

### corundum-framework-gui

- **Shell**: the app's screens on a rail (a bottom bar when the window is narrow, as on a
  phone), with a footer for Settings and a slot of the app's own; wide, a status bar along
  the bottom, the app's from the left (a log line, the services' state, the track
  playing), its slot and the detail pane's toggle at the right end. Screens and actions
  mix on the rail in any order (a "new entry" action first, say). When some are left off
  it — no room, or hidden — a "More" button takes the footer's place, its menu holding
  the footer first, then those left off. A settings page (`settings::RailPage`) orders
  them and chooses which the rail shows.
- **Theme**: light and dark, three contrast levels and an accent colour, built in OKLCH;
  a settings widget and a tint picker (palette, hue/chroma disk, preview) to choose them.
- **Fonts**: finds a CJK fallback font on the system (CoreText on iOS, the system font
  directories elsewhere).
- **Icons**: a set of [Lucide](https://lucide.dev) icons and the SVG loader that draws them.
- **Translations**: every string looked up by id through [Fluent](https://projectfluent.org)
  in the user's language (`tr!`), with English and Japanese bundled and a language picker.
- **Settings**: the Settings screen, a list of pages beside the open one, into which the app
  and the frameworks under it put their pages; tabs for the scopes a setting can be kept
  in (a device and its workgroup, a client and its server); and an Appearance page, the
  theme and the language over those tabs. The pages store nothing: the app keeps the
  values.
- **Look**: the theme and the language in effect, kept with the rail's order in
  `corundum.toml` in the app's config directory (`look` and `store`, with the `serde`
  feature) and installed before the first frame, so the app opens in them. An app that merges settings from elsewhere (a workgroup, a
  server) applies the result there; an app with nothing else to keep them in stores them
  there alone, as the demo does.
- **Detail pane**: the side pane every shell has (`Shell::show`, filled by the app's
  `ShellApp::side_ui`), beside every screen: what the main
  view's item holds (the playlist playing, the journal open) or a new entry begun from the
  rail's action, in a right column that a button on the status bar hides, on a card over
  the middle of the screen when the column is hidden, or on a phone in its place.
- **Platform**: what a window asks of the system and egui cannot give: the system's dialogs
  to pick files (`platform::FilePicker`, with filters such as `IMAGE_EXTENSIONS`) and
  folders (`pick_folder`), the file manager (`open_in_file_manager`), and on Android the
  clipboard and the insets. The dialogs are the desktops' for now: on a phone
  `file_picker_available` says there are none.
- **Window**: `window::run` opens a desktop window on wgpu the same way for every app;
  `window::run_android` does the same on an Android phone (GameActivity). On iOS the window
  is put in a scene, which iOS 27 requires (the app's Info.plist declares one with
  `CorundumSceneDelegate` as its delegate: see `window`), and takes the keyboards that
  compose (Japanese, Chinese, Korean), which winit 0.30 alone does not.
- **App menu entry**: `WindowOptions::launcher(spec)` puts the app in the desktop's app
  menu at start (and sets the window's app id and icon), and `settings::LauncherRow`, a row
  for an About page, removes it and adds it back; with a store the removal is kept, and the
  entry is not written again on the next start. A Windows app also sets
  `#![cfg_attr(windows, windows_subsystem = "windows")]` in its `main.rs`, or the console
  opens with it.

| Feature | What it enables | Default |
|---|---|---|
| `eframe` | The desktop window (`window::run`) and its wgpu setup (`gpu`) | yes |
| `android` | The Android window (`window::run_android`), with logcat and the platform calls | no |
| `serde` | `Serialize` / `Deserialize` on the theme and language settings | no |
| `update` | `update::UpdatesPanel`, the Settings → About rows that look for a newer release and say where it is (brings corundum-framework-update) | no |
| `self-update` | The panel also downloads the release and replaces the running binary (brings `update`). A store build leaves it off, as the store updates the app | no |

The components depend on egui alone. An app that renders egui without eframe turns
default features off; an Android app turns on `android` (eframe's default window does not
build there).

### corundum-framework-platform

What an app asks of the OS, without egui, so the crates without a window (a server, a
CLI) can use it too, and no app has to call JNI or Objective-C itself.

- **Directories**: `dirs::{config_dir, data_dir, cache_dir}` and an app's own `AppDirs`
  under them: the `dirs` crate's on the desktops, the sandbox's on iOS (`NSFileManager`)
  and Android (the context's files and cache directories). `AppDirs` is
  `<root>/<app name>` on the desktops and the sandbox's roots themselves on a phone.
- **Languages**: `locale::system_languages`, the system's preferred languages (Android's
  whole `LocaleList`, where `sys-locale` gives only one).
- **Launcher**: `launcher`, the app's entry in the desktop's app menu (a `.desktop` file
  and icon on Linux, an app bundle on macOS, a Start menu shortcut on Windows), written
  from a `LauncherSpec`, and the process's Windows taskbar id. Skipped for a build that
  runs from a cargo target directory.
- **Window support**: the clipboard and the window insets on Android, which the gui
  crate's `platform` reads.
- **Android**: `android::with_env`, the process's JavaVM attached with Java exceptions
  caught, and the application context and activity, for an app's own JNI calls.

### corundum-framework

One dependency for the crates above, as sapphire-framework's facade is: each feature
re-exports a crate as a module (`corundum_framework::platform`, `::gui`), and the rest pass
through to the gui crate.

| Feature | What it enables | Default |
|---|---|---|
| `platform` | `platform`: corundum-framework-platform | yes |
| `gui` | `gui` and `egui`: corundum-framework-gui without eframe (brings `platform`) | no |
| `update` | `update`: corundum-framework-update | no |
| `self-update` | `update` with the download and the replacement; a store build leaves it off | no |
| `package` | `package`: corundum-framework-package (brings `platform`) | no |
| `gui-update` | The gui crate's `update` panel (brings `gui` and `update`) | no |
| `gui-self-update` | The gui crate's `self-update` (brings `gui-update` and `self-update`) | no |
| `gui-package` | The gui crate's `package`: the panel's row for the app's own package (brings `gui-update` and `package`) | no |
| `eframe` | `gui::window::run`, the desktop window (brings `gui`) | no |
| `android` | `gui::window::run_android`, the Android window (brings `gui`) | no |
| `serde` | The gui crate's `serde` | no |

The default leaves egui out, for a server or a CLI; a desktop app turns on `eframe`, an
Android app `android`.

## Demo

[`demo/`](demo/README.md) is an app that shows every shared piece — the shell, the theme,
the fonts, the icons, the translations and the platform calls — to try them on each OS
apart from the apps: `cargo run -p corundum-framework-demo` on a desktop, and an Xcode and
a Gradle project for the iOS simulator and the Android emulator. It is not published.

## Status

Unreleased. The first release, 0.1.0, waits for the demo app and a manual check on each
desktop and phone OS, so there is no release workflow yet. Until then, use it as a git
dependency:

```toml
[dependencies]
corundum-framework-gui = { git = "https://github.com/sapphire-turtle/corundum-framework" }
```

Use the egui it re-exports (`corundum_framework_gui::egui`), so your app and the components
share one.

The gui crate used to live in
[sapphire-framework](https://github.com/sapphire-turtle/sapphire-framework) as
`crates/corundum-gui`; its history came along. Issue and pull request numbers in that
history refer to sapphire-framework.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. The bundled Lucide icons are under the ISC
license, those derived from Feather under the MIT license
([`crates/corundum-framework-gui/assets/icons/LICENSE-Lucide`](crates/corundum-framework-gui/assets/icons/LICENSE-Lucide)).
