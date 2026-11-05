# hayro 0.9 against the PDFs the issue names (#98)

Investigation for the PDF preview plan. Fixtures are in
`crates/corundum-framework-pdf/tests/fixtures/` (see its README). Everything below was run
with `hayro 0.9.0` (`hayro-syntax`/`hayro-interpret` 0.9.0, `vello_cpu` 0.3.0), Rust 1.98
release build on Windows 11, page rendered at 1024 px wide (scale = 1024 / page width in pt).

## Result in one line

hayro renders every fixture the plan needs. The one gap is text in a CJK font that is not
embedded: it renders with hayro's default resolver as blank glyphs, and renders correctly when
the resolver answers `FontQuery::Fallback` with a system Japanese font.

## What each fixture shows

- `ja-embedded.pdf` (Noto Sans JP subset): renders correctly, all four lines, kanji, kana and
  Latin digits. Nothing wrong. Needs no font resolver.
- `ja-not-embedded.pdf` (Adobe-Japan1, `90ms-RKSJ-H`, no font file):
  - With the default resolver (`embed-fonts`, Foxit): the page is mostly blank. Only the
    half-width Latin characters and digits show, in Foxit Sans. All kanji and kana are missing.
    No error, no panic, no warning from the render call itself.
  - With a resolver that answers `FontQuery::Fallback` for `CidFamily::AdobeJapan1` with Yu
    Gothic Medium (`C:\Windows\Fonts\YuGothM.ttc`, face 0) or Meiryo (`meiryo.ttc`, face 0):
    renders correctly, all four lines. The query arrives as `post_script_name =
    Some("HeiseiKakuGo-W5-90ms-RKSJ-H")`, `font_name = None`, `character_collection =
    Some(Adobe-Japan1)`. (hayro's doc comment on `FontQuery::Fallback` still says it is "not
    supported"; it is, as of 0.9.)
  - The wide gaps around Latin characters in both renders are a property of the fixture, not
    of hayro: it has `/DW 1000` and no `/W` array, so every glyph is full-width.
- `scan.pdf` (JPEG only, DeviceGray DCTDecode): renders correctly, the image fills the page
  (1024 x 1449 px). The JPEG is coarse because it is compressed hard (15 KB), not because of
  the renderer.
- `encrypted.pdf` (RC4 128-bit, empty user password): `Pdf::new(data)` succeeds and the page
  renders with its text decrypted. Nothing to ask the user. See "Encryption" below.
- `two-pages.pdf`: `pdf.pages().len() == 2`. Page 0 reports `render_dimensions() = (595.0,
  842.0)` and page 1 `(612.0, 792.0)`, so the sizes differ per page and come through. At
  1024 px wide they are 1024 x 1449 and 1024 x 1325 px. Helvetica (not embedded) renders
  with the Foxit Sans substitute. Frame and text are correct.

## Encryption

- Empty user password: opens and renders (above). `Pdf::new` is `new_with_password(data, "")`.
- A password that does not match: `Pdf::new_with_password(data, "x")` returns
  `Err(LoadPdfError::Decryption(DecryptionError::PasswordProtected))`. So a file that really
  needs a password gives `LoadPdfError::Decryption(PasswordProtected)` from `Pdf::new`.
- `DecryptionError` has `MissingIDEntry`, `PasswordProtected`, `InvalidEncryption`,
  `UnsupportedAlgorithm`. `LoadPdfError` has `Decryption(DecryptionError)` and `Invalid`.
- Observed: the owner password ("owner") is not accepted as a password for this file;
  `new_with_password(data, "owner")` also gave `PasswordProtected`. Irrelevant for an empty
  user password, but do not promise owner-password support.

## Robustness

- Truncated files (cut at 10, 25, 50, 75, 90, 99 percent) of four fixtures, plus empty input,
  `%PDF-1.4\ngarbage`, and 1000 zero bytes, were run under `catch_unwind` through `Pdf::new`,
  `pages()` and `render`. **No panic anywhere.** Outcomes are `Err(LoadPdfError::Invalid)`
  or a successful load with fewer pages. Two cases to handle in Task 2:
  - a truncated file can load with **zero pages** (`two-pages.pdf` cut at 25 percent); treat
    `pages().len() == 0` as an error ("no pages");
  - a truncated file can load with **fewer pages than it should**; hayro rebuilds the xref and
    shows what it finds, without saying so.
- Still run the load and render inside `catch_unwind` or on a worker whose panic is caught,
  since a fuzz-style input might exist that this small sample did not find. The sample was
  six cut points on four files, not a fuzz run.

## API names Task 2 should use

Dependencies: `hayro = "0.9"` (re-exports `hayro_interpret`, `hayro_syntax`, `kurbo`,
`vello_cpu`). Features: `embed-fonts` and `embed-cmaps`, both on by default.

```rust
use hayro::hayro_syntax::Pdf;                          // Pdf, LoadPdfError live in hayro_syntax
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_interpret::font::{FontQuery, FontData, StandardFont};
use hayro::hayro_interpret::hayro_cmap::CidFamily;
use hayro::{render, RenderCache, RenderSettings, PixmapSettings};
use hayro::vello_cpu::color::palette::css::WHITE;
```

- Load: `Pdf::new(data: impl Into<PdfData>) -> Result<Pdf, LoadPdfError>`. A `Vec<u8>` and
  an `Arc<Vec<u8>>` both work as `PdfData` (the examples pass a `Vec<u8>`). With a password:
  `Pdf::new_with_password(data, password: &str)`.
- Pages: `pdf.pages()` returns `&Pages`; `.len()`, `.iter()` (yielding `&Page`). Page size in
  points: `page.render_dimensions() -> (f32, f32)` (accounts for `/Rotate` and crop box).
  Also `page.media_box()`, `page.crop_box()`, `page.rotation()`.
- Render one page:
  `hayro::render(page: &Page, cache: &RenderCache, interp: &InterpreterSettings,
  render: &RenderSettings, pixmap: &PixmapSettings) -> vello_cpu::Pixmap`.
  Output pixel size is `(w * x_scale) as u16` by `(h * y_scale) as u16`, so it is capped at
  65535 px per side and **truncates**; compute the scale from the wanted width
  (`scale = target_px / w_pt`) and clamp it.
  `PixmapSettings { x_scale, y_scale, bg_color }`; use `bg_color: WHITE` for a page, since
  `Default` is `TRANSPARENT`.
  `RenderCache::new()` should be kept per document and reused for its pages.
- Pixel format: `Pixmap` is **premultiplied RGBA8**. Read it with `pixmap.data_as_u8_slice()`
  (`&[u8]`, 4 bytes per pixel, row-major), `pixmap.width()` and `pixmap.height()` (`u16`).
  `pixmap.take_rgba8(ImageAlphaType::..)` converts to straight alpha if needed.
  With an opaque `bg_color` (WHITE) premultiplied and straight are identical, and egui's
  `ColorImage::from_rgba_premultiplied([w, h], bytes)` is the right constructor in any case.
  `pixmap.into_png()` exists for debugging.
- Font resolver: `InterpreterSettings { font_resolver: Arc<dyn Fn(FontQuery) ->
  Option<(FontData, u32)> + Send + Sync>, cmap_resolver, warning_sink, render_content }`.
  `FontData = Arc<dyn AsRef<[u8]> + Send + Sync>`, and the `u32` is the face index inside a
  `.ttc` (0 for single fonts). `FontQuery::Standard(StandardFont)` and
  `FontQuery::Fallback(FallbackFontQuery)`; the latter has `character_collection:
  Option<CharacterCollection { family: CidFamily, .. }>`, `post_script_name`, `font_name`,
  `font_family`, `is_bold`, `is_italic`, `is_serif`, `is_fixed_pitch`, `font_weight`, and
  `pick_standard_font()`. `StandardFont::get_font_data()` (only with `embed-fonts`) gives the
  Foxit font. `CidFamily` variants seen: `AdobeJapan1`, `AdobeGB1`, `AdobeCNS1`,
  `AdobeKorea1`.
  `InterpreterSettings::default()` with `embed-fonts` answers both queries from Foxit; without
  the feature it answers `None` to everything.

## Send / Sync

Compile-time checked: `Pdf`, `InterpreterSettings` and `Pixmap` are all `Send + Sync`.
`RenderCache<'a>` holds `Rc<RefCell<..>>` internally, so it is **not** `Send` (the compiler rejects it). `Page<'a>` is `Send` (checked) but
borrows from the `Pdf`. Consequence for Task 2: a worker thread must own the `Pdf` and its
`RenderCache` and create both on that thread (or hold them together there). Send the `Pdf`'s
bytes (`Vec<u8>`) to the worker, not a `Pdf` plus `Page` borrows. Send only the finished
`Pixmap` (or its bytes) back. `Pdf` itself may be moved into the thread before it starts
rendering.

## What `embed-fonts` adds

- Size: it embeds the 14 Foxit Type 1 programs from `hayro-font` (`assets/*.pfb`), about
  290 KB of raw font data. Measured on the throwaway binary: release exe 10,247,168 bytes with
  the feature, 9,974,272 without, a difference of about 273 KB.
- Coverage: only the 14 PDF standard fonts (Helvetica, Times, Courier families, Symbol,
  ZapfDingbats) as metric-compatible Foxit look-alikes, i.e. Latin. **No CJK, no Cyrillic or
  Greek beyond what Foxit covers.** The `FontQuery::Fallback` default only calls
  `pick_standard_font()`, so any non-embedded CJK font falls to Foxit Sans and loses all
  its glyphs (verified above).
- Without the feature, the default resolver returns `None`; a PDF that does not embed
  Helvetica (very common for generated PDFs, e.g. `two-pages.pdf`) then shows frames and
  images but **no text** (verified: page 2 renders only the frame).
- `embed-cmaps` is separate and is also needed for `ja-not-embedded.pdf`: the predefined
  `90ms-RKSJ-H` CMap is loaded through it. It was on in every run here and its size was not
  measured separately.

## Recommendation for the defaults

1. Keep `embed-fonts` on in `corundum-framework-pdf` and `embed-cmaps` on. 273 KB buys
   readable text for every PDF that relies on the standard 14 fonts, which is the common
   case; turning it off turns those pages into frames.
2. Provide a resolver that handles `Fallback` for CJK families from the system fonts, with
   the Foxit font as the last resort. Ask for the family in this order of preference per OS:
   Windows `YuGothM.ttc` / `meiryo.ttc` (index 0), macOS Hiragino, Linux Noto Sans CJK;
   then `pick_standard_font()`. On any miss, return the Foxit default instead of `None`, so
   Latin text still renders. This should be a documented hook (a `font_resolver` or a
   "fallback fonts" option), not hard-coded, so an app can ship its own CJK font. It should be on by default:
   leaving it off would leave the ja-not-embedded case blank.
3. Do not embed a CJK font in the crate: Noto Sans JP is 9.5 MB, a CJK subset is still
   several MB, and almost every real PDF with Japanese text embeds its font (as
   `ja-embedded.pdf` does).
4. Call `Pdf::new` for the empty-password case and surface
   `LoadPdfError::Decryption(PasswordProtected)` as a distinct "password protected" state;
   surface `Invalid` as "cannot read this PDF"; treat zero pages as an error.
5. Render at the width the pane needs, scale from `render_dimensions()`, `WHITE` background,
   and read `data_as_u8_slice()` as premultiplied RGBA8.
