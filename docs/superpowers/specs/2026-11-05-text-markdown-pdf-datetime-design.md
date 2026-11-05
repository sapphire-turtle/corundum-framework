# Text, Markdown and PDF previews; alumina's editors and date pickers in corundum

Asked for on 2026-11-05, for issues #99, #100 and #98, plus a new issue for the date pickers.
It builds on the detail pane's previews from #97
([design](2026-11-05-detail-pane-previews-design.md), PR #118).

## Why

#97 made the preview parts location-agnostic. The app hands in a `Content` (bytes or a
reader opener) and a `State`, and `FilePreview` picks a part by the file's kind. Images and
sounds have parts. Text, Markdown and PDF files only get the info card.
[file-previews.md](../../file-previews.md) says the pane should answer "what is this?" for
every file it reasonably can. That includes the cue sheet and `notes.txt` in a music folder,
a Markdown note, and the tickets PDF in a photo folder.

The text and Markdown widgets already exist in sapphire-turtle/alumina: alumina-code
(`CodeEditor`, which has a read-only mode) and alumina-markdown (`MarkdownEditor`, which
also has a read-only view, plus `MarkdownToolbar`). The apps need these editors for their
own editing screens too: sapphire's notes and journal, and asteria's descriptions. So
corundum adopts alumina for both the previews and the editors. It hands them to apps
dressed in corundum's look, the same way it already dresses egui. alumina-datetime (the
calendar and the date and time pickers) comes in the same way.

alumina depends only on egui 0.37, which is the version corundum uses. The crates are on
crates.io: alumina-code 0.1.0, alumina-markdown 0.2.0 and alumina-datetime 0.2.0.

## Delivery

Four PRs, stacked in this order. Each one closes its own issue.

1. **#99: text.** `TextPreview`, and alumina-code re-exported (`code-editor`).
2. **#100: Markdown.** The `.md` preview, and alumina-markdown re-exported with a toolbar
   in corundum's look (`markdown-editor`).
3. **A new issue: date pickers.** alumina-datetime wrapped in corundum's look (`datetime`).
4. **#98: PDF.** `PdfPreview` over a new egui-free crate.

Each PR stands alone on the one below it. Each passes CI and adds its own section to the
`preview` module doc, the README feature tables (en and ja), the umbrella crate's
features, Mobile CI's feature loop, and the demo.

## Shared decisions

- **Every new piece sits behind a feature that is off by default**, following
  file-previews.md. The default build does not grow.
- **The preview parts follow #97's contract.** Each part takes `State<'_>` and returns
  `PreviewResponse`. It reads and decodes on a worker and keeps its results by
  `Content::key`. A reader failure shows Retry, which evicts the failed entry. A panic is
  caught on the worker and becomes a failure. The UI thread never calls `Content::open`.
- **`FilePreview` dispatches the new kinds.** `wants_content` is true only for files a part
  will actually draw. Text-like extensions go to `TextPreview`; `md` and `markdown` go to
  the Markdown preview when `markdown-preview` is on, and otherwise to `TextPreview`; `pdf`
  goes to `PdfPreview`. Office documents such as doc and docx stay with the info card.
- **The re-exports are modules of corundum-framework-gui:**
  - `gui::code`, which is alumina-code;
  - `gui::markdown`, which is alumina-markdown plus corundum's toolbar dressing;
  - `gui::datetime`, which is alumina-datetime plus corundum's config.

  Apps then depend on corundum alone, at matching versions.
- **The look follows corundum's theme.** alumina already draws with `ui.visuals()`. The
  editors' `CodeTheme` follows the visuals, and the calendar's selection is
  `visuals.selection`, which `theme::install` sets from the accent. So "theme colours"
  means:
  - no colour of corundum's own goes missing;
  - where alumina takes colours or icons as values, corundum fills them from its theme and
    from `icons` (lucide);
  - the words come from `tr!`, in English and Japanese.

## 1. #99: text (`code-editor`, `text-preview`)

- **`code-editor`** brings alumina-code and re-exports it as `gui::code`.
- **`text-preview`** includes `code-editor` and brings no new dependency: text is UTF only, decided by the person (a cross-platform app grows no code for legacy encodings),
  so chardetng and encoding_rs are not used.
- **An egui-free function** turns bytes into text, or into a reason there is none. It is
  tested on its own.
  - **Limit:** only the first 256 KiB are read, and the result says whether it was cut.
  - **Encodings:** UTF-8 with or without a BOM, and UTF-16 LE or BE with a BOM. Anything
    else is read as UTF-8, and bytes that are not UTF-8 (a Shift_JIS or EUC-JP file) are
    shown lossily as U+FFFD.
  - **Binary:** a NUL in the first bytes read, with no UTF-16 BOM, means binary.
- **`TextPreview`** reads and decodes on its worker. It shows the text in a read-only,
  monospace, scrollable `CodeEditor`, where text can be selected and copied. If the text
  was cut, a weak line says so. A binary file shows "This file can't be previewed." No
  highlighter is passed in.
