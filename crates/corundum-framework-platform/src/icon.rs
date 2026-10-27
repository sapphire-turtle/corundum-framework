//! An app's SVG icon rendered to the formats the desktop OSes want: PNG, raw RGBA for a
//! window icon, and the icns and ico containers.

use std::io;

use resvg::{tiny_skia, usvg};

/// The icns entry types and the pixel size each holds.
const ICNS_ENTRIES: [(&[u8; 4], u32); 7] = [
    (b"icp4", 16),
    (b"icp5", 32),
    (b"icp6", 64),
    (b"ic07", 128),
    (b"ic08", 256),
    (b"ic09", 512),
    (b"ic10", 1024),
];

/// The pixel sizes an ico file holds.
const ICO_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

/// Rasterizes `svg` into a `size` by `size` pixmap, scaled to fit and centered, on transparency.
fn render(svg: &[u8], size: u32) -> io::Result<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default()).map_err(invalid)?;
    let mut pixmap = tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| invalid("an icon must be at least one pixel wide"))?;
    let (w, h) = (tree.size().width(), tree.size().height());
    let scale = size as f32 / w.max(h);
    let transform = tiny_skia::Transform::from_translate(
        (size as f32 - w * scale) / 2.0,
        (size as f32 - h * scale) / 2.0,
    )
    .pre_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(pixmap)
}

/// Renders `svg` to a square PNG of `size` pixels on a transparent background.
pub fn render_png(svg: &[u8], size: u32) -> io::Result<Vec<u8>> {
    render(svg, size)?.encode_png().map_err(invalid)
}

/// Renders `svg` to straight (not premultiplied) RGBA8 pixels, row-major, `size * size * 4` bytes.
pub fn render_rgba(svg: &[u8], size: u32) -> io::Result<Vec<u8>> {
    let pixmap = render(svg, size)?;
    Ok(pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect())
}

/// Builds an icns file (macOS) holding `svg` as PNG entries from 16 to 1024 pixels.
pub fn icns(svg: &[u8]) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    for (ty, size) in ICNS_ENTRIES {
        let png = render_png(svg, size)?;
        body.extend_from_slice(ty);
        body.extend_from_slice(&(8 + png.len() as u32).to_be_bytes());
        body.extend_from_slice(&png);
    }
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&(8 + body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Builds an ico file (Windows) holding `svg` as PNG images from 16 to 256 pixels.
pub fn ico(svg: &[u8]) -> io::Result<Vec<u8>> {
    let pngs = ICO_SIZES
        .iter()
        .map(|&s| render_png(svg, s))
        .collect::<io::Result<Vec<_>>>()?;
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(ICO_SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * ICO_SIZES.len() as u32;
    for (&size, png) in ICO_SIZES.iter().zip(&pngs) {
        // A width or height of 256 is written as 0.
        let dim = if size >= 256 { 0 } else { size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for png in &pngs {
        out.extend_from_slice(png);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" fill="#1f4fbf"/></svg>"##;
    const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

    fn be32(b: &[u8]) -> u32 {
        u32::from_be_bytes(b[..4].try_into().unwrap())
    }

    #[test]
    fn a_png_has_the_asked_size() {
        let png = render_png(SVG, 48).unwrap();
        assert_eq!(&png[..8], PNG_MAGIC);
        assert_eq!(be32(&png[16..20]), 48);
        assert_eq!(be32(&png[20..24]), 48);
    }

    #[test]
    fn an_unparsable_svg_is_invalid_data() {
        assert_eq!(
            render_png(b"<nope", 16).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn rgba_is_straight_and_sized() {
        let px = render_rgba(SVG, 8).unwrap();
        assert_eq!(px.len(), 8 * 8 * 4);
        assert_eq!(&px[..4], &[0x1f, 0x4f, 0xbf, 0xff]);
    }

    #[test]
    fn a_wide_svg_is_centered_and_fitted() {
        let wide = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10"><rect width="20" height="10" fill="#000"/></svg>"##;
        let px = render_rgba(wide, 20).unwrap();
        let alpha = |x: usize, y: usize| px[(y * 20 + x) * 4 + 3];
        assert_eq!(alpha(10, 0), 0);
        assert_eq!(alpha(10, 10), 255);
        assert_eq!(alpha(10, 19), 0);
    }

    #[test]
    fn the_icns_lists_every_size() {
        let f = icns(SVG).unwrap();
        assert_eq!(&f[0..4], b"icns");
        assert_eq!(be32(&f[4..8]) as usize, f.len());
        let mut at = 8;
        let mut seen = Vec::new();
        while at < f.len() {
            let ty = std::str::from_utf8(&f[at..at + 4]).unwrap().to_string();
            let len = be32(&f[at + 4..]) as usize;
            let data = &f[at + 8..at + len];
            assert_eq!(&data[..8], PNG_MAGIC);
            seen.push((ty, be32(&data[16..20])));
            at += len;
        }
        assert_eq!(at, f.len());
        let want: Vec<(String, u32)> = [
            ("icp4", 16),
            ("icp5", 32),
            ("icp6", 64),
            ("ic07", 128),
            ("ic08", 256),
            ("ic09", 512),
            ("ic10", 1024),
        ]
        .iter()
        .map(|(t, s)| (t.to_string(), *s))
        .collect();
        assert_eq!(seen, want);
    }

    #[test]
    fn the_ico_lists_every_size() {
        let f = ico(SVG).unwrap();
        assert_eq!(&f[0..4], &[0, 0, 1, 0]);
        assert_eq!(u16::from_le_bytes([f[4], f[5]]), 6);
        let mut widths = Vec::new();
        for i in 0..6 {
            let e = &f[6 + i * 16..6 + (i + 1) * 16];
            widths.push(e[0]);
            assert_eq!(e[1], e[0]);
            assert_eq!(u16::from_le_bytes([e[4], e[5]]), 1);
            assert_eq!(u16::from_le_bytes([e[6], e[7]]), 32);
            let len = u32::from_le_bytes(e[8..12].try_into().unwrap()) as usize;
            let off = u32::from_le_bytes(e[12..16].try_into().unwrap()) as usize;
            assert_eq!(&f[off..off + 8], PNG_MAGIC);
            if i == 5 {
                assert_eq!(off + len, f.len());
            }
        }
        assert_eq!(widths, [16, 24, 32, 48, 64, 0]);
    }
}
