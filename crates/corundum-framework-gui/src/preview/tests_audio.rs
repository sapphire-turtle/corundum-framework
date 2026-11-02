//! [`AudioPreview`] drawn without a window, playing on a fake device.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use corundum_framework_audio::fakes::FakeOutputs;
use corundum_framework_audio::{FileOpener, Player, Status};

use super::AudioPreview;

/// How long a test waits for the probe before it fails.
const DEADLINE: Duration = Duration::from_secs(10);

/// How long a test waits for a press of Play to show in the player: as long as for the
/// probe, since a busy CI runner can take seconds to start the decoder.
const PLAYS_WITHIN: Duration = DEADLINE;

/// What a frame drew.
#[derive(Default, Debug)]
struct Drawn {
    /// The text of every label, and where it was drawn.
    texts: Vec<(String, egui::Rect)>,
    /// Whether a stroked path (the spinner) was drawn.
    path: bool,
}

impl Drawn {
    fn has(&self, text: &str) -> bool {
        self.texts.iter().any(|(t, _)| t == text)
    }

    fn starts(&self, prefix: &str) -> bool {
        self.texts.iter().any(|(t, _)| t.starts_with(prefix))
    }

    fn rect(&self, text: &str) -> Option<egui::Rect> {
        self.texts.iter().find(|(t, _)| t == text).map(|(_, r)| *r)
    }
}

/// Frames of a preview filling a 400 × 300 screen.
struct Screen {
    ctx: egui::Context,
}

impl Screen {
    fn new() -> Screen {
        let ctx = egui::Context::default();
        crate::icons::install_loaders(&ctx);
        Screen { ctx }
    }

    fn frame(&self, preview: &mut AudioPreview, path: &Path, player: &Player<PathBuf>) -> Drawn {
        self.frame_with(preview, path, player, Vec::new())
    }

    fn frame_with(
        &self,
        preview: &mut AudioPreview,
        path: &Path,
        player: &Player<PathBuf>,
        events: Vec<egui::Event>,
    ) -> Drawn {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(400.0, 300.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| preview.ui(ui, path, player));
        });
        // Unapplied texture deltas panic on drop; there is no renderer here.
        output.textures_delta.clear();
        let mut drawn = Drawn::default();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut drawn);
        }
        drawn
    }

    /// Frames of `path` until `done` holds of one, within `within`; that frame's.
    fn until(
        &self,
        preview: &mut AudioPreview,
        path: &Path,
        player: &Player<PathBuf>,
        within: Duration,
        mut done: impl FnMut(&AudioPreview, &Drawn) -> bool,
    ) -> Drawn {
        let start = Instant::now();
        loop {
            let drawn = self.frame(preview, path, player);
            if done(preview, &drawn) {
                return drawn;
            }
            assert!(start.elapsed() < within, "gave up waiting: {drawn:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A click on the label `text`: pressed in one frame, released in the next.
    fn click(&self, preview: &mut AudioPreview, path: &Path, player: &Player<PathBuf>, text: &str) {
        let drawn = self.frame(preview, path, player);
        let at = drawn
            .rect(text)
            .unwrap_or_else(|| panic!("no {text:?} to click: {drawn:?}"))
            .center();
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        self.frame_with(
            preview,
            path,
            player,
            vec![egui::Event::PointerMoved(at), button(true)],
        );
        self.frame_with(preview, path, player, vec![button(false)]);
    }
}

fn collect(shape: &egui::Shape, drawn: &mut Drawn) {
    match shape {
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect(shape, drawn);
            }
        }
        egui::Shape::Text(text) => drawn
            .texts
            .push((text.galley.text().to_owned(), text.visual_bounding_rect())),
        egui::Shape::Path(_) => drawn.path = true,
        _ => {}
    }
}

/// A player of files on a fake device.
fn player(outputs: FakeOutputs) -> Player<PathBuf> {
    Player::spawn(Box::new(FileOpener::new()), Box::new(outputs), || {}).expect("spawn the player")
}

