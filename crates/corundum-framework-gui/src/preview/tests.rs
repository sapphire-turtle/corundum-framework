use std::path::Path;
use std::time::{Duration, Instant};

use super::*;

/// How long a test waits for the worker before it fails.
const DEADLINE: Duration = Duration::from_secs(10);

/// What a frame drew.
#[derive(Default, Debug)]
struct Drawn {
    /// The text of every label.
    texts: Vec<String>,
    /// The pixel size of every texture drawn but the font atlas.
    textures: Vec<[usize; 2]>,
    /// Where each of those was drawn, in points.
    rects: Vec<egui::Rect>,
    /// Whether a stroked path (the spinner) was drawn.
    path: bool,
}

/// Frames of a preview filling a 400 × 300 screen.
struct Screen {
    ctx: egui::Context,
    /// Whether the preview is in a vertical scroll area, as in the detail pane.
    scroll: bool,
}

impl Screen {
    fn new() -> Screen {
        let ctx = egui::Context::default();
        crate::icons::install_loaders(&ctx);
        Screen { ctx, scroll: false }
    }

    fn frame(&self, preview: &mut ImagePreview, content: &Content) -> Drawn {
        self.frame_state(preview, State::Ready(content))
    }

    fn frame_state(&self, preview: &mut ImagePreview, state: State<'_>) -> Drawn {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            ..Default::default()
        };
        let mut output = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                if self.scroll {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let _ = preview.ui(ui, state);
                    });
                } else {
                    let _ = preview.ui(ui, state);
                }
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
        drawn
    }

    /// Frames of `content` until `done` holds of one, within the deadline; that frame's.
    fn until(
        &self,
        preview: &mut ImagePreview,
        content: &Content,
        mut done: impl FnMut(&ImagePreview, &Drawn) -> bool,
    ) -> Drawn {
        let start = Instant::now();
        loop {
            let drawn = self.frame(preview, content);
            if done(preview, &drawn) {
                return drawn;
            }
            assert!(start.elapsed() < DEADLINE, "gave up waiting: {drawn:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn collect(shape: &egui::Shape, textures: &egui::epaint::TextureManager, drawn: &mut Drawn) {
    match shape {
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect(shape, textures, drawn);
            }
        }
        egui::Shape::Text(text) => drawn.texts.push(text.galley.text().to_owned()),
        egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
            if let Some(meta) = textures.meta(mesh.texture_id) {
                drawn.textures.push(meta.size);
                drawn.rects.push(mesh.calc_bounds());
            }
        }
        egui::Shape::Path(_) => drawn.path = true,
        _ => {}
    }
}

/// A `width` × `height` PNG named `name` in `dir`, as a content of that file keyed `key`.
fn png(dir: &Path, key: &str, name: &str, width: u32, height: u32) -> Content {
    let path = dir.join(name);
    ::image::RgbaImage::from_pixel(width, height, ::image::Rgba([200, 40, 40, 255]))
        .save(&path)
        .expect("write the PNG");
    Content::file(key, path)
}

/// A `width` × `height` PNG named `name`, as a content of bytes keyed `key`.
fn png_bytes(key: &str, name: &str, width: u32, height: u32) -> Content {
    let mut bytes = Vec::new();
    ::image::RgbaImage::from_pixel(width, height, ::image::Rgba([200, 40, 40, 255]))
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            ::image::ImageFormat::Png,
        )
        .expect("encode the PNG");
    Content::bytes(key, name, bytes)
}