- **Tests** use UTF-8 with and without a BOM, UTF-16 with a BOM, a Shift_JIS cue sheet, a
  binary file and a file over the limit. The part's frames are tested the same way as
  #97's.

## 2. #100: Markdown (`markdown-editor`, `markdown-preview`)

- **`markdown-editor`** brings alumina-markdown and re-exports it as `gui::markdown`. It
  adds corundum's dressing:
  - `ToolbarIcons` filled with lucide icons from `icons`;
  - `ToolbarLabels` and `ToolbarNames` from `tr!` (en and ja).

  The dressing is a small function or builder that hands back the toolbar ready to place.
  The editor itself is alumina's, unchanged.
- **`markdown-preview`** includes `markdown-editor` and `text-preview`, so it gets the
  decoding and the limit. It shows `.md` files in alumina-markdown's read-only view.
  - A link click is reported back to the app through `PreviewResponse` as a new field,
    for example `link: Option<String>`. corundum does not open links itself.
  - Adding that field is a breaking change to `PreviewResponse`. It lands in this PR, as a
    `!` commit.
- **Tests:** a note renders, a click on a link is reported, and both the toolbar's
  labels and its icons are present.

## 3. Date pickers (`datetime`), on its own issue

- **`datetime`** brings alumina-datetime and re-exports it as `gui::datetime`. It adds a
  function that builds corundum's `CalendarConfig` for a context:
  - `CalendarIcons` from lucide: previous, next, today, clear and now;
  - `Labels` from `tr!` (en and ja), including the month and weekday names.

  An app uses that config with alumina's widgets unchanged.
- **Theme:** the selection and today colours come from visuals, which corundum's theme
  already sets. Where alumina-datetime takes colours as values (for example a
  `DayBadge`), corundum gives helpers built from its palette's `Swatch`es.
- **The first weekday is Monday, fixed**, because the week numbers are ISO (decided by
  the person). Sunday-first weeks with US week numbers will come to alumina-datetime
  later, as a pair (alumina issue linked from the PR); corundum's config does not offer
  them until then.
- **The demo** gets a page that shows the date, period, time and timestamp buttons in the
  dressed config.
- **The issue** is opened when the PR is.

## 4. #98: PDF (`pdf-preview`)

- **A new crate, `corundum-framework-pdf`,** has no egui. It is laid out like
  corundum-framework-image. It counts the pages and renders one page to RGBA at a given
  longest side, using hayro, which is pure Rust under MIT or Apache-2.0. Its error type
  covers encrypted, broken, unsupported and too large.
- **Limits:** at most `MAX_PAGES` pages are opened (500), a page renders to at most 2048
  px on its longest side, and the input is at most 64 MiB.
- **`gui` `pdf-preview` adds `PdfPreview`.** It renders the first page on its worker,
  fitted to the space given, the way `ImagePreview` is. It has previous and next buttons
  and shows "page n / N". A rendered page is kept per key and page.
- **The issue's first checkbox comes first:** check that hayro renders a PDF with embedded
  Japanese fonts, one without embedded fonts, and a scan-only PDF.
  - Commit the small sample PDFs as test fixtures, or generate them.
  - Record what does not render well in the PR.
  - Missing glyphs without embedded fonts are acceptable if hayro has no fallback. Say so
    in the docs.
- **Text selection and search are out of scope.**
- *(My calls, made while writing this up.)*
  - **Long PDFs:** a PDF over `MAX_PAGES` still previews. Paging stops at 500, and the
    count reads "page n / 500 of N", the same idea as cut text. Input over 64 MiB is the
    too-large error.
  - **Fixtures:**
    - Generate the PDFs in a test helper, or commit tiny ones.
    - The embedded-Japanese one uses a subset of a font under the SIL Open Font License
      (Noto Sans JP, say), and the fixture's licence goes in a README next to it.
    - No new tool dependency in the build.

## Also settled while writing this up (my calls)

- **Cut Markdown:** a note cut at 256 KiB shows whatever alumina's parser makes of it.
  An unclosed fence or front matter is acceptable. The same weak "cut" line as
  `TextPreview` follows it.
- **The `PreviewResponse` change in PR 2:** it is only the `!` commit. There is no release
  or version bump in these PRs; release-plz does that when they merge.

## Non-goals

- Syntax highlighting in `TextPreview` (no highlighter).
- Editing anything in a preview. Previews are read-only. The editors are for the apps' own
  screens.
- Changes to alumina itself. If corundum needs something alumina lacks, open an issue on
  sapphire-turtle/alumina and work around it here, or leave it out.
- PDF text selection, search, annotations and forms.

## Left to the builder

- Exact names: the dressing functions, the new response field, and the pdf crate's API.
- How the `FilePreview` dispatch table is written.
- The text-like extension list, starting from the issue's list (txt, cue, log, m3u, m3u8,
  json, toml, yaml, yml, csv, ini, xml, srt, md before #100).
- Where the demo shows the editors and pickers.
