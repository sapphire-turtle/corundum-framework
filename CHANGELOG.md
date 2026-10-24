# Changelog

All notable changes to the corundum-framework crates are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


## [Unreleased]

`corundum-framework-gui` moved out of sapphire-framework, where it was `corundum-gui`
([`crates/corundum-gui`](https://github.com/sapphire-turtle/sapphire-framework/tree/e716455a49daeaf9ab25e666f6cb99cf5e858f02/crates/corundum-gui)),
into this repository, with its history kept. Issue and pull request numbers in that
history refer to sapphire-framework. Nothing is published yet; 0.1.0 will be the first
release.

### Changed

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

### Added

- `corundum-framework` (#10), the facade over the crates: `platform` (default) and `gui`
  re-export them as modules; `eframe`, `android` and `serde` pass through to the gui crate.
  The default leaves egui out, as sapphire-framework's facade leaves out its gui.
- `corundum-framework-platform` (#9), a crate without egui for what an app asks of the OS:
  `dirs` (config, data and cache roots, the `dirs` crate's on the desktops and the
  sandbox's on iOS and Android, and an app's own `AppDirs` under them), `locale`
  (`system_languages`, moved from the gui crate), `clipboard` and `insets` (Android, moved
  from the gui crate's `platform`), and `android` (`with_env`, the application context and
  the window's activity, for an app's own JNI calls).

- `icons`: Lucide icons for media apps such as asteria (#6): `picture` (Lucide's `image`),
  `images`, `music`, `list_music`, `smartphone`, `monitor_smartphone`, `play`, `pause`,
  `skip_back`, `skip_forward`, `volume_2`, `volume_x`, `star`, `tag`, `search`,
  `clipboard_copy`, `upload`, `download` and `cloud_off`.
- `icons`: icons in theme colours (#12). `image_as` draws in a `Tone` of a `Swatch`,
  `image_in` and `paint` in a given colour, and `button` makes an icon button whose icon
  takes the colour of its state: stronger when hovered, the accent when selected, faded
  when disabled. `Shell`'s rail and bottom bar use it, so the open screen's icon is in the
  accent.

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