#[test]
fn a_picture_shows_a_spinner_then_the_image() {
    let dir = tempfile::tempdir().unwrap();
    let content = png(dir.path(), "k", "small.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    let first = screen.frame(&mut preview, &content);
    assert!(first.path, "no spinner: {first:?}");
    assert!(first.textures.is_empty(), "{first:?}");

    let drawn = screen.until(&mut preview, &content, |_, d| d.textures == [[64, 32]]);
    // Not enlarged to fill the 400 × 300 screen.
    assert_eq!(drawn.rects[0].size(), egui::vec2(64.0, 32.0));
}

#[test]
fn in_a_scroll_area_it_fits_the_visible_height() {
    let dir = tempfile::tempdir().unwrap();
    let content = png(dir.path(), "k", "square.png", 1000, 1000);
    let mut screen = Screen::new();
    screen.scroll = true;
    let mut preview = ImagePreview::new();

    let drawn = screen.until(&mut preview, &content, |_, d| !d.textures.is_empty());
    let rect = drawn.rects[0];
    assert!(rect.top() >= 0.0 && rect.bottom() <= 300.0, "{rect:?}");
    assert!((rect.width() - rect.height()).abs() < 0.5, "{rect:?}");
}

#[test]
fn a_newer_key_wins() {
    let dir = tempfile::tempdir().unwrap();
    let a = png(dir.path(), "a", "a.png", 3000, 3000);
    let b = png(dir.path(), "b", "b.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    screen.frame(&mut preview, &a);
    // From the moment B is asked for, A's picture, however late it comes, is never drawn.
    screen.until(&mut preview, &b, |p, d| {
        assert!(d.textures.iter().all(|&t| t == [64, 32]), "{d:?}");
        p.idle()
    });
    let drawn = screen.frame(&mut preview, &b);
    assert_eq!(drawn.textures, [[64, 32]]);
}

#[test]
fn bytes_are_decoded_like_a_file() {
    let content = png_bytes("k", "small.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    screen.until(&mut preview, &content, |_, d| d.textures == [[64, 32]]);
}

#[test]
fn same_name_other_key_is_decoded_anew() {
    let one = png_bytes("1", "cover.png", 64, 32);
    let two = png_bytes("2", "cover.png", 32, 64);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    screen.until(&mut preview, &one, |_, d| d.textures == [[64, 32]]);
    screen.until(&mut preview, &two, |_, d| d.textures == [[32, 64]]);
}

#[test]
fn a_changed_key_decodes_again() {
    let dir = tempfile::tempdir().unwrap();
    let v1 = png(dir.path(), "v1", "small.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();
    screen.until(&mut preview, &v1, |_, d| d.textures == [[64, 32]]);

    // The file is rewritten; the app says so by another key for the same path.
    let v2 = png(dir.path(), "v2", "small.png", 48, 16);
    screen.until(&mut preview, &v2, |_, d| d.textures == [[48, 16]]);
}

#[test]
fn a_reader_that_fails_says_why() {
    let content = Content::reader("k", "gone.png", || {
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"))
    });
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    screen.until(&mut preview, &content, |_, d| {
        d.texts
            .iter()
            .any(|t| t.starts_with("Couldn't read this file"))
    });
}

#[test]
fn loading_and_failed_states_draw_no_picture() {
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    let loading = screen.frame_state(&mut preview, State::Loading);
    assert!(loading.path, "no spinner: {loading:?}");
    assert!(loading.textures.is_empty(), "{loading:?}");

    let failed = screen.frame_state(&mut preview, State::Failed("x"));
    assert!(failed.textures.is_empty(), "{failed:?}");
    assert!(failed.texts.iter().any(|t| t == "x"), "{failed:?}");
    assert!(failed.texts.iter().any(|t| t == "Retry"), "{failed:?}");
}

#[test]
fn an_unsupported_file_says_so() {
    let mut bytes = vec![0, 0, 0, 24];
    bytes.extend_from_slice(b"ftypheic");
    bytes.extend_from_slice(&[0; 12]);
    let content = Content::bytes("k", "photo.heic", bytes);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    screen.until(&mut preview, &content, |_, d| {
        d.texts.iter().any(|t| t == "This file can't be previewed.")
    });
}

#[test]
fn the_strings_exist_in_both_languages() {
    use crate::i18n::{Language, LanguageSettings, check, tr};

    assert_eq!(
        check::key_difference(&crate::i18n::CORUNDUM, "en", "ja"),
        Vec::<String>::new()
    );
    let ctx = egui::Context::default();
    let ids = [
        "corundum-preview-unsupported",
        "corundum-preview-too-large",
        "corundum-preview-failed",
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
            } else {
                assert!(!english.contains(&text), "{id} is not translated");
            }
        }
    }
    assert_eq!(english[0], "This file can't be previewed.");
}

#[test]
fn the_thread_ends_with_the_preview() {
    let content = png_bytes("k", "small.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();
    screen.frame(&mut preview, &content);
    let token = preview.thread_token().expect("the thread started");
    drop(preview);
    let start = Instant::now();
    while token.upgrade().is_some() {
        assert!(
            start.elapsed() < DEADLINE,
            "the thread outlived the preview"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn the_target_size_rounds_up_to_a_step() {
    use super::image::target;
    assert_eq!(target(egui::vec2(100.0, 50.0), 1.0), 256);
    assert_eq!(target(egui::vec2(200.0, 300.0), 1.0), 512);
    assert_eq!(target(egui::vec2(300.0, 200.0), 2.0), 1024);
    assert_eq!(target(egui::vec2(1024.0, 10.0), 1.0), 1024);
    assert_eq!(target(egui::vec2(3000.0, 2000.0), 1.0), 2048);
    assert_eq!(target(egui::vec2(1500.0, 800.0), 3.0), 2048);
}

#[test]
fn the_default_limits_suit_a_phone() {
    use corundum_framework_image::Limits;
    let preview = ImagePreview::new();
    assert_eq!(
        preview.limits(),
        Limits::default().with_max_pixels(64_000_000)
    );
}

#[test]
fn a_picture_over_the_limits_says_so() {
    use corundum_framework_image::Limits;
    let content = png_bytes("k", "small.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new().with_limits(Limits::default().with_max_pixels(100));

    screen.until(&mut preview, &content, |_, d| {
        assert!(d.textures.is_empty(), "{d:?}");
        d.texts
            .iter()
            .any(|t| t == "This image is too large to preview.")
    });
}

#[test]
fn a_panic_while_decoding_fails_once() {
    // The worker panics on this name, past the decoder's own guard (see `image::decode`).
    let content = png_bytes("k", super::image::PANIC_NAME, 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    screen.until(&mut preview, &content, |_, d| {
        d.texts
            .iter()
            .any(|t| t.starts_with("Couldn't read this file:") && t.contains("panicked"))
    });
    for _ in 0..5 {
        screen.frame(&mut preview, &content);
        std::thread::sleep(Duration::from_millis(5));
    }
    screen.until(&mut preview, &content, |p, _| p.idle());
    assert_eq!(super::image::panics(), 1);
}
