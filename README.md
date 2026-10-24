# corundum-framework

> Language: **English** | [日本語](README.ja.md)

The application framework shared by the sapphire apps and asteria: what an
[egui](https://github.com/emilk/egui) app is made of before any of its own screens, on the
desktops and on phones.

## Crates

| Crate | What it is | Status |
|---|---|---|
| [`corundum-framework-gui`](crates/corundum-framework-gui) | The look: the shell, theme, fonts, icons, translations and window | unreleased |
| `corundum-framework-platform` | Without egui: the app's directories (mobile included), the system's languages, the JNI / Objective-C bridging | planned |
| `corundum-framework` | The facade over the crates above | planned |
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
