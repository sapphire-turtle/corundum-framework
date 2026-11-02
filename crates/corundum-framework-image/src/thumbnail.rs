use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{PixelType, Resizer};

use crate::{Error, Image, Limits, Source, decode};

/// Decode `source` and shrink it so its longest side is `longest` pixels, keeping the aspect
/// ratio (the short side is rounded, and at least 1). An image already that small is
/// returned as decoded: a thumbnail is never enlarged. `longest == 0` also returns the
/// image at full size.
pub fn thumbnail(source: Source, longest: u32, limits: &Limits) -> Result<Image, Error> {
    let img = decode(source, limits)?;
    let long = img.width.max(img.height);
    if longest == 0 || long <= longest {
        return Ok(img);
    }
    let scale = |side: u32| {
        ((u64::from(side) * u64::from(longest) + u64::from(long) / 2) / u64::from(long)).max(1)
            as u32
    };
    let (width, height) = if img.width >= img.height {
        (longest, scale(img.height))
    } else {
        (scale(img.width), longest)
    };
    let src = FirImage::from_vec_u8(img.width, img.height, img.rgba, PixelType::U8x4)
        .map_err(|e| Error::Decode(e.to_string()))?;
    let mut dst = FirImage::new(width, height, PixelType::U8x4);
    // The default options are Lanczos3, with alpha taken into account.
    Resizer::new()
        .resize(&src, &mut dst, None)
        .map_err(|e| Error::Decode(e.to_string()))?;
    Ok(Image {
        width,
        height,
        rgba: dst.into_vec(),
    })
}
