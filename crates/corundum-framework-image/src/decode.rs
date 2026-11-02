use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader};

use crate::Error;

/// Where an image comes from.
#[derive(Debug, Clone, Copy)]
pub enum Source<'a> {
    /// A file.
    Path(&'a Path),
    /// Bytes already in memory. `ext` (such as `"jpg"`) is only a tie-break for when the
    /// bytes alone do not name a format.
    Bytes {
        bytes: &'a [u8],
        ext: Option<&'a str>,
    },
}

/// A decoded image: RGBA8, row after row, orientation applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// How big an image [`decode`] accepts. Checked from the header, before pixels are allocated.
///
/// Peak memory while decoding is a few times the pixel buffer (the decoded buffer, its
/// RGBA conversion and the copy an orientation makes), so size `max_pixels` for that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The most bytes the file may have.
    pub max_bytes: u64,
    /// The most pixels (width times height) the image may have.
    pub max_pixels: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 512 * 1024 * 1024,
            max_pixels: 200_000_000,
        }
    }
}

/// Decode `source` to RGBA8 with its EXIF orientation applied.
///
/// The format comes from the bytes; the extension decides only when they say nothing.
/// HEIC, HEIF and AVIF, and any format this build lacks, are [`Error::Unsupported`]. A
/// decoder that panics on a bad file is an [`Error::Decode`].
pub fn decode(source: Source, limits: &Limits) -> Result<Image, Error> {
    match source {
        Source::Path(path) => {
            if std::fs::metadata(path)?.len() > limits.max_bytes {
                return Err(Error::TooLarge);
            }
            let bytes = std::fs::read(path)?;
            let ext = path.extension().and_then(|e| e.to_str());
            decode_bytes(&bytes, ext, limits)
        }
        Source::Bytes { bytes, ext } => decode_bytes(bytes, ext, limits),
    }
}

fn decode_bytes(bytes: &[u8], ext: Option<&str>, limits: &Limits) -> Result<Image, Error> {
    if bytes.len() as u64 > limits.max_bytes {
        return Err(Error::TooLarge);
    }
    if is_heif_family(bytes) {
        return Err(Error::Unsupported);
    }
    let format = image::guess_format(bytes)
        .ok()
        .or_else(|| ext.and_then(ImageFormat::from_extension))
        .ok_or(Error::Unsupported)?;
    catch_unwind(AssertUnwindSafe(|| decode_as(bytes, format, limits)))
        .unwrap_or_else(|_| Err(Error::Decode("decoder panicked".into())))
}

/// An ISO base media file whose brand is HEIC, HEIF or AVIF.
fn is_heif_family(bytes: &[u8]) -> bool {
    bytes.len() >= 12
        && &bytes[4..8] == b"ftyp"
        && matches!(
            &bytes[8..12],
            b"heic"
                | b"heix"
                | b"hevc"
                | b"hevx"
                | b"heim"
                | b"heis"
                | b"mif1"
                | b"msf1"
                | b"avif"
                | b"avis"
        )
}

fn decode_as(bytes: &[u8], format: ImageFormat, limits: &Limits) -> Result<Image, Error> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(codec_limits(limits));
    let mut decoder = reader.into_decoder().map_err(map_error)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > limits.max_pixels {
        return Err(Error::TooLarge);
    }
    let orientation = decoder.orientation().map_err(map_error)?;
    // Task 2 converts the pixels from this profile to sRGB.
    let _icc = icc_profile(&mut decoder);
    let mut img = DynamicImage::from_decoder(decoder).map_err(map_error)?;
    img.apply_orientation(orientation);
    let rgba = img.into_rgba8();
    Ok(Image {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// The codec's own limits, made to follow ours.
///
/// `image` would otherwise cap the allocation at its own 512 MiB, whatever our limits say.
/// Room for an RGBA16 buffer per pixel makes `max_pixels` the real bound.
fn codec_limits(limits: &Limits) -> image::Limits {
    let mut codec = image::Limits::default();
    codec.max_image_width = None;
    codec.max_image_height = None;
    codec.max_alloc = Some(limits.max_pixels.saturating_mul(8));
    codec
}

/// The decoder's embedded ICC profile, if it has a readable one.
fn icc_profile(decoder: &mut impl ImageDecoder) -> Option<Vec<u8>> {
    decoder.icc_profile().ok().flatten()
}

// The bytes are in memory, so an I/O error here is a truncated file: a decode error.
fn map_error(err: ImageError) -> Error {
    match err {
        ImageError::Unsupported(_) => Error::Unsupported,
        ImageError::Limits(_) => Error::TooLarge,
        other => Error::Decode(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_limits_follow_ours() {
        let l = codec_limits(&Limits {
            max_pixels: 300_000_000,
            ..Limits::default()
        });
        assert_eq!((l.max_image_width, l.max_image_height), (None, None));
        assert_eq!(l.max_alloc, Some(2_400_000_000));
        let l = codec_limits(&Limits {
            max_pixels: u64::MAX,
            ..Limits::default()
        });
        assert_eq!(l.max_alloc, Some(u64::MAX));
    }
}
