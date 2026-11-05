use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_interpret::font::FontQuery;
use hayro::hayro_interpret::hayro_cmap::CidFamily;
use hayro::hayro_syntax::{DecryptionError, LoadPdfError, Pdf};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

use crate::{Error, MAX_INPUT, MAX_PAGES};

const PANICKED: &str = "the PDF renderer panicked";

/// A font for text the PDF does not embed: the app's answer to "a CJK font, please".
///
/// It serves the Adobe-Japan1, GB1, CNS1 and Korea1 character collections. The standard 14
/// fonts (Helvetica and the like) are drawn from the fonts built into hayro.
#[derive(Clone)]
pub struct FallbackFont {
    /// The font file (TrueType, OpenType or a collection).
    pub data: Arc<Vec<u8>>,
    /// The face inside a collection (`.ttc`); 0 for a single font.
    pub index: u32,
}

/// One page rendered: sRGB RGBA8, opaque (it is drawn on white), row-major.
pub struct Page {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// An opened PDF.
///
/// Not meant to cross threads: hayro keeps `Rc` caches, so open and render on the same
/// thread (the gui crate's worker does) and send the [`Page`] back.
pub struct Document {
    pdf: Pdf,
    settings: InterpreterSettings,
}

/// The bytes as hayro wants them, without copying the `Arc<[u8]>`.
struct Bytes(Arc<[u8]>);

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

fn is_cjk(family: &CidFamily) -> bool {
    matches!(
        family,
        CidFamily::AdobeJapan1
            | CidFamily::AdobeGB1
            | CidFamily::AdobeCNS1
            | CidFamily::AdobeKorea1
    )
}

fn settings(cjk: Option<FallbackFont>) -> InterpreterSettings {
    let mut settings = InterpreterSettings::default();
    if let Some(cjk) = cjk {
        settings.font_resolver = Arc::new(move |query| match query {
            FontQuery::Standard(font) => Some(font.get_font_data()),
            FontQuery::Fallback(query) => {
                if query
                    .character_collection
                    .as_ref()
                    .is_some_and(|c| is_cjk(&c.family))
                {
                    let data: Arc<dyn AsRef<[u8]> + Send + Sync> = cjk.data.clone();
                    Some((data, cjk.index))
                } else {
                    Some(query.pick_standard_font().get_font_data())
                }
            }
        });
    }
    settings
}

impl Document {
    /// Open `bytes`, at most [`MAX_INPUT`] of them. `cjk` serves the Adobe-Japan1, GB1, CNS1
    /// and Korea1 fallback queries (text in a CJK font the PDF does not embed); without it
    /// such text is blank. A file with an empty user password opens as it does in a
    /// viewer; one that needs a password is [`Error::Encrypted`]; one with no pages is
    /// [`Error::Broken`].
    pub fn open(bytes: Arc<[u8]>, cjk: Option<FallbackFont>) -> Result<Document, Error> {
        if bytes.len() > MAX_INPUT {
            return Err(Error::TooLarge);
        }
        let opened = catch_unwind(AssertUnwindSafe(|| Pdf::new(Arc::new(Bytes(bytes)))))
            .map_err(|_| Error::Broken(PANICKED.into()))?;
        let pdf = opened.map_err(|e| match e {
            LoadPdfError::Decryption(DecryptionError::PasswordProtected) => Error::Encrypted,
            LoadPdfError::Decryption(other) => {
                Error::Unsupported(format!("the PDF's encryption: {other:?}"))
            }
            LoadPdfError::Invalid => Error::Broken("not a readable PDF".into()),
        })?;
        if pdf.pages().is_empty() {
            return Err(Error::Broken("the PDF has no pages".into()));
        }
        Ok(Document {
            pdf,
            settings: settings(cjk),
        })
    }

    /// How many pages the file has (may exceed [`MAX_PAGES`]).
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Render page `index` (0-based, below `min(page_count, MAX_PAGES)`) to sRGB RGBA8 with
    /// its longest side `longest` pixels, on white. The scale comes from the page's size in
    /// points, so a huge page is never drawn at its own size.
    pub fn render(&self, index: usize, longest: u32) -> Result<Page, Error> {
        if index >= self.page_count().min(MAX_PAGES) {
            return Err(Error::Unsupported(format!("no page {index}")));
        }
        catch_unwind(AssertUnwindSafe(|| self.render_page(index, longest)))
            .unwrap_or_else(|_| Err(Error::Broken(PANICKED.into())))
    }

    fn render_page(&self, index: usize, longest: u32) -> Result<Page, Error> {
        let page = &self.pdf.pages()[index];
        let (w, h) = page.render_dimensions();
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            return Err(Error::Broken("the page has no size".into()));
        }
        // The pixmap is `(points * scale) as u16`: a hair over, so that rounding in f32
        // does not truncate the longest side to one pixel short.
        let longest = longest.clamp(1, u32::from(u16::MAX));
        let scale = longest as f32 / w.max(h) * 1.0001;
        if w.min(h) * scale < 1.0 {
            return Err(Error::Unsupported("the page is too narrow to draw".into()));
        }
        let pixmap = render(
            page,
            &RenderCache::new(),
            &self.settings,
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        // On an opaque white, premultiplied and straight alpha are the same bytes.
        Ok(Page {
            width: u32::from(pixmap.width()),
            height: u32::from(pixmap.height()),
            rgba: pixmap.data_as_u8_slice().to_vec(),
        })
    }
}
