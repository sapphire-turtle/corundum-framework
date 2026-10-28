//! The look shared by the sapphire apps and asteria: what an egui app's window is made of
//! before any of its own screens.
//!
//! [`Shell`] is the window frame: the app's screens on a rail (a bottom bar when narrow, as
//! on a phone), with a footer for Settings and a slot of the app's own. [`fonts`] finds a
//! CJK fallback font on the system; [`icons`] is a set of Lucide icons and the SVG loader
//! that draws them; [`theme`] is the colours — light and dark, the contrast, the accent;
//! [`i18n`] is the translations — every string looked up by id in the user's language,
//! through [`tr!`]. [`settings`] is the Settings screen: the pages an app and the
//! frameworks under it put in, the tabs of the scopes a setting can be kept in, and the
//! theme and the language over them. [`detail`] is the app's side pane: a right column
//! beside every screen, a card over it, or on a phone the screen in its place; its column's
//! toggle sits at the right end of the shell's status bar. [`explorer`] is a folder's
//! contents the app hands in, to browse, pick, move and rename. With the `serde` feature,
//! [`store`] is corundum's file on the device, `corundum.toml`: what the person left the UI
//! in — [`look`], the theme and the language in effect, and the order of the shell's rail —
//! put back before the app's first frame.
//!
//! The components depend on egui alone, so they render inside any host's egui context, on
//! the desktops and on phones. With the `eframe` feature (on by default), [`window::run`]
//! opens a desktop window on wgpu the same way for every app, set up by [`gpu`]; an app that
//! renders egui without eframe turns default features off.
//!
//! With the `media-session` feature, `media_session` puts a music player under the OS's
//! media keys and in its "now playing", and `media_session::start` makes one for an eframe
//! window.

/// The egui this crate renders with. Apps should use this re-export so their egui is
/// always the one the components were built against.
pub use egui;

/// The eframe [`window::run`] opens windows with, for the same reason as [`egui`].
#[cfg(feature = "eframe")]
pub use eframe;

pub mod detail;
pub mod explorer;
pub mod fonts;
#[cfg(feature = "eframe")]
pub mod gpu;
pub mod i18n;
pub mod icons;
pub mod logcat;
#[cfg(feature = "serde")]
pub mod look;
#[cfg(feature = "media-session")]
pub mod media_session;
pub mod platform;
pub mod settings;
mod shell;
#[cfg(feature = "serde")]
pub mod store;
pub mod theme;
#[cfg(feature = "update")]
pub mod update;
#[cfg(feature = "eframe")]
pub mod window;

#[cfg(test)]
mod test_support;

pub use shell::{Action, COMPACT_WIDTH, Destination, RailItem, RailOrder, Shell, ShellApp, Side};