/// A 16-bit PCM WAV of a 440 Hz tone, with the 44-byte header written by hand and no tags.
fn wav(dir: &Path, name: &str, secs: f32, rate: u32, channels: u16) -> PathBuf {
    let frames = (secs * rate as f32) as u32;
    let data_len = frames * u32::from(channels) * 2;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let t = i as f32 / rate as f32;
        let v = ((t * 440.0 * std::f32::consts::TAU).sin() * 0.5 * f32::from(i16::MAX)) as i16;
        for _ in 0..channels {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// Wait, within [`PLAYS_WITHIN`], for the player to play `path` alone.
fn wait_playing(player: &Player<PathBuf>, path: &Path) {
    let start = Instant::now();
    loop {
        let state = player.state();
        if state.queue == [path.to_owned()]
            && state.current == Some(0)
            && state.status == Status::Playing
        {
            return;
        }
        assert!(start.elapsed() < PLAYS_WITHIN, "not playing: {state:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn an_audio_file_shows_its_name_and_a_play_button() {
    let dir = tempfile::tempdir().unwrap();
    let path = wav(dir.path(), "tone.wav", 61.0, 1_000, 1);
    let player = player(FakeOutputs::new());
    let screen = Screen::new();
    let mut preview = AudioPreview::new();

    screen.until(&mut preview, &path, &player, DEADLINE, |p, _| p.idle());
    let drawn = screen.frame(&mut preview, &path, &player);
    assert!(!drawn.path, "still probing: {drawn:?}");
    assert!(drawn.has("tone.wav"), "{drawn:?}");
    assert!(drawn.has("Play"), "{drawn:?}");
    assert!(drawn.has("0:00 / 1:01"), "{drawn:?}");
    // Nothing plays on its own.
    for _ in 0..3 {
        screen.frame(&mut preview, &path, &player);
    }
    let state = player.state();
    assert!(state.queue.is_empty(), "{state:?}");
    assert_eq!(state.status, Status::Stopped);
}

#[test]
fn play_replaces_the_queue_with_this_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = wav(dir.path(), "tone.wav", 3.0, 8_000, 1);
    let player = player(FakeOutputs::new());
    let screen = Screen::new();
    let mut preview = AudioPreview::new();

    screen.click(&mut preview, &path, &player, "Play");
    wait_playing(&player, &path);
    screen.until(&mut preview, &path, &player, DEADLINE, |_, d| {
        d.has("Pause") && !d.has("Play")
    });
}

#[test]
fn a_second_file_shows_play_while_the_first_plays() {
    let dir = tempfile::tempdir().unwrap();
    let a = wav(dir.path(), "a.wav", 3.0, 8_000, 1);
    let b = wav(dir.path(), "b.wav", 3.0, 8_000, 1);
    let player = player(FakeOutputs::new());
    let screen = Screen::new();
    let mut preview = AudioPreview::new();

    screen.click(&mut preview, &a, &player, "Play");
    wait_playing(&player, &a);

    screen.until(&mut preview, &b, &player, DEADLINE, |p, _| p.idle());
    let drawn = screen.frame(&mut preview, &b, &player);
    assert!(drawn.has("b.wav"), "{drawn:?}");
    assert!(drawn.has("Play"), "{drawn:?}");
    assert!(!drawn.has("Pause"), "{drawn:?}");
    assert_eq!(player.state().queue, [a]);
}

#[test]
fn the_seek_bar_is_off_for_a_file_not_playing() {
    let dir = tempfile::tempdir().unwrap();
    let a = wav(dir.path(), "a.wav", 3.0, 8_000, 1);
    let b = wav(dir.path(), "b.wav", 3.0, 8_000, 1);
    let player = player(FakeOutputs::new());
    let screen = Screen::new();
    let mut preview = AudioPreview::new();

    screen.click(&mut preview, &a, &player, "Play");
    wait_playing(&player, &a);
    // On for the file that plays, its length known.
    screen.until(&mut preview, &a, &player, DEADLINE, |p, _| {
        p.seek_enabled() == Some(true)
    });

    screen.until(&mut preview, &b, &player, DEADLINE, |p, _| p.idle());
    screen.frame(&mut preview, &b, &player);
    assert_eq!(preview.seek_enabled(), Some(false));
}

#[test]
fn a_file_that_is_not_audio_offers_no_play() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.mp3");
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&[0, 0, 0, 13]);
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&[0; 17]);
    std::fs::write(&path, png).unwrap();
    let player = player(FakeOutputs::new());
    let screen = Screen::new();
    let mut preview = AudioPreview::new();

    let drawn = screen.until(&mut preview, &path, &player, DEADLINE, |_, d| {
        d.has("This file can't be previewed.")
    });
    assert!(!drawn.has("Play"), "{drawn:?}");
    assert_eq!(preview.seek_enabled(), None, "a seek bar was drawn");
}

#[test]
fn a_missing_file_shows_the_error_after_play() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gone.flac");
    let player = player(FakeOutputs::new());
    let screen = Screen::new();
    let mut preview = AudioPreview::new();

    screen.until(&mut preview, &path, &player, DEADLINE, |p, _| p.idle());
    screen.click(&mut preview, &path, &player, "Play");
    screen.until(&mut preview, &path, &player, PLAYS_WITHIN, |_, d| {
        d.starts("Couldn't read this file:")
    });
}

#[test]
fn no_device_shows_the_output_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = wav(dir.path(), "tone.wav", 1.0, 8_000, 1);
    let outputs = FakeOutputs::new();
    outputs.refuse_next(u32::MAX);
    let player = player(outputs);
    let screen = Screen::new();
    let mut preview = AudioPreview::new();

    screen.click(&mut preview, &path, &player, "Play");
    screen.until(&mut preview, &path, &player, PLAYS_WITHIN, |_, d| {
        d.starts("Couldn't play through the audio device:")
    });
}

