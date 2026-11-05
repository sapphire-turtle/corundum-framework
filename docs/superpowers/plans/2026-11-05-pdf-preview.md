# PDF Preview (#98) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preview PDF files in the detail pane. The preview renders their pages with
hayro, a pure-Rust renderer, through a new crate that does not depend on egui.

**Architecture:**
- **`corundum-framework-pdf`** is laid out like corundum-framework-image. It opens a PDF
  from bytes, counts its pages, and renders one page to RGBA at a given longest side. Its
  `Error` says why a file can't be shown: encrypted, broken, too large, or unsupported.
  It has a font hook for fonts the PDF does not embed.
- **`pdf-preview`** (gui) adds `PdfPreview`. It follows #97's part contract. It renders
  the current page on a worker and keeps each rendered page by key and page. It shows
  previous and next buttons and the text "page n / N".
- **Fonts:** if a PDF needs a CJK font it does not embed, corundum passes the system's CJK
  font, the one `fonts::system_cjk_font()` already finds for egui. No font ships with the
  app.

**Tech Stack:** Rust 2024, hayro 0.9 (MIT OR Apache-2.0), egui 0.37.

**Spec:** `docs/superpowers/specs/2026-11-05-text-markdown-pdf-datetime-design.md`,
section "4. #98", which includes the calls written there. The branch is
`feat/pdf-preview`, stacked on `feat/datetime` (PR #124).

## Global Constraints

- **Features:**
  - `corundum-framework-pdf` has no egui, and its default features are hayro's needs
    only.
  - The gui feature `pdf-preview = ["dep:corundum-framework-pdf"]` is off by default.
  - The umbrella crate gets `pdf = ["dep:corundum-framework-pdf"]`, re-exported as
    `corundum_framework::pdf`, and `pdf-preview = ["gui", "pdf", "corundum-framework-gui/pdf-preview"]`.
    Mirror `image` and `image-preview`.
  - The default build gains no dependency, and `cargo deny check` passes.
- **Limits, from the spec:**
  - `MAX_INPUT: usize = 64 * 1024 * 1024` bytes. Anything larger is `Error::TooLarge`.
  - `MAX_PAGES: usize = 500`. Paging stops at 500, and the count reads
    "page n / 500 of N".
  - A page renders to at most 2048 px on its longest side. Use `ImagePreview`'s sizes,
    256, 512, 1024 and 2048, chosen the same way.
- **The part contract from #97:**
  - `ui(&mut self, ui, State<'_>) -> PreviewResponse`;
  - render on the worker, catching a panic as a failure;
  - keep at most 8 rendered pages, by `(key, page, size)`;
  - Retry evicts, and the UI thread never calls `Content::open`;
  - a reader `Content` is read whole on the worker, up to `MAX_INPUT`.
- **Strings:** every string goes through `tr!`, with `corundum-preview-pdf-*` keys in both
  `.ftl` files. Egui ids never start with `corundum-`.
- **CI:**
  - clippy `-D warnings --all-targets` is clean for the gui crate with no default
    features, and with `--no-default-features --features pdf-preview`;
  - clippy is clean for `-p corundum-framework-pdf`;
  - add `pdf-preview` to the gui loops in `ci.yml:46` and both `mobile-ci.yml` loops;
  - add a `pdf-without-egui` check like the existing `image-without-egui` job (see
    `ci.yml`).
- **Release:** the new crate is published like the image crate. Add it to the workspace
  members and to `release-plz.toml`'s group, mirroring `corundum-framework-image`. Give it
  a README.
- **Commits:** conventional, ending with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not edit `CHANGELOG.md`.

## Review Focus

1. **An encrypted PDF** shows "This PDF is password-protected and can't be previewed."
   It does not panic and does not show a blank page. Test: Task 2 `an_encrypted_pdf_says_so`.
2. **A broken or truncated PDF,** for example the first half of a valid file, gives
   `Error::Broken` and doesn't panic. Test: Task 2 `a_truncated_pdf_is_broken`.
3. **Paging past the end and before the start.** Next on the last page and Previous on
   the first are disabled, and the page index never leaves `0..min(N, 500)`. Test: Task 3
   `the_page_buttons_stop_at_the_ends`.
4. **Switching files while a page renders.** The new file's page 1 shows, and the page
   index resets to 0 for a new key. Test: Task 3 `a_new_file_starts_on_its_first_page`.
5. **A page with a huge MediaBox,** such as a 5 m poster, renders at most 2048 px and
   does not allocate its size at 72 dpi. Test: Task 2 `a_huge_page_renders_within_the_limit`.

---

### Task 1: Can hayro render the files the issue names? Fixtures and a finding

**Files:**
- Create:
  - `crates/corundum-framework-pdf/tests/fixtures/` with the PDFs below and a
    `README.md` giving each fixture's source and licence;
  - `docs/superpowers/specs/2026-11-05-pdf-hayro-findings.md`, a short write-up.

**What to do:** this is the issue's first checkbox. Produce these small PDFs, each under
200 KB:
1. `ja-embedded.pdf`: Japanese text, for example 「旅行のチケット」 and a few lines, with
   the font embedded as a subset. Use a font under the SIL OFL, such as Noto Sans JP. The
   simplest way is a throwaway generator in the scratchpad
   `C:\Users\fluo10\AppData\Local\Temp\claude\f5af5ba235431f37\3fddbf90-24e7-47ba-a962-1b37537693aa\scratchpad`
   that uses the `krilla` crate (MIT OR Apache-2.0), which subsets fonts. Download the
   font there too. Commit only the output PDF. Nothing the generator needs goes into the
   repo or the build.
2. `ja-not-embedded.pdf`: the same text as a Type0/CIDFontType0 font with Adobe-Japan1
   ordering and no font file. Hand-write it, since a PDF is text, or generate it.
3. `scan.pdf`: one page that holds only a JPEG image, a few KB.
4. `encrypted.pdf`: any page, with the standard security handler (RC4 or AES) and an
   empty user password. Hand-write it, or generate it with any tool.
5. `two-pages.pdf`: two pages with different sizes (A4 and Letter), plus plain text in a
   standard-14 font (Helvetica).

Render each one with hayro 0.9 in the same throwaway generator, at about 1024 px, and look
at the PNGs. For `ja-not-embedded.pdf`, also render it with a resolver that answers
`FontQuery::Fallback` (Adobe-Japan1) with a system Japanese font: on Windows, Yu Gothic
or Meiryo from `C:\Windows\Fonts`. hayro's `examples/render.rs` shows the resolver. Write
the findings doc with these points:
- what each fixture shows, and anything wrong;
- whether the encrypted file errors or renders;
- whether the fallback works;
- what the `embed-fonts` feature adds, both in size and in what it covers;
- your recommendation for the defaults.

Attach no PNGs; a sentence each is enough.

- [ ] **Step 1:** Make the fixtures and render them.
- [ ] **Step 2:** Write the findings and the fixture README.
- [ ] **Step 3: Commit:** `test(corundum-framework-pdf): sample PDFs, and what hayro makes of them (#98)`.
  The commit holds only the fixtures and the docs; the crate comes in Task 2.

**Stop and report** (NEEDS_CONTEXT) if hayro cannot render `ja-embedded.pdf` or
`two-pages.pdf` at all. The rest of the plan assumes it can.

### Task 2: `corundum-framework-pdf`, rendering without egui

**Files:**
- Create: `crates/corundum-framework-pdf/{Cargo.toml,README.md,src/lib.rs,src/error.rs,src/render.rs}`, and `tests/render.rs`
- Modify: the workspace `Cargo.toml`, `release-plz.toml`, the umbrella crate (`pdf` feature and re-export, `features.rs`), and `README.md`/`README.ja.md` (the crate list and the umbrella feature table)

**Interfaces:**
```rust
/// Why a PDF has no picture.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error { Encrypted, Broken(String), TooLarge, Unsupported(String) }

pub const MAX_INPUT: usize = 64 * 1024 * 1024;
pub const MAX_PAGES: usize = 500;

/// A font for text the PDF does not embed: the app's answer to "a CJK font, please".
#[derive(Clone)]
pub struct FallbackFont { pub data: std::sync::Arc<Vec<u8>>, pub index: u32 }

/// An opened PDF.
pub struct Document { /* hayro Pdf, page sizes, fallback */ }
impl Document {
    /// Open `bytes`, at most MAX_INPUT of them. `cjk` serves Adobe-Japan1/GB1/CNS1/Korea1
    /// fallback queries; the standard 14 fonts are hayro's own (per Task 1's finding).
    pub fn open(bytes: std::sync::Arc<[u8]>, cjk: Option<FallbackFont>) -> Result<Document, Error>;
    /// How many pages the file has (may exceed MAX_PAGES).
    pub fn page_count(&self) -> usize;
    /// Render page `index` (0-based, < min(page_count, MAX_PAGES)) to sRGB RGBA8 with its
    /// longest side `longest` px, on white.
    pub fn render(&self, index: usize, longest: u32) -> Result<Page, Error>;
}
pub struct Page { pub width: u32, pub height: u32, pub rgba: Vec<u8> }
```
- `Document` need not be `Send`. hayro uses `Rc`, so the gui worker opens and renders on
  its own thread. State this in the doc.
- Wrap `open` and `render` in `catch_unwind`, so a panic becomes
  `Error::Broken("the PDF renderer panicked")`.
- The scale for `longest` comes from the page's size in points. A huge MediaBox is
  therefore never rendered at its native size.

- [ ] **Step 1: Write the failing tests** in `tests/render.rs`. They use Task 1's
  fixtures.

```rust
#[test] fn two_pages_are_counted_and_sized()        // two-pages.pdf: page_count 2; render(0, 512) longest side 512, aspect ≈ A4; render(1, 512) ≈ Letter
#[test] fn text_draws_ink()                         // render(0, 512) of two-pages.pdf has some non-white pixels in its top half
#[test] fn embedded_japanese_draws_ink()            // ja-embedded.pdf → non-white pixels
#[test] fn a_scan_renders()                         // scan.pdf → non-white pixels
#[test] fn an_encrypted_pdf_says_so()               // encrypted.pdf → open(..) or render(..) is Err(Error::Encrypted)
#[test] fn a_truncated_pdf_is_broken()              // first half of two-pages.pdf → Err(Broken(_)), no panic
#[test] fn too_large_input_is_refused()             // vec![0; MAX_INPUT + 1] → Err(TooLarge) without parsing
#[test] fn a_huge_page_renders_within_the_limit()   // a hand-built PDF string with MediaBox [0 0 14173 14173] (5 m) → render(0, 2048) is 2048×2048, fast
#[test] fn not_embedded_japanese_uses_the_fallback() // ja-not-embedded.pdf: with cjk: None renders without panic; with a FallbackFont from the system (skip the assertion when none is found, as fonts.rs's tests do) → more ink than without
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-pdf`. Expected: it does not
  compile.
- [ ] **Step 3: Implement** the crate. Then wire the workspace, release-plz, the umbrella
  `pdf` feature and re-export, the READMEs and the `pdf-without-egui` CI job.
- [ ] **Step 4: Run** the tests, then `cargo clippy -p corundum-framework-pdf --all-targets -- -D warnings`
  and `cargo deny check`. Also run `cargo tree -p corundum-framework-pdf -e normal | grep -c egui`,
  which must be 0. Expected: all pass, clippy is clean, deny is ok, and egui is absent.
- [ ] **Step 5: Commit:** `feat(corundum-framework-pdf): count a PDF's pages and render one, in pure Rust (hayro) (#98)`

### Task 3: `PdfPreview`

**Files:**
- Create: `crates/corundum-framework-gui/src/preview/pdf.rs`, `crates/corundum-framework-gui/src/preview/tests_pdf.rs`
- Modify: `crates/corundum-framework-gui/Cargo.toml` (`pdf-preview`), `crates/corundum-framework-gui/src/preview/mod.rs`, both `.ftl` files

**Interfaces:**
```rust
/// A PDF in the detail pane: its pages rendered on a worker thread, one at a time, fitted
/// in the space given, with buttons to page through them.
#[derive(Default)]
pub struct PdfPreview { /* worker, asked, kept (key, page, size) ≤ 8, page per key, counts per key */ }
impl PdfPreview {
    pub fn new() -> PdfPreview;
    pub fn ui(&mut self, ui: &mut egui::Ui, state: State<'_>) -> PreviewResponse;
}
```
- **The worker** follows `image.rs`. It owns one `Document` at a time, cached for the
  current key, because `Document` is not `Send` and must stay on the thread. A request
  `(Content, page, longest)` opens the document if the key changed, then renders the page.
  The answer carries the `page_count`. The CJK fallback comes from
  `crate::fonts::system_cjk_font()`, looked up once on the worker and kept there.
- **Drawing:**
  - the page is drawn like `ImagePreview`'s picture: a texture fitted, aspect kept, never
    upscaled;
  - under it is a row with a `chevron_left` icon button, the label, and a `chevron_right`
    icon button;
  - a pending page keeps showing the previous one with a small spinner, or only a spinner
    if there is none;
  - each `Error` gets its own message, and `Broken` also shows Retry, which evicts the
    entry.
- **Strings (en / ja):**
  - `corundum-preview-pdf-page` = `{ $page } / { $count }` / `{ $page } / { $count }`
  - `corundum-preview-pdf-page-of` = `{ $page } / { $shown } of { $count }` / `{ $page } / { $shown }（全 { $count } ページ）`
  - `corundum-preview-pdf-previous` = Previous page / 前のページ (accessible names and hover text)
  - `corundum-preview-pdf-next` = Next page / 次のページ
  - `corundum-preview-pdf-encrypted` = This PDF is password-protected and can't be previewed. / このPDFはパスワードで保護されているため、プレビューできません。
  - `corundum-preview-pdf-broken` = Couldn't read this PDF: { $reason } / このPDFを読めませんでした：{ $reason }
  - `corundum-preview-too-large` and `corundum-preview-unsupported` are reused.

- [ ] **Step 1: Write the failing tests** in `tests_pdf.rs`. Copy the fixtures in with
  `include_bytes!("../../../corundum-framework-pdf/tests/fixtures/…")`.

```rust
#[test] fn the_first_page_shows_after_a_spinner()      // two-pages.pdf bytes → spinner, then a texture; label "1 / 2"
#[test] fn next_shows_the_second_page()                 // click next → label "2 / 2", a texture of Letter aspect
#[test] fn the_page_buttons_stop_at_the_ends()          // on page 1 previous is disabled (clicking does nothing); on page 2 next is disabled
#[test] fn a_new_file_starts_on_its_first_page()        // page 2 of key "a", then key "b" → "1 / 2"
#[test] fn an_encrypted_pdf_says_so()                   // the encrypted message
#[test] fn a_broken_pdf_offers_retry()                  // truncated bytes → reason + Retry; click → response.retry
#[test] fn loading_and_failed_states()                  // State::Loading → spinner; Failed("x") → "x" + Retry
#[test] fn the_pdf_strings_exist_in_both_languages()
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --no-default-features --features pdf-preview --lib preview::tests_pdf`.
  Expected: it does not compile.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, then `cargo test -p corundum-framework-gui --all-features --lib`,
  and clippy for the gui crate with no default features and with `pdf-preview`. Expected:
  all pass and clippy is clean.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui): PdfPreview, a PDF's pages in the detail pane (#98)`

