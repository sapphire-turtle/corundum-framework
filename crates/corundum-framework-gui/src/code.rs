//! A code editor for egui that keeps the indentation: [alumina-code], re-exported so an app
//! depends on corundum alone.
//!
//! The editor's colours follow the visuals, so corundum's theme applies to it. It knows no
//! language: the app brings its own [`Highlighter`] and names the language to it.
//!
//! [alumina-code]: https://crates.io/crates/alumina-code

pub use alumina_code::*;
