//! `corundum-framework` — a single-dependency facade over the crates the sapphire apps and
//! asteria share.
//!
//! Depend on this one crate and turn on the features you need; each feature re-exports a
//! `corundum-framework-*` crate as a module, the same way sapphire-framework's facade does.
//! The default is the part without egui, for the crates without a window (a server, a CLI);
//! an app turns on `gui`.
//!
//! ```toml
//! # A desktop app:
//! corundum-framework = { version = "0.1", features = ["eframe", "serde"] }
//! # The same app on an Android phone:
//! corundum-framework = { version = "0.1", features = ["android", "serde"] }
//! ```
//!
//! ## Modules (feature-gated)
//!
//! | feature | module | crate |
//! |---|---|---|
//! | `platform` (default) | [`platform`] | `corundum-framework-platform`: the app's directories, the system's languages, the JNI / Objective-C bridging |
//! | `gui` | [`gui`] | `corundum-framework-gui`: the shell, theme, fonts, icons and translations (brings `platform`) |
//! | `update` | [`update`] | `corundum-framework-update`: finding newer GitHub releases of an app's binary, and replacing it with one |
//! | `self-update` | `update` | `update` with the download and the replacement; a store build leaves it off, as the store updates the app |
//! | `gui-update` | `gui::update` | the gui crate's `update` panel: the Settings → About rows that look for a newer release (brings `gui` and `update`) |
//! | `gui-self-update` | `gui::update` | the panel also downloads the release and replaces the running binary (brings `gui-update` and `self-update`) |
//! | `eframe` | `gui::window::run`, [`eframe`] | `corundum-framework-gui` with its `eframe` feature: a desktop window started the same way for every app |
//! | `android` | `gui::window::run_android`, [`eframe`] | `corundum-framework-gui` with its `android` feature: the window on an Android phone (brings `eframe`) |
//! | `media-session` | `platform::media_session`, `gui::media_session` | the OS's media keys and its "now playing", for a music player; `gui::media_session::start` makes one for an eframe window (brings `platform`). An iOS app adds `UIBackgroundModes` `audio` to its Info.plist |
//! | `serde` | `gui::look`, `gui::store` | `Serialize` / `Deserialize` on the gui's theme and language settings, and `store`: the look in effect and the rail's order, kept on the device in a TOML file and put back before the first frame |

#[cfg(feature = "platform")]
pub use corundum_framework_platform as platform;

#[cfg(feature = "gui")]
pub use corundum_framework_gui as gui;

#[cfg(feature = "update")]
pub use corundum_framework_update as update;

// The egui the components are built against, at the top as well: an app that depends on
// this facade alone renders with this one.
#[cfg(feature = "gui")]
pub use corundum_framework_gui::egui;

// The eframe the window runs on, for the app's `eframe::App`; `android` brings `eframe`.
#[cfg(feature = "eframe")]
pub use corundum_framework_gui::eframe;
