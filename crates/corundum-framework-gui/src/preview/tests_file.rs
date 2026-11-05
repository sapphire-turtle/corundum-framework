//! The kind of a file, and the info card, drawn without a window.

use std::time::{Duration, SystemTime};

use super::{FileInfo, FilePreview, InfoCard, Kind, PreviewResponse, State, kind_of};
use crate::explorer::Entry;

/// What a frame drew.
#[derive(Default, Debug)]
struct Drawn {
    texts: Vec<(String, egui::Rect)>,
    /// Each text's row heights, in step with `texts`.
    rows: Vec<Vec<f32>>,
    image: bool,
    /// Where each textured rect was drawn.
    images: Vec<egui::Rect>,
    /// Whether a stroked path (the spinner) was drawn.
    path: bool,
}

fn collect(shape: &egui::Shape, drawn: &mut Drawn) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, drawn)),
        egui::Shape::Text(text) => {
            drawn
                .texts
                .push((text.galley.text().to_owned(), text.visual_bounding_rect()));
            drawn
                .rows
                .push(text.galley.rows.iter().map(|r| r.height()).collect());
        }
        egui::Shape::Mesh(mesh) => {
            drawn.image = true;
            drawn.images.push(mesh.calc_bounds());
        }
        egui::Shape::Rect(rect) if rect.brush.is_some() => {
            drawn.image = true;
            drawn.images.push(rect.visual_bounding_rect());
        }
        egui::Shape::Path(_) => drawn.path = true,
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
            *response = std::mem::take(response).merge(card().ui(ui));
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
    crate::icons::install_loaders(&ctx);
    let file = leaked(FileInfo {
        size: Some(1536),
        icon: Some(crate::icons::file()),
        ..FileInfo::new("a.zip")
    });
    let mut response = PreviewResponse::default();
    // The icon loads over a frame or two: draw the full card until it shows.
    let mut full = Drawn::default();
    for _ in 0..5 {
        full = frame(&ctx, || InfoCard::new(file), Vec::new(), &mut response);
    }
    assert!(full.image, "positive control: {full:?}");
    let mut bare = Drawn::default();
    for _ in 0..5 {
        bare = frame(
            &ctx,
            || InfoCard::new(file).icon(false),
            Vec::new(),
            &mut response,
        );
    }
    assert!(!bare.image && bare.has("a.zip"), "{bare:?}");
    let mut drawn = Drawn::default();
    for _ in 0..5 {
        drawn = frame(
            &ctx,
            || InfoCard::new(file).footer(),
            Vec::new(),
            &mut response,
        );
    }
    assert!(!drawn.image, "{drawn:?}");
    assert!(drawn.has("a.zip"), "{drawn:?}");
    // A row is one line: its label and value are one text.
    assert!(drawn.has("Size: 1.5 KB"), "{drawn:?}");
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

impl Drawn {
    fn rect(&self, text: &str) -> Option<egui::Rect> {
        self.texts.iter().find(|(t, _)| t == text).map(|(_, r)| *r)
    }
}

/// One frame of whatever `add` draws, on a 400 x 300 screen with `events`.
fn frame_ui(
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    mut add: impl FnMut(&mut egui::Ui),
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
        egui::CentralPanel::default().show(ui, |ui| add(ui));
    });
    output.textures_delta.clear();
    let mut drawn = Drawn::default();
    for clipped in &output.shapes {
        collect(&clipped.shape, &mut drawn);
    }
    drawn
}

#[test]
fn an_unknown_file_needs_no_content() {
    let ctx = egui::Context::default();
    let file = leaked(FileInfo::new("a.zip"));
    let mut preview = FilePreview::new();
    assert!(!preview.wants_content("a.zip"));
    let drawn = frame_ui(&ctx, Vec::new(), |ui| {
        let _ = preview.ui(ui, file, State::Loading, false);
    });
    assert!(drawn.has("a.zip"), "{drawn:?}");
    assert!(!drawn.path, "a spinner: {drawn:?}");
}

