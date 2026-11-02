mod common;

use common::{crc32, encode, jpeg_with_orientation};
use corundum_framework_image::{Limits, Source, decode, thumbnail};
use image::{
    ExtendedColorType, ImageEncoder, ImageFormat, Rgba, RgbaImage, codecs::png::PngEncoder,
};

fn bytes(b: &[u8]) -> Source<'_> {
    Source::Bytes {
        bytes: b,
        ext: None,
    }
}

fn png(w: u32, h: u32) -> Vec<u8> {
    encode(
        &RgbaImage::from_pixel(w, h, Rgba([200, 100, 50, 255])),
        ImageFormat::Png,
    )
}

#[test]
fn the_longest_side_is_shrunk_to_the_size() {
    let data = png(400, 200);
    let t = thumbnail(bytes(&data), 100, &Limits::default()).unwrap();
    assert_eq!((t.width, t.height), (100, 50));
    assert_eq!(t.rgba.len(), 100 * 50 * 4);
}

#[test]
fn a_small_image_is_never_enlarged() {
    let data = png(200, 100);
    let t = thumbnail(bytes(&data), 2048, &Limits::default()).unwrap();
    assert_eq!((t.width, t.height), (200, 100));
    assert_eq!(t, decode(bytes(&data), &Limits::default()).unwrap());
}

#[test]
fn a_portrait_thumbnail_keeps_its_orientation() {
    let data = jpeg_with_orientation(400, 200, 6);
    let t = thumbnail(bytes(&data), 100, &Limits::default()).unwrap();
    assert_eq!((t.width, t.height), (50, 100));
}

#[test]
fn the_short_side_is_at_least_one() {
    let data = png(1000, 2);
    let t = thumbnail(bytes(&data), 10, &Limits::default()).unwrap();
    assert_eq!((t.width, t.height), (10, 1));
}

fn png_with_profile(profile: Option<Vec<u8>>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = PngEncoder::new(&mut out);
    if let Some(p) = profile {
        enc.set_icc_profile(p).unwrap();
    }
    enc.write_image(&[200, 100, 50], 1, 1, ExtendedColorType::Rgb8)
        .unwrap();
    out
}

#[test]
fn a_display_p3_profile_is_converted_to_srgb() {
    let p3 = moxcms::ColorProfile::new_display_p3().encode().unwrap();
    let data = png_with_profile(Some(p3));
    let img = decode(bytes(&data), &Limits::default()).unwrap();
    // P3 is wider than sRGB, so the same numbers are a more saturated sRGB colour.
    assert!(img.rgba[0] > 200, "r = {}", img.rgba[0]);
    assert!(img.rgba[2] < 50, "b = {}", img.rgba[2]);
    assert_eq!(img.rgba[3], 255);

    let plain = decode(bytes(&png_with_profile(None)), &Limits::default()).unwrap();
    assert_eq!(plain.rgba, [200, 100, 50, 255]);
}

#[test]
fn an_unreadable_profile_is_ignored() {
    let mut data = png_with_profile(None);
    // An iCCP chunk (name "x", method 0) whose compressed data is garbage, before IDAT.
    let mut payload = b"x\0\0".to_vec();
    payload.extend_from_slice(&[0xAB; 13]);
    let mut chunk = (payload.len() as u32).to_be_bytes().to_vec();
    let mut body = b"iCCP".to_vec();
    body.extend_from_slice(&payload);
    let crc = crc32(&body);
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc.to_be_bytes());
    let idat = data.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
    data.splice(idat..idat, chunk);
    let img = decode(bytes(&data), &Limits::default()).unwrap();
    assert_eq!(img.rgba, [200, 100, 50, 255]);
}
