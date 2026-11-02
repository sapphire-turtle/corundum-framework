use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use image::{DynamicImage, ImageDecoder, ImageError, ImageFormat, ImageReader};

use crate::{Error, color};

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
///
/// Made by [`decode`] or, from pixels of your own, by [`Image::new`]:
///
/// ```compile_fail
/// let img = corundum_framework_image::Image { width: 1, height: 1, rgba: vec![0; 4] };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl Image {
    /// An image of `width` × `height` pixels from `rgba` (RGBA8, row after row), or `None`
    /// if `rgba` is not `width * height * 4` bytes.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Image> {
        let len = u64::from(width) * u64::from(height) * 4;
        (rgba.len() as u64 == len).then_some(Image {
            width,
            height,
            rgba,
        })
    }
}

/// How big an image [`decode`] accepts. Checked from the header, before pixels are allocated.
///
/// Peak memory while decoding is a few times the pixel buffer (the decoded buffer, its
/// RGBA conversion and the copy an orientation makes), so size `max_pixels` for that.
///
/// Made with [`Limits::new`], or from the default with [`Limits::with_max_pixels`] and
/// [`Limits::with_max_bytes`]:
///
/// ```compile_fail
/// let limits = corundum_framework_image::Limits { max_bytes: 1, max_pixels: 1 };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The most bytes the file may have.
    pub max_bytes: u64,
    /// The most pixels (width times height) the image may have.
    pub max_pixels: u64,
}

impl Limits {
    /// Limits of `max_bytes` for the file and `max_pixels` for the image.
    pub const fn new(max_bytes: u64, max_pixels: u64) -> Limits {
        Limits {
            max_bytes,
            max_pixels,
        }
    }

    /// These limits with `max_bytes` for the file.
    pub const fn with_max_bytes(self, max_bytes: u64) -> Limits {
        Limits { max_bytes, ..self }
    }

    /// These limits with `max_pixels` for the image.
    pub const fn with_max_pixels(self, max_pixels: u64) -> Limits {
        Limits { max_pixels, ..self }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 512 * 1024 * 1024,
            max_pixels: 200_000_000,
        }
    }
}

/// Decode `source` to sRGB RGBA8 with its EXIF orientation applied.
///
/// An embedded ICC profile is converted to sRGB; one that cannot be read is ignored.
///
/// The format comes from the bytes; the extension decides only when they say nothing.
/// HEIC, HEIF and AVIF, and any format this build lacks, are [`Error::Unsupported`]. A
/// decoder that panics on a bad file is an [`Error::Decode`].
///
/// With the `raw` feature, a camera RAW file (CR2, CR3, NEF, ARW, RW2, RAF or DNG, told
/// by its content, or else by its extension) decodes to the embedded preview rawler picks
/// (the large one, where a file has several), with the RAW's own orientation applied; a
/// file named as a RAW without one is [`Error::Unsupported`], while one only recognised by
/// its content goes on to the other decoders. ORF, and any RAW file without the feature,
/// is [`Error::Unsupported`].
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
    let guessed = image::guess_format(bytes).ok();
    // Most RAW files are TIFF underneath, and the rest have no signature `image` knows.
    if matches!(guessed, None | Some(ImageFormat::Tiff)) {
        let raw_ext = ext.and_then(RawExt::from_ext);
        if raw_ext == Some(RawExt::Orf) {
            return Err(Error::Unsupported);
        }
        #[cfg(feature = "raw")]
        if let Some(result) = catch_panic(|| crate::raw::decode(bytes, raw_ext.is_some(), limits))
            // A panic in rawler is reported only for a file named as a RAW; anything else
            // goes on to `image`, as when rawler does not recognise it.
            .unwrap_or_else(|err| raw_ext.is_some().then_some(Err(err)))
        {
            return result;
        }
        #[cfg(not(feature = "raw"))]
        if raw_ext.is_some() {
            return Err(Error::Unsupported);
        }
    }
    let format = guessed
        .or_else(|| ext.and_then(ImageFormat::from_extension))
        .ok_or(Error::Unsupported)?;
    catch_panic(|| decode_as(bytes, format, limits))?
}

/// Run a decoder, turning its panic on a bad file into an [`Error::Decode`].
fn catch_panic<T>(f: impl FnOnce() -> T) -> Result<T, Error> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| Error::Decode("decoder panicked".into()))
}

/// The extensions of the RAW families: read with the `raw` feature, except ORF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RawExt {
    Previewed,
    Orf,
}

impl RawExt {
    fn from_ext(ext: &str) -> Option<Self> {
        let ext = ext.to_ascii_lowercase();
        match ext.as_str() {
            "cr2" | "cr3" | "nef" | "arw" | "rw2" | "raf" | "dng" => Some(Self::Previewed),
            "orf" => Some(Self::Orf),
            _ => None,
        }
    }
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
    let icc = icc_profile(&mut decoder);
    let mut img = DynamicImage::from_decoder(decoder).map_err(map_error)?;
    img.apply_orientation(orientation);
    // The colour step is a per-pixel map, so it commutes with orientation; doing it last
    // keeps it on the final buffer. Grey and 16-bit sources are brought to RGBA8 first, so
    // the transform runs at 8 bits.
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    let mut rgba = rgba.into_raw();
    if let Some(icc) = icc {
        color::to_srgb(&mut rgba, &icc);
    }
    Ok(Image {
        width,
        height,
        rgba,
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
