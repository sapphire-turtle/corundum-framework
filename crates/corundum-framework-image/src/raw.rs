//! Camera RAW files, read for their embedded preview through rawler.

use image::DynamicImage;
use image::metadata::Orientation;
use rawler::decoders::{Decoder, FormatHint, RawDecodeParams, WellKnownIFD};
use rawler::rawsource::RawSource;
use rawler::tags::TiffCommonTag;

use crate::{Error, Image, Limits};

/// Decode the largest embedded preview of a RAW file, with the RAW's own orientation applied.
///
/// `None` means rawler does not take the bytes for one of the RAW families we read and
/// they were not named as one either, so another decoder may try them. ORF, whose files
/// rawler gives no preview for, is [`Error::Unsupported`], as is a RAW without a preview.
pub(crate) fn decode(
    bytes: &[u8],
    named_raw: bool,
    limits: &Limits,
) -> Option<Result<Image, Error>> {
    let source = RawSource::new_from_slice(bytes);
    let decoder = match rawler::get_decoder(&source) {
        Ok(decoder) => decoder,
        Err(err) if named_raw => return Some(Err(Error::Decode(err.to_string()))),
        Err(_) => return None,
    };
    match decoder.format_hint() {
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
    Some(preview(decoder.as_ref(), &source, limits))
}

fn preview(decoder: &dyn Decoder, source: &RawSource, limits: &Limits) -> Result<Image, Error> {
    let params = RawDecodeParams::default();
    // rawler decodes the preview JPEG itself, so its size is only known afterwards; the
    // `image` crate's own allocation cap bounds that decode.
    let img = decoder
        .preview_image(source, &params)
        .map_err(|err| Error::Decode(err.to_string()))?
        .ok_or(Error::Unsupported)?;
    if u64::from(img.width()) * u64::from(img.height()) > limits.max_pixels {
        return Err(Error::TooLarge);
    }
    Ok(oriented(img, orientation(decoder, source, &params)))
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
