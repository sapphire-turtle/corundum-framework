# Markdown Preview and Editor (#100) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show `.md` files formatted in the detail pane. Give apps alumina-markdown's
editor as `gui::markdown`, with a toolbar that uses corundum's lucide icons and words.

**Architecture:**
- **`markdown-editor`** re-exports alumina-markdown. It adds dressing functions that fill
  `ToolbarIcons`, `ToolbarLabels` and `ToolbarNames` from corundum's icons and `tr!`.
- **`markdown-preview`** adds `MarkdownPreview`. It shares `TextPreview`'s worker, so the
  same decoding, limit, key cache and Retry apply. It draws with a read-only
  `MarkdownEditor`.
- **Links:** a click on a link is reported back to the app as a new `PreviewResponse::link`
  field. Adding the field is a breaking change.
- **Dispatch:** `FilePreview` sends `md` and `markdown` files to the Markdown preview when
  the feature is on.

**Tech Stack:** Rust 2024, egui 0.37, alumina-markdown 0.2, and Lucide SVGs (ISC).

**Spec:** `docs/superpowers/specs/2026-11-05-text-markdown-pdf-datetime-design.md`,
sections "Shared decisions" and "2. #100". The branch is `feat/markdown-preview`, stacked on
`feat/text-preview` (PR #120).

## Global Constraints

- **Features** are off by default:
  - `markdown-editor = ["dep:alumina-markdown"]`, with `alumina-markdown = "0.2"`;
  - `markdown-preview = ["markdown-editor", "text-preview"]`;
  - the umbrella crate gets the same two features, also forwarding `gui`.

  The default build gains no dependency. `cargo deny check` passes.
- **The part contract from #97 and #99:**
  - `ui(&mut self, ui, State<'_>) -> PreviewResponse`;
  - decoding runs on the worker, and the UI thread never calls `Content::open`;
  - results are kept by key, at most 8;
  - Retry evicts the failed entry;
  - a panic becomes a failure;
  - the text limit and the UTF-only decoding are `decode_text`'s, unchanged.
- **Words:** everything shown goes through `tr!`, with keys in both
  `crates/corundum-framework-gui/locales/{en,ja}/corundum.ftl`.
  - The toolbar's keys are `corundum-markdown-<field>-label` and
    `corundum-markdown-<field>-name` for each of alumina's 11 fields: `strong`,
    `emphasis`, `strikethrough`, `code`, `link`, `heading`, `bullet-list`,
    `numbered-list`, `task-list`, `quote` and `code-block`.
  - Labels stay ASCII, as alumina's defaults are, because they show only when a button has
    no icon.
- **Egui ids:** egui id strings never start with `corundum-`. The i18n test reads such
  strings as Fluent ids.
- **Lucide icons:** new SVGs go in `crates/corundum-framework-gui/assets/icons/`, with the
  same white stroke and size attributes as the existing ones (compare `list.svg`), under
  the existing `LICENSE-Lucide`. Each gets a `pub fn` in `icons.rs`, and the demo's Icons
  screen lists it.
- **CI:** clippy `-D warnings --all-targets` is clean for the gui crate with
  `--no-default-features` alone, and with each of `image-preview`, `audio-preview`,
  `code-editor`, `text-preview`, `markdown-editor` and `markdown-preview`. Add the two new
  features to the loops in `.github/workflows/ci.yml:46` and `mobile-ci.yml:56,132`.
- **Commits** are conventional and end with
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Use `!` for the
  `PreviewResponse` change. Do not edit `CHANGELOG.md`.

## Review Focus

1. **A link with no URL, or a reference-style link.** A click either reports what alumina
   gives or nothing, and never panics. Test: Task 2 `a_clicked_link_is_reported`, which
   also checks that a frame with no click reports `None`.
2. **A cut Markdown file with an unclosed fence or front matter.** It renders, and the cut
   note shows. Test: Task 2 `a_cut_note_with_an_open_fence_still_shows`.
3. **The toolbar beside a read-only preview.** The preview has no toolbar, and typing in
   it does nothing. Test: Task 2 `the_preview_is_read_only`.
4. **Japanese toolbar names.** With the language set to `ja`, the hover and accessible
   names are Japanese. Test: Task 1 `the_toolbar_speaks_the_language`.
5. **`PreviewResponse::merge` with the new field.** `link` is kept from either side, and
   the right side wins when both have one. Test: Task 2 `merge_keeps_a_link`.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/corundum-framework-gui/src/markdown.rs` (new) | `gui::markdown`: `pub use alumina_markdown::*;`, plus `toolbar_icons()`, `toolbar_labels(ctx)`, `toolbar_names(ctx)` and `toolbar(ctx, text, id)`. |
| `crates/corundum-framework-gui/assets/icons/*.svg` (new), `src/icons.rs` | The toolbar's Lucide icons. |
| `crates/corundum-framework-gui/src/preview/content.rs` | `PreviewResponse::link`. |
| `crates/corundum-framework-gui/src/preview/text/{mod.rs,markdown.rs}` | The shared worker, and `MarkdownPreview`. |
| `crates/corundum-framework-gui/src/preview/{file.rs,mod.rs,tests_markdown.rs}` | The dispatch, the re-exports and the doc, and the tests. |
| `demo/src/screens/{editors.rs,mod.rs,icons.rs}`, `demo/locales/{en,ja}/demo.ftl` | A new Editors screen, and the new icons on the Icons screen. |
| umbrella `Cargo.toml`, `lib.rs` and `features.rs`; READMEs en and ja; `docs/file-previews{,.ja}.md`; CI | Wiring. |

---

### Task 1: `markdown-editor`, with corundum's toolbar

**Files:**
- Create: `crates/corundum-framework-gui/src/markdown.rs`, and the 10 SVGs listed below
- Modify:
  - `crates/corundum-framework-gui/{Cargo.toml,src/lib.rs,src/icons.rs}`;
  - both `.ftl` files;
  - the umbrella crate's `Cargo.toml`, `src/lib.rs` and `tests/features.rs`;
  - `README.md` and `README.ja.md`;
  - `.github/workflows/ci.yml` and `mobile-ci.yml` (add `markdown-editor`);
  - `demo/src/screens/icons.rs`.

**Interfaces:**
- **Icons.** Add these Lucide icons to `icons.rs`. Fetch the SVGs from
  `https://raw.githubusercontent.com/lucide-icons/lucide/main/icons/<name>.svg`, then
  adapt them to the repo's white-stroke convention.

  | Function | Lucide name |
  |---|---|
  | `bold()` | `bold` |
  | `italic()` | `italic` |
  | `strikethrough()` | `strikethrough` |
  | `code()` | `code` |
  | `link()` | `link` |
  | `heading()` | `heading` |
  | `list_ordered()` | `list-ordered` |
  | `list_todo()` | `list-todo` |
  | `text_quote()` | `text-quote` |
  | `square_code()` | `square-code` |

  The bulleted list reuses the existing `list()`.
- **`gui::markdown`:**
  ```rust
  pub use alumina_markdown::*;
  /// The toolbar's buttons as corundum's Lucide icons.
  pub fn toolbar_icons() -> ToolbarIcons;
  /// The toolbar's labels (shown without an icon) in the context's language.
  pub fn toolbar_labels(ctx: &egui::Context) -> ToolbarLabels;
  /// The buttons' accessible names and hover text in the context's language.
  pub fn toolbar_names(ctx: &egui::Context) -> ToolbarNames;
  /// A toolbar for the editor with `editor_id`, with all three above applied.
  pub fn toolbar<'t>(ctx: &egui::Context, text: &'t mut String, editor_id: egui::Id) -> MarkdownToolbar<'t>;
  ```
  The three structs are `#[non_exhaustive]`, so build each one from `Default::default()`
  and then set its fields.
- **Strings (en / ja).** The names:

  | Field | en | ja |
  |---|---|---|
  | `strong` | Bold | 太字 |
  | `emphasis` | Italic | 斜体 |
  | `strikethrough` | Strikethrough | 取り消し線 |
  | `code` | Code | コード |
  | `link` | Link | リンク |
  | `heading` | Heading | 見出し |
  | `bullet-list` | Bulleted list | 箇条書き |
  | `numbered-list` | Numbered list | 番号付きリスト |
  | `task-list` | Task list | タスクリスト |
  | `quote` | Quote | 引用 |
  | `code-block` | Code block | コードブロック |

  The labels are alumina's ASCII defaults (`B`, `I`, `S`, `</>`, `Link`, `H`, `-`, `1.`,
  `[ ]`, `>`, `{ }`) in both languages. They still go through `tr!` so an app can override
  them.
- **The module doc:** colours follow the visuals, via `MarkdownStyle::from_visuals`. Use
  `toolbar(..)` to get corundum's look. The editor itself is alumina's, unchanged.

- [ ] **Step 1: Write the failing tests** in a `#[cfg(test)] mod tests` in `markdown.rs`,
  plus one in `features.rs`.

```rust
#[test] fn every_toolbar_button_has_an_icon()        // toolbar_icons(): each of the 11 fields is Some
#[test] fn the_toolbar_speaks_the_language()         // ctx with ja installed (see tests_content.rs strings test): toolbar_names(&ctx).strong == "太字"; en → "Bold"
#[test] fn the_markdown_strings_exist_in_both_languages()
// features.rs:
#[cfg(feature = "markdown-editor")] #[test] fn markdown_editor_is_alumina_s() // names gui::markdown::MarkdownEditor::new and gui::markdown::toolbar
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --no-default-features --features markdown-editor --lib markdown`.
  Expected: it does not compile.
- [ ] **Step 3: Implement** the feature, the icons, the module, the strings, the umbrella
  feature, the README rows, the CI loops and the demo's Icons list.
- [ ] **Step 4: Run** these:
  - the tests;
  - `cargo test -p corundum-framework --features markdown-editor --test features`;
  - clippy with `--features markdown-editor`;
  - `cargo deny check`.

  Expected: all pass, clippy is clean and deny is ok.
- [ ] **Step 5: Commit:** `feat(corundum-framework-gui): alumina-markdown as gui::markdown, with a toolbar in corundum's icons and words (markdown-editor)`

### Task 2: `MarkdownPreview`, and links in `PreviewResponse`

**Files:**
- Modify:
  - `crates/corundum-framework-gui/src/preview/content.rs` (`link`, `merge`, and their test);
  - `crates/corundum-framework-gui/src/preview/text/mod.rs` (factor the worker out);
  - `crates/corundum-framework-gui/Cargo.toml` (`markdown-preview`);
  - `crates/corundum-framework-gui/src/preview/mod.rs`;
  - both `.ftl` files, only if a new string is needed.
- Create: `crates/corundum-framework-gui/src/preview/text/markdown.rs`, `crates/corundum-framework-gui/src/preview/tests_markdown.rs`

**Interfaces:**
- **`PreviewResponse`** becomes
  `{ pub retry: bool, pub open: bool, pub link: Option<String> }`. It stays `Default` and
  `#[must_use]`, and drops `Copy`, because a `String` can't be `Copy`. Keep `Clone`,
  `PartialEq`, `Eq` and `Debug`.
  - `merge` ORs the two bools and takes `other.link.or(self.link)`.
  - Fix every place that relied on `Copy`: the parts, `FilePreview`, the demo and the
    tests.
  - **This is the breaking change, so mark its commit `!`.**
- **The worker:** move `TextPreview`'s worker, its `kept` list and its Retry handling into
  a private `TextSource` in `text/mod.rs`. It returns either a still-pending state or the
  outcome (`Text`, `Binary`, or a read failure) for the current key. `TextPreview` and
  `MarkdownPreview` each hold one. Behaviour is unchanged, and the existing `tests_text`
  tests still pass.
- **`MarkdownPreview`** (`markdown-preview`):
  ```rust
  /// A Markdown file in the detail pane: decoded like a TextPreview, shown formatted and
  /// read-only by alumina-markdown; a link clicked is reported, not opened.
  #[derive(Default)]
  pub struct MarkdownPreview { /* source: TextSource */ }
  impl MarkdownPreview {
      pub fn new() -> MarkdownPreview;
      pub fn ui(&mut self, ui: &mut egui::Ui, state: State<'_>) -> PreviewResponse;
  }
  ```
- **Drawing:** the text is shown in `egui::ScrollArea::vertical()` as
  `crate::markdown::MarkdownEditor::new(&mut text).read_only(true).id(<from the key, not starting "corundum-">)`.
  The output's `link_clicked` becomes `response.link`. The cut note, the binary message
  and Retry are the same as `TextPreview`'s, so reuse its helpers. There is no toolbar.

- [ ] **Step 1: Write the failing tests** in `tests_markdown.rs` and `content.rs`.

```rust
#[test] fn merge_keeps_a_link()                          // (content.rs) a{link:Some("x")}.merge(b{..default}) → Some("x"); both Some → right's
#[test] fn a_note_shows_formatted()                      // "# Title\n\nSome **bold** text" → a text shape containing "Title" drawn larger than the body's (compare galley row heights / font size)
#[test] fn a_clicked_link_is_reported()                  // "[corundum](https://example.com)" → click the "corundum" text's rect → response.link == Some("https://example.com"); a frame without a click → None
#[test] fn the_preview_is_read_only()                    // focus the editor, send Event::Text("x") → the kept text is unchanged (assert the drawn text has no "x" added)
#[test] fn a_cut_note_with_an_open_fence_still_shows()   // "```\n" + "a\n" × (TEXT_LIMIT) → the cut note and some "a" drawn, no panic
#[test] fn a_reader_that_failed_offers_retry()           // same as tests_text's
```

- [ ] **Step 2: Run** `cargo test -p corundum-framework-gui --no-default-features --features markdown-preview --lib preview`.
  Expected: it does not compile.
- [ ] **Step 3: Implement.** Factor the worker first, and re-run `preview::tests_text` to
  confirm it still passes. Then add the field and `MarkdownPreview`.
- [ ] **Step 4: Run** the tests, the `text-preview` tests, and clippy in all the builds
  listed under Global Constraints. Expected: they pass and clippy is clean.
- [ ] **Step 5: Commit** in two commits:
  - `refactor(corundum-framework-gui): the text preview's worker, shared`
  - `feat(corundum-framework-gui)!: MarkdownPreview; a link clicked in a preview is reported in PreviewResponse::link (#100)`

  The second commit's body says that `PreviewResponse` gained `link` and is no longer
  `Copy`.

### Task 3: Dispatch, docs, CI and the demo's Editors screen

**Files:**
- Modify:
  - `crates/corundum-framework-gui/src/preview/{file.rs,tests_file.rs,mod.rs}`;
  - the umbrella crate's `Cargo.toml`, `lib.rs` and `features.rs`;
  - `README.md` and `README.ja.md`;
  - `docs/file-previews{,.ja}.md`;
  - CI (add `markdown-preview`);
  - `demo/Cargo.toml` (`markdown-preview` in both target sections), `demo/src/screens/mod.rs`, and `demo/locales/{en,ja}/demo.ftl`.
- Create: `demo/src/screens/editors.rs`

**Interfaces:**
- **`FilePreview`** gains a `markdown: MarkdownPreview` field (`markdown-preview`). In
  `part()`, an `md` or `markdown` file goes to it when the feature is on, and otherwise to
  `TextPreview` as now. It gets the same bounded rect as text. The response is passed on
  as it is, so `link` reaches the app.
- **The umbrella crate:** `markdown-editor = ["gui", "corundum-framework-gui/markdown-editor"]`,
  and `markdown-preview = ["gui", "markdown-editor", "text-preview", "corundum-framework-gui/markdown-preview"]`.
- **The demo:**
  - It gets a new `Screen::Editors` (rail label `demo-screen-editors` = "Editors" /
    "エディター"), placed before Settings. Follow how the other screens are registered in
    `screens/mod.rs`, including `ALL`, the icon and the docs comment.
  - The screen shows two things:
    1. a `gui::markdown::toolbar(..)` above a `MarkdownEditor` over a sample note, with
       headings, a list, a task list, a link and a fenced block;
    2. a `gui::code::CodeEditor` over a small JSON sample, with no highlighter.
  - A link clicked in the Markdown editor sets the demo's status line, as
    `demo-status-opened` does.
  - On the Files screen, a `link` response from the preview also sets the status line.
    The sample's `Documents/Old/plan.md` gains a heading, a list and a link.

- [ ] **Step 1: Write the failing tests.**

```rust
#[cfg(feature = "markdown-preview")] #[test] fn markdown_files_go_to_the_markdown_preview() // "# Hi\n\nbody" as "a.md" → "Hi" drawn larger than "body"; "a.txt" with the same text → plain (same height)
// features.rs:
#[cfg(feature = "markdown-preview")] #[test] fn markdown_preview_is_the_gui_crate_s()      // names gui::preview::MarkdownPreview::new
```

- [ ] **Step 2: Run** them. Expected: FAIL.
- [ ] **Step 3: Implement** the dispatch, the wiring, the docs and the demo.
- [ ] **Step 4: Run all of these:**
  - `cargo test --workspace --all-features --locked`;
  - clippy in every build listed under Global Constraints;
  - clippy for the demo;
  - `cargo fmt --all --check`;
  - `cargo deny check`.

  Expected: all pass and are clean.
- [ ] **Step 5: Commit:** `feat: FilePreview shows Markdown; markdown-preview in the umbrella crate, CI, and the demo's Editors screen`
