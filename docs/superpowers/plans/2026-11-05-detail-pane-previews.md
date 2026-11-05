# Detail Pane Previews Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the detail pane location-agnostic preview parts. The app hands in content
(bytes, or a reader opener) and a loading state. corundum draws an image, a sound or an
info card for it, and the app composes these parts into Default, Replace or Add.

**Architecture:** A `preview` module in `corundum-framework-gui`, present in every build,
holds the shared types (`Content`, `State`, `PreviewResponse`), `InfoCard` and
`FilePreview`. `ImagePreview` (feature `image-preview`) and `AudioPreview` (feature
`audio-preview`) are changed to take `State` and `Content` instead of a `&Path`. An
`Opener<Content>` lets one `Player<Content>` play both bytes and readers. The demo's Files
screen gets a `DetailPane` that uses all of this.

**Tech Stack:** Rust 2024, egui, Fluent strings through `tr!`, corundum-framework-image,
and corundum-framework-audio (symphonia, cpal).

**Spec:** `docs/superpowers/specs/2026-11-05-detail-pane-previews-design.md`

## Global Constraints

- The preview API has no path or fetcher types. Content is either bytes or a reader opener
  (spec decision 1). `Content::file` is only a convenience that builds a reader opener of a
  `File`.
- The app decides when a file has changed, and says so through `Content::key`. corundum
  never stats a path. Remove `ImagePreview`'s two-second recheck.
- The UI thread never calls a reader opener, decodes, or reads tags. All of these happen on
  the parts' workers, or for playback on the player's thread.
- A broken or unsupported image or sound shows the part's own message, with the metadata
  under it. It does not switch to the info card (spec decision 5).
- Retry and "Open with the system's app" are only reported back in `PreviewResponse`.
  corundum never refetches or opens anything by itself.
- Everything that needs a dependency stays behind `image-preview` or `audio-preview`. The
  `preview` module itself adds no dependency.
- Every string the user sees goes through `tr!`, with a key in both
  `crates/corundum-framework-gui/locales/en/corundum.ftl` and `ja/corundum.ftl`. Name keys
  `corundum-preview-*`.
- Breaking changes are allowed. Mark the commits that make them `feat(corundum-framework-gui)!:`
  and describe the break in the commit body: release-plz writes the CHANGELOG from these
  commits, and the maintainer curates it at release time. Do not edit `CHANGELOG.md` by
  hand.
- CI runs clippy with `-D warnings`. Run it on the gui crate in three ways: with no default
  features, with `--features image-preview`, and with `--features audio-preview`
  (`.github/workflows/ci.yml:46`).
- Doc comments follow the crate's style: plain sentences that say what a thing is and
  does, and the module doc lists its parts.

## Review Focus

1. **Switching selection quickly.** When the person moves from file A to file B before A
   has decoded, B is shown and A's late answer never appears in its place. Tests: Task 2
   (`a_newer_key_wins`) and Task 3 (`a_newer_file_wins`).
2. **A reader opener that fails, or a file that has vanished.** The image shows "Couldn't
   read this file: …". For a sound, the probe reports it unread, and Play then shows the
   player's error. Neither panics, and neither blocks the UI. Tests: Task 2
   (`a_reader_that_fails_says_why`) and Task 3 (`a_reader_that_fails_shows_the_error_after_play`).
3. **Two different files with the same name.** In asteria, `cover.jpg` appears in many
   album folders. Content is matched by key and never by name, so the second file is
   decoded rather than the first being shown. Tests: Task 2 (`same_name_other_key_is_decoded_anew`)
   and Task 3 (`same_name_other_key_is_another_track`).
4. **A non-previewable file in asteria.** The app must not download a large ZIP only to
   show the info card. `FilePreview::wants_content` says no, and `FilePreview` draws the
   card from `State::Loading` without waiting for anything. Test: Task 5
   (`an_unknown_file_needs_no_content`).
