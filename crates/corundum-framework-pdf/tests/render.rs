//! Rendering, against the fixtures in `tests/fixtures` (see its README).

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use corundum_framework_pdf::{Document, Error, FallbackFont, MAX_INPUT, Page};

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn open(name: &str) -> Document {
    Document::open(fixture(name).into(), None).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Pixels that are not white, in rows `rows` of the page.
fn ink(page: &Page, rows: std::ops::Range<u32>) -> usize {
    let width = page.width as usize;
    page.rgba
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(i, _)| rows.contains(&((i / width) as u32)))
        .filter(|(_, px)| px[..3] != [255, 255, 255])
        .count()
}

fn all_ink(page: &Page) -> usize {
    ink(page, 0..page.height)
}

#[test]
fn two_pages_are_counted_and_sized() {
    let doc = open("two-pages.pdf");
    assert_eq!(doc.page_count(), 2);

    let a4 = doc.render(0, 512).unwrap();
    assert_eq!(a4.width.max(a4.height), 512);
    assert_eq!(a4.rgba.len(), a4.width as usize * a4.height as usize * 4);
    let aspect = a4.width as f32 / a4.height as f32;
    assert!((aspect - 595.0 / 842.0).abs() < 0.01, "{aspect}");

    let letter = doc.render(1, 512).unwrap();
    assert_eq!(letter.width.max(letter.height), 512);
    let aspect = letter.width as f32 / letter.height as f32;
    assert!((aspect - 612.0 / 792.0).abs() < 0.01, "{aspect}");
}

#[test]
fn a_page_past_the_end_is_an_error() {
    let doc = open("two-pages.pdf");
    assert!(matches!(doc.render(2, 256), Err(Error::Unsupported(_))));
}

#[test]
fn the_page_is_opaque() {
    let page = open("two-pages.pdf").render(0, 256).unwrap();
    assert!(page.rgba.as_chunks::<4>().0.iter().all(|px| px[3] == 255));
}

#[test]
fn text_draws_ink() {
    let page = open("two-pages.pdf").render(0, 512).unwrap();
    assert!(ink(&page, 0..page.height / 2) > 100);
}

#[test]
fn embedded_japanese_draws_ink() {
    let page = open("ja-embedded.pdf").render(0, 512).unwrap();
    assert!(all_ink(&page) > 100);
}

#[test]
fn a_scan_renders() {
    let page = open("scan.pdf").render(0, 512).unwrap();
    assert!(all_ink(&page) > 1000);
}

#[test]
fn a_password_protected_pdf_says_so() {
    let result = Document::open(fixture("password.pdf").into(), None);
    assert!(matches!(result, Err(Error::Encrypted)));
}

#[test]
fn an_encrypted_pdf_with_no_password_renders() {
    let page = open("encrypted.pdf").render(0, 512).unwrap();
    assert!(all_ink(&page) > 100);
}

#[test]
fn a_truncated_pdf_is_broken() {
    let bytes = fixture("two-pages.pdf");
    let cut =
        |percent: usize| -> Arc<[u8]> { bytes[..bytes.len() * percent / 100].to_vec().into() };
    // Cut early, nothing is left to read; at 25 percent hayro loads it with no pages.
    assert!(matches!(
        Document::open(cut(10), None),
        Err(Error::Broken(_))
    ));
    assert!(matches!(
        Document::open(cut(25), None),
        Err(Error::Broken(_))
    ));
    // Cut in the middle, hayro rebuilds what it finds: fewer pages, and no panic.
    let half = Document::open(cut(50), None).unwrap();
    assert!(half.page_count() < 2);
    half.render(0, 256).unwrap();
    for percent in [1, 40, 60, 75, 90, 99] {
        if let Ok(doc) = Document::open(cut(percent), None) {
            let _ = doc.render(0, 256);
        }
    }
}

#[test]
fn not_a_pdf_is_broken() {
    let result = Document::open(b"hello".to_vec().into(), None);
    assert!(matches!(result, Err(Error::Broken(_))));
}

#[test]
fn too_large_input_is_refused() {
    let result = Document::open(vec![0; MAX_INPUT + 1].into(), None);
    assert!(matches!(result, Err(Error::TooLarge)));
}

#[test]
fn a_huge_page_renders_within_the_limit() {
    // A 5 m square page with one filled rectangle.
    let pdf = "%PDF-1.4\n\
1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n\
3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 14173 14173] /Contents 4 0 R >>\nendobj\n\
4 0 obj\n<< /Length 33 >>\nstream\n0 0 0 rg 1000 1000 8000 8000 re f\nendstream\nendobj\n\
trailer\n<< /Root 1 0 R /Size 5 >>\n%%EOF\n";
    let start = Instant::now();
    let doc = Document::open(pdf.as_bytes().to_vec().into(), None).unwrap();
    let page = doc.render(0, 2048).unwrap();
    assert_eq!((page.width, page.height), (2048, 2048));
    assert!(all_ink(&page) > 0);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "{:?}",
        start.elapsed()
    );
}

/// A Japanese system font, where the machine has one (as the gui crate's font tests look).
fn system_cjk() -> Option<FallbackFont> {
    let candidates: &[&str] = &[
        r"C:\Windows\Fonts\YuGothM.ttc",
        r"C:\Windows\Fonts\meiryo.ttc",
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    ];
    candidates.iter().find_map(|path| {
        let data = std::fs::read(path).ok()?;
        Some(FallbackFont {
            data: Arc::new(data),
            index: 0,
        })
    })
}

#[test]
fn not_embedded_japanese_uses_the_fallback() {
    let without = Document::open(fixture("ja-not-embedded.pdf").into(), None)
        .unwrap()
        .render(0, 512)
        .unwrap();
    let Some(font) = system_cjk() else {
        eprintln!("no Japanese system font here: the fallback is not checked");
        return;
    };
    let with = Document::open(fixture("ja-not-embedded.pdf").into(), Some(font))
        .unwrap()
        .render(0, 512)
        .unwrap();
    assert!(
        all_ink(&with) > all_ink(&without),
        "{} against {}",
        all_ink(&with),
        all_ink(&without)
    );
}
