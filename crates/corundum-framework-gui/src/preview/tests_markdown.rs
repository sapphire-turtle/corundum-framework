//! `MarkdownPreview`, drawn without a window.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use super::text::MarkdownPreview;
use super::{Content, PreviewResponse, State};

/// How long a test waits for the worker before it fails.
const DEADLINE: Duration = Duration::from_secs(10);

/// What a frame drew.
#[derive(Debug, Default)]
struct Drawn {
    /// Each text drawn, with where and the height of its first row.
    texts: Vec<(String, egui::Rect, Vec<f32>)>,
}

impl Drawn {
    fn has(&self, needle: &str) -> bool {
        self.texts.iter().any(|(t, ..)| t.contains(needle))
    }

    fn find(&self, needle: &str) -> &(String, egui::Rect, Vec<f32>) {
        self.texts
            .iter()
            .find(|(t, ..)| t.contains(needle))
            .unwrap_or_else(|| panic!("no {needle:?}: {self:?}"))
    }

    fn row_heights(&self, needle: &str) -> Vec<f32> {
        self.find(needle).2.clone()
    }

    fn rect(&self, needle: &str) -> egui::Rect {
        self.find(needle).1
    }
}

fn collect(shape: &egui::Shape, drawn: &mut Drawn) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, drawn)),
        egui::Shape::Text(text) => {
            let row = text.galley.rows.iter().map(|r| r.height()).collect();
            drawn.texts.push((
                text.galley.text().to_owned(),
                text.visual_bounding_rect(),
                row,
            ));
        }
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

    fn frame(&self, preview: &mut MarkdownPreview, state: State<'_>) -> Drawn {
        self.frame_events(preview, state, Vec::new()).0
    }

    fn frame_events(
        &self,
        preview: &mut MarkdownPreview,
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
        output.textures_delta.clear();
        let mut drawn = Drawn::default();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut drawn);
        }
        (drawn, response)
    }

    /// Frames of `content` until `done` holds of one, within the deadline; that frame's.
    fn until(
        &self,
        preview: &mut MarkdownPreview,
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
}

/// The pointer-click events on `pos`.
fn click(pos: egui::Pos2) -> Vec<egui::Event> {
    let mut events = vec![egui::Event::PointerMoved(pos)];
    events.extend(
        [true, false]
            .into_iter()
            .map(|pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }),
    );
    events
}

#[test]
fn a_note_shows_formatted() {
    let content = Content::bytes("k", "a.md", b"# Title\n\nSome **bold** text".to_vec());
    let screen = Screen::new();
    let mut preview = MarkdownPreview::new();

    let drawn = screen.until(&mut preview, &content, |d| d.has("Title") && d.has("Some"));
    // One galley holds the note: the heading's row is taller than the body's last.
    let rows = drawn.row_heights("Title");
    assert!(
        rows.first().unwrap() > &(rows.last().unwrap() * 1.1),
        "{drawn:?}"
    );
}

#[test]
fn a_clicked_link_is_reported() {
    let content = Content::bytes("k", "a.md", b"[corundum](https://example.com)".to_vec());
    let screen = Screen::new();
    let mut preview = MarkdownPreview::new();

    screen.until(&mut preview, &content, |d| d.has("corundum"));
    // A frame with no click reports nothing.
    let (drawn, response) = screen.frame_events(&mut preview, State::Ready(&content), vec![]);
    assert_eq!(response.link, None);
    let rect = drawn.rect("corundum");
    let (_, response) =
        screen.frame_events(&mut preview, State::Ready(&content), click(rect.center()));
    assert_eq!(response.link.as_deref(), Some("https://example.com"));
    let (_, response) = screen.frame_events(&mut preview, State::Ready(&content), vec![]);
    assert_eq!(response.link, None);
}

#[test]
fn a_reference_link_is_reported_resolved() {
    let content = Content::bytes("k", "a.md", b"[x][r]\n\n[r]: https://e.com".to_vec());
    let screen = Screen::new();
    let mut preview = MarkdownPreview::new();

    let drawn = screen.until(&mut preview, &content, |d| d.has("x"));
    let rect = drawn.rect("x");
    let (_, response) = screen.frame_events(
        &mut preview,
        State::Ready(&content),
        click(rect.min + egui::vec2(6.0, 8.0)),
    );
    assert_eq!(response.link.as_deref(), Some("https://e.com"));
}

#[test]
fn an_empty_link_does_not_panic() {
    let content = Content::bytes("k", "a.md", b"[x]()".to_vec());
    let screen = Screen::new();
    let mut preview = MarkdownPreview::new();

    let drawn = screen.until(&mut preview, &content, |d| d.has("x"));
    let rect = drawn.rect("x");
    let (_, response) =
        screen.frame_events(&mut preview, State::Ready(&content), click(rect.center()));
    assert_eq!(response.link.as_deref(), Some(""));
}

#[test]
fn the_preview_is_read_only() {
    let content = Content::bytes("k", "a.md", b"hello".to_vec());
    let screen = Screen::new();
    let mut preview = MarkdownPreview::new();

    let drawn = screen.until(&mut preview, &content, |d| d.has("hello"));
    // Focus the editor with a click on its text, then type.
    let _ = screen.frame_events(
        &mut preview,
        State::Ready(&content),
        click(drawn.rect("hello").center()),
    );
    let (drawn, response) = screen.frame_events(
        &mut preview,
        State::Ready(&content),
        vec![egui::Event::Text("x".into())],
    );
    assert!(drawn.has("hello"), "{drawn:?}");
    assert!(!drawn.has("hellox") && !drawn.has("xhello"), "{drawn:?}");
    assert!(
        !drawn.texts.iter().any(|(t, ..)| t.contains('x')),
        "{drawn:?}"
    );
    assert_eq!(response, PreviewResponse::default());
}

#[test]
fn a_cut_note_with_an_open_fence_still_shows() {
    let mut text = String::from("```\n");
    text.push_str(&"a\n".repeat(256 * 1024));
    let content = Content::bytes("k", "a.md", text.into_bytes());
    let screen = Screen::new();
    let mut preview = MarkdownPreview::new();

    let drawn = screen.until(&mut preview, &content, |d| d.has("Showing the first"));
    assert!(drawn.has("a"), "{drawn:?}");
}

#[test]
fn a_binary_file_cannot_be_previewed() {
    let content = Content::bytes("k", "a.md", vec![0u8, 1, 2, 0]);
    let screen = Screen::new();
    let mut preview = MarkdownPreview::new();

    screen.until(&mut preview, &content, |d| {
        d.has("This file can't be previewed.")
    });
}

#[test]
fn a_reader_that_failed_offers_retry() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let content = Content::reader("k", "a.md", move || {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            Err(std::io::Error::other("offline"))
        } else {
            Ok(Box::new(std::io::Cursor::new(b"hello".to_vec())))
        }
    });
    let screen = Screen::new();
    let mut preview = MarkdownPreview::new();

    screen.until(&mut preview, &content, |d| d.has("Retry"));
    let drawn = screen.frame(&mut preview, State::Ready(&content));
    let rect = drawn.rect("Retry");
    let (_, response) =
        screen.frame_events(&mut preview, State::Ready(&content), click(rect.center()));
    assert!(response.retry, "Retry did not report");
    screen.until(&mut preview, &content, |d| d.has("hello"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