#[cfg(feature = "image-preview")]
fn png_content() -> super::Content {
    let mut bytes = Vec::new();
    ::image::RgbaImage::from_pixel(64, 32, ::image::Rgba([200, 40, 40, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            ::image::ImageFormat::Png,
        )
        .expect("encode the PNG");
    super::Content::bytes("k", "a.png", bytes)
}

#[cfg(feature = "image-preview")]
#[test]
fn an_image_shows_its_picture_and_details_under_it() {
    let ctx = egui::Context::default();
    crate::icons::install_loaders(&ctx);
    let content = png_content();
    let file = leaked(FileInfo {
        size: Some(10),
        ..FileInfo::new("a.png")
    });
    let mut preview = FilePreview::new();
    assert!(preview.wants_content("a.png"));
    let start = std::time::Instant::now();
    loop {
        let drawn = frame_ui(&ctx, Vec::new(), |ui| {
            let _ = preview.ui(ui, file, State::Ready(&content), false);
        });
        if let Some(picture) = drawn.images.first() {
            let size = drawn.rect("Size").expect("the Size label");
            assert!(size.top() > picture.bottom(), "{drawn:?}");
            assert!(drawn.has("a.png"), "{drawn:?}");
            return;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "{drawn:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[cfg(feature = "image-preview")]
#[test]
fn a_failed_image_fetch_offers_retry() {
    let ctx = egui::Context::default();
    let file = leaked(FileInfo::new("a.png"));
    let mut preview = FilePreview::new();
    let mut response = PreviewResponse::default();
    let drawn = frame_ui(&ctx, Vec::new(), |ui| {
        response = std::mem::take(&mut response).merge(preview.ui(
            ui,
            file,
            State::Failed("offline"),
            false,
        ));
    });
    assert!(drawn.has("offline"), "{drawn:?}");
    let at = drawn.rect("Retry").expect("a Retry button").center();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    };
    for events in [
        vec![egui::Event::PointerMoved(at), button(true)],
        vec![button(false)],
    ] {
        let _ = frame_ui(&ctx, events, |ui| {
            response = std::mem::take(&mut response).merge(preview.ui(
                ui,
                file,
                State::Failed("offline"),
                false,
            ));
        });
    }
    assert!(response.retry);
}

#[cfg(feature = "audio-preview")]
fn fake_player() -> std::sync::Arc<corundum_framework_audio::Player<super::Content>> {
    std::sync::Arc::new(
        corundum_framework_audio::Player::spawn(
            Box::new(super::ContentOpener),
            Box::new(corundum_framework_audio::fakes::FakeOutputs::default()),
            || {},
        )
        .expect("spawn the player"),
    )
}

#[cfg(feature = "audio-preview")]
#[test]
fn audio_without_a_player_is_an_info_card() {
    let ctx = egui::Context::default();
    crate::icons::install_loaders(&ctx);
    let content = super::Content::bytes("k", "a.wav", Vec::new());
    let file = leaked(FileInfo::new("a.wav"));
    let mut preview = FilePreview::new();
    assert!(!preview.wants_content("a.wav"));
    let drawn = frame_ui(&ctx, Vec::new(), |ui| {
        let _ = preview.ui(ui, file, State::Ready(&content), false);
    });
    assert!(drawn.has("a.wav"), "{drawn:?}");
    assert!(drawn.has("Kind"), "{drawn:?}");
    assert!(!drawn.has("Play"), "{drawn:?}");
}

#[cfg(feature = "audio-preview")]
#[test]
fn audio_with_a_player_shows_play() {
    let ctx = egui::Context::default();
    crate::icons::install_loaders(&ctx);
    let content = silent_wav();
    let file = leaked(FileInfo::new("a.wav"));
    let mut preview = FilePreview::new().with_player(fake_player());
    assert!(preview.wants_content("a.wav"));
    let drawn = frame_ui(&ctx, Vec::new(), |ui| {
        let _ = preview.ui(ui, file, State::Ready(&content), false);
    });
    assert!(drawn.has("Play"), "{drawn:?}");
    assert!(drawn.has("Kind"), "{drawn:?}");
}

#[test]
fn replace_is_the_app_s_view_and_the_footer() {
    let ctx = egui::Context::default();
    crate::icons::install_loaders(&ctx);
    let file = leaked(FileInfo {
        size: Some(1536),
        icon: Some(crate::icons::file()),
        ..FileInfo::new("a.md")
    });
    let mut drawn = Drawn::default();
    for _ in 0..5 {
        drawn = frame_ui(&ctx, Vec::new(), |ui| {
            ui.label("NOTES");
            let _ = InfoCard::new(file).footer().ui(ui);
        });
    }
    let notes = drawn.rect("NOTES").expect("the app's view");
    let size = drawn.rect("Size: 1.5 KB").expect("the footer");
    assert!(notes.bottom() <= size.top(), "{drawn:?}");
    assert!(!drawn.image, "{drawn:?}");
}

#[cfg(feature = "image-preview")]
#[test]
fn add_puts_the_app_s_section_above_the_default() {
    let ctx = egui::Context::default();
    crate::icons::install_loaders(&ctx);
    let content = png_content();
    let file = leaked(FileInfo::new("a.png"));
    let mut preview = FilePreview::new();
    let start = std::time::Instant::now();
    loop {
        let drawn = frame_ui(&ctx, Vec::new(), |ui| {
            ui.label("TAGS");
            let _ = preview.ui(ui, file, State::Ready(&content), false);
        });
        if let Some(picture) = drawn.images.first() {
            let tags = drawn.rect("TAGS").expect("the app's section");
            assert!(tags.bottom() <= picture.top(), "{drawn:?}");
            return;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "{drawn:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A short silent 16-bit mono WAV.
#[cfg(feature = "audio-preview")]
fn silent_wav() -> super::Content {
    let data_len = 8_000u32 * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&8_000u32.to_le_bytes());
    bytes.extend_from_slice(&16_000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    bytes.resize(bytes.len() + data_len as usize, 0);
    super::Content::bytes("k", "a.wav", bytes)
}

#[cfg(feature = "image-preview")]
#[test]
fn an_image_the_decoder_cannot_read_is_not_fetched() {
    let preview = FilePreview::new();
    for name in ["a.svg", "a.heic", "a.avif", "a.bmp"] {
        assert!(!preview.wants_content(name), "{name}");
        assert_eq!(kind_of(name), Kind::Image, "{name}");
    }
    for name in ["a.png", "a.JPG", "a.webp", "a.gif"] {
        assert!(preview.wants_content(name), "{name}");
    }
}

#[cfg(feature = "text-preview")]
#[test]
fn text_files_want_content() {
    let preview = FilePreview::new();
    assert!(preview.wants_content("a.cue"));
    assert!(preview.wants_content("B.LOG"));
    assert!(preview.wants_content("x.md"));
    assert!(!preview.wants_content("a.docx"));
    assert_eq!(
        preview.wants_content("a.pdf"),
        cfg!(feature = "pdf-preview")
    );
}

#[cfg(feature = "text-preview")]
#[test]
fn a_text_file_shows_its_text_and_details() {
    let ctx = egui::Context::default();
    let content = super::Content::bytes("k", "notes.txt", b"hi".to_vec());
    let file = leaked(FileInfo {
        size: Some(2),
        ..FileInfo::new("notes.txt")
    });
    let mut preview = FilePreview::new();
    let start = std::time::Instant::now();
    loop {
        let drawn = frame_ui(&ctx, Vec::new(), |ui| {
            let _ = preview.ui(ui, file, State::Ready(&content), false);
        });
        if let (Some(text), Some(size)) = (drawn.rect("hi"), drawn.rect("Size")) {
            assert!(size.top() > text.bottom(), "{drawn:?}");
            return;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "{drawn:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[cfg(feature = "text-preview")]
#[test]
fn a_ts_file_is_a_video_not_text() {
    // MPEG-TS recordings are multi-GB videos; they must not be fetched as text.
    assert!(!FilePreview::new().wants_content("rec.ts"));
}

/// The row heights of the first text drawn that contains `needle`, polling until it shows.
#[cfg(feature = "markdown-preview")]
fn rows_of(name: &str, needle: &str) -> Vec<f32> {
    let ctx = egui::Context::default();
    let content = super::Content::bytes("k", name, b"# Hi\n\nbody".to_vec());
    let file = leaked(FileInfo::new(name));
    let mut preview = FilePreview::new();
    let start = std::time::Instant::now();
    loop {
        let drawn = frame_ui(&ctx, Vec::new(), |ui| {
            let _ = preview.ui(ui, file, State::Ready(&content), false);
        });
        if let Some(i) = drawn.texts.iter().position(|(t, _)| t.contains(needle)) {
            return drawn.rows[i].clone();
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "{drawn:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[cfg(feature = "markdown-preview")]
#[test]
fn markdown_files_go_to_the_markdown_preview() {
    for name in ["a.md", "b.MARKDOWN"] {
        let rows = rows_of(name, "Hi");
        assert!(
            rows.first().unwrap() > &(rows.last().unwrap() * 1.1),
            "{name} {rows:?}"
        );
    }
    let rows = rows_of("a.txt", "Hi");
    assert!(
        (rows.first().unwrap() - rows.last().unwrap()).abs() < 0.5,
        "{rows:?}"
    );
}

#[cfg(feature = "pdf-preview")]
#[test]
fn a_pdf_wants_content_and_shows_a_page() {
    let ctx = egui::Context::default();
    crate::icons::install_loaders(&ctx);
    let content = super::Content::bytes(
        "k",
        "tickets.pdf",
        include_bytes!("../../../corundum-framework-pdf/tests/fixtures/two-pages.pdf").to_vec(),
    );
    let file = leaked(FileInfo {
        size: Some(10),
        ..FileInfo::new("tickets.pdf")
    });
    let mut preview = FilePreview::new();
    assert!(preview.wants_content("a.PDF"));
    let start = std::time::Instant::now();
    loop {
        let drawn = frame_ui(&ctx, Vec::new(), |ui| {
            let _ = preview.ui(ui, file, State::Ready(&content), false);
        });
        if let (Some(page), Some(count)) = (drawn.images.first(), drawn.rect("1 / 2")) {
            let size = drawn.rect("Size").expect("the Size label");
            assert!(size.top() > page.bottom(), "{drawn:?}");
            assert!(size.top() > count.bottom(), "{drawn:?}");
            return;
        }
        assert!(
            start.elapsed() < std::time::Duration::from_secs(20),
            "{drawn:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
