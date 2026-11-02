use moxcms::{ColorProfile, Layout, TransformOptions};

/// Convert `rgba` (RGBA8) from the embedded ICC profile `icc` to sRGB, in place.
///
/// A profile moxcms cannot read, or cannot build a transform from, leaves the pixels as
/// they are. Alpha is carried over unchanged.
pub(crate) fn to_srgb(rgba: &mut [u8], icc: &[u8]) {
    let result = ColorProfile::new_from_slice(icc).and_then(|src| {
        let transform = src.create_transform_8bit(
            Layout::Rgba,
            &ColorProfile::new_srgb(),
            Layout::Rgba,
            TransformOptions::default(),
        )?;
        let mut out = vec![0u8; rgba.len()];
        transform.transform(rgba, &mut out)?;
        Ok(out)
    });
    match result {
        Ok(out) => rgba.copy_from_slice(&out),
        Err(err) => tracing::debug!("ignoring an unusable ICC profile: {err}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_leaves_the_pixels_alone() {
        let mut px = [200, 100, 50, 255];
        to_srgb(&mut px, &[7; 16]);
        assert_eq!(px, [200, 100, 50, 255]);
    }
}
