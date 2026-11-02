# Images and audio: `corundum-framework-image`, `corundum-framework-audio` and their previews

Asked for on 2026-11-02, for asteria (sapphire-turtle/asteria) and sapphire-sync.

## Goal

The detail pane beside the [`FileExplorer`](../../../crates/corundum-framework-gui/src/explorer/mod.rs)
should show a picked image and play a picked audio file, in every app that lists files:
sapphire-sync's synced folders and asteria's libraries. Both need the same pieces, which
asteria already has in its own crates:

- **Audio:** asteria-music's player (about 5,000 lines, `asteria-music-gui/src/player`):
  decoding with symphonia and opus-pure, rubato resampling, a cpal device behind a ring
  buffer, and the playback engine's state machine on a thread of its own.
- **Images:** asteria's server makes thumbnails with external programs (vipsthumbnail,
  ffmpeg, macOS `sips`). A pure-Rust decoder in the process would serve both a preview and
  most of those thumbnails.

So corundum takes them over, each behind a feature. Why a framework previews formats its
apps were not made for is recorded in [file-previews.md](../../file-previews.md). Success means:

- an app turns on `image-preview` / `audio-preview` and shows a picked file in its detail
  pane with a few lines of code, without blocking the UI thread;
- asteria-music plays through `corundum-framework-audio`'s engine, keeping only what is
  its own (fetching from its server, its queue entries);
- asteria's server makes JPEG/PNG/WebP/GIF/TIFF and RAW-preview thumbnails with
  `corundum-framework-image`, with no egui and no audio in its build.

## Decisions

1. **Two crates, one per concern**, as `keys`, `service`, `package` and `update` are:
   `corundum-framework-image` and `corundum-framework-audio`. Neither depends on egui, so a
   server can use the image crate. The widgets live in `corundum-framework-gui` behind
   `image-preview` and `audio-preview`.
2. **-sys crates are allowed behind features.** corundum avoided them so far; these
   features need cpal (ALSA on Linux) and, for WebP encoding, libwebp. They are off by
   default, so an app that does not turn them on builds as before.
