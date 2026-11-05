//! `TextPreview`, drawn without a window.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::text::TextPreview;
use super::{Content, PreviewResponse, State};

/// How long a test waits for the worker before it fails.
const DEADLINE: Duration = Duration::from_secs(10);

/// What a frame drew.
#[derive(Debug)]
struct Drawn {
    /// The text of every label and galley.
    texts: Vec<String>,
    /// Whether a stroked path (the spinner) was drawn.
    path: bool,
    /// Where each text was drawn.
    rects: Vec<(String, egui::Rect)>,
    /// The space the preview took.
    used: egui::Rect,
}

impl Drawn {
    fn has(&self, needle: &str) -> bool {
        self.texts.iter().any(|t| t.contains(needle))
    }
}

fn collect(shape: &egui::Shape, drawn: &mut Drawn) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, drawn)),
        egui::Shape::Text(text) => {
            drawn.texts.push(text.galley.text().to_owned());
            drawn
                .rects
                .push((text.galley.text().to_owned(), text.visual_bounding_rect()));
        }
        egui::Shape::Path(_) => drawn.path = true,
        _ => {}
    }
}

struct Screen {
    ctx: egui::Context,
}

impl Screen {
    fn new() -> Screen {
        let ctx = egui::Context::default();
        crate::icons::install_loaders(&ctx);
        Screen { ctx }
    }

    fn frame(&self, preview: &mut TextPreview, state: State<'_>) -> Drawn {
        self.frame_events(preview, state, Vec::new()).0
    }

