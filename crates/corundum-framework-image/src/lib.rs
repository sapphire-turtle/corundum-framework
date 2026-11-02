//! Decode an image to RGBA8 pixels with the EXIF orientation applied, without a GUI.
//!
//! [`decode`] reads a [`Source`] (a file or bytes), checks it against [`Limits`] from its
//! header, and returns an [`Image`] in sRGB: an embedded ICC profile is converted, and
//! [`thumbnail`] shrinks one without ever enlarging it. No egui is involved, so a server can use it too.
//!
//! Two features, both off by default:
//!
//! - `webp`: `encode_webp`, lossy WebP encoding through libwebp, built from its bundled C
//!   sources.
//! - `raw`: camera RAW files (CR2, CR3, NEF, ARW, RW2, RAF, DNG) decode to their largest
//!   embedded preview, through [rawler](https://crates.io/crates/rawler). rawler is licensed
//!   under LGPL-2.1, so an app that turns this feature on takes on its terms. ORF stays
//!   unsupported.

mod color;
mod decode;
mod error;
#[cfg(feature = "raw")]
mod raw;
mod thumbnail;
#[cfg(feature = "webp")]
mod webp;

pub use decode::{Image, Limits, Source, decode};
pub use error::Error;
pub use thumbnail::thumbnail;
#[cfg(feature = "webp")]
pub use webp::encode_webp;
