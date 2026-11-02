//! Camera RAW files, read for their embedded preview through rawler.

use image::DynamicImage;
use image::metadata::Orientation;
use rawler::decoders::{Decoder, FormatHint, RawDecodeParams, WellKnownIFD};
use rawler::formats::tiff::reader::TiffReader;
use rawler::formats::tiff::{GenericTiffReader, IFD};
use rawler::rawsource::RawSource;
use rawler::tags::{ExifTag, TiffCommonTag};

use crate::{Error, Image, Limits};

/// Decode the embedded preview of a RAW file, with the RAW's own orientation applied.
///
/// `None` means the bytes are not a RAW we read and were not named as one, so `image` may
/// try them: rawler does not take them for one of our RAW families, or (when they were not
/// named as a RAW) it finds no preview in them or fails on it. ORF, whose files rawler gives
/// no preview for, is [`Error::Unsupported`], as is a file named as a RAW without a preview.
pub(crate) fn decode(
    bytes: &[u8],
    named_raw: bool,
    limits: &Limits,
) -> Option<Result<Image, Error>> {
    let source = RawSource::new_from_slice(bytes);
    let decoder = match rawler::get_decoder(&source) {
        Ok(decoder) => decoder,
        Err(err) if named_raw => return Some(Err(preview_error(err))),
        Err(_) => return None,
    };
    let hint = decoder.format_hint();
    match hint {
        FormatHint::ORF => return Some(Err(Error::Unsupported)),
        FormatHint::CR2
        | FormatHint::CR3
        | FormatHint::NEF
        | FormatHint::ARW
        | FormatHint::RW2
        | FormatHint::RAF
        | FormatHint::DNG => {}
        _ if named_raw => return Some(Err(Error::Unsupported)),
        _ => return None,
    }
    match preview(decoder.as_ref(), hint, &source, limits) {
        Err(Error::Decode(_) | Error::Unsupported) if !named_raw => None,
        result => Some(result),
    }
}

fn preview(
    decoder: &dyn Decoder,
    hint: FormatHint,
    source: &RawSource,
    limits: &Limits,
) -> Result<Image, Error> {
    check_preview_header(hint, source, limits)?;
    let params = RawDecodeParams::default();
    // A CR3 whose preview is HDR (HEIF or PQ) has none rawler can give: `None`.
    let img = decoder
        .preview_image(source, &params)
        .map_err(preview_error)?
        .ok_or(Error::Unsupported)?;
    if u64::from(img.width()) * u64::from(img.height()) > limits.max_pixels {
        return Err(Error::TooLarge);
    }
    Ok(oriented(img, orientation(decoder, source, &params)))
}

/// Check the preview's size from its header, before rawler allocates it.
///
/// A DNG's preview, and a NEF's when it has no JPEG preview, is a TIFF image that rawler
/// decodes itself, allocating the size the IFD declares; a failed allocation aborts the
/// process. So that IFD (the one rawler will pick) is measured first. The other families'
/// previews are JPEGs that rawler hands to `image`, whose own allocation cap (512 MiB by
/// default) turns an oversized one into an error, and the `max_pixels` check after the
/// decode applies to them.
fn check_preview_header(
    hint: FormatHint,
    source: &RawSource,
    limits: &Limits,
) -> Result<(), Error> {
    if !matches!(hint, FormatHint::DNG | FormatHint::NEF) {
        return Ok(());
    }
    let tiff = GenericTiffReader::new(&mut source.reader(), 0, 0, None, &[])
        .map_err(|err| Error::Decode(err.to_string()))?;
    let root = tiff.root_ifd();
    let ifd = if hint == FormatHint::DNG {
        root.get_sub_ifd_all(TiffCommonTag::SubIFDs)
            .and_then(|subs| subs.iter().find(|ifd| new_sub_file_type(ifd) == Some(1)))
    } else {
        let jpeg = tiff.find_ifds_with_filter(|ifd| {
            new_sub_file_type(ifd) == Some(1) && ifd.has_entry(ExifTag::JPEGInterchangeFormatLength)
        });
        jpeg.is_empty().then_some(root)
    };
    match ifd {
        Some(ifd) => check_ifd(ifd, limits),
        None => Ok(()),
    }
}

