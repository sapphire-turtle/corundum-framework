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
| [`corundum-framework`](crates/corundum-framework) | The facade over the crates above | unreleased |
| `corundum-framework-demo` | A demo app showing every component | planned |

### corundum-framework-gui

- **Shell**: the app's screens on a rail (a bottom bar when the window is narrow, as on a
  phone), with a footer for Settings and a slot of the app's own.
- **Theme**: light and dark, three contrast levels and an accent colour, built in OKLCH;
  a settings widget and a tint picker (palette, hue/chroma disk, preview) to choose them.
- **Fonts**: finds a CJK fallback font on the system (CoreText on iOS, the system font
  directories elsewhere).
- **Icons**: a set of [Lucide](https://lucide.dev) icons and the SVG loader that draws them.
- **Translations**: every string looked up by id through [Fluent](https://projectfluent.org)
  in the user's language (`tr!`), with English and Japanese bundled and a language picker.
- **Window**: `window::run` opens a desktop window on wgpu the same way for every app;
  `window::run_android` does the same on an Android phone (GameActivity).

| Feature | What it enables | Default |
|---|---|---|
| `eframe` | The desktop window (`window::run`) and its wgpu setup (`gpu`) | yes |
| `android` | The Android window (`window::run_android`), with logcat and the platform calls | no |
| `serde` | `Serialize` / `Deserialize` on the theme and language settings | no |

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
