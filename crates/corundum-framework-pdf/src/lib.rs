//! Count a PDF's pages and render one to RGBA8, with no GUI in the way, in pure Rust
//! ([hayro](https://crates.io/crates/hayro)).
//!
//! ```no_run
//! use corundum_framework_pdf::Document;
//!
//! let bytes = std::fs::read("report.pdf").unwrap();
//! let doc = Document::open(bytes.into(), None)?;
//! let page = doc.render(0, 1024)?;
//! println!("{} pages; the first is {}x{}", doc.page_count(), page.width, page.height);
//! # Ok::<(), corundum_framework_pdf::Error>(())
//! ```
//!
//! A [`Document`] is **not `Send`**: hayro shares its caches with `Rc`. Open it and render
//! from it on one thread, the worker's, and send the finished [`Page`] (plain bytes) back.

mod error;
mod render;

pub use error::Error;
pub use render::{Document, FallbackFont, Page};

/// The most input [`Document::open`] takes: 64 MiB. A larger one is [`Error::TooLarge`].
pub const MAX_INPUT: usize = 64 * 1024 * 1024;

/// The pages a caller should offer: [`Document::render`] takes an index below
/// `min(page_count, MAX_PAGES)`.
pub const MAX_PAGES: usize = 500;
