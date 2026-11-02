//! Decode an image to RGBA8 pixels with the EXIF orientation applied, without a GUI.
//!
//! [`decode`] reads a [`Source`] (a file or bytes), checks it against [`Limits`] from its
//! header, and returns an [`Image`]. No egui is involved, so a server can use it too.

mod decode;
mod error;

pub use decode::{Image, Limits, Source, decode};
pub use error::Error;
