# Text Preview (#99) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preview plain text files in the detail pane. This adds `text-preview`, which
handles the encoding and a size limit, and re-exports alumina-code's editor for the apps
behind `code-editor`.

**Architecture:**
- **Decoding:** an egui-free function in `corundum-framework-gui/src/preview/text/decode.rs`
  turns the first 256 KiB of a file into text, or into the reason there is none.
- **`TextPreview`:** follows #97's part contract. It decodes on a worker and keeps the
  result by key, with Retry when reading fails. It shows the text in alumina-code's
  `CodeEditor` with `read_only(true)`.
- **Dispatch:** `FilePreview` sends text-like extensions to `TextPreview`.
- **Re-export:** `gui::code` is alumina-code, under `code-editor`.

**Tech Stack:** Rust 2024, egui 0.37, alumina-code 0.1, chardetng and encoding_rs.

**Spec:** `docs/superpowers/specs/2026-11-05-text-markdown-pdf-datetime-design.md` (its
sections "Shared decisions" and "1. #99"). This plan builds on #97
(`docs/superpowers/specs/2026-11-05-detail-pane-previews-design.md`, PR #118). The branch is
`feat/text-preview`, stacked on `feat/detail-pane-previews`.

## Global Constraints

- **Features:** every new piece sits behind a feature that is off by default.
  - `code-editor = ["dep:alumina-code"]`.
  - `text-preview = ["code-editor", "dep:chardetng", "dep:encoding_rs"]`.
  - The default build gains no dependency.
- **Dependency versions:** `alumina-code = "0.1"`, `chardetng = "0.1"`,
  `encoding_rs = "0.8"`. Take the latest of each on crates.io that builds with egui 0.37.
  All three are pure Rust under MIT or Apache-2.0. `cargo deny check` must pass
  (`deny.toml`).
- **The part contract from #97:**
  - `TextPreview::ui(&mut self, ui, state: State<'_>) -> PreviewResponse`.
  - `Loading` and `Failed` are drawn through `status_ui`.
  - Results are kept by `Content::key`, at most 8.
  - A reader failure shows `retry_button`. A click evicts the entry and sets
    `response.retry`.
  - A panic on the worker becomes a failure.
  - The UI thread never calls `Content::open`.
- **Limit:** `TEXT_LIMIT: usize = 256 * 1024` bytes are read at most. Read
  `TEXT_LIMIT + 1` to learn whether the file was cut.
- **Strings:** every string the user sees goes through `tr!`, with keys in
  `crates/corundum-framework-gui/locales/{en,ja}/corundum.ftl`, named `corundum-preview-*`.
  Reuse `corundum-preview-unsupported` for binary files.
- **CI:** clippy `-D warnings --all-targets` is clean in each of these gui builds:
  `--no-default-features`, and `--no-default-features --features` with `image-preview`,
  `audio-preview`, `code-editor` and `text-preview` in turn. Add `code-editor` and
  `text-preview` to the feature loops in `.github/workflows/ci.yml:46` and
  `.github/workflows/mobile-ci.yml:56,132`.
- **Commits:** conventional, each ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not edit `CHANGELOG.md`.

## Review Focus

1. **A file cut in the middle of a multibyte character**, such as UTF-8 or Shift_JIS cut
   at 256 KiB. The text shows without a replacement character at the end, and the "cut"
   note is shown. Test: Task 2 `a_cut_inside_a_character_drops_the_partial_character`.
2. **An empty file.** It shows an empty text, not "can't be previewed". Test: Task 2
   `an_empty_file_is_empty_text`.
3. **UTF-16 text contains NUL bytes,** so the binary check must not reject a file with a
   BOM. Test: Task 2 `utf16_with_a_bom_is_text_not_binary`.
4. **A short Latin-1 or ASCII file** that chardetng could guess wrong. ASCII decodes as
   itself. Test: Task 2 `ascii_is_itself`.
5. **Switching quickly between text files.** The text of B is never replaced by A's late
   answer. Test: Task 3 `a_newer_key_wins`.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/corundum-framework-gui/Cargo.toml` | The `code-editor` and `text-preview` features, with comments, and the new dependencies. |
| `crates/corundum-framework-gui/src/code.rs` (new) | `gui::code`, the alumina-code re-export (`pub use alumina_code::*;`), with a module doc. |
| `crates/corundum-framework-gui/src/lib.rs` | `#[cfg(feature = "code-editor")] pub mod code;`, and the crate doc. |
| `crates/corundum-framework-gui/src/preview/text/mod.rs` (new) | `TextPreview`. |
| `crates/corundum-framework-gui/src/preview/text/decode.rs` (new) | `decode_text`, `Text` and `NotText`: egui-free. |
| `crates/corundum-framework-gui/src/preview/tests_text.rs` (new) | `TextPreview`'s frames. The decode tests sit in `decode.rs`. |
| `crates/corundum-framework-gui/src/preview/{mod.rs,file.rs,kind.rs}` | The re-exports, the dispatch, and `wants_content`. |
| `crates/corundum-framework/{Cargo.toml,src/lib.rs,tests/features.rs}`, `README.md`, `README.ja.md`, `docs/file-previews{,.ja}.md` | The umbrella features and the docs. |
| `.github/workflows/{ci.yml,mobile-ci.yml}` | The feature loops. |
| `demo/Cargo.toml`, `demo/src/screens/files.rs` | `text-preview` on, and a Shift_JIS `.cue` in the sample. |

---

### Task 1: `code-editor`, alumina-code as `gui::code`

**Files:**
- Create: `crates/corundum-framework-gui/src/code.rs`
- Modify: `crates/corundum-framework-gui/Cargo.toml`, `crates/corundum-framework-gui/src/lib.rs`, `crates/corundum-framework/Cargo.toml`, `crates/corundum-framework/src/lib.rs` (the feature table), `crates/corundum-framework/tests/features.rs`, `README.md` and `README.ja.md` (both feature tables), and `.github/workflows/ci.yml:46` plus `mobile-ci.yml:56,132` (add `code-editor`)

**Interfaces:**
- Produces:
  - `corundum_framework_gui::code`, which re-exports everything public in `alumina_code`
    (`CodeEditor`, `CodeOutput`, `CodeTheme`, `Highlighter`, `TokenKind`, `Indent`, and the
    edit functions);
  - the gui feature `code-editor`;
  - the umbrella feature `code-editor = ["gui", "corundum-framework-gui/code-editor"]`.
- The module doc says three things: the editor's colours follow the visuals, so
  corundum's theme applies; the app brings its own highlighter; and the module is
  alumina-code, re-exported so an app depends on corundum alone.

- [ ] **Step 1: Write the failing test.** In `crates/corundum-framework/tests/features.rs`,
  add `#[cfg(feature = "code-editor")] #[test] fn code_editor_is_alumina_s()`. It names
  `corundum_framework::gui::code::CodeEditor::new`, which needs a `&mut String`, and
  `corundum_framework::gui::code::Indent::default()`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework --features code-editor --test features`.
  Expected: it fails, because the feature does not exist yet.
- [ ] **Step 3: Implement.** Add the features and the module. Update the docs, the
  README rows and the CI loops.
- [ ] **Step 4: Run** the test, then
  `cargo clippy -p corundum-framework-gui --all-targets --no-default-features --features code-editor -- -D warnings`,
  then `cargo deny check`. Expected: PASS, clean, and ok.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui): alumina-code's editor as gui::code (code-editor)`

### Task 2: Decoding text, egui-free

**Files:**
- Create: `crates/corundum-framework-gui/src/preview/text/decode.rs` (with its `#[cfg(test)] mod tests`), and `crates/corundum-framework-gui/src/preview/text/mod.rs` (only `mod decode;` in this task)
- Modify: `crates/corundum-framework-gui/Cargo.toml` (`text-preview`, chardetng, encoding_rs), `crates/corundum-framework-gui/src/preview/mod.rs` (`#[cfg(feature = "text-preview")] mod text;`)

**Interfaces:**
- Produces, `pub(crate)` inside `preview::text`:
  ```rust
  pub(crate) const TEXT_LIMIT: usize = 256 * 1024;
  #[derive(Clone, Debug, PartialEq, Eq)]
  pub(crate) struct Text { pub text: String, pub cut: bool, pub encoding: &'static str } // encoding: encoding_rs name, e.g. "UTF-8", "Shift_JIS"
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub(crate) enum NotText { Binary }
  /// `bytes` are the file's first bytes, at most TEXT_LIMIT + 1 of them: more than
  /// TEXT_LIMIT means the file was cut, and only the first TEXT_LIMIT are decoded.
  pub(crate) fn decode_text(bytes: &[u8]) -> Result<Text, NotText>;
  ```
- The rules, in this order:
  1. Keep only the first `TEXT_LIMIT` bytes. `cut` is whether there were more.
  2. **BOM:** `EF BB BF` means UTF-8; `FF FE` means UTF-16LE; `FE FF` means UTF-16BE.
     The BOM is dropped from the text.
  3. **No BOM:** a NUL byte in the kept bytes means `Err(Binary)`.
  4. **No BOM:** valid UTF-8, allowing an incomplete sequence at the very end when
     `cut`, means UTF-8.
  5. **Otherwise:** guess with `chardetng::EncodingDetector`. Feed it the bytes, with
     `last = !cut`, then call `guess(None, true)`, and decode with that encoding.
  6. **When `cut`,** a partial character at the end is dropped, not shown as U+FFFD. With
     encoding_rs, decode with `Decoder::decode_to_string_without_replacement` and
     `last = false`. Stop at the first malformed sequence only if it is at the end.
     Elsewhere, replacement is fine.

- [ ] **Step 1: Write the failing tests** in `decode.rs`.

```rust
#[test] fn utf8_without_a_bom()                    // "メモ\nnote" bytes → text == "メモ\nnote", encoding "UTF-8", !cut
#[test] fn utf8_bom_is_dropped()                   // EF BB BF + "a" → "a"
#[test] fn utf16_with_a_bom_is_text_not_binary()   // FF FE + "a\0b\0" → "ab", encoding "UTF-16LE"; FE FF + "\0a" → "a" ("UTF-16BE")
#[test] fn a_shift_jis_cue_sheet()                 // encoding_rs::SHIFT_JIS.encode("TITLE \"夜の歌\"\r\nPERFORMER \"山田\"\r\n") → text contains "夜の歌", encoding "Shift_JIS"
#[test] fn euc_jp_is_read()                        // encoding_rs::EUC_JP.encode of a few lines of Japanese (≥ 40 chars) → round-trips, encoding "EUC-JP"
#[test] fn a_nul_without_a_bom_is_binary()         // [0x89, b'P', b'N', b'G', 0, 0] → Err(Binary)
#[test] fn an_empty_file_is_empty_text()           // [] → Ok(text "", !cut)
#[test] fn ascii_is_itself()                       // b"hello, world\n" → "hello, world\n", encoding "UTF-8"
#[test] fn over_the_limit_is_cut()                 // vec![b'a'; TEXT_LIMIT + 1] → text.len() == TEXT_LIMIT, cut
#[test] fn a_cut_inside_a_character_drops_the_partial_character()
                                                   // "あ" repeated to TEXT_LIMIT+3 bytes (3 bytes each), take TEXT_LIMIT+1 → no '\u{FFFD}', text.len() % 3 == 0, cut
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --no-default-features --features text-preview --lib preview::text`.
  Expected: it does not compile.
- [ ] **Step 3: Implement** `decode.rs` by the rules above.
- [ ] **Step 4: Run** the tests, then clippy with `--features text-preview`. Expected:
  PASS and clean. Clippy may warn that `decode_text` is dead code until Task 3; until
  then, mark it with `#[cfg_attr(not(test), allow(dead_code))]` and a comment.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui): decode a text file's first 256 KiB, guessing its encoding`

