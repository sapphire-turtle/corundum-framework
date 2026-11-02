//! RAW files. The DNG is written here with rawler's own DNG writer: a 160×120 JPEG preview
//! in a SubIFD, which is how a camera DNG carries its preview.

use std::io::Cursor;

use corundum_framework_image::{Error, Limits, Source, decode};
use image::{DynamicImage, Rgb, RgbImage};
use rawler::dng::writer::DngWriter;
use rawler::tags::TiffCommonTag;

/// A DNG with a `w`×`h` preview and `orientation` (an EXIF value) on its root IFD.
fn dng(w: u32, h: u32, orientation: u16) -> Vec<u8> {
    let preview = DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, 128])
    }));
    let mut buf = Cursor::new(Vec::new());
    let mut writer = DngWriter::new(&mut buf, [1, 4, 0, 0]).unwrap();
    writer
        .root_ifd_mut()
        .add_tag(TiffCommonTag::Orientation, orientation);
    let mut sub = writer.subframe(1);
    sub.preview(&preview, 0.9).unwrap();
    sub.finalize().unwrap();
    writer.close().unwrap();
    buf.into_inner()
}

fn named<'a>(bytes: &'a [u8], ext: &'a str) -> Source<'a> {
    Source::Bytes {
        bytes,
        ext: Some(ext),
    }
}

#[test]
fn an_orf_is_unsupported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.orf");
    let mut data = b"IIRO\x08\0\0\0".to_vec();
    data.extend_from_slice(&[0; 64]);
    std::fs::write(&path, &data).unwrap();
    let r = decode(Source::Path(&path), &Limits::default());
    assert!(matches!(r, Err(Error::Unsupported)), "{r:?}");
}

#[cfg(not(feature = "raw"))]
#[test]
fn without_the_feature_raw_files_are_unsupported() {
    let data = dng(160, 120, 1);
    for ext in ["cr2", "cr3", "nef", "arw", "rw2", "raf", "dng", "DNG"] {
        let r = decode(named(&data, ext), &Limits::default());
        assert!(matches!(r, Err(Error::Unsupported)), "{ext}: {r:?}");
    }
}

#[cfg(feature = "raw")]
mod with_the_feature {
    use super::*;
    use corundum_framework_image::thumbnail;

    #[test]
    fn a_dng_preview_is_thumbnailed() {
        let data = dng(160, 120, 1);
        let t = thumbnail(named(&data, "dng"), 64, &Limits::default()).unwrap();
        assert_eq!((t.width, t.height), (64, 48));
    }

    #[test]
    fn a_dng_is_found_by_its_content() {
        let data = dng(160, 120, 1);
        let source = Source::Bytes {
            bytes: &data,
            ext: None,
        };
        let img = decode(source, &Limits::default()).unwrap();
        assert_eq!((img.width, img.height), (160, 120));
    }

    #[test]
    fn the_raw_orientation_is_applied_to_the_preview() {
        let data = dng(160, 120, 6);
        let img = decode(named(&data, "dng"), &Limits::default()).unwrap();
        assert_eq!((img.width, img.height), (120, 160));
    }

    #[test]
    fn a_preview_over_max_pixels_is_too_large() {
        let data = dng(160, 120, 1);
        let limits = Limits {
            max_pixels: 160 * 120 - 1,
            ..Limits::default()
        };
        let r = decode(named(&data, "dng"), &limits);
        assert!(matches!(r, Err(Error::TooLarge)), "{r:?}");
    }

    #[test]
    fn a_raw_file_over_max_bytes_is_too_large() {
        let data = dng(160, 120, 1);
        let limits = Limits {
            max_bytes: data.len() as u64 - 1,
            ..Limits::default()
        };
        let r = decode(named(&data, "dng"), &limits);
        assert!(matches!(r, Err(Error::TooLarge)), "{r:?}");
    }

    #[test]
    fn random_bytes_named_nef_are_not_ok() {
        let mut data = b"II*\0\x08\0\0\0".to_vec();
        data.extend((0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8));
        for ext in ["nef", "cr2", "cr3", "arw", "rw2", "raf", "dng"] {
            let r = decode(named(&data, ext), &Limits::default());
            assert!(r.is_err(), "{ext}: {r:?}");
            let r = decode(named(&data[8..], ext), &Limits::default());
            assert!(r.is_err(), "{ext}: {r:?}");
        }
    }
}