#[test]
fn the_audio_strings_exist_in_both_languages() {
    use crate::i18n::{Language, LanguageSettings, check, tr};

    assert_eq!(
        check::key_difference(&crate::i18n::CORUNDUM, "en", "ja"),
        Vec::<String>::new()
    );
    let ctx = egui::Context::default();
    let ids = [
        "corundum-preview-play",
        "corundum-preview-pause",
        "corundum-preview-audio-device",
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
    assert_eq!(english[0], "Play");
    assert_eq!(english[1], "Pause");
}

#[test]
fn the_probe_thread_ends_with_the_preview() {
    let dir = tempfile::tempdir().unwrap();
    let path = wav(dir.path(), "tone.wav", 0.5, 8_000, 1);
    let player = player(FakeOutputs::new());
    let screen = Screen::new();
    let mut preview = AudioPreview::new();
    screen.frame(&mut preview, &path, &player);
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
fn the_time_is_the_player_s_position_when_the_length_is_unknown() {
    use super::audio::shown_position;
    // Playing, length unknown: the player's position, not a bar clamped to nothing.
    assert_eq!(shown_position(true, None, 75_000), 75_000);
    // A held bar shows where it is held.
    assert_eq!(shown_position(true, Some(12_500.0), 75_000), 12_500);
    // Another file: from its head.
    assert_eq!(shown_position(false, None, 75_000), 0);
}

#[test]
fn the_length_falls_back_to_the_probe_s() {
    use super::audio::shown_length;
    assert_eq!(shown_length(true, None, Some(3_000)), Some(3_000));
    assert_eq!(shown_length(true, Some(2_000), Some(3_000)), Some(2_000));
    assert_eq!(shown_length(true, None, None), None);
    // Not playing: the player's length is another file's.
    assert_eq!(shown_length(false, Some(2_000), Some(3_000)), Some(3_000));
}

#[test]
fn the_seek_bar_seeks_on_a_keyboard_change_and_on_release() {
    use super::audio::seek_step;
    // An arrow key: changed, not dragged — seek at once, nothing held.
    assert_eq!(
        seek_step(None, 500.0, false, true, false),
        (None, Some(500))
    );
    // Dragging: held, no seek yet.
    assert_eq!(
        seek_step(None, 700.0, true, true, false),
        (Some(700.0), None)
    );
    assert_eq!(
        seek_step(Some(700.0), 800.0, true, false, false),
        (Some(800.0), None)
    );
    // Let go: seek where it was held.
    assert_eq!(
        seek_step(Some(800.0), 800.0, false, false, true),
        (None, Some(800))
    );
    // Nothing happened: nothing changes.
    assert_eq!(seek_step(None, 0.0, false, false, false), (None, None));
}

#[test]
fn a_disabled_seek_bar_shows_empty() {
    use super::audio::seek_value;
    assert_eq!(seek_value(true, 2_000, 10_000), 2_000.0);
    assert_eq!(seek_value(true, 20_000, 10_000), 10_000.0);
    // No length known, or not this file: the bar rests at 0, not full.
    assert_eq!(seek_value(false, 5_000, 1), 0.0);
    assert_eq!(seek_value(false, 0, 1), 0.0);
}

#[test]
fn an_arrow_key_on_the_focused_seek_bar_seeks() {
    let dir = tempfile::tempdir().unwrap();
    let path = wav(dir.path(), "tone.wav", 30.0, 8_000, 1);
    let player = player(FakeOutputs::new());
    let screen = Screen::new();
    let mut preview = AudioPreview::new();

    screen.until(&mut preview, &path, &player, DEADLINE, |p, _| p.idle());
    screen.click(&mut preview, &path, &player, "Play");
    wait_playing(&player, &path);
    screen.until(&mut preview, &path, &player, DEADLINE, |p, _| {
        p.seek_enabled() == Some(true)
    });
    // The fake device plays nothing out, so the position stays at 0 until a seek.
    assert_eq!(player.state().position_ms, 0);

    let id = preview.seek_id().expect("a seek bar was drawn");
    screen.ctx.memory_mut(|m| m.request_focus(id));
    screen.frame(&mut preview, &path, &player);
    let right = egui::Event::Key {
        key: egui::Key::ArrowRight,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::default(),
    };
    screen.frame_with(&mut preview, &path, &player, vec![right]);
    let start = Instant::now();
    while player.state().position_ms == 0 {
        assert!(
            start.elapsed() < PLAYS_WITHIN,
            "no seek: {:?}",
            player.state()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    // One press moves about five seconds, not a point of the bar.
    let ms = player.state().position_ms;
    assert!((4_000..=6_000).contains(&ms), "one press moved {ms} ms");
}
