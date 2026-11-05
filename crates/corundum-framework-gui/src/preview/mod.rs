//! Previews of files the app picked but has no use of its own for, for the detail pane: why
//! they exist is in docs/file-previews.md.
//!
//! The app fetches and the parts draw. What a preview draws is a [`Content`] the app hands
//! in, as bytes or as a reader opener, and [`State`] says where the app is with it. A
//! preview reports what the person asked for in a [`PreviewResponse`]: the app refetches on
//! `retry` and opens the file on `open`; corundum does neither itself. The parts are:
//!
//! - [`FilePreview`]: the default for one file. By its kind it draws a picture or a player
//!   with the file's details under it, or just the info card. [`FilePreview::wants_content`]
//!   tells the app whether to fetch the content at all.
//! - [`ImagePreview`] (the `image-preview` feature): draws a picture, decoded on a worker
//!   thread.
//! - [`AudioPreview`] (the `audio-preview` feature): shows a sound file's tags, read on a
//!   worker thread, with a play button and a seek bar over the app's
//!   corundum-framework-audio `Player`.
//! - [`TextPreview`] (the `text-preview` feature): shows a text file read-only, as UTF-8 or
//!   UTF-16 with a BOM, decoded on a worker thread; [`FilePreview`] uses it for the text extensions.
//! - [`InfoCard`]: says what is known of a file ([`FileInfo`]) that no preview draws.
//! - [`kind_of`]: tells what a file is by its name.
//!
//! There are three ways to compose them.
//!
//! **Default**: the app hands the whole pane to [`FilePreview`].
//!
//! ```no_run
//! use corundum_framework_gui::preview::{FileInfo, FilePreview, State};
//!
//! fn pane(ui: &mut egui::Ui, preview: &mut FilePreview, file: &FileInfo) {
//!     // `wants_content` says whether the app should fetch; here nothing is fetched yet.
//!     if preview.wants_content(&file.name) {
//!         // ... start fetching the content, and show `State::Loading` until it comes.
//!     }
//!     let response = preview.ui(ui, file, State::Loading, true);
//!     if response.retry || response.open {
//!         // ... fetch again, or open the file with the system's app.
//!     }
//! }
//! ```
//!
//! **Replace**: the app draws its own view of the file, and puts the [`InfoCard`] footer
//! under it.
//!
//! ```no_run
//! use corundum_framework_gui::preview::{FileInfo, InfoCard};
//!
//! fn pane(ui: &mut egui::Ui, file: &FileInfo) {
//!     ui.label("the app's own view of the file");
//!     let _ = InfoCard::new(file).footer().ui(ui);
//! }
//! ```
//!
//! **Add**: the app draws its own section, and [`FilePreview`] draws the default under it.
//!
//! ```no_run
//! use corundum_framework_gui::preview::{FileInfo, FilePreview, State};
//!
//! fn pane(ui: &mut egui::Ui, preview: &mut FilePreview, file: &FileInfo) {
//!     ui.label("the app's own section: tags, say");
//!     let _ = preview.ui(ui, file, State::Loading, false);
//! }
//! ```

#[cfg(feature = "audio-preview")]
mod audio;
mod content;
mod file;
#[cfg(feature = "image-preview")]
mod image;
mod info;
mod kind;
#[cfg(feature = "text-preview")]
mod text;

#[cfg(feature = "audio-preview")]
pub use audio::{AudioPreview, ContentOpener};
pub use content::{Content, Data, PreviewResponse, Reader, State};
pub use file::FilePreview;
#[cfg(feature = "image-preview")]
pub use image::ImagePreview;
pub use info::{FileInfo, InfoCard};
pub use kind::{Kind, kind_of};
#[cfg(feature = "text-preview")]
pub use text::TextPreview;

#[cfg(all(test, feature = "image-preview"))]
mod tests;
#[cfg(all(test, feature = "audio-preview"))]
mod tests_audio;
#[cfg(test)]
mod tests_content;
#[cfg(test)]
mod tests_file;
#[cfg(all(test, feature = "text-preview"))]
mod tests_text;