    fn frame_events(
        &self,
        preview: &mut TextPreview,
        state: State<'_>,
        events: Vec<egui::Event>,
    ) -> (Drawn, PreviewResponse) {
        let mut response = PreviewResponse::default();
        let mut used = egui::Rect::ZERO;
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
                used = ui.min_rect();
            });
        });
        // Unapplied texture deltas panic on drop; there is no renderer here.
        output.textures_delta.clear();
        let mut drawn = Drawn {
            used,
            texts: Vec::new(),
            path: false,
            rects: Vec::new(),
        };
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut drawn);
        }
        (drawn, response)
    }

    /// Frames of `content` until `done` holds of one, within the deadline; that frame's.
    fn until(
        &self,
        preview: &mut TextPreview,
        content: &Content,
        mut done: impl FnMut(&TextPreview, &Drawn) -> bool,
    ) -> Drawn {
        let start = Instant::now();
        loop {
            let drawn = self.frame(preview, State::Ready(content));
            if done(preview, &drawn) {
                return drawn;
            }
            assert!(start.elapsed() < DEADLINE, "gave up waiting: {drawn:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

/// The pointer-click events on `pos`.
fn click(pos: egui::Pos2) -> Vec<egui::Event> {
    [true, false]
        .into_iter()
        .map(|pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        })
        .collect()
}

#[test]
fn a_text_shows_after_a_spinner() {
    let content = Content::bytes("k", "a.txt", b"hello".to_vec());
    let screen = Screen::new();
    let mut preview = TextPreview::new();

    let first = screen.frame(&mut preview, State::Ready(&content));
    assert!(first.path, "no spinner: {first:?}");
    assert!(!first.has("hello"), "{first:?}");

    screen.until(&mut preview, &content, |_, d| d.has("hello"));
}

#[test]
fn a_cut_file_says_so() {
    let content = Content::bytes("k", "a.txt", vec![b'a'; 256 * 1024 + 10]);
    let screen = Screen::new();
    let mut preview = TextPreview::new();

    screen.until(&mut preview, &content, |_, d| d.has("Showing the first"));
}

#[test]
fn a_whole_file_has_no_note() {
    let content = Content::bytes("k", "a.txt", b"hello".to_vec());
    let screen = Screen::new();
    let mut preview = TextPreview::new();

    let drawn = screen.until(&mut preview, &content, |_, d| d.has("hello"));
    assert!(!drawn.has("Showing the first"), "{drawn:?}");
}

#[test]
fn a_binary_file_cannot_be_previewed() {
    let content = Content::bytes("k", "a.bin", vec![0u8, 1, 2, 0]);
    let screen = Screen::new();
    let mut preview = TextPreview::new();

    screen.until(&mut preview, &content, |_, d| {
        d.has("This file can't be previewed.")
    });
}

#[test]
fn a_reader_that_failed_offers_retry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let content = Content::reader("k", "a.txt", move || {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(std::io::Error::other("offline"))
        } else {
            Ok(Box::new(std::io::Cursor::new(b"hello".to_vec())))
        }
    });
    let screen = Screen::new();
    let mut preview = TextPreview::new();

    screen.until(&mut preview, &content, |_, d| d.has("Retry"));
    let drawn = screen.frame(&mut preview, State::Ready(&content));
    let rect = drawn
        .rects
        .iter()
        .find(|(t, _)| t == "Retry")
        .unwrap_or_else(|| panic!("no Retry: {drawn:?}"))
        .1;
    let (_, response) =
        screen.frame_events(&mut preview, State::Ready(&content), click(rect.center()));
    assert!(response.retry, "Retry did not report");
    screen.until(&mut preview, &content, |_, d| d.has("hello"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn the_text_wraps_inside_the_space_given() {
    let screen = Screen::new();
    let short = Content::bytes("s", "a.txt", b"hello".to_vec());
    let long = Content::bytes("l", "b.txt", vec![b'x'; 200 * 1024]);
    let mut preview = TextPreview::new();
    let one_row = screen.until(&mut preview, &short, |_, d| d.has("hello"));
    let row = one_row.rects.iter().find(|(t, _)| t == "hello").unwrap().1;
    let mut preview = TextPreview::new();
    let many = screen.until(&mut preview, &long, |_, d| d.has("xxxx"));
    for drawn in [&one_row, &many] {
        let used = drawn.used;
        assert!(
            used.width().is_finite() && used.width() <= 400.0,
            "{used:?}"
        );
        assert!(used.height() <= 300.0, "{used:?}");
        for (t, r) in &drawn.rects {
            assert!(r.right() <= 400.5, "{t:.20}: {r:?}");
        }
    }
    // The long line wraps: its text is taller than one row.
    let rect = many
        .rects
        .iter()
        .find(|(t, _)| t.starts_with("xxxx"))
        .unwrap()
        .1;
    assert!(rect.height() > row.height() * 1.5, "{rect:?} vs {row:?}");
}

#[test]
fn a_newer_key_wins() {
    let a = Content::bytes("a", "a.txt", b"text of a".to_vec());
    let b = Content::bytes("b", "b.txt", b"text of b".to_vec());
    let screen = Screen::new();
    let mut preview = TextPreview::new();

    screen.frame(&mut preview, State::Ready(&a));
    // From the moment B is asked for, A's text, however late it comes, is never drawn.
    screen.until(&mut preview, &b, |p, d| {
        assert!(!d.has("text of a"), "{d:?}");
        p.idle() && d.has("text of b")
    });
}

#[test]
fn loading_and_failed_states() {
    let screen = Screen::new();
    let mut preview = TextPreview::new();

    let loading = screen.frame(&mut preview, State::Loading);
    assert!(loading.path, "{loading:?}");

    let failed = screen.frame(&mut preview, State::Failed("offline"));
    assert!(failed.has("offline") && failed.has("Retry"), "{failed:?}");
}

#[test]
fn the_worker_ends_with_the_preview() {
    let content = Content::bytes("k", "a.txt", b"hello".to_vec());
    let screen = Screen::new();
    let mut preview = TextPreview::new();
    screen.until(&mut preview, &content, |_, d| d.has("hello"));
    let token = preview.thread_token().expect("the thread started");
    drop(preview);
    let start = Instant::now();
    while token.upgrade().is_some() {
        assert!(start.elapsed() < DEADLINE, "the thread did not end");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_text_strings_exist_in_both_languages() {
    use crate::i18n::{Language, LanguageSettings, check, tr};

    assert_eq!(
        check::key_difference(&crate::i18n::CORUNDUM, "en", "ja"),
        Vec::<String>::new()
    );
    let ctx = egui::Context::default();
    let mut english = String::new();
    for tag in ["en", "ja"] {
        crate::i18n::install(
            &ctx,
            &LanguageSettings {
                language: Language::Explicit(tag.into()),
            },
        );
        let text = tr(&ctx, "corundum-preview-text-cut");
        assert_ne!(text, "corundum-preview-text-cut", "{tag} lacks it");
        if tag == "en" {
            english = text;
        } else {
            assert_ne!(english, text, "not translated");
        }
    }
}
