use crate::{Error, Image};

/// The longest side a WebP can have.
const MAX_SIDE: u32 = 16383;

/// Encode `image` as a lossy WebP. `quality` runs from 0 (smallest) to 100 (best) and is
/// clamped to that range. Alpha is kept.
///
/// A side over 16383 pixels, the most WebP allows, is [`Error::TooLarge`]; an `rgba` that is
/// not `width * height * 4` bytes is [`Error::Encode`].
pub fn encode_webp(image: &Image, quality: f32) -> Result<Vec<u8>, Error> {
    let Image {
        width,
        height,
        rgba,
    } = image;
    if *width == 0
        || *height == 0
        || u64::from(*width) * u64::from(*height) * 4 != rgba.len() as u64
    {
        return Err(Error::Encode(format!(
            "{} bytes is not a {width}x{height} RGBA image",
            rgba.len()
        )));
    }
    if *width > MAX_SIDE || *height > MAX_SIDE {
        return Err(Error::TooLarge);
    }
    let quality = if quality.is_nan() {
        0.0
    } else {
        quality.clamp(0.0, 100.0)
    };
    let mut out = std::ptr::null_mut();
    // SAFETY: `rgba` holds `height` rows of `width * 4` bytes, checked above, and both sides
    // fit a C int. libwebp only reads it, and on success hands back a buffer of `len` bytes
    // that is ours to free with `WebPFree`.
    let len = unsafe {
        libwebp_sys::WebPEncodeRGBA(
            rgba.as_ptr(),
            *width as i32,
            *height as i32,
            (*width * 4) as i32,
            quality,
            &mut out,
        )
    };
    if len == 0 || out.is_null() {
        return Err(Error::Encode("libwebp could not encode the image".into()));
    }
    // SAFETY: libwebp wrote `len` bytes at `out`; they are copied before the buffer is freed.
    let bytes = unsafe { std::slice::from_raw_parts(out, len) }.to_vec();
    unsafe { libwebp_sys::WebPFree(out.cast()) };
    Ok(bytes)
}
