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
//! A [`Document`] is deliberately neither `Send` nor `Sync`: it stays on the thread that
//! opened it, so a future hayro that is not `Send` is not a breaking change. Open and render
//! on a worker thread, and send the finished [`Page`] (plain bytes) back.
//!
//! [`MAX_INPUT`] bounds the input and [`MAX_SIDE`] the output, not hayro's memory in between:
//! it decodes a compressed stream or an image whole, whatever its size. A file built to
//! exhaust memory aborts the process, which `catch_unwind` cannot stop (nor can any panic
//! under `panic = "abort"`). Render files you do not trust in a process of their own.

mod error;
mod render;

pub use error::Error;
pub use render::{Document, FallbackFont, Page};

/// The most input [`Document::open`] takes: 64 MiB. A larger one is [`Error::TooLarge`].
pub const MAX_INPUT: usize = 64 * 1024 * 1024;

/// The pages a caller should offer: [`Document::render`] takes an index below
/// `min(page_count, MAX_PAGES)`.
pub const MAX_PAGES: usize = 500;

/// The longest side [`Document::render`] draws: 2048 pixels. A larger request is clamped.
pub const MAX_SIDE: u32 = 2048;
