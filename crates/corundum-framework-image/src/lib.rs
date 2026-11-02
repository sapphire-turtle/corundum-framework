//! Decode an image to RGBA8 pixels with the EXIF orientation applied, without a GUI.
//!
//! [`decode`] reads a [`Source`] (a file or bytes), checks it against [`Limits`] from its
//! header, and returns an [`Image`] in sRGB: an embedded ICC profile is converted, and
//! [`thumbnail`] shrinks one without ever enlarging it. No egui is involved, so a server can use it too.

mod color;
mod decode;
mod error;
mod thumbnail;

pub use decode::{Image, Limits, Source, decode};
pub use error::Error;
pub use thumbnail::thumbnail;
