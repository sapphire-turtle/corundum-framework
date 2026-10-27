# Changelog

All notable changes to the corundum-framework crates are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [Unreleased]

## [0.3.0](https://github.com/sapphire-turtle/corundum-framework/compare/v0.2.1...v0.3.0) - 2026-10-27

### Added

- *(demo)* put the demo in the app menu
- *(corundum-framework-build)* embed the app icon in a Windows executable
- *(demo)* the updates panel on About
- *(corundum-framework-gui)* the update panel's row states
- *(corundum-framework-update)* move the release lookup and self-update from sapphire-framework
- *(corundum-framework-gui)* pick files and folders, and show them in the file manager
- *(corundum-framework-gui)* [**breaking**] every shell has the side pane; draw it with `show`
- *(corundum-framework-platform)* the macOS app bundle
- *(corundum-framework-platform)* the Windows Start-menu shortcut
- *(corundum-framework-platform)* render an app icon to PNG, icns and ico
- *(corundum-framework-platform)* the Linux desktop entry
- *(corundum-framework-platform)* the launcher module's spec, results and skip rules
- *(corundum-framework-gui)* register the launcher entry and match the window to it
- *(corundum-framework-gui)* the updates panel
- *(corundum-framework-update)* the advice's command on its own
- *(corundum-framework-update)* recognise an app installed from a store
- *(corundum-framework-update)* replace another program's executable
- *(corundum-framework-gui)* the side pane in the shell, and ShellApp
- *(corundum-framework-gui)* one side pane for the app, beside every screen
- *(corundum-framework-gui)* mix actions with screens, More in the footer's place, order in Settings
- *(corundum-framework-gui)* put what the rail cannot hold in a menu, and keep its order
- *(corundum-framework-gui)* keep the look in effect on the device
- *(corundum-framework-gui)* a detail pane, and a status bar to toggle it from
- *(corundum-framework-gui)* the Settings screen, moved from sapphire-framework-gui
- add the demo app corundum-framework-demo
- add the corundum-framework facade crate
- *(corundum-framework-platform)* a platform crate without egui

### Fixed

- *(corundum-framework-platform)* launcher final-review fixes
- *(corundum-framework-platform)* leave the macOS launcher out of iOS and Android test builds
- *(corundum-framework-platform)* follow the bundle's link to the real executable on macOS
- *(corundum-framework-platform)* report no app menu where there is none
- *(corundum-framework-platform)* store the shortcut's app id as a wide string
- *(corundum-framework-gui)* track running versions per row and resolve symlinked executables
- *(demo)* the updates panel on desktop only
- *(corundum-framework-gui)* keep an installing row from a late check or a stopped worker
- *(corundum-framework-update)* tighten replace_file tests
- *(corundum-framework-gui)* put the iOS window in a scene and take composing keyboards
- *(corundum-framework-platform)* use the sandbox's roots as a phone app's own dirs

### Other

- Merge pull request #48 from sapphire-turtle/docs/logo
- add corundum's logo and show it in the README
- correct the update feature notes and the worker-stopped wording
- *(corundum-framework-platform)* say when status is Unsupported
- *(corundum-framework-gui)* pin the row states' edge cases
- *(corundum-framework-update)* keep install-method tests off the host's store markers
- *(corundum-framework-update)* check the replaced file's mode exactly
- *(corundum-framework-gui)* name the file corundum.toml, and the store Store
- stand alone as sapphire-turtle/corundum-framework

## [0.2.1](https://github.com/sapphire-turtle/corundum-framework/compare/v0.2.0...v0.2.1) - 2026-10-25

### Fixed

- *(corundum-framework-gui)* put the iOS window in a scene and take composing keyboards

## [0.2.0](https://github.com/sapphire-turtle/corundum-framework/compare/v0.1.0...v0.2.0) - 2026-10-25

### Breaking

- *(corundum-framework-gui)* `DetailPane::ui`'s main view draws as the root — its own
  panels, then a `CentralPanel` — like the shell's content; a `main` closure that drew
  straight into the pane's `ui` now draws its own `CentralPanel`.