5. **An audio file when the app has no player**, for example an app that turned on
   `audio-preview` but gave `FilePreview` no player. The pane falls back to the info card
   and does not panic. Test: Task 5 (`audio_without_a_player_is_an_info_card`).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/corundum-framework-gui/src/lib.rs` | `pub mod preview;` with no `cfg`. Update the crate doc lines 18–20. |
| `crates/corundum-framework-gui/src/preview/mod.rs` | The module doc, which shows the three compositions; the re-exports. |
| `crates/corundum-framework-gui/src/preview/content.rs` (new) | `Reader`, `Content`, `State`, `PreviewResponse`, and `status_ui`. |
| `crates/corundum-framework-gui/src/preview/kind.rs` (new) | `Kind`, and `kind_of(name)` from the extension. |
| `crates/corundum-framework-gui/src/preview/info.rs` (new) | `FileInfo` and `InfoCard`. |
| `crates/corundum-framework-gui/src/preview/file.rs` (new) | `FilePreview`. |
| `crates/corundum-framework-gui/src/preview/image.rs` | `ImagePreview` over `State` and `Content`. |
| `crates/corundum-framework-gui/src/preview/audio.rs` | `AudioPreview` over `State` and `Content`, `ContentOpener`, and `QueueItem for Content`. |
| `crates/corundum-framework-gui/src/preview/tests_content.rs`, `tests_file.rs` (new); `tests.rs`, `tests_audio.rs` | Frames drawn without a window, in the style the existing tests use (`ctx.run_ui`, then collect the shapes). |
| `crates/corundum-framework-gui/src/platform/mod.rs` | `open_with_system(path)` and `open_with_system_available()`. |
| `crates/corundum-framework-gui/locales/{en,ja}/corundum.ftl` | The new strings. |
| `crates/corundum-framework-gui/Cargo.toml`, `crates/corundum-framework/{Cargo.toml,src/lib.rs,tests/features.rs}`, `README.md`, `README.ja.md`, `docs/file-previews{,.ja}.md` | The feature comments and docs. |
| `demo/Cargo.toml`, `demo/src/screens/files.rs`, `demo/assets/sample.png` (new) | The demo. |

---

### Task 1: Content, State and the loading/failed drawing

**Files:**
- Create: `crates/corundum-framework-gui/src/preview/content.rs`, `crates/corundum-framework-gui/src/preview/tests_content.rs`
- Modify: `crates/corundum-framework-gui/src/lib.rs:57-58` (drop the `cfg`), `crates/corundum-framework-gui/src/preview/mod.rs`, both `.ftl` files

**Interfaces:**
- Produces, re-exported from `preview`:
  ```rust
  /// Anything a preview can read and seek: a file, a cursor over bytes, a download.
  pub trait Reader: std::io::Read + std::io::Seek + Send + Sync {}
  impl<T: std::io::Read + std::io::Seek + Send + Sync> Reader for T {}

  #[derive(Clone)]
  pub enum Data {
      Bytes(std::sync::Arc<[u8]>),
      Reader(std::sync::Arc<dyn Fn() -> std::io::Result<Box<dyn Reader>> + Send + Sync>),
  }

  /// What a preview draws: the app's key for it, the file's name, and its data.
  #[derive(Clone)]
  pub struct Content { pub key: String, pub name: String, pub data: Data }
  impl Content {
      pub fn bytes(key: impl Into<String>, name: impl Into<String>, bytes: impl Into<Arc<[u8]>>) -> Content;
      pub fn reader(key: impl Into<String>, name: impl Into<String>,
                    open: impl Fn() -> std::io::Result<Box<dyn Reader>> + Send + Sync + 'static) -> Content;
      /// A reader opener of the local file at `path`, named by its file name.
      pub fn file(key: impl Into<String>, path: impl Into<std::path::PathBuf>) -> Content;
      /// The extension of `name`, in lower case.
      pub fn ext(&self) -> Option<String>;
      /// A fresh reader: a Cursor over the bytes, or the opener's result. Call only off the UI thread.
      pub fn open(&self) -> std::io::Result<Box<dyn Reader>>;
  }
  impl PartialEq for Content  // by key only
  impl Eq for Content
  impl std::fmt::Debug for Content  // key and name, never the data

  /// Where the app is with a file's content.
  #[derive(Clone, Copy)]
  pub enum State<'a> { Loading, Failed(&'a str), Ready(&'a Content) }

  /// What the person asked of a preview in this frame.
  #[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
  #[must_use]
  pub struct PreviewResponse { pub retry: bool, pub open: bool }
  impl PreviewResponse { pub fn merge(self, other: PreviewResponse) -> PreviewResponse; } // OR of each field

  /// Draws a spinner for Loading, or the reason and a Retry button for Failed, centred in
  /// `ui`, and returns `None`. For Ready, draws nothing and returns the content. Sets
  /// `response.retry` when Retry is clicked.
  pub(crate) fn status_ui<'a>(ui: &mut egui::Ui, state: State<'a>, response: &mut PreviewResponse) -> Option<&'a Content>;
  ```
- New strings: `corundum-preview-retry = Retry` / `再試行`.

- [ ] **Step 1: Write the failing tests** in `tests_content.rs`. Register them in
  `mod.rs` with `#[cfg(test)] mod tests_content;`, which needs no feature.

