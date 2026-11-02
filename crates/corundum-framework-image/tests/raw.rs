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

#[cfg(feature = "raw")]
/// A DNG whose only preview IFD declares a `w`×`h` uncompressed RGB image over a few bytes
/// of data: a malformed file that would make a decoder trusting the header allocate it.
fn dng_with_declared_preview(w: u32, h: u32) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    let mut writer = DngWriter::new(&mut buf, [1, 4, 0, 0]).unwrap();
    let mut sub = writer.subframe(1);
    let ifd = sub.ifd_mut();
    ifd.add_tag(TiffCommonTag::ImageWidth, w);
    ifd.add_tag(TiffCommonTag::ImageLength, h);
    ifd.add_tag(TiffCommonTag::Compression, 1_u16);
    ifd.add_tag(TiffCommonTag::BitsPerSample, [8_u16, 8, 8]);
    ifd.add_tag(TiffCommonTag::PhotometricInt, 2_u16);
    ifd.add_tag(TiffCommonTag::SamplesPerPixel, 3_u16);
    ifd.add_tag(TiffCommonTag::RowsPerStrip, h);
    ifd.add_tag(TiffCommonTag::StripOffsets, 8_u32);
    ifd.add_tag(TiffCommonTag::StripByteCounts, 16_u32);
    sub.finalize().unwrap();
    writer.close().unwrap();
    buf.into_inner()
}

#[cfg(feature = "raw")]
/// A DNG with no preview, only an uncompressed RGB thumbnail in its root IFD: a TIFF
/// image that `image` can read.
fn dng_without_preview() -> Vec<u8> {
    let thumb = DynamicImage::ImageRgb8(RgbImage::from_pixel(80, 40, Rgb([10, 20, 30])));
    let mut buf = Cursor::new(Vec::new());
    let mut writer = DngWriter::new(&mut buf, [1, 4, 0, 0]).unwrap();
    writer.thumbnail(&thumb).unwrap();
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
        let limits = Limits::default().with_max_pixels(160 * 120 - 1);
        let r = decode(named(&data, "dng"), &limits);
        assert!(matches!(r, Err(Error::TooLarge)), "{r:?}");
    }

    #[test]
    fn a_raw_file_over_max_bytes_is_too_large() {
        let data = dng(160, 120, 1);
        let limits = Limits::default().with_max_bytes(data.len() as u64 - 1);
        let r = decode(named(&data, "dng"), &limits);
        assert!(matches!(r, Err(Error::TooLarge)), "{r:?}");
    }

    #[test]
    fn a_preview_header_over_max_pixels_is_too_large_before_decoding() {
        // 60000×60000×3 is 10.8 GB: refused from the header, never allocated.
        let data = dng_with_declared_preview(60_000, 60_000);
        let r = decode(named(&data, "dng"), &Limits::default());
        assert!(matches!(r, Err(Error::TooLarge)), "{r:?}");
    }

    #[test]
    fn a_raw_with_no_preview_is_unsupported_when_named_so() {
        let data = dng_without_preview();
        let r = decode(named(&data, "dng"), &Limits::default());
        assert!(matches!(r, Err(Error::Unsupported)), "{r:?}");
    }

    #[test]
    fn an_unnamed_file_rawler_cannot_preview_falls_back_to_image() {
        let data = dng_without_preview();
        let source = Source::Bytes {
            bytes: &data,
            ext: Some("tif"),
        };
        let img = decode(source, &Limits::default()).unwrap();
        assert!(img.width > 0 && img.height > 0);
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
