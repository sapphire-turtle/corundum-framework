use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions};

/// Convert `rgba` (RGBA8) from the embedded ICC profile `icc` to sRGB, in place.
///
/// Only an RGB profile (connected through XYZ or Lab) is applied. Anything else leaves the
/// pixels as they are: a CMYK or grey profile describes the file's own samples, not the RGB
/// the decoder has already made from them (and moxcms would misread RGBA as CMYK, or
/// refuse a grey profile). A profile moxcms cannot read or build a transform from is
/// ignored too. Alpha is carried over unchanged.
pub(crate) fn to_srgb(rgba: &mut [u8], icc: &[u8]) {
    let result = ColorProfile::new_from_slice(icc).and_then(|src| {
        if !applies(&src) {
            return Err(moxcms::CmsError::UnsupportedProfileConnection);
        }
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

/// Whether `profile` describes RGB pixels connected through XYZ or Lab.
fn applies(profile: &ColorProfile) -> bool {
    profile.color_space == DataColorSpace::Rgb
        && matches!(profile.pcs, DataColorSpace::Xyz | DataColorSpace::Lab)
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

    // The positive control for a real RGB profile is the Display P3 test in tests/thumbnail.rs.
    fn srgb_with(f: impl FnOnce(&mut ColorProfile)) -> ColorProfile {
        let mut p = ColorProfile::new_srgb();
        f(&mut p);
        p
    }

    #[test]
    fn only_an_rgb_profile_applies() {
        assert!(applies(&ColorProfile::new_srgb()));
        assert!(applies(&srgb_with(|p| p.pcs = DataColorSpace::Lab)));
        assert!(!applies(&srgb_with(
            |p| p.color_space = DataColorSpace::Cmyk
        )));
        assert!(!applies(&srgb_with(
            |p| p.color_space = DataColorSpace::Gray
        )));
        assert!(!applies(&srgb_with(|p| p.pcs = DataColorSpace::Rgb)));
    }

    #[test]
    fn an_unusable_profile_leaves_the_pixels_alone() {
        // A profile moxcms cannot use (its header says CMYK, but it has no tables).
        let mut icc = ColorProfile::new_srgb().encode().unwrap();
        icc[16..20].copy_from_slice(b"CMYK");
        let mut px = [200, 100, 50, 255];
        to_srgb(&mut px, &icc);
        assert_eq!(px, [200, 100, 50, 255]);
    }
}