### Task 4: Dispatch, docs, CI and the demo

**Files:**
- Modify:
  - `crates/corundum-framework-gui/src/preview/{file.rs,tests_file.rs,mod.rs}`;
  - the umbrella `Cargo.toml`, `lib.rs` and `features.rs` (`pdf-preview`);
  - both READMEs;
  - `docs/file-previews{,.ja}.md`;
  - the CI loops (`pdf-preview`);
  - `demo/Cargo.toml` (both target sections) and `demo/src/screens/files.rs` (sample `Pictures/tickets.pdf`, from `two-pages.pdf`).

**Interfaces:**
- `FilePreview` gains `pdf: PdfPreview` (`pdf-preview`). `wants_content` is true for `pdf`,
  matched case-insensitively. `part()` gives it the bounded rect. Make that
  `min(width, avail * 0.7)`, a little taller than an image's, to leave room for the page
  row.

- [ ] **Step 1: Write the failing tests.**

```rust
#[cfg(feature = "pdf-preview")] #[test] fn a_pdf_wants_content_and_shows_a_page() // wants_content("a.PDF"); "tickets.pdf" bytes → a texture and "1 / 2" drawn, "Size" below
// features.rs:
#[cfg(feature = "pdf-preview")] #[test] fn pdf_preview_is_the_gui_crate_s()      // names gui::preview::PdfPreview::new and corundum_framework::pdf::Document::open
```

- [ ] **Step 2: Run** them. Expected: FAIL.
- [ ] **Step 3: Implement** the dispatch, the wiring, the docs and the demo sample.
- [ ] **Step 4: Run all of these.** Expected: all pass and are clean.
  - `cargo test --workspace --all-features --locked`;
  - clippy for the gui crate with no default features, with `pdf-preview`, and with each
    of the other preview features;
  - clippy for `corundum-framework-pdf` and for the demo;
  - `cargo fmt --all --check`;
  - `cargo deny check`.
- [ ] **Step 5: Commit:** `feat: FilePreview shows PDFs; pdf-preview in the umbrella crate, CI and the demo`