- *(corundum-framework-gui)* the shell's bottom bar keeps full-size buttons instead of
  narrowing them, and with the rail puts what does not fit into a More menu that takes the
  footer's place (Settings goes into the menu first).
- *(corundum-framework-gui)* the `serde` feature brings `toml` instead of `serde_json`.

### Added

- *(corundum-framework-gui)* the side pane in the shell, and `ShellApp`: `Shell::show`,
  `with_side_pane`, `with_status_bar`, `Side`
- *(corundum-framework-gui)* mix actions with screens on the rail (`Shell::from_items`,
  `RailItem`), the More menu, and the rail's order on a settings page
  (`settings::RailPage`, `RailOrder`)
- *(corundum-framework-gui)* keep the look in effect, the rail's order and the side column on
  the device in `corundum.toml` (`store`, `look`, with the `serde` feature)

## [0.1.0](https://github.com/sapphire-turtle/corundum-framework/releases/tag/v0.1.0) - 2026-10-25

The first release. `corundum-framework-gui` moved out of sapphire-framework, where it was
`corundum-gui`
([`crates/corundum-gui`](https://github.com/sapphire-turtle/sapphire-framework/tree/e716455a49daeaf9ab25e666f6cb99cf5e858f02/crates/corundum-gui)),
into this repository, with its history kept. Issue and pull request numbers in that
history refer to sapphire-framework; those below are this repository's.

### Added

`corundum-framework-gui`, as it came from sapphire-framework:

- `Shell`: the app's screens on a rail, a bottom bar when narrower than `COMPACT_WIDTH`,
  with a Settings footer and a slot of the app's own. Labels are message ids, translated
  when drawn.
- `theme`: OKLCH colours and WCAG contrast; light and dark, three contrast levels and an
  accent `Tint` (a palette colour, random or hashed from a name), turned into egui visuals
  and installed in the egui context. A theme settings widget and `TintPicker` (palette,
  hue/chroma disk, preview).
- `fonts`: the system CJK fallback font, found through CoreText on iOS and in the system
  font directories elsewhere, Android included.
- `icons`: Lucide icons and the SVG loader for them.
- `i18n`: Fluent translations looked up by id in the egui context (`tr!`), English and
  Japanese bundled, the system's languages read (through `LocaleList` on Android), a
  language picker, and `i18n::check` for tests: the translation files parse, every
  language has the same keys, and every id the code uses exists.
- `window::run` (feature `eframe`, default): the desktop window on wgpu, set up by `gpu`.
- `window::run_android` (feature `android`): the window on Android (GameActivity), with
  logcat, the clipboard and the window insets.
- Feature `serde`: `Serialize` / `Deserialize` on the theme and language settings.

`corundum-framework-gui`, new here:

- `icons`: Lucide icons for media apps such as asteria (#6): `picture` (Lucide's `image`),
  `images`, `music`, `list_music`, `smartphone`, `monitor_smartphone`, `play`, `pause`,
  `skip_back`, `skip_forward`, `volume_2`, `volume_x`, `star`, `tag`, `search`,
  `clipboard_copy`, `upload`, `download`, `cloud_off` and `panel_right`.
- `icons`: icons in theme colours (#12). `image_as` draws in a `Tone` of a `Swatch`,
  `image_in` and `paint` in a given colour, and `button` makes an icon button whose icon
  takes the colour of its state: stronger when hovered, the accent when selected, faded
  when disabled. `Shell`'s rail and bottom bar use it, so the open screen's icon is in the
  accent.
- `settings` (#27): the Settings screen, moved from sapphire-framework-gui without its
  storage. `Settings` holds the pages (`SettingsSection`) an app and the frameworks under
  it put in, by `SettingsGroup`, in a list beside the open page (narrow, one at a time
  under a way back); `SettingsLayout` is that layout alone, for a host that gives the list
  every frame. `scope_tabs` draws a tab per scope a setting can be kept in (`ScopeTab`,
  over the host's own scope type), and `AppearancePage` the theme and the language over
  them, giving back what changed (`AppearanceChange`) for the host to store.
- `detail::DetailPane` (#28): a main view and the detail of its item. Wide, the detail is
  in a right column (dragged wider or narrower) that `detail::toggle_ui` turns off and on;
  with it off, or no room for it, an opened detail is a card over the middle of the
  screen, closed by `×`, `Esc` or a click outside; on a phone it takes the main view's
  place under a way back.
- `Shell::ui_with_status` (#28): wide, a status bar along the window's bottom. The app
  fills it from the left (a log line, the services' state, the track playing); its right
  end holds the foot slot, moved there from the rail's foot, and the detail pane's toggle
  when the open screen has one. Narrow, there is no status bar and the foot stays on the
  bottom bar.

The other crates:

- `corundum-framework-platform` (#9), a crate without egui for what an app asks of the OS:
  `dirs` (config, data and cache roots, the `dirs` crate's on the desktops and the
  sandbox's on iOS and Android, and an app's own `AppDirs` under them: `<root>/<app
  name>` on the desktops, the sandbox's roots themselves on a phone (#26)), `locale`
  (`system_languages`, moved from the gui crate), `clipboard` and `insets` (Android, moved
  from the gui crate's `platform`), and `android` (`with_env`, the application context and
  the window's activity, for an app's own JNI calls).
- `corundum-framework` (#10), the facade over the crates: `platform` (default) and `gui`
  re-export them as modules; `eframe`, `android` and `serde` pass through to the gui crate.
  `eframe`, and `android` (which brings `eframe`), re-export eframe as
  `corundum_framework::eframe` (#22). The default leaves egui out, as sapphire-framework's
  facade leaves out its gui.
- `corundum-framework-demo` (#7, not published), in `demo/`: an app showing the shell, the
  theme, the fonts, the icons, the translations, the platform calls, the Settings screen
  and the detail pane, for the desktops (`cargo run -p corundum-framework-demo`), the iOS
  simulator (an XcodeGen project) and the Android emulator (a Gradle project,
  GameActivity). Mobile CI builds it, starts it on the simulator and the emulator, and
  uploads a screenshot.

### Changed

From `corundum-gui` in sapphire-framework:

- `corundum-framework-gui`: renamed from `corundum-gui` (lib `corundum_framework_gui`);
  its translations' bundle name is `corundum-framework-gui`. Message ids keep their
  `corundum-` prefix.
- `corundum-framework-gui`: the system's languages (`i18n::system_languages`, re-exported)
  and the Android clipboard and insets (`platform`) are read through
  `corundum-framework-platform`; the gui crate no longer depends on jni, ndk-context or
  sys-locale itself. `platform::clipboard_text` now reads the clipboard on Android without
  the `android` feature too.
- `corundum-framework-gui`: egui, eframe and egui_extras 0.37 (were 0.36). Apps must use
  egui 0.37 as well. On iOS, eframe is built without its `wayland` feature.

### Fixed

- `Shell`: a crowded bottom bar keeps the footer and the foot on the screen (#21). They
  keep the width they need at the bar's end; the screens and the actions share the rest,
  their buttons narrowing (a smaller icon, then less padding) when the cells do.
- `Shell`: keeps clear of egui's safe area (the status bar, the notch and the home
  indicator on iOS) (#5). The rail, the bar's buttons and the screens stay inside it; the
  bottom bar's fill runs on under the home indicator. A margin the app already keeps is not
  added again, so apps can drop their own.
- `window::run` on iOS: the picture is no longer stretched taller with its bottom (the
  Shell's bar) cut off (#20). eframe 0.37 made the wgpu surface the size of the safe area
  but laid egui out over the whole screen; the window now makes the surface the whole
  screen, and gives egui the safe area itself.
- `gpu::wgpu_options` asks for wgpu's default limits only as far as the adapter has them.
  The iOS simulator's GPU falls short of the defaults, and the window failed to open
  there.
- On Android, `gpu::wgpu_options` leaves out wgpu's debug labels in a debug build. The
  emulator's Vulkan driver crashed the app when wgpu named its objects.
- `window::run_android` reads the keyboard's text only while a field has focus.
  android-activity 0.6 slices that text from a null pointer until it is set (#19), and a
  debug build aborted on its first frame. Its docs also say to build android-activity
  without debug assertions in a debug build, as this workspace does, until #19 is fixed
  upstream.
