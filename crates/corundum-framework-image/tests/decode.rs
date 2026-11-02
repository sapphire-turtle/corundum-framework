mod common;

use std::io::Cursor;

use common::{crc32, encode, jpeg_from, jpeg_with_orientation};
use corundum_framework_image::{Error, Limits, Source, decode};
use image::{ImageFormat, Rgba, RgbaImage};

#[test]
fn each_format_decodes_to_rgba() {
    let img = RgbaImage::from_pixel(4, 3, Rgba([200, 100, 50, 255]));
    for (format, ext) in [
        (ImageFormat::Jpeg, "jpg"),
        (ImageFormat::Png, "png"),
        (ImageFormat::Gif, "gif"),
        (ImageFormat::Tiff, "tiff"),
        (ImageFormat::WebP, "webp"),
    ] {
        let bytes = encode(&img, format);
        let out = decode(
            Source::Bytes {
                bytes: &bytes,
                ext: Some(ext),
            },
            &Limits::default(),
        )
        .unwrap_or_else(|e| panic!("{ext}: {e}"));
        assert_eq!((out.width, out.height), (4, 3), "{ext}");
        assert_eq!(out.rgba.len(), 48, "{ext}");
    }
}

#[test]
fn orientation_is_applied() {
    for (o, dims) in [(1, (40, 20)), (3, (40, 20)), (6, (20, 40)), (8, (20, 40))] {
        let bytes = jpeg_with_orientation(40, 20, o);
        let img = decode(
            Source::Bytes {
                bytes: &bytes,
                ext: Some("jpg"),
            },
            &Limits::default(),
        )
        .unwrap();
        assert_eq!((img.width, img.height), dims, "orientation {o}");
    }
}

#[test]
fn a_mirrored_orientation_flips_pixels() {
    let mut src = RgbaImage::new(16, 8);
    for (x, _, p) in src.enumerate_pixels_mut() {
        *p = if x < 8 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        };
    }
    for (o, left_is_blue) in [(1, false), (2, true)] {
        let bytes = jpeg_from(&src, o);
        let img = decode(
            Source::Bytes {
                bytes: &bytes,
                ext: None,
            },
            &Limits::default(),
        )
        .unwrap();
        let left = &img.rgba[(4 * 4)..(4 * 4 + 4)]; // row 0, x = 4
        assert_eq!(
            left[2] > 150 && left[0] < 100,
            left_is_blue,
            "orientation {o}"
        );
    }
}

#[test]
fn heic_and_avif_are_unsupported() {
    for brand in [b"heic", b"avif"] {
        let mut bytes = vec![0, 0, 0, 24];
        bytes.extend_from_slice(b"ftyp");
        bytes.extend_from_slice(brand);
        bytes.extend_from_slice(&[0; 12]);
        let r = decode(
            Source::Bytes {
                bytes: &bytes,
                ext: None,
            },
            &Limits::default(),
        );
        assert!(matches!(r, Err(Error::Unsupported)), "{brand:?}");
    }
}

#[test]
fn garbage_is_a_decode_error() {
    let bytes: Vec<u8> = (0..100u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
        .collect();
    let r = decode(
        Source::Bytes {
            bytes: &bytes,
            ext: Some("jpg"),
        },
        &Limits::default(),
    );
    assert!(matches!(r, Err(Error::Decode(_) | Error::Unsupported)));
}

fn png_header(w: u32, h: u32) -> Vec<u8> {
    let mut ihdr = b"IHDR".to_vec();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(&ihdr);
    png.extend_from_slice(&crc32(&ihdr).to_be_bytes());
    // The decoder reads up to the first IDAT before it reports the size.
    png.extend_from_slice(&0u32.to_be_bytes());
    png.extend_from_slice(b"IDAT");
    png.extend_from_slice(&crc32(b"IDAT").to_be_bytes());
    png
}

#[test]
fn a_huge_header_is_too_large_before_decoding() {
    let png = png_header(100_000, 100_000);
    let r = decode(
        Source::Bytes {
            bytes: &png,
            ext: Some("png"),
        },
        &Limits::default(),
    );
    assert!(matches!(r, Err(Error::TooLarge)), "{r:?}");
}

fn png16(w: u32, h: u32) -> Vec<u8> {
    let img = image::ImageBuffer::<Rgba<u16>, Vec<u16>>::new(w, h);
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn max_pixels_alone_bounds_a_decode() {
    let limits = Limits {
        max_pixels: 64,
        ..Limits::default()
    };
    let at = png16(8, 8);
    let img = decode(
        Source::Bytes {
            bytes: &at,
            ext: None,
        },
        &limits,
    )
    .unwrap();
    assert_eq!((img.width, img.height), (8, 8));
    let over = png16(9, 8);
    let r = decode(
        Source::Bytes {
            bytes: &over,
            ext: None,
        },
        &limits,
    );
    assert!(matches!(r, Err(Error::TooLarge)), "{r:?}");
}

#[test]
fn a_file_path_source_reads_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.png");
    std::fs::write(&path, encode(&RgbaImage::new(5, 2), ImageFormat::Png)).unwrap();
    let img = decode(Source::Path(&path), &Limits::default()).unwrap();
    assert_eq!((img.width, img.height), (5, 2));
}

#[test]
fn an_over_large_file_is_too_large() {
    let png = encode(
        &RgbaImage::from_pixel(8, 8, Rgba([1, 2, 3, 4])),
        ImageFormat::Png,
    );
    let limits = Limits {
        max_bytes: 10,
        ..Limits::default()
    };
    let r = decode(
        Source::Bytes {
            bytes: &png,
            ext: None,
        },
        &limits,
    );
    assert!(matches!(r, Err(Error::TooLarge)));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.png");
    std::fs::write(&path, &png).unwrap();
    assert!(matches!(
        decode(Source::Path(&path), &limits),
        Err(Error::TooLarge)
    ));
}