### Task 3: `TextPreview`

**Files:**
- Modify: `crates/corundum-framework-gui/src/preview/text/mod.rs`, `crates/corundum-framework-gui/src/preview/mod.rs` (`pub use text::TextPreview;` and `mod tests_text;` under `text-preview`), and both `.ftl` files
- Create: `crates/corundum-framework-gui/src/preview/tests_text.rs`

**Interfaces:**
- Consumes:
  - `Content`, `State`, `PreviewResponse`, `status_ui` and `retry_button` from
    `preview/content.rs`;
  - `decode_text`, `Text` and `TEXT_LIMIT` from Task 2;
  - `crate::code::CodeEditor` from Task 1.
- Produces:
  ```rust
  /// A text file in the detail pane: decoded on a worker thread, shown read-only and
  /// monospace in a scroll area, where it can be selected and copied.
  #[derive(Default)]
  pub struct TextPreview { /* worker: Option<Worker>, asked: Option<String>, kept: Vec<(String, Outcome)> */ }
  impl TextPreview {
      pub fn new() -> TextPreview;
      pub fn ui(&mut self, ui: &mut egui::Ui, state: State<'_>) -> PreviewResponse;
  }
  ```
- **The worker** follows `image.rs`'s worker. The newest request wins; the answer is
  `Result<Text, NotText>` or a read error. It reads with `content.open()?` and
  `.take(TEXT_LIMIT as u64 + 1).read_to_end(..)`, inside `catch_unwind`.
