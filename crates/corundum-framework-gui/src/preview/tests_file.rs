//! The kind of a file, and the info card, drawn without a window.

use std::time::{Duration, SystemTime};

use super::{FileInfo, InfoCard, Kind, PreviewResponse, kind_of};
use crate::explorer::Entry;

/// What a frame drew.
#[derive(Default, Debug)]
struct Drawn {
    texts: Vec<(String, egui::Rect)>,
    image: bool,
}

fn collect(shape: &egui::Shape, drawn: &mut Drawn) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, drawn)),
        egui::Shape::Text(text) => drawn
            .texts
            .push((text.galley.text().to_owned(), text.visual_bounding_rect())),
        egui::Shape::Mesh(_) => drawn.image = true,
        _ => {}
    }
}

impl Drawn {
    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|(t, _)| t == text)
    }
}

/// One frame of `card` on a 400 × 300 screen with `events`.
fn frame(
    ctx: &egui::Context,
    card: impl Fn() -> InfoCard<'static>,
    events: Vec<egui::Event>,
    response: &mut PreviewResponse,
) -> Drawn {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 300.0),
        )),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            *response = response.merge(card().ui(ui));
        });
    });
    // Unapplied texture deltas panic on drop; there is no renderer here.
    output.textures_delta.clear();
    let mut drawn = Drawn::default();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut drawn);
    }
    drawn
}

/// A card over a file that lives for the whole test run.
fn leaked(info: FileInfo) -> &'static FileInfo {
    Box::leak(Box::new(info))
}

#[test]
fn kinds_come_from_the_extension() {
    assert_eq!(kind_of("a.JPG"), Kind::Image);
    assert_eq!(kind_of("b.flac"), Kind::Audio);
    assert_eq!(kind_of("c.mkv"), Kind::Video);
    assert_eq!(kind_of("d.md"), Kind::Text);
    assert_eq!(kind_of("e.zip"), Kind::Other);
    assert_eq!(kind_of("noext"), Kind::Other);
    assert_eq!(kind_of("f.svg"), Kind::Image);
}

#[test]
fn the_card_shows_the_known_rows_only() {
    let ctx = egui::Context::default();
    let file = leaked(FileInfo {
        size: Some(1536),
        ..FileInfo::new("a.zip")
    });
    let mut response = PreviewResponse::default();
    let drawn = frame(&ctx, || InfoCard::new(file), Vec::new(), &mut response);
    assert!(drawn.has("a.zip"), "{drawn:?}");
    assert!(drawn.has("1.5 KB"), "{drawn:?}");
    assert!(drawn.has("ZIP file"), "{drawn:?}");
    assert!(!drawn.has("Modified") && !drawn.has("Created"), "{drawn:?}");

    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(86_400);
    let file = leaked(FileInfo {
        modified: Some(at),
        created: Some(at),
        ..FileInfo::new("noext")
    });
    let drawn = frame(&ctx, || InfoCard::new(file), Vec::new(), &mut response);
    assert!(drawn.has("Modified") && drawn.has("Created"), "{drawn:?}");
    assert!(drawn.has("1970-01-02 00:00"), "{drawn:?}");
    assert!(drawn.has("File"), "{drawn:?}");
    assert!(!drawn.has("Size"), "{drawn:?}");
}

#[test]
fn the_open_button_is_the_app_s_choice() {
    let ctx = egui::Context::default();
    let file = leaked(FileInfo::new("a.zip"));
    let mut response = PreviewResponse::default();
    let drawn = frame(&ctx, || InfoCard::new(file), Vec::new(), &mut response);
    assert!(!drawn.has("Open with the system's app"), "{drawn:?}");

    let card = || InfoCard::new(file).open_button(true);
    let drawn = frame(&ctx, card, Vec::new(), &mut response);
    let at = drawn
        .texts
        .iter()
        .find(|(t, _)| t == "Open with the system's app")
        .unwrap_or_else(|| panic!("no button: {drawn:?}"))
        .1
        .center();
    assert!(!response.open);
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    let _ = frame(
        &ctx,
        card,
        vec![egui::Event::PointerMoved(at), button(true)],
        &mut response,
    );
    let _ = frame(&ctx, card, vec![button(false)], &mut response);
    assert!(response.open);
}

#[test]
fn the_footer_has_no_icon() {
    let ctx = egui::Context::default();
    let file = leaked(FileInfo {
        size: Some(1536),
        icon: Some(crate::icons::file()),
        ..FileInfo::new("a.zip")
    });
    let mut response = PreviewResponse::default();
    let drawn = frame(
        &ctx,
        || InfoCard::new(file).footer(),
        Vec::new(),
        &mut response,
    );
    assert!(!drawn.image, "{drawn:?}");
    assert!(!drawn.has("a.zip"), "{drawn:?}");
    assert!(drawn.has("1.5 KB"), "{drawn:?}");
}

#[test]
fn file_info_from_an_entry() {
    let info = FileInfo::from(&Entry::file("x.png").size(3));
    assert_eq!(info.name, "x.png");
    assert_eq!(info.size, Some(3));
    assert!(info.icon.is_some());
    assert_eq!(info.created, None);
}

#[test]
fn the_card_strings_exist_in_both_languages() {
    use crate::i18n::{Language, LanguageSettings, tr, tr_args};

    const KEYS: [&str; 6] = [
        "corundum-preview-size",
        "corundum-preview-kind",
        "corundum-preview-modified",
        "corundum-preview-created",
        "corundum-preview-open",
        "corundum-preview-kind-unknown",
    ];
    const KINDS: [&str; 5] = ["image", "audio", "video", "text", "other"];
    let ctx = egui::Context::default();
    let mut all: [Vec<String>; 2] = Default::default();
    for (i, tag) in ["en", "ja"].into_iter().enumerate() {
        crate::i18n::install(
            &ctx,
            &LanguageSettings {
                language: Language::Explicit(tag.into()),
            },
        );
        for key in KEYS {
            let text = tr(&ctx, key);
            assert_ne!(text, key, "{tag} lacks it");
            all[i].push(text);
        }
        for kind in KINDS {
            let key = format!("corundum-preview-kind-{kind}");
            let text = tr_args(&ctx, &key, &[("ext", "PNG".into())]);
            assert!(text.contains("PNG"), "{tag} {key}: {text}");
            all[i].push(text);
        }
    }
    for (en, ja) in all[0].iter().zip(&all[1]) {
        assert_ne!(en, ja);
    }
}
