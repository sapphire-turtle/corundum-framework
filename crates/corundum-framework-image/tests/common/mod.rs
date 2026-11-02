//! Helpers shared by the test files; each uses only some of them.
#![allow(dead_code)]

use std::io::Cursor;

use image::{ImageFormat, Rgba, RgbaImage};

pub fn encode(img: &RgbaImage, format: ImageFormat) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    // JPEG has no alpha channel.
    if format == ImageFormat::Jpeg {
        image::DynamicImage::ImageRgba8(img.clone())
            .to_rgb8()
            .write_to(&mut out, format)
            .unwrap();
    } else {
        img.write_to(&mut out, format).unwrap();
    }
    out.into_inner()
}

pub fn jpeg_with_orientation(w: u32, h: u32, o: u16) -> Vec<u8> {
    jpeg_from(&RgbaImage::from_pixel(w, h, Rgba([10, 200, 30, 255])), o)
}

/// Splice an APP1 Exif segment (Orientation = `o`) after SOI.
pub fn jpeg_from(img: &RgbaImage, o: u16) -> Vec<u8> {
    let jpeg = encode(img, ImageFormat::Jpeg);
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"MM\0\x2A\0\0\0\x08"); // big endian, IFD0 at 8
    tiff.extend_from_slice(&[0, 1]); // one entry
    tiff.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1]); // Orientation, SHORT, 1
    tiff.extend_from_slice(&o.to_be_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // pad the value, no next IFD
    let mut seg = b"Exif\0\0".to_vec();
    seg.extend_from_slice(&tiff);
    let len = (seg.len() + 2) as u16;
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&seg);
    out.extend_from_slice(&jpeg[2..]);
    out
}

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}
