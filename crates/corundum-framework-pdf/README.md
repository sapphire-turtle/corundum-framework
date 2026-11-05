# corundum-framework-pdf

Count a PDF's pages and draw one, with no GUI in the way and no C library: rendering is
[hayro](https://crates.io/crates/hayro), in pure Rust. Give it bytes; it gives back RGBA8,
opaque, drawn on white.

- `Document::open(bytes, cjk)`: at most 64 MiB (`MAX_INPUT`), else `Error::TooLarge`. A file
  that needs a password is `Error::Encrypted`; one with an empty user password opens as it
  does in a viewer. A file that is not a PDF, or has no pages, is `Error::Broken`.
- `Document::page_count()` and `Document::render(index, longest)`: a page is drawn with its
  longest side `longest` pixels, the scale taken from the page's size in points, so a huge
  page is never drawn at its own size. `MAX_PAGES` (500) is how many pages a caller should
  offer.
- A panic in the renderer on a bad file becomes `Error::Broken`.
- A `Document` is deliberately neither `Send` nor `Sync`: it stays on the thread that opened it,
  so a future hayro that is not `Send` is not a breaking change. Open and render on a worker
  thread, and send the finished `Page` back. `render` clamps `longest` to `MAX_SIDE` (2048).

The standard 14 fonts (Helvetica, Times and the like), which most generated PDFs use without
embedding them, are drawn from fonts built into hayro (about 270 KB). Text in a CJK font the
PDF does not embed is blank unless the app passes a `FallbackFont`: any font file it has, a
system font or its own, serving the Adobe-Japan1, GB1, CNS1 and Korea1 collections. This
crate ships no CJK font.

```rust,no_run
use corundum_framework_pdf::Document;

let bytes = std::fs::read("report.pdf").unwrap();
let doc = Document::open(bytes.into(), None)?;
let page = doc.render(0, 1024)?;
println!("{} pages; the first is {}x{}", doc.page_count(), page.width, page.height);
# Ok::<(), corundum_framework_pdf::Error>(())
```

Licensed under MIT or Apache-2.0, at your option.