- **Drawing:**
  - `Ready`, still pending: a spinner.
  - A `Text`: `egui::ScrollArea::both()` holding
    `CodeEditor::new(&mut kept_text).read_only(true).id(..)`. The id is made from the key,
    so the scroll and the selection stay per file. If `cut`, a weak label first:
    `corundum-preview-text-cut` = "Showing the first { $size }." /
    "先頭の { $size } だけを表示しています。". `$size` is `explorer::format_size(TEXT_LIMIT)`.
  - `Binary`: the file icon and `corundum-preview-unsupported`, laid out as
    `ImagePreview` does it.
  - A read error: `corundum-preview-failed` and `retry_button`.
- `CodeEditor` needs `&mut String`. Keep the decoded `String` in `kept`, and lend it
  mutably. Read-only means it never changes.

- [ ] **Step 1: Write the failing tests** in `tests_text.rs`, with the helpers of
  `tests_content.rs` and `tests.rs`.

```rust
#[test] fn a_text_shows_after_a_spinner()        // Content::bytes("k","a.txt","hello") → spinner first, then a label/galley containing "hello"
#[test] fn a_cut_file_says_so()                  // vec![b'a'; TEXT_LIMIT + 10] → text containing "Showing the first"
#[test] fn a_binary_file_cannot_be_previewed()   // [0,1,2,0] → "This file can't be previewed."
#[test] fn a_reader_that_failed_offers_retry()   // opener fails once then succeeds → "Retry"; click → response.retry; then the text shows
#[test] fn a_newer_key_wins()                    // ask "a" then "b" before a's answer → no frame after b shows a's text; b's shows
#[test] fn loading_and_failed_states()           // Loading → spinner; Failed("offline") → "offline" + "Retry"
#[test] fn the_text_strings_exist_in_both_languages()
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --no-default-features --features text-preview --lib preview::tests_text`.
  Expected: it does not compile.
