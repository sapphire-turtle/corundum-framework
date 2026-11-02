use std::path::{Path, PathBuf};
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

    fn frame(&self, preview: &mut ImagePreview, path: &Path) -> Drawn {
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
                    egui::ScrollArea::vertical().show(ui, |ui| preview.ui(ui, path));
                } else {
                    preview.ui(ui, path);
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

    /// Frames of `path` until `done` holds of one, within the deadline; that frame's.
    fn until(
        &self,
        preview: &mut ImagePreview,
        path: &Path,
        mut done: impl FnMut(&ImagePreview, &Drawn) -> bool,
    ) -> Drawn {
        let start = Instant::now();
        loop {
            let drawn = self.frame(preview, path);
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

/// A `width` × `height` PNG named `name` in `dir`.
fn png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let path = dir.join(name);
    ::image::RgbaImage::from_pixel(width, height, ::image::Rgba([200, 40, 40, 255]))
        .save(&path)
        .expect("write the PNG");
    path
}

#[test]
fn a_picture_shows_a_spinner_then_the_image() {
    let dir = tempfile::tempdir().unwrap();
    let path = png(dir.path(), "small.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    let first = screen.frame(&mut preview, &path);
    assert!(first.path, "no spinner: {first:?}");
    assert!(first.textures.is_empty(), "{first:?}");

    let drawn = screen.until(&mut preview, &path, |_, d| d.textures == [[64, 32]]);
    // Not enlarged to fill the 400 × 300 screen.
    assert_eq!(drawn.rects[0].size(), egui::vec2(64.0, 32.0));
}

#[test]
fn in_a_scroll_area_it_fits_the_visible_height() {
    let dir = tempfile::tempdir().unwrap();
    let path = png(dir.path(), "square.png", 1000, 1000);
    let mut screen = Screen::new();
    screen.scroll = true;
    let mut preview = ImagePreview::new();

    let drawn = screen.until(&mut preview, &path, |_, d| !d.textures.is_empty());
    let rect = drawn.rects[0];
    assert!(rect.top() >= 0.0 && rect.bottom() <= 300.0, "{rect:?}");
    assert!((rect.width() - rect.height()).abs() < 0.5, "{rect:?}");
}

#[test]
fn a_newer_selection_wins() {
    let dir = tempfile::tempdir().unwrap();
    let a = png(dir.path(), "a.png", 3000, 3000);
    let b = png(dir.path(), "b.png", 64, 32);
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
fn an_unsupported_file_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.heic");
    let mut bytes = vec![0, 0, 0, 24];
    bytes.extend_from_slice(b"ftypheic");
    bytes.extend_from_slice(&[0; 12]);
    std::fs::write(&path, bytes).unwrap();
    let screen = Screen::new();
    let mut preview = ImagePreview::new();

    screen.until(&mut preview, &path, |_, d| {
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
    let dir = tempfile::tempdir().unwrap();
    let path = png(dir.path(), "small.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();
    screen.frame(&mut preview, &path);
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
fn a_file_is_not_looked_at_every_frame() {
    let dir = tempfile::tempdir().unwrap();
    let path = png(dir.path(), "small.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();
    screen.until(&mut preview, &path, |_, d| d.textures == [[64, 32]]);

    // Gone from under it: the frames that follow (well within the recheck) do not stat the
    // file, as a hung network mount would freeze them, and keep the picture.
    std::fs::remove_file(&path).unwrap();
    for _ in 0..3 {
        screen.frame(&mut preview, &path);
    }
    let drawn = screen.until(&mut preview, &path, |p, _| p.idle());
    assert_eq!(drawn.textures, [[64, 32]], "{drawn:?}");
    let drawn = screen.frame(&mut preview, &path);
    assert_eq!(drawn.textures, [[64, 32]], "{drawn:?}");
}

#[test]
fn a_changed_file_is_shown_anew() {
    let dir = tempfile::tempdir().unwrap();
    let path = png(dir.path(), "small.png", 64, 32);
    let screen = Screen::new();
    let mut preview = ImagePreview::new();
    preview.set_recheck(Duration::ZERO);
    screen.until(&mut preview, &path, |_, d| d.textures == [[64, 32]]);

    png(dir.path(), "small.png", 48, 16);
    screen.until(&mut preview, &path, |_, d| d.textures == [[48, 16]]);
}
