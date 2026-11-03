# corundum-framework-audio

Play audio, with no GUI in the way: decode, resample, queue and play through the
default sound device.

- `decode::open(Box<dyn Source>, hint)`: a `TrackDecoder` of interleaved `f32` blocks, with
  seeking and the length. FLAC, MP3, AAC, ALAC, Vorbis, MP4, WAV (symphonia) and Ogg Opus
  (ogg + opus-pure).
- `decode::probe(Box<dyn Source>, hint)`: the title, artist, album and length, read without
  decoding any audio.
- `resample::Converter`: channels first, then the rate with rubato, to an `OutputConfig`.
- `Engine`, the playback state machine over a queue of any `QueueItem` (`PathBuf` is one),
  an `Opener` and an `OutputFactory`: gapless preload of the next track, "previous" restarts
  a track after 3 s of play, and three unplayable tracks in a row stop playback.
- `FileOpener` opens files on disk; `Player` runs an engine on a thread
  (`corundum-audio-player`) and reports its `Status`.
- `CpalOutputs` (feature `cpal`) plays through the default output device, via a 200 ms ring.

| Feature        | What it adds |
| -------------- | ------------ |
| (none)         | Decoding, resampling, the engine, `FileOpener` and `Player` |
| `cpal`         | `CpalOutputs`, the real sound device (brings cpal and rtrb; on Linux it needs `libasound2-dev`) |
| `test-support` | The fakes of the device and the opener, for tests |

Both features are off by default. No feature brings egui.

```rust,no_run
use corundum_framework_audio::{CpalOutputs, FileOpener, Player};
use std::path::PathBuf;

let player: Player<PathBuf> = Player::spawn(
    Box::new(FileOpener::new()),
    Box::new(CpalOutputs),
    move || { /* e.g. ctx.request_repaint() */ },
)?;
# Ok::<(), std::io::Error>(())
```

Licensed under MIT or Apache-2.0, at your option.
