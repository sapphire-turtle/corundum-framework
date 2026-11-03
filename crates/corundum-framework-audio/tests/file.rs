//! `FileOpener` and the feeds behind it, on WAV files written by the tests.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use corundum_framework_audio::feed::live_feeds;
use corundum_framework_audio::{
    AudioOutput, Command, Feed, FeedEvent, FileOpener, Opener, OutputConfig, OutputFactory, Player,
    PlayerState, Status, TrackError,
};

const TARGET: OutputConfig = OutputConfig {
    sample_rate: 48_000,
    channels: 2,
};

/// A 16-bit PCM WAV of a 440 Hz tone, with the 44-byte header written by hand.
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

// Two tests look at the process-wide count of decode jobs, so no test may overlap another.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Polls to the end: the samples, and how it finished.
fn drain(feed: &mut dyn Feed) -> (Vec<f32>, FeedEvent) {
    let mut pcm = Vec::new();
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < end, "the feed did not finish");
        match feed.poll() {
            FeedEvent::Pcm(p) => pcm.extend(p),
            FeedEvent::Waiting(_) => std::thread::sleep(Duration::from_millis(5)),
            last => return (pcm, last),
        }
    }
}

#[test]
fn a_wav_file_comes_out_as_pcm_then_end() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let path = wav(dir.path(), "tone.wav", 1.0, 44_100, 2);
    let mut feed = FileOpener::new().open(&path, 0, TARGET);
    let (pcm, last) = drain(feed.as_mut());
    assert_eq!(last, FeedEvent::End);
    let frames = pcm.len() / 2;
    assert!(frames.abs_diff(48_000) < 480, "{frames} frames");
    assert!(pcm.iter().fold(0.0f32, |m, v| m.max(v.abs())) > 0.2);
    assert_eq!(feed.duration_ms().map(|d| d / 100), Some(10));
}

#[test]
fn opening_at_a_position_starts_there() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let path = wav(dir.path(), "tone.wav", 2.0, 48_000, 2);
    let mut feed = FileOpener::new().open(&path, 1_500, TARGET);
    let (pcm, last) = drain(feed.as_mut());
    assert_eq!(last, FeedEvent::End);
    let frames = pcm.len() / 2;
    assert!(frames.abs_diff(24_000) <= 960, "{frames} frames");
}

#[test]
fn a_missing_file_is_unplayable() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let mut feed = FileOpener::new().open(&dir.path().join("nope.wav"), 0, TARGET);
    assert!(matches!(
        feed.poll(),
        FeedEvent::Failed(TrackError::Unplayable(_))
    ));
}

#[test]
fn a_file_that_is_not_audio_is_unplayable() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("text.wav");
    std::fs::write(&path, b"this is not audio at all").unwrap();
    let mut feed = FileOpener::new().open(&path, 0, TARGET);
    let (_, last) = drain(feed.as_mut());
    assert!(matches!(last, FeedEvent::Failed(TrackError::Unplayable(_))));
}

#[test]
fn a_dropped_feed_ends_its_thread() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let path = wav(dir.path(), "long.wav", 10.0, 48_000, 2);
    let before = live_feeds();
    let mut feed = FileOpener::new().open(&path, 0, TARGET);
    let end = Instant::now() + Duration::from_secs(10);
    while !matches!(feed.poll(), FeedEvent::Pcm(_)) {
        assert!(Instant::now() < end, "no pcm came");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(live_feeds(), before + 1);
    drop(feed);
    let end = Instant::now() + Duration::from_secs(2);
    while live_feeds() > before {
        assert!(Instant::now() < end, "the decode thread is still running");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A device that takes a little and never plays it, so a feed stays unfinished.
struct FullDevice(usize);

struct FullDevices;

impl OutputFactory for FullDevices {
    fn open(&mut self) -> Result<Box<dyn AudioOutput>, String> {
        Ok(Box::new(FullDevice(0)))
    }
}

impl AudioOutput for FullDevice {
    fn config(&self) -> OutputConfig {
        TARGET
    }
    fn writable(&self) -> usize {
        4_800usize.saturating_sub(self.0)
    }
    fn write(&mut self, samples: &[f32]) {
        self.0 += samples.len() / 2;
    }
    fn played(&self) -> u64 {
        0
    }
    fn set_paused(&mut self, _paused: bool) {}
    fn clear(&mut self) {
        self.0 = 0;
    }
    fn failed(&self) -> Option<String> {
        None
    }
}

fn wait_for(p: &Player<PathBuf>, what: &str, ok: impl Fn(&PlayerState<PathBuf>) -> bool) {
    wait_until(what, || ok(&p.state()));
}

#[test]
fn replacing_the_one_track_queue_plays_the_new_file() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let a = wav(dir.path(), "a.wav", 10.0, 44_100, 2);
    let b = wav(dir.path(), "b.wav", 10.0, 22_050, 1);
    let before = live_feeds();
    let p = Player::<PathBuf>::spawn(Box::new(FileOpener::new()), Box::new(FullDevices), || {})
        .unwrap();
    p.send(Command::Replace {
        items: vec![a.clone()],
        start: 0,
    });
    wait_for(&p, "a playing", |s| {
        s.status == Status::Playing && s.current == Some(0) && s.queue == [a.clone()]
    });
    p.send(Command::Replace {
        items: vec![b.clone()],
        start: 0,
    });
    wait_for(&p, "b playing", |s| {
        s.status == Status::Playing && s.current == Some(0) && s.queue == [b.clone()]
    });
    let s = p.state();
    assert_eq!(s.queue, vec![b]);
    assert_eq!(s.current, Some(0));
    assert_eq!(s.status, Status::Playing);
    let end = Instant::now() + Duration::from_secs(2);
    while live_feeds() != before + 1 {
        assert!(
            Instant::now() < end,
            "{} feeds live, {:?}",
            live_feeds().saturating_sub(before),
            p.state()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
