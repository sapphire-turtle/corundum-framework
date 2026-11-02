//! Previews of files the app picked but has no use of its own for, for the detail pane: why
//! they exist is in docs/file-previews.md.
//!
//! [`ImagePreview`] (the `image-preview` feature) draws a picture, decoded on a worker
//! thread. [`AudioPreview`] (the `audio-preview` feature) shows a sound file's tags, read on
//! a worker thread, with a play button and a seek bar over the app's
//! corundum-framework-audio `Player`.

#[cfg(feature = "audio-preview")]
mod audio;
#[cfg(feature = "image-preview")]
mod image;

#[cfg(feature = "audio-preview")]
pub use audio::AudioPreview;
#[cfg(feature = "image-preview")]
pub use image::ImagePreview;

#[cfg(all(test, feature = "image-preview"))]
mod tests;
#[cfg(all(test, feature = "audio-preview"))]
mod tests_audio;
