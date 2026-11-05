use std::time::{Duration, Instant};

use super::*;

const TWO_PAGES: &[u8] =
    include_bytes!("../../../corundum-framework-pdf/tests/fixtures/two-pages.pdf");
const PASSWORD: &[u8] =
    include_bytes!("../../../corundum-framework-pdf/tests/fixtures/password.pdf");

/// How long a test waits for the worker before it fails.
const DEADLINE: Duration = Duration::from_secs(20);

/// What a frame drew.
#[derive(Default, Debug)]
struct Drawn {
    /// The text of every label.
    texts: Vec<String>,
    /// Where each of those was drawn, in points.
    text_rects: Vec<egui::Rect>,
    /// The pixel size of every texture drawn but the font atlas.
    textures: Vec<[usize; 2]>,
    /// Whether a stroked path (the spinner) was drawn.
    path: bool,
}

impl Drawn {
    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|t| t.contains(text))
    }

    /// Where the text containing `text` was drawn.
    fn rect_of(&self, text: &str) -> egui::Rect {
        let at = self.texts.iter().position(|t| t.contains(text));
        self.text_rects[at.unwrap_or_else(|| panic!("no {text:?}: {self:?}"))]
    }
}

fn collect(shape: &egui::Shape, textures: &egui::epaint::TextureManager, drawn: &mut Drawn) {
    match shape {
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect(shape, textures, drawn);
            }
        }
        egui::Shape::Text(text) => {
            drawn.texts.push(text.galley.text().to_owned());
            drawn.text_rects.push(text.visual_bounding_rect());
        }
        egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
            if let Some(meta) = textures.meta(mesh.texture_id) {
                drawn.textures.push(meta.size);
            }
        }
        egui::Shape::Path(_) => drawn.path = true,
        _ => {}
    }
}

/// Frames of a preview filling a 400 x 300 screen.
struct Screen {
    ctx: egui::Context,
}

impl Screen {
    fn new() -> Screen {
        let ctx = egui::Context::default();
        crate::icons::install_loaders(&ctx);
        Screen { ctx }
    }

    fn frame(&self, preview: &mut PdfPreview, state: State<'_>) -> Drawn {
        self.frame_events(preview, state, Vec::new()).0
    }