- [ ] **Step 3: Implement** `TextPreview`. Remove Task 2's `allow(dead_code)`.
- [ ] **Step 4: Run** the tests, then clippy with `--features text-preview` and with no
  features. Expected: PASS and clean.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui): TextPreview, a text file read-only in the detail pane (#99)`

### Task 4: `FilePreview` dispatch, the umbrella crate, docs, CI and the demo

**Files:**
- Modify:
  - `crates/corundum-framework-gui/src/preview/{file.rs,tests_file.rs,mod.rs}` (dispatch, tests, module doc);
  - `crates/corundum-framework/{Cargo.toml,src/lib.rs,tests/features.rs}`;
  - `README.md` and `README.ja.md`;
  - `docs/file-previews.md` and `docs/file-previews.ja.md` ("Where this is used": a link to the spec);
  - `.github/workflows/ci.yml:46` and `mobile-ci.yml:56,132` (add `text-preview`);
  - `demo/Cargo.toml` (add `text-preview` to both target sections) and `demo/src/screens/files.rs`.

**Interfaces:**
- `FilePreview` gains a `text: TextPreview` field (`text-preview`).
- `wants_content(name)` is true for `TEXT_EXTENSIONS_SHOWN` when `text-preview` is on.
  These are `["txt", "text", "md", "markdown", "cue", "log", "m3u", "m3u8", "json",
  "toml", "yaml", "yml", "csv", "tsv", "ini", "cfg", "conf", "xml", "srt", "vtt", "nfo",
  "rs", "py", "js", "ts", "sh"]`, matched case-insensitively. Put the constant in
  `file.rs`.
- In `part()`, the text kind gets the bounded rect, the same height as an image
  (`min(width, avail * 0.6)`), so the details card stays visible under it.
- **Umbrella feature:**
  `text-preview = ["gui", "code-editor", "corundum-framework-gui/text-preview"]`.
- **The demo** keeps its own `.txt` view (Replace), as it is. The default text preview
  shows everything else. `make_sample` adds
  `Music/First album/album.cue`, a cue sheet encoded as Shift_JIS with
  `encoding_rs::SHIFT_JIS.encode`. If the demo has no encoding_rs, write the bytes in a
  `const` array.

- [ ] **Step 1: Write the failing tests** in `tests_file.rs` (under `text-preview`) and
  `features.rs`.

```rust
#[cfg(feature = "text-preview")] #[test] fn text_files_want_content()      // wants_content("a.cue"), ("B.LOG"), ("x.md") true; ("a.docx"), ("a.pdf") false
#[cfg(feature = "text-preview")] #[test] fn a_text_file_shows_its_text_and_details() // "notes.txt" bytes "hi" → "hi" drawn, "Size" label below it
// features.rs:
#[cfg(feature = "text-preview")] #[test] fn text_preview_is_the_gui_crate_s() // names gui::preview::TextPreview::new
```

- [ ] **Step 2: Run** them. Expected: FAIL.
- [ ] **Step 3: Implement** the dispatch, the umbrella feature, the docs, the CI loops
  and the demo.
- [ ] **Step 4: Run all of these:**
  - `cargo test --workspace --all-features --locked`;
  - clippy for the gui crate in the six builds listed under Global Constraints;
  - `cargo clippy -p corundum-framework-demo --all-targets -- -D warnings`;
  - `cargo fmt --all --check`;
  - `cargo deny check`.

  Expected: all pass and are clean.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui): FilePreview shows text files; text-preview in the umbrella crate, CI and the demo`
