//! [`Content`] and the loading and failed drawing of `status_ui`, drawn without a window.

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::content::status_ui;
use super::{Content, PreviewResponse, State};

/// What a frame drew.
#[derive(Default, Debug)]
struct Drawn {
    texts: Vec<(String, egui::Rect)>,
    path: bool,
}

fn collect(shape: &egui::Shape, drawn: &mut Drawn) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, drawn)),
        egui::Shape::Text(text) => drawn
            .texts
            .push((text.galley.text().to_owned(), text.visual_bounding_rect())),
        egui::Shape::Path(_) => drawn.path = true,
        _ => {}
    }
}

/// One frame of `state` on a 400 × 300 screen with `events`: what it drew, and the key of
/// the content `status_ui` gave back.
fn frame(
    ctx: &egui::Context,
    state: State,
    events: Vec<egui::Event>,
    response: &mut PreviewResponse,
) -> (Drawn, Option<String>) {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 300.0),
        )),
        events,
        ..Default::default()
    };
    let mut got = None;
    let mut output = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            got = status_ui(ui, state, response).map(|c| c.key.clone());
        });
    });
    // Unapplied texture deltas panic on drop; there is no renderer here.
    output.textures_delta.clear();
    let mut drawn = Drawn::default();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut drawn);
    }
    (drawn, got)
}

fn read_all(content: &Content) -> Vec<u8> {
    let mut out = Vec::new();
    content.open().unwrap().read_to_end(&mut out).unwrap();
    out
}

#[test]
fn bytes_open_as_a_fresh_cursor_each_time() {
    let content = Content::bytes("k", "a.bin", vec![1u8, 2, 3]);
    assert_eq!(read_all(&content), [1, 2, 3]);
    assert_eq!(read_all(&content), [1, 2, 3]);
}

#[test]
fn a_reader_is_opened_on_each_open() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let content = Content::reader("k", "a.bin", move || {
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(std::io::Cursor::new(vec![7u8])))
    });
    assert_eq!(read_all(&content), [7]);
    assert_eq!(read_all(&content), [7]);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn file_content_reads_the_file_and_takes_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.TXT");
    std::fs::write(&path, b"hello").unwrap();
    let content = Content::file("k", &path);
    assert_eq!(content.name, "a.TXT");
    assert_eq!(content.ext(), Some("txt".to_owned()));
    assert_eq!(read_all(&content), b"hello");
}

#[test]
fn content_is_equal_by_key_only() {
    let a = Content::bytes("k", "a", vec![1u8]);
    assert_eq!(a, Content::bytes("k", "b", vec![2u8]));
    assert_ne!(a, Content::bytes("other", "a", vec![1u8]));
}

#[test]
fn loading_draws_a_spinner_and_no_content() {
    let ctx = egui::Context::default();
    let mut response = PreviewResponse::default();
    let (drawn, got) = frame(&ctx, State::Loading, Vec::new(), &mut response);
    assert_eq!(got, None);
    assert!(drawn.path, "{drawn:?}");
}

#[test]
fn failed_shows_the_reason_and_retry_reports() {
    let ctx = egui::Context::default();
    let state = State::Failed("disk gone");
    let mut response = PreviewResponse::default();
    let (drawn, got) = frame(&ctx, state, Vec::new(), &mut response);
    assert_eq!(got, None);
    assert!(
        drawn.texts.iter().any(|(t, _)| t == "disk gone"),
        "{drawn:?}"
    );
    assert!(!response.retry);

    let at = drawn
        .texts
        .iter()
        .find(|(t, _)| t == "Retry")
        .unwrap_or_else(|| panic!("no Retry: {drawn:?}"))
        .1
        .center();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    let _ = frame(
        &ctx,
        state,
        vec![egui::Event::PointerMoved(at), button(true)],
        &mut response,
    );
    let _ = frame(&ctx, state, vec![button(false)], &mut response);
    assert!(response.retry);
}

#[test]
fn ready_draws_nothing_and_gives_the_content() {
    let ctx = egui::Context::default();
    let mut response = PreviewResponse::default();
    let content = Content::bytes("k", "a", vec![1u8]);
    let (drawn, got) = frame(&ctx, State::Ready(&content), Vec::new(), &mut response);
    assert_eq!(got, Some("k".to_owned()));
    assert!(drawn.texts.is_empty() && !drawn.path, "{drawn:?}");
}

#[test]
fn merging_responses_sets_a_field_if_either_has_it() {
    let retry = PreviewResponse {
        retry: true,
        open: false,
        link: None,
    };
    let open = PreviewResponse {
        retry: false,
        open: true,
        link: None,
    };
    assert_eq!(
        retry.merge(open.clone()),
        PreviewResponse {
            retry: true,
            open: true,
            link: None
        }
    );
    assert_eq!(PreviewResponse::default().merge(open.clone()), open);
}

#[test]
fn the_retry_string_exists_in_both_languages() {
    use crate::i18n::{Language, LanguageSettings, tr};

    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    for tag in ["en", "ja"] {
        crate::i18n::install(
            &ctx,
            &LanguageSettings {
                language: Language::Explicit(tag.into()),
            },
        );
        let text = tr(&ctx, "corundum-preview-retry");
        assert_ne!(text, "corundum-preview-retry", "{tag} lacks it");
        texts.push(text);
    }
    assert_ne!(texts[0], texts[1]);
}

#[test]
fn merge_keeps_a_link() {
    let link = |s: &str| PreviewResponse {
        link: Some(s.into()),
        ..Default::default()
    };
    let merged = link("x").merge(PreviewResponse::default());
    assert_eq!(merged.link.as_deref(), Some("x"));
    let merged = PreviewResponse::default().merge(link("y"));
    assert_eq!(merged.link.as_deref(), Some("y"));
    // Both have one: the right side wins.
    let merged = link("x").merge(link("y"));
    assert_eq!(merged.link.as_deref(), Some("y"));
}