/// rawler's error, as ours: a file or preview it does not handle is [`Error::Unsupported`],
/// so a caller can hand the file to another tool; anything else is a broken file.
///
/// rawler tells the two apart only in its message for some: a preview refused under
/// `RAWLER_IGNORE_PREVIEWS` (a CR3's), and one in a compression it does not decode.
fn preview_error(err: rawler::RawlerError) -> Error {
    match err {
        rawler::RawlerError::Unsupported { .. } => Error::Unsupported,
        rawler::RawlerError::DecoderFailed(message)
            if message == "Unable to extract preview image"
                || message.starts_with("Unsupported compression method") =>
        {
            Error::Unsupported
        }
        other => Error::Decode(other.to_string()),
    }
}

fn new_sub_file_type(ifd: &IFD) -> Option<u32> {
    ifd.get_entry(TiffCommonTag::NewSubFileType)
        .map(|e| e.force_u32(0))
}

/// The pixels an IFD's image takes once decoded (rounded up to whole tiles, as rawler
/// allocates them) must fit `max_pixels`, with at most four samples a pixel.
fn check_ifd(ifd: &IFD, limits: &Limits) -> Result<(), Error> {
    let tag = |t: TiffCommonTag| ifd.get_entry(t).map(|e| u64::from(e.force_u32(0)));
    let (Some(width), Some(height)) = (
        tag(TiffCommonTag::ImageWidth),
        tag(TiffCommonTag::ImageLength),
    ) else {
        // rawler refuses an IFD without them before allocating anything.
        return Ok(());
    };
    if tag(TiffCommonTag::SamplesPerPixel).unwrap_or(1) > 4 {
        return Err(Error::Decode(
            "a preview with more than four samples a pixel".into(),
        ));
    }
    let round_up = |side: u64, tile: Option<u64>| match tile {
        Some(0) => None,
        Some(tile) => Some(side.div_ceil(tile).saturating_mul(tile)),
        None => Some(side),
    };
    let (Some(width), Some(height)) = (
        round_up(width, tag(TiffCommonTag::TileWidth)),
        round_up(height, tag(TiffCommonTag::TileLength)),
    ) else {
        return Err(Error::Decode("a preview with tiles of no size".into()));
    };
    if width.saturating_mul(height) > limits.max_pixels {
        return Err(Error::TooLarge);
    }
    Ok(())
}

/// The RAW's EXIF orientation: from its metadata, or else from the root IFD (a DNG's
/// metadata needs the raw image, which a preview-only reader may not have).
fn orientation(decoder: &dyn Decoder, source: &RawSource, params: &RawDecodeParams) -> Orientation {
    let from_metadata = decoder
        .raw_metadata(source, params)
        .ok()
        .and_then(|m| m.exif.orientation);
    let from_root = || {
        decoder
            .ifd(WellKnownIFD::Root)
            .ok()
            .flatten()
            .and_then(|ifd| {
                ifd.get_entry(TiffCommonTag::Orientation)
                    .map(|e| e.force_u16(0))
            })
    };
    from_metadata
        .or_else(from_root)
        .and_then(|o| Orientation::from_exif(o.min(255) as u8))
        .unwrap_or(Orientation::NoTransforms)
}

fn oriented(mut img: DynamicImage, orientation: Orientation) -> Image {
    img.apply_orientation(orientation);
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    Image {
        width,
        height,
        rgba: rgba.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    use rawler::RawlerError;

    use super::*;

    #[test]
    fn a_preview_rawler_cannot_give_is_unsupported() {
        let camera = RawlerError::Unsupported {
            what: "preview".into(),
            model: "m".into(),
            make: "k".into(),
            mode: String::new(),
        };
        assert!(matches!(preview_error(camera), Error::Unsupported));
        // What a CR3 says under RAWLER_IGNORE_PREVIEWS.
        let ignored = RawlerError::DecoderFailed("Unable to extract preview image".into());
        assert!(matches!(preview_error(ignored), Error::Unsupported));
        let deflate = RawlerError::DecoderFailed(
            "Unsupported compression method: Deflate, storage: Strips".into(),
        );
        assert!(matches!(preview_error(deflate), Error::Unsupported));
    }

    #[test]
    fn a_broken_preview_is_a_decode_error() {
        let broken = RawlerError::DecoderFailed("Failed to read JPEG image: eof".into());
        assert!(matches!(preview_error(broken), Error::Decode(_)));
    }
}
