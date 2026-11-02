//! Decoding the library's formats and converting to the device's format.

use std::fs::File;
use std::path::PathBuf;

use corundum_framework_audio::OutputConfig;
use corundum_framework_audio::decode::{self, TrackDecoder};
use corundum_framework_audio::resample::Converter;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn open(name: &str) -> Result<Box<dyn TrackDecoder>, decode::DecodeError> {
    let path = fixture(name);
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_owned);
    decode::open(Box::new(File::open(&path).unwrap()), ext.as_deref())
}

/// Every sample of the track, interleaved.
fn read_all(d: &mut dyn TrackDecoder) -> Result<Vec<f32>, decode::DecodeError> {
    let mut out = Vec::new();
    while let Some(b) = d.next_block()? {
        out.extend_from_slice(&b);
    }
    Ok(out)
}

fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0, |m, v| m.max(v.abs()))
}

#[test]
fn each_format_decodes_to_its_length_rate_and_channels() {
    for name in [
        "tone.flac",
        "tone.mp3",
        "tone.m4a",
        "tone-alac.m4a",
        "tone.ogg",
        "tone.opus",
    ] {
        let mut d = open(name).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let rate = d.sample_rate();
        assert_eq!(
            rate,
            if name == "tone.opus" { 48_000 } else { 44_100 },
            "{name}"
        );
        assert_eq!(d.channels(), 2, "{name}");
        let samples = read_all(d.as_mut()).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let frames = samples.len() / 2;
        let want = 2.0 * f64::from(rate);
        assert!(
            (frames as f64 - want).abs() <= want * 0.02,
            "{name}: {frames} frames, wanted about {want}"
        );
        let p = peak(&samples);
        assert!((0.2..=1.0).contains(&p), "{name}: peak {p}");
        let ms = d
            .duration_ms()
            .unwrap_or_else(|| panic!("{name}: no duration"));
        assert!((1_950..=2_050).contains(&ms), "{name}: duration {ms}");
    }
}

#[test]
fn seek_lands_near_the_asked_position() {
    for name in ["tone.flac", "tone.mp3", "tone.m4a", "tone.ogg", "tone.opus"] {
        let mut d = open(name).unwrap();
        let rate = u64::from(d.sample_rate());
        let at = d.seek(1_500).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert!(at.abs_diff(1_500) <= 100, "{name}: landed at {at}");
        let left = read_all(d.as_mut()).unwrap().len() as u64 / 2;
        let left_ms = left * 1_000 / rate;
        assert!(left_ms.abs_diff(500) <= 120, "{name}: {left_ms} ms remain");
    }
}

#[test]
fn seek_past_the_end_is_clamped() {
    for name in ["tone.flac", "tone.mp3", "tone.ogg", "tone.opus"] {
        let mut d = open(name).unwrap();
        let at = d.seek(60_000).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert!(at <= 2_050, "{name}: landed at {at}");
        assert!(read_all(d.as_mut()).unwrap().len() / 2 < 4_410, "{name}");
    }
}

#[test]
fn a_truncated_file_is_an_error_not_a_panic() {
    let result = open("broken.flac").and_then(|mut d| read_all(d.as_mut()));
    assert!(result.is_err());
}

#[test]
fn mono_22k_converts_to_stereo_48k() {
    let mut d = open("tone-mono-22k.flac").unwrap();
    assert_eq!((d.sample_rate(), d.channels()), (22_050, 1));
    let mut c = Converter::new(
        22_050,
        1,
        OutputConfig {
            sample_rate: 48_000,
            channels: 2,
        },
    )
    .unwrap();
    let mut out = Vec::new();
    while let Some(b) = d.next_block().unwrap() {
        out.extend(c.process(&b));
    }
    out.extend(c.flush());
    let frames = out.len() / 2;
    assert!(frames.abs_diff(48_000) <= 480, "{frames} frames");
    assert!(out.as_chunks::<2>().0.iter().all(|f| f[0] == f[1]));
    assert!(peak(&out) > 0.2);
}

#[test]
fn same_rate_passes_through() {
    let mut c = Converter::new(
        48_000,
        2,
        OutputConfig {
            sample_rate: 48_000,
            channels: 2,
        },
    )
    .unwrap();
    let block: Vec<f32> = (0..200).map(|i| i as f32 / 200.0).collect();
    assert_eq!(c.process(&block), block);
    assert!(c.flush().is_empty());
}

#[test]
fn flush_makes_the_output_exactly_as_long_as_the_ratio_says() {
    for (from, to, frames) in [
        (44_100u32, 48_000u32, 10_000usize),
        (48_000, 44_100, 10_007),
    ] {
        let mut c = Converter::new(
            from,
            2,
            OutputConfig {
                sample_rate: to,
                channels: 2,
            },
        )
        .unwrap();
        let block: Vec<f32> = (0..frames * 2)
            .map(|i| ((i / 2) as f32 * 0.05).sin())
            .collect();
        let mut out = Vec::new();
        for part in block.chunks(2 * 777) {
            out.extend(c.process(part));
        }
        out.extend(c.flush());
        let want = (frames as f64 * f64::from(to) / f64::from(from)).round() as usize;
        assert_eq!(out.len() / 2, want, "{from} -> {to}");
    }
}

fn probe(name: &str) -> Result<decode::Info, decode::DecodeError> {
    let path = fixture(name);
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_owned);
    decode::probe(Box::new(File::open(&path).unwrap()), ext.as_deref())
}

#[test]
fn probe_reads_the_tags_and_length() {
    for name in ["tone-tagged.flac", "tone-tagged.opus"] {
        let info = probe(name).unwrap();
        assert_eq!(info.title.as_deref(), Some("Test Tone"), "{name}");
        assert_eq!(info.artist.as_deref(), Some("corundum"), "{name}");
        assert_eq!(info.album.as_deref(), Some("Fixtures"), "{name}");
        let ms = info.duration_ms.unwrap();
        assert!((1_950..=2_050).contains(&ms), "{name}: {ms}");
    }
}

#[test]
fn probe_of_an_untagged_file_has_no_tags_but_a_length() {
    let info = probe("tone.mp3").unwrap();
    assert_eq!((info.title, info.artist, info.album), (None, None, None));
    assert!(info.duration_ms.is_some());
}

#[test]
fn probe_of_a_file_that_is_not_audio_is_an_error() {
    // A PNG signature named .mp3.
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("x.mp3");
    std::fs::write(&p, b"\x89PNG\r\n\x1a\n not audio at all").unwrap();
    assert!(decode::probe(Box::new(File::open(&p).unwrap()), Some("mp3")).is_err());
}