3. **Formats are what decodes in pure Rust.** Images: JPEG, PNG, WebP, GIF (the first
   frame), TIFF. HEIC, AVIF and ORF return `Error::Unsupported`; the caller falls back (an
   external program, or an icon). HEIC is looked at again later (a pure-Rust decoder as it
   matures, or the OS's ImageIO / WIC behind a feature). Audio: what symphonia and
   opus-pure decode (FLAC, MP3, AAC, ALAC, Vorbis, MP4, Ogg, WAV, Ogg Opus).
4. **RAW previews are a separate feature, `raw`**, because `rawler` is LGPL-2.1 while
   corundum is MIT OR Apache-2.0. An app that turns it on takes on the LGPL's terms for
   that crate; the feature's doc says so, and `deny.toml` gets a per-crate exception.
5. **The preview has no queue.** The audio engine keeps its queue, because asteria-music
   needs it; the preview hands it a queue of one track (`Command::Replace(vec![path], 0)`),
   so picking another file replaces what plays, and an app holds one player for all its
   previews. The preview's controls are play/pause, a seek bar and the time.
6. **Nothing plays on its own.** Picking an audio file shows the controls; playing starts
   on the button.

## `corundum-framework-image`

No egui. Features: default (decoding), `raw`, `webp`.

```rust
pub enum Source<'a> { Path(&'a Path), Bytes { bytes: &'a [u8], ext: Option<&'a str> } }

pub struct Image { pub width: u32, pub height: u32, pub rgba: Vec<u8> } // sRGB, RGBA8

pub fn decode(source: Source, limits: &Limits) -> Result<Image, Error>;
pub fn thumbnail(source: Source, longest: u32, limits: &Limits) -> Result<Image, Error>;
#[cfg(feature = "webp")]
pub fn encode_webp(image: &Image, quality: f32) -> Result<Vec<u8>, Error>;

pub enum Error { Unsupported, Decode(String), TooLarge, Io(std::io::Error) }
pub struct Limits { pub max_bytes: u64, pub max_pixels: u64 } // Default: 512 MiB, 200 MP
```

- **Decoding:** `image` 0.25 with only its pure-Rust decoders (JPEG through zune-jpeg).
  The format comes from the bytes, the extension only breaking a tie.
- **Orientation:** the decoder's EXIF orientation (1–8) is applied to the pixels.
- **Colour:** an embedded ICC profile (JPEG APP2, PNG iCCP, WebP ICCP, TIFF) is converted
  to sRGB with `moxcms` (pure Rust); without one the pixels are taken as sRGB.
- **Scaling:** `thumbnail` shrinks the longest side to `longest` and never enlarges;
  `fast_image_resize`, Lanczos3. Decoding at a reduced size where the decoder can (JPEG's
  DCT scaling) is used when it is available, not required.
- **RAW (`raw`):** `rawler` reads the preview JPEG the camera embedded (CR2, CR3, NEF, ARW,
  RW2, RAF, DNG), then goes through the JPEG path above (orientation from the RAW's own
  metadata). No demosaicing. ORF and files without a preview are `Unsupported`.
- **WebP (`webp`):** lossy encoding through `libwebp-sys` (bundled libwebp, built with `cc`).
- **Safety:** `Limits` are checked from the header before the pixels are allocated
  (`TooLarge`). A panic in a decoder is caught and returned as `Decode`.

## `corundum-framework-audio`

No egui. Features: default (decoding, resampling, the engine), `cpal` (the real device),
`test-support` (the fakes). Moved from asteria-music's player, renamed where its names were
asteria's.

- `decode`: `open(reader: Box<dyn ReadSeek>, hint: Option<&str>) -> Result<Box<dyn
  TrackDecoder>, DecodeError>` — symphonia (gapless trimming on), and ogg + opus-pure for
  Ogg Opus. Seek, duration, and the tags symphonia reads (title, artist, album).
- `resample`: `Converter` — channels first, then the rate with rubato.
- `output`: `OutputConfig`, `AudioOutput`, `OutputFactory`, the 200 ms ring, and (`cpal`)
  `CpalOutputs`, the default output device.
- `engine`: `Engine<T>`, generic over the queued item:
  - `Opener<T>` turns an item into a `Feed` (decoded audio on a thread of its own), and is
    told the next item to preload;
  - `Command<T>` (replace, insert, play, pause, seek, next, previous, volume, stop) and
    `PlayerState<T>` (the queue, the current index, position, playing, the last error);
  - the rules asteria-music has today: "previous" restarts a track after 3 s of play,
    three unplayable tracks in a row stop playback.
- `Player<T>`: the engine on its thread — commands in, state out, a repaint callback.
- `FileOpener`: `Opener<PathBuf>` reading local files, for the preview and any app playing
  local files.

What stays in asteria: its `Opener` that fetches a track from its server in `Range` chunks
into a temporary file, its queue entry (a server item with a title), and its quality
setting.

## The previews (`corundum-framework-gui`)

- **`image-preview` → `ImagePreview`:** `ui(ui, path)`. Decodes on a worker thread to the
  pane's size times `pixels_per_point`, at most 2048 px on the longest side, uploads one
  texture, and keeps the last few by (path, modified time). Shows a spinner while working,
  and a file icon with the reason (translated) for `Unsupported` or an error. Keeps the
  aspect ratio and fits the space it is given.
- **`audio-preview` → `AudioPreview`:** `ui(ui, path, player: &Player<PathBuf>)`. Shows the
  tags (else the file name), play/pause, a seek bar, the time, and a playback error.
  Pressing play replaces the player's queue with this one file. The app makes the player
  once (`Player::spawn(FileOpener, CpalOutputs, repaint)`) and passes it to every preview.
- Both work on a phone: the decoders are pure Rust, and cpal plays on iOS and Android.

## Facade

`corundum-framework` features: `image`, `image-raw`, `image-webp`, `audio`, `audio-cpal`,
`image-preview` (brings `gui`, `image`), `audio-preview` (brings `gui`, `audio`,
`audio-cpal`).

## Testing

- Image: small fixtures generated in the tests where possible (orientations 1–8; a Display
  P3 profile turning to sRGB; no enlarging; a truncated file; a header claiming more pixels
  than the limit); one tiny RAW fixture per family the `raw` feature claims, if a
  freely licensed one exists, else the claim is limited to the families tested.
- Audio: the player's existing tests move with it (the fake device and fake openers);
  new ones for `FileOpener` on a generated WAV, and for a one-track queue replaced by another.
- Previews: a frame drawn without a window (as `explorer` and `first_run` are tested): the
  spinner, then the image; the controls for an audio file, and play sending `Replace`.

## Order

1. `corundum-framework-image` and `image-preview` (its own plan and release).
2. `corundum-framework-audio` and `audio-preview` (its own plan; asteria's player moves).
3. In asteria, its own plan: asteria-music plays through `corundum-framework-audio`; the
   server makes in-process thumbnails with `corundum-framework-image` (`raw`, `webp`),
   keeping its external programs for HEIC, AVIF, ORF and video.

## Out of scope

- Thumbnails in the explorer's grid of tiles.
- HEIC and AVIF decoding; video frames.
- The OS's media keys for the preview (`media-session` exists; joining it to the player is
  later work).
- Shuffle, repeat and gapless playback across tracks (asteria#17, asteria#18).
