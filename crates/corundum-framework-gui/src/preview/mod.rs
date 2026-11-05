//! Previews of files the app picked but has no use of its own for, for the detail pane: why
//! they exist is in docs/file-previews.md.
//!
//! What a preview draws is a [`Content`] the app hands in, as bytes or as a reader opener,
//! and [`State`] says where the app is with it. A preview reports what the person asked
//! for in a [`PreviewResponse`]. [`ImagePreview`] (the `image-preview` feature) draws a
//! picture, decoded on a worker thread. [`AudioPreview`] (the `audio-preview` feature)
//! shows a sound file's tags, read on a worker thread, with a play button and a seek bar
//! over the app's corundum-framework-audio `Player`.

#[cfg(feature = "audio-preview")]
mod audio;
mod content;
#[cfg(feature = "image-preview")]
mod image;

#[cfg(feature = "audio-preview")]
pub use audio::AudioPreview;
pub use content::{Content, Data, PreviewResponse, Reader, State};
#[cfg(feature = "image-preview")]
pub use image::ImagePreview;

#[cfg(all(test, feature = "image-preview"))]
mod tests;
#[cfg(all(test, feature = "audio-preview"))]
mod tests_audio;
#[cfg(test)]
mod tests_content;
