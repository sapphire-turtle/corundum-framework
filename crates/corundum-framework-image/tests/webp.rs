#![cfg(feature = "webp")]

use corundum_framework_image::{Image, Limits, Source, decode, encode_webp};

fn gradient(w: u32, h: u32) -> Image {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            rgba.extend_from_slice(&[(x * 4) as u8, (y * 8) as u8, ((x + y) * 3) as u8, 255]);
        }
    }
    Image::new(w, h, rgba).unwrap()
}

#[test]
fn a_webp_decodes_back_to_its_size() {
    let out = encode_webp(&gradient(64, 32), 80.0).unwrap();
    assert_eq!(&out[..4], b"RIFF");
    assert_eq!(&out[8..12], b"WEBP");
    let back = decode(
        Source::Bytes {
            bytes: &out,
            ext: None,
        },
        &Limits::default(),
    )
    .unwrap();
    assert_eq!((back.width, back.height), (64, 32));
}

#[test]
fn a_lower_quality_is_smaller() {
    let img = gradient(64, 32);
    let low = encode_webp(&img, 10.0).unwrap();
    let high = encode_webp(&img, 90.0).unwrap();
    assert!(low.len() < high.len(), "{} vs {}", low.len(), high.len());
}

#[test]
fn a_buffer_of_the_wrong_length_is_an_error() {
    let mut img = gradient(4, 4);
    img.rgba.pop();
    assert!(encode_webp(&img, 80.0).is_err());
}