```rust
#[test] fn bytes_open_as_a_fresh_cursor_each_time()      // read_to_end twice == the bytes, both times
#[test] fn a_reader_is_opened_on_each_open()             // an opener counting its calls: 2 opens → 2
#[test] fn file_content_reads_the_file_and_takes_its_name() // temp file "a.TXT": name "a.TXT", ext Some("txt")
#[test] fn content_is_equal_by_key_only()                // same key, other bytes → ==; other key → !=
#[test] fn loading_draws_a_spinner_and_no_content()      // status_ui(Loading) → None, a stroked path drawn
#[test] fn failed_shows_the_reason_and_retry_reports()   // label "disk gone" drawn; a click on "Retry" → response.retry
#[test] fn ready_draws_nothing_and_gives_the_content()   // Some(c) with c.key == "k", no labels
#[test] fn the_retry_string_exists_in_both_languages()   // the same pattern as tests.rs:176
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --no-default-features --lib preview::tests_content`.
  Expected: it does not compile, because `content` does not exist yet.
- [ ] **Step 3: Implement `content.rs`** with the interfaces above. Make `preview` a module
  in every build. Change the `mod.rs` doc to describe the module as a whole.
- [ ] **Step 4: Run** the same test command. Expected: PASS. Then run `cargo clippy -p corundum-framework-gui --all-targets --no-default-features -- -D warnings`.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui): preview content the app hands in, and its loading and failed states`

### Task 2: ImagePreview over Content

**Files:**
- Modify: `crates/corundum-framework-gui/src/preview/image.rs`, `crates/corundum-framework-gui/src/preview/tests.rs`

**Interfaces:**
- Consumes: `Content`, `State`, `PreviewResponse`, and `status_ui` (Task 1).
- Produces: `ImagePreview::ui(&mut self, ui: &mut egui::Ui, state: State<'_>) -> PreviewResponse`.
  `new` and `with_limits` are unchanged. The worker's `Request` carries the `Content`
  (which is `Clone`); `Kept` is matched by `content.key`.
- Removes: `Key::of` and its stat call, `RECHECK`, `Answer::Unchanged`, `set_recheck`,
  `Kept::checked`, and the `request_repaint_after` loop.

Decoding: for `Data::Bytes`, use `thumbnail(Source::Bytes { bytes, ext }, longest, limits)`.
For `Data::Reader`, read the whole file on the worker, then decode the same way. Read at
most `READ_MAX: u64 = 256 * 1024 * 1024` bytes; anything longer is `Decoded::TooLarge`. An
`io::Error` from opening or reading is `Decoded::Failed(e.to_string())`. Keep the
`PANIC_NAME` test hook, matching on `content.name`.

- [ ] **Step 1: Rewrite the tests.** Change `Screen::frame`, `Screen::until` and `png()` so
  they take a `Content`. `png` writes a PNG with the `image` dev-dependency and returns
  `Content::file(key, path)`; `png_bytes` returns `Content::bytes`. Keep the tests that
  still apply: the spinner then the image, the scroll area, an unsupported file, the
  strings, the thread ending, the target size, too large, and a panic. Delete
  `a_file_is_not_looked_at_every_frame` and `a_changed_file_is_shown_anew`. Add these:

```rust
#[test] fn bytes_are_decoded_like_a_file()          // a 64×32 PNG as Content::bytes → one texture of [64, 32]
#[test] fn a_newer_key_wins()                        // until-loop as a_newer_selection_wins, keyed "a", "b"
#[test] fn same_name_other_key_is_decoded_anew()     // "cover.png" 64×32 under key "1", then 32×64 under key "2" → [32, 64]
#[test] fn a_changed_key_decodes_again()             // key "v1" then "v2" for the same path, rewritten between → new size shown
#[test] fn a_reader_that_fails_says_why()            // opener returning Err(NotFound, "gone") → a label starting "Couldn't read this file"
#[test] fn loading_and_failed_states_draw_no_picture() // State::Loading → spinner, no texture; Failed("x") → "x" and "Retry"
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --no-default-features --features image-preview --lib preview::tests`.
  Expected: it does not compile, because of the old signature.
- [ ] **Step 3: Implement** the new `ui` and the worker, as described above. Update the
  type doc: remove the paragraph about the recheck, and say that the app's key names the
  picture.
- [ ] **Step 4: Run** the test command again, then clippy with `--features image-preview`.
  Expected: PASS, and no warnings.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui)!: ImagePreview draws content the app hands in`. In
  the body, say that `ui` takes a `State` and returns a `PreviewResponse`, and that the
  file is no longer re-stat'ed; the app's key says when it changed.

### Task 3: AudioPreview over Content, and one player for bytes and readers

**Files:**
- Modify: `crates/corundum-framework-gui/src/preview/audio.rs`, `crates/corundum-framework-gui/src/preview/tests_audio.rs`, `crates/corundum-framework-gui/src/preview/mod.rs` (re-export `ContentOpener`)

**Interfaces:**
- Consumes: Task 1's types; `corundum_framework_audio::{Opener, Feed, OutputConfig, QueueItem, Player, feed::spawn_feed, feed::FailedFeed?, decode::{Source, probe}}`.
  If `FailedFeed` is not public, add `pub` to it in `corundum-framework-audio/src/feed.rs`
  and re-export it.
- Produces:
  ```rust
  impl corundum_framework_audio::QueueItem for Content { fn title(&self) -> String { self.name.clone() } }
  /// Plays Content: bytes from memory, a reader from its opener; the extension is the hint.
  #[derive(Clone, Copy, Debug, Default)] pub struct ContentOpener;
  impl Opener<Content> for ContentOpener { /* open → spawn_feed(Box::new(AsSource(reader)), None, content.ext(), at_ms, target) */ }
  AudioPreview::ui(&mut self, ui: &mut egui::Ui, state: State<'_>, player: &Player<Content>) -> PreviewResponse
  ```
  `AsSource(Box<dyn Reader>)` is a private adapter that implements `decode::Source`. Its
  `byte_len` is found once, when it is made, by seeking to the end and back.
- The probe worker takes a `Content`, and `kept` is matched by key. `plays(state, content)`
  compares queue items by key. The title falls back to `content.name`.

- [ ] **Step 1: Rewrite the tests.** Change `Screen::frame`, `frame_with`, `until` and
  `click` so they take `State` or `&Content` and a `&Player<Content>`. `player()` spawns
  with `ContentOpener`. `wav()` returns `Content::file`, and `wav_bytes` returns
  `Content::bytes`. Port every existing test. Add these:

```rust
#[test] fn bytes_play_like_a_file()                       // wav_bytes → Play → wait_playing; queue == [content]
#[test] fn a_newer_file_wins()                            // frames of a then b before a's probe lands → b's name shown, never a's tags
#[test] fn same_name_other_key_is_another_track()         // play "x.wav" key "1"; show "x.wav" key "2" → Play shown, not Pause
#[test] fn a_reader_that_fails_shows_the_error_after_play() // opener Err → no tags; Play → player's error text shown
#[test] fn loading_state_shows_the_spinner_and_no_play()  // State::Loading → no "Play" label
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --no-default-features --features audio-preview --lib preview::tests_audio`.
  Expected: it does not compile.
- [ ] **Step 3: Implement** the code. Keep the existing behaviour: nothing plays on its own,
  Play sends `Command::Replace { items: vec![content.clone()], start: 0 }`, and the seek
  bar works only for the file that is playing. Update the docs on the type and the module.
- [ ] **Step 4: Run** the tests, then clippy with `--features audio-preview` and with
  `-p corundum-framework-audio --all-targets`. Expected: PASS, and no warnings.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui)!: AudioPreview plays content the app hands in, through ContentOpener`.
  In the body, say that the preview player is now `Player<Content>`, spawned with
  `ContentOpener`, and that `ui` takes a `State` and returns a `PreviewResponse`.

### Task 4: The info card, the kind of a file, and opening with the system's app

**Files:**
- Create: `crates/corundum-framework-gui/src/preview/kind.rs`, `crates/corundum-framework-gui/src/preview/info.rs`, `crates/corundum-framework-gui/src/preview/tests_file.rs` (the info-card tests start it)
- Modify: `crates/corundum-framework-gui/src/platform/mod.rs`, `crates/corundum-framework-gui/src/explorer/mod.rs` (make `TEXT_EXTENSIONS` `pub(crate)`), both `.ftl` files

**Interfaces:**
- Produces:
  ```rust
  // kind.rs
  #[derive(Clone, Copy, PartialEq, Eq, Debug)]
  pub enum Kind { Image, Audio, Video, Text, Other }
  /// By `name`'s extension, from platform::{IMAGE,AUDIO,VIDEO}_EXTENSIONS and explorer's TEXT_EXTENSIONS ("svg" is Image).
  pub fn kind_of(name: &str) -> Kind;

  // info.rs
  /// What the card says of a file. Built from an explorer Entry, or by hand.
  #[derive(Clone, Debug, Default)]
  pub struct FileInfo {
      pub name: String,
      pub size: Option<u64>,
      pub modified: Option<std::time::SystemTime>,
      pub created: Option<std::time::SystemTime>,
      pub icon: Option<egui::ImageSource<'static>>,
  }
  impl FileInfo { pub fn new(name: impl Into<String>) -> FileInfo; }
  impl From<&crate::explorer::Entry> for FileInfo   // name, size, modified, icon (else icon_for)

  /// The file's icon, name, size, kind and dates; and "Open with the system's app" when asked.
  pub struct InfoCard<'a> { /* file, open: bool, icon: bool, compact: bool */ }
  impl<'a> InfoCard<'a> {
      pub fn new(file: &'a FileInfo) -> InfoCard<'a>;   // icon on, open off, not compact
      pub fn open_button(self, on: bool) -> Self;       // the app's say; asteria leaves it off
      pub fn icon(self, on: bool) -> Self;              // the large icon and name above the details
      pub fn footer(self) -> Self;                      // compact: a separator, then small weak lines, no icon
      pub fn ui(self, ui: &mut egui::Ui) -> PreviewResponse; // .open set on a click of the button
  }

  // platform/mod.rs
  pub fn open_with_system_available() -> bool;  // DESKTOP
  /// Open `path` in the app the system gives its kind. Failures are ignored.
  pub fn open_with_system(path: &Path);         // the same programs as open_in_file_manager (explorer / open / xdg-open), with the file
  ```
- The details rows are Size, Kind, Modified and Created. A row is shown only when its value
  is known. Write the size in binary units with one decimal place: `1.2 KiB`, `3.4 MiB`;
  under 1024 bytes, write it in bytes (`512 B`). Write dates with the same formatting the
  explorer's list view already uses. Find it in `explorer/view.rs` and reuse it; do not add
  a second date format. The kind label is the uppercase extension plus a localized word for
  the kind, for example "PNG image" / "PNG 画像". With no extension, it is the word "File"
  / "ファイル".
- New strings, written in en / ja:
  - `corundum-preview-size` = Size / サイズ
  - `corundum-preview-kind` = Kind / 種類
  - `corundum-preview-modified` = Modified / 更新日時
  - `corundum-preview-created` = Created / 作成日時
  - `corundum-preview-open` = Open with the system's app / システムのアプリで開く
  - `corundum-preview-kind-image` = { $ext } image / { $ext } 画像
  - `-audio` = { $ext } audio / { $ext } 音声
  - `-video` = { $ext } video / { $ext } 動画
  - `-text` = { $ext } document / { $ext } 文書
  - `-other` = { $ext } file / { $ext } ファイル
  - `corundum-preview-kind-unknown` = File / ファイル

- [ ] **Step 1: Write the failing tests** in `tests_file.rs`. Register them in `mod.rs` with
  no feature `cfg`.

```rust
#[test] fn kinds_come_from_the_extension()          // "a.JPG"→Image, "b.flac"→Audio, "c.mkv"→Video, "d.md"→Text, "e.zip"→Other, "noext"→Other
#[test] fn the_card_shows_the_known_rows_only()     // FileInfo{name:"a.zip", size:Some(1536)} → "a.zip", "1.5 KiB", "ZIP file"; no "Modified"/"Created" label
#[test] fn the_open_button_is_the_app_s_choice()    // default → no "Open with the system's app"; open_button(true) → shown, and a click → response.open
#[test] fn the_footer_has_no_icon()                 // footer() → no texture/image shape from the icon; the size row still drawn
#[test] fn file_info_from_an_entry()                // Entry::file("x.png").size(3) → name, size Some(3)
#[test] fn the_card_strings_exist_in_both_languages()
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --no-default-features --lib preview::tests_file`.
  Expected: it does not compile.
- [ ] **Step 3: Implement** `kind.rs`, `info.rs`, the two platform functions, and the
  strings.
- [ ] **Step 4: Run** the tests, then clippy with no default features. Expected: PASS, and
  no warnings.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui): the preview's info card, and opening a file with the system's app`

### Task 5: FilePreview, the default, and the three compositions

**Files:**
- Create: `crates/corundum-framework-gui/src/preview/file.rs`
- Modify: `crates/corundum-framework-gui/src/preview/tests_file.rs`, `crates/corundum-framework-gui/src/preview/mod.rs` (the module doc with the three compositions)

**Interfaces:**
- Consumes: everything from Tasks 1–4.
- Produces:
  ```rust
  /// The default for one file: its picture or its player at the top, its details under
  /// them; the info card for a file with neither.
  #[derive(Default)]
  pub struct FilePreview { /* image: Option<ImagePreview> (image-preview), audio: Option<AudioPreview> + player: Option<Arc<Player<Content>>> (audio-preview) */ }
  impl FilePreview {
      pub fn new() -> FilePreview;
      #[cfg(feature = "audio-preview")]
      pub fn with_player(self, player: std::sync::Arc<Player<Content>>) -> FilePreview;
      /// Whether this preview draws `name`'s content. When it is false, the app need not
      /// fetch the content: the info card needs none.
      pub fn wants_content(&self, name: &str) -> bool;
      /// Draw `file`, with its content as `state`. `open_button` is passed to the card.
      pub fn ui(&mut self, ui: &mut egui::Ui, file: &FileInfo, state: State<'_>, open_button: bool) -> PreviewResponse;
  }
  ```
- `wants_content` is true when `kind_of(name)` is `Image` and `image-preview` is on, or
  when it is `Audio`, `audio-preview` is on, and there is a player.
- Layout when it wants content:
  - The part is drawn in a child rect of the full width. For an image, the rect's height is
    `min(width, available_height * 0.6)`. For audio, use the part's natural height.
  - Under it, `InfoCard::new(file).icon(false).open_button(open_button)` is drawn.
  - The part draws `State::Loading` and `Failed` itself, through `status_ui`.
- Layout otherwise: `InfoCard::new(file).open_button(open_button)` is drawn, and `state` is
  ignored.
- The response merges the part's response with the card's.

- [ ] **Step 1: Write the failing tests** in `tests_file.rs`. Gate each one on the feature
  it needs.

```rust
#[test] fn an_unknown_file_needs_no_content()                 // wants_content("a.zip") false; ui with State::Loading → card's "a.zip", no spinner
#[cfg(feature = "image-preview")]
#[test] fn an_image_shows_its_picture_and_details_under_it()  // until a texture is drawn; "Size" label's rect.top() > texture rect.bottom()
#[cfg(feature = "image-preview")]
#[test] fn a_failed_image_fetch_offers_retry()                // State::Failed("offline") → "offline", "Retry"; click → retry
#[cfg(feature = "audio-preview")]
#[test] fn audio_without_a_player_is_an_info_card()           // no with_player: wants_content("a.wav") false; card drawn, no "Play"
#[cfg(feature = "audio-preview")]
#[test] fn audio_with_a_player_shows_play()                   // with_player(fake) → "Play" label
#[test] fn replace_is_the_app_s_view_and_the_footer()         // compose: app label "NOTES" then InfoCard::footer → "NOTES" above "Size"; no icon
#[cfg(feature = "image-preview")]
#[test] fn add_puts_the_app_s_section_above_the_default()     // app label "TAGS" then FilePreview → "TAGS".bottom() <= texture.top()
```

- [ ] **Step 2: Run** the tests three ways: with `--no-default-features`, with
  `--no-default-features --features image-preview`, and with
  `--no-default-features --features audio-preview`, each over `preview::tests_file`.
  Expected: they do not compile, or they fail.
- [ ] **Step 3: Implement `file.rs`.** Then write the `preview` module doc. It explains
  that the app fetches and the parts draw, and gives a short `no_run` example of each of
  Default, Replace (with the footer) and Add.
- [ ] **Step 4: Run** the three test runs, then clippy for each of the three feature sets.
  Expected: PASS, and no warnings.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui): FilePreview, the detail pane's default by a file's kind (#97)`

### Task 6: The docs and the umbrella crate

**Files:**
- Modify: `crates/corundum-framework-gui/Cargo.toml` (the comments on `image-preview` and `audio-preview`), `crates/corundum-framework-gui/src/lib.rs:18-20`, `crates/corundum-framework/Cargo.toml:95-100`, `crates/corundum-framework/src/lib.rs:32-35`, `crates/corundum-framework/tests/features.rs:90-95`, `README.md:118-119,200-203`, `README.ja.md` (the same rows), `docs/file-previews.md` and `docs/file-previews.ja.md` ("Where this is used")

- [ ] **Step 1: Update `features.rs`.** The `audio-preview` test also checks that
  `corundum_framework::gui::preview::ContentOpener` is an `Opener<Content>`. Add a test
  with no `cfg` that names `gui::preview::FilePreview::new` and
  `gui::preview::Content::bytes`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework --features gui,audio-preview --test features`.
  Expected: PASS once the re-exports are in place. Before them, it fails.
- [ ] **Step 3: Update the doc text** in the files listed above. It must say that the app
  hands in content; that `FilePreview` and `InfoCard` exist without any feature; and that
  the preview player is `Player<Content>` with `ContentOpener`. In file-previews,
  "Where this is used" gains a line that links to
  `superpowers/specs/2026-11-05-detail-pane-previews-design.md`.
- [ ] **Step 4: Run** `cargo doc -p corundum-framework-gui --no-deps --all-features`.
  Expected: no broken intra-doc links.
- [ ] **Step 5: Commit:** `docs: the detail pane's previews over content the app hands in`

### Task 7: The demo's detail pane

**Files:**
- Modify: `demo/Cargo.toml`, `demo/src/screens/files.rs`, and the demo's `.ftl` files (wherever `demo-files-*` keys live)
- Create: `demo/assets/sample.png`, a PNG of 32 KB or less. Render it from
  `demo/assets/app-icon.svg` at 256 px with the demo's `icons` example
  (`cargo run -p corundum-framework-demo --example icons`, which writes `icons/icon.png`),
  or with any tool, and commit it.

**Interfaces:**
- Consumes: `FilePreview`, `InfoCard`, `FileInfo`, `Content`, `State`, `ContentOpener`,
  `platform::open_with_system`, `detail::DetailPane`, and `CpalOutputs`.

**Behaviour:**
- **Features:** add `image-preview` and `audio-preview` to the demo's `corundum-framework`
  features on both target sections: not-Android and Android. Mobile CI already builds the
  gui crate with each of these on iOS and Android (`.github/workflows/mobile-ci.yml:56,132`).
  Only if `cargo ndk … clippy -p corundum-framework-demo` or the iOS clippy fails because
  of them, move `audio-preview` to the targets where it builds, and say why in a comment.
- **State:** the Files screen holds a `DetailPane::new("files-detail")`, a `FilePreview`,
  and an `Option<Arc<Player<Content>>>`. Spawn the player once, lazily, with
  `ContentOpener`, `CpalOutputs`, and a repaint of the context. If spawning fails, leave it
  `None`; audio files then show the info card.
- **Selection:** when `explorer.selected()` is exactly one file, the detail shows it.
  Otherwise it shows the existing weak text that nothing is selected. (Add
  `demo-files-none` if no such string exists.) Selecting a file also calls
  `DetailPane::open()`, for the card and phone layouts.
- **Content:**
  - local files use `Content::file(key, path)`, where `key` is `"{path}:{modified_secs}:{size}"`
    taken from the `Entry`;
  - the state is always `State::Ready`;
  - a `retry` response re-reads the folder;
  - an `open` response calls `open_with_system(path)`;
  - `open_button` is `open_with_system_available()`.
- **The demo's own view (Replace):** for `.txt` files, the demo reads the text and shows it
  in a monospace label, with the first 4 KiB at most, followed by
  `InfoCard::new(&info).footer()`. Read the text on the same listing thread, or lazily with
  a cache keyed by the key; it is a demo, and the file is small.
- **Samples:**
  - `make_sample` writes `demo/assets/sample.png` (`include_bytes!`) to `Pictures/sample.png`
    and to `Music/First album/cover.png`. The latter replaces today's 1,200 zero bytes.
  - It also writes `Music/First album/00 Tone.wav`, a 2 s, 44.1 kHz mono 440 Hz tone made
    by a `fn tone_wav() -> Vec<u8>` that writes the 44-byte header by hand. Take it from
    `tests_audio.rs`'s `wav()`.
  - The zero-filled `.mp3` tracks stay as they are. They show the "not audio" path.
- **Tests:** the existing demo tests still pass. Add
  `#[test] fn the_tone_is_a_wav_the_decoder_reads()`: `decode::probe` of `tone_wav()` in a
  `Cursor` gives `duration_ms` of `Some(2000)`, give or take 50.

- [ ] **Step 1: Write the `tone_wav` test** in `files.rs`'s tests, or add a test module if
  there is none. Run `cargo test -p corundum-framework-demo tone`. Expected: it does not
  compile.
- [ ] **Step 2: Implement** the features, the state, the detail pane, the `.txt` view, and
  the samples, as described above.
- [ ] **Step 3: Run** `cargo test -p corundum-framework-demo`, then `cargo clippy -p corundum-framework-demo --all-targets -- -D warnings`.
  Expected: PASS, and no warnings.
- [ ] **Step 4: Run the demo** (`cargo run -p corundum-framework-demo`) and check the
  Files screen:
  - `Pictures/sample.png` shows the picture with Size, Kind and Modified under it, and an
    Open button;
  - `00 Tone.wav` plays;
  - `notes.txt` shows its text with the footer;
  - a `.mp3` shows "not audio" with the details;
  - `Many files/file 1.txt` uses the `.txt` view;
  - a folder shows nothing selected.

  Take a screenshot of the picture case for the PR.
- [ ] **Step 5: Commit:** `feat(demo): the Files screen's detail pane, with the default previews and a view of its own for text`

### Task 8: Whole-crate verification

- [ ] **Step 1: Run** `cargo test --workspace --all-features --locked`. Expected: PASS.
- [ ] **Step 2: Run** clippy for the gui crate with the CI loop's feature sets
  (`ci.yml:19-47`): `--no-default-features`; `--no-default-features --features serde`; and
  `--no-default-features --features $f` for each of `image-preview` and `audio-preview`.
  Expected: no warnings.
- [ ] **Step 3: Run** `cargo fmt --all --check`. Expected: no diff.
