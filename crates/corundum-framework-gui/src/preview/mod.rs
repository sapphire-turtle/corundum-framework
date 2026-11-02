//! Previews of files the app picked but has no use of its own for, for the detail pane: why
//! they exist is in docs/file-previews.md.
//!
//! [`ImagePreview`] (the `image-preview` feature) draws a picture, decoded on a worker
//! thread.

mod image;

pub use image::ImagePreview;

#[cfg(test)]
mod tests;
