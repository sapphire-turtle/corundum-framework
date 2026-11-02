use fast_image_resize::images::{Image as FirImage, ImageRef};
use fast_image_resize::{PixelType, Resizer};

use crate::{Error, Image, Limits, Source, decode};

/// Decode `source` and [`shrink`] it so its longest side is `longest` pixels.
pub fn thumbnail(source: Source, longest: u32, limits: &Limits) -> Result<Image, Error> {
    let img = decode(source, limits)?;
    let long = img.width.max(img.height);
    if longest == 0 || long <= longest {
        return Ok(img);
    }
    Ok(shrink(&img, longest))
}

/// `image` shrunk so its longest side is `longest` pixels, keeping the aspect ratio (the
/// short side is rounded, and at least 1). An image already that small is returned as it
/// is: it is never enlarged. `longest == 0` also returns it at full size.
///
/// # Panics
///
/// If `image.rgba` is not `width * height * 4` bytes.
pub fn shrink(image: &Image, longest: u32) -> Image {
    let long = image.width.max(image.height);
    if longest == 0 || long <= longest {
        return image.clone();
    }
    let scale = |side: u32| {
        ((u64::from(side) * u64::from(longest) + u64::from(long) / 2) / u64::from(long)).max(1)
            as u32
    };
    let (width, height) = if image.width >= image.height {
        (longest, scale(image.height))
    } else {
        (scale(image.width), longest)
    };
    let src = ImageRef::new(image.width, image.height, &image.rgba, PixelType::U8x4)
        .expect("an image's rgba is width * height * 4 bytes");
    let mut dst = FirImage::new(width, height, PixelType::U8x4);
    // The default options are Lanczos3, with alpha taken into account. The source and
    // destination have the same pixel type, the only thing that makes a resize fail.
    Resizer::new()
        .resize(&src, &mut dst, None)
        .expect("resize RGBA8 to RGBA8");
    Image {
        width,
        height,
        rgba: dst.into_vec(),
    }
}