    fn frame_events(
        &self,
        preview: &mut PdfPreview,
        state: State<'_>,
        events: Vec<egui::Event>,
    ) -> (Drawn, PreviewResponse) {
        let mut response = PreviewResponse::default();
        let input = egui::RawInput {
            events,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            ..Default::default()
        };
        let mut output = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                response = preview.ui(ui, state);
            });
        });
        // Unapplied texture deltas panic on drop; there is no renderer here.
        output.textures_delta.clear();
        let mut drawn = Drawn::default();
        let textures = self.ctx.tex_manager();
        let textures = textures.read();
        for clipped in &output.shapes {
            collect(&clipped.shape, &textures, &mut drawn);
        }
        (drawn, response)
    }

    /// Frames of `content` until `done` holds of one, within the deadline; that frame's.
    fn until(
        &self,
        preview: &mut PdfPreview,
        content: &Content,
        mut done: impl FnMut(&Drawn) -> bool,
    ) -> Drawn {
        let start = Instant::now();
        loop {
            let drawn = self.frame(preview, State::Ready(content));
            if done(&drawn) {
                return drawn;
            }
            assert!(start.elapsed() < DEADLINE, "gave up waiting: {drawn:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Click at `pos`: move there, press, release, each in a frame of its own.
    fn click(
        &self,
        preview: &mut PdfPreview,
        content: &Content,
        pos: egui::Pos2,
    ) -> PreviewResponse {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let state = State::Ready(content);
        let _ = self.frame_events(preview, state, vec![egui::Event::PointerMoved(pos)]);
        let _ = self.frame_events(preview, state, vec![button(true)]);
        self.frame_events(preview, state, vec![button(false)]).1
    }
}

fn two_pages(key: &str) -> Content {
    Content::bytes(key, "two.pdf", TWO_PAGES)
}

/// Where the page buttons are, from the label: one either side of it.
fn previous_at(label: egui::Rect) -> egui::Pos2 {
    egui::pos2(label.left() - 16.0, label.center().y)
}

fn next_at(label: egui::Rect) -> egui::Pos2 {
    egui::pos2(label.right() + 16.0, label.center().y)
}

#[test]
fn the_first_page_shows_after_a_spinner() {
    let content = two_pages("k");
    let screen = Screen::new();
    let mut preview = PdfPreview::new();

    let first = screen.frame(&mut preview, State::Ready(&content));
    assert!(first.path, "no spinner: {first:?}");
    assert!(first.textures.is_empty(), "{first:?}");

    let drawn = screen.until(&mut preview, &content, |d| !d.textures.is_empty());
    assert!(drawn.has("1 / 2"), "{drawn:?}");
}

#[test]
fn next_shows_the_second_page() {
    let content = two_pages("k");
    let screen = Screen::new();
    let mut preview = PdfPreview::new();
    let first = screen.until(&mut preview, &content, |d| {
        !d.textures.is_empty() && d.has("1 / 2")
    });

    let _ = screen.click(&mut preview, &content, next_at(first.rect_of("1 / 2")));
    let drawn = screen.until(&mut preview, &content, |d| {
        d.has("2 / 2") && !d.textures.is_empty() && !d.path
    });
    // US Letter, 612 x 792 points: portrait.
    let [w, h] = drawn.textures[0];
    assert!(
        (w as f32 / h as f32 - 612.0 / 792.0).abs() < 0.01,
        "{w} x {h}"
    );
}

#[test]
fn the_page_buttons_stop_at_the_ends() {
    let content = two_pages("k");
    let screen = Screen::new();
    let mut preview = PdfPreview::new();
    let first = screen.until(&mut preview, &content, |d| {
        !d.textures.is_empty() && d.has("1 / 2")
    });

    // Previous on the first page does nothing.
    let _ = screen.click(&mut preview, &content, previous_at(first.rect_of("1 / 2")));
    let still = screen.frame(&mut preview, State::Ready(&content));
    assert!(still.has("1 / 2"), "{still:?}");

    let _ = screen.click(&mut preview, &content, next_at(first.rect_of("1 / 2")));
    let last = screen.until(&mut preview, &content, |d| d.has("2 / 2") && !d.path);
    // Next on the last page does nothing.
    let _ = screen.click(&mut preview, &content, next_at(last.rect_of("2 / 2")));
    let still = screen.frame(&mut preview, State::Ready(&content));
    assert!(still.has("2 / 2"), "{still:?}");
}

#[test]
fn a_new_file_starts_on_its_first_page() {
    let a = two_pages("a");
    let b = two_pages("b");
    let screen = Screen::new();
    let mut preview = PdfPreview::new();
    let first = screen.until(&mut preview, &a, |d| {
        d.has("1 / 2") && !d.textures.is_empty()
    });
    let _ = screen.click(&mut preview, &a, next_at(first.rect_of("1 / 2")));
    screen.until(&mut preview, &a, |d| d.has("2 / 2") && !d.path);

    screen.until(&mut preview, &b, |d| {
        assert!(!d.has("2 / 2"), "{d:?}");
        d.has("1 / 2") && !d.textures.is_empty()
    });
}

#[test]
fn an_encrypted_pdf_says_so() {
    let content = Content::bytes("k", "locked.pdf", PASSWORD);
    let screen = Screen::new();
    let mut preview = PdfPreview::new();

    let drawn = screen.until(&mut preview, &content, |d| {
        d.has("This PDF is password-protected and can't be previewed.")
    });
    assert!(drawn.textures.is_empty(), "{drawn:?}");
    assert!(!drawn.has("Retry"), "{drawn:?}");
}

#[test]
fn a_broken_pdf_offers_retry() {
    let content = Content::bytes("k", "cut.pdf", &TWO_PAGES[..TWO_PAGES.len() / 10]);
    let screen = Screen::new();
    let mut preview = PdfPreview::new();

    let drawn = screen.until(&mut preview, &content, |d| {
        d.has("Couldn't read this PDF: ")
    });
    let retry = drawn.rect_of("Retry").center();
    let response = screen.click(&mut preview, &content, retry);
    assert!(response.retry);
}

#[test]
fn a_reader_over_the_limit_is_too_large() {
    let content = Content::reader("k", "huge.pdf", || {
        let zeroes = vec![0u8; corundum_framework_pdf::MAX_INPUT + 1];
        Ok(Box::new(std::io::Cursor::new(zeroes)) as Box<dyn Reader>)
    });
    let screen = Screen::new();
    let mut preview = PdfPreview::new();
    screen.until(&mut preview, &content, |d| {
        d.has("This PDF is too large to preview.")
    });
}

#[test]
fn loading_and_failed_states() {
    let screen = Screen::new();
    let mut preview = PdfPreview::new();
    let loading = screen.frame(&mut preview, State::Loading);
    assert!(loading.path, "{loading:?}");

    let failed = screen.frame(&mut preview, State::Failed("x"));
    assert!(failed.has("x") && failed.has("Retry"), "{failed:?}");
}

#[test]
fn the_pdf_strings_exist_in_both_languages() {
    use crate::i18n::{Language, LanguageSettings, check, tr};

    assert_eq!(
        check::key_difference(&crate::i18n::CORUNDUM, "en", "ja"),
        Vec::<String>::new()
    );
    let ctx = egui::Context::default();
    let ids = [
        "corundum-preview-pdf-page",
        "corundum-preview-pdf-page-of",
        "corundum-preview-pdf-previous",
        "corundum-preview-pdf-next",
        "corundum-preview-pdf-encrypted",
        "corundum-preview-pdf-broken",
        "corundum-preview-pdf-too-large",
    ];
    let mut english = Vec::new();
    for tag in ["en", "ja"] {
        crate::i18n::install(
            &ctx,
            &LanguageSettings {
                language: Language::Explicit(tag.into()),
            },
        );
        for id in ids {
            let text = tr(&ctx, id);
            assert_ne!(text, id, "{tag} lacks {id}");
            if tag == "en" {
                english.push(text);
            } else if id != "corundum-preview-pdf-page" {
                // "1 / 2" reads the same in both.
                assert!(!english.contains(&text), "{id} is not translated");
            }
        }
    }
    assert_eq!(english[2], "Previous page");
}
