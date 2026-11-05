//! What a preview draws and what it reports: the [`Content`] the app hands in, the
//! [`State`] of getting it, and the [`PreviewResponse`] of what the person asked for.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;

use crate::tr;

/// Anything a preview can read and seek: a file, a cursor over bytes, a download.
pub trait Reader: std::io::Read + std::io::Seek + Send + Sync {}
impl<T: std::io::Read + std::io::Seek + Send + Sync> Reader for T {}

/// A file's data: all of it in memory, or a way to open a reader over it.
#[derive(Clone)]
pub enum Data {
    /// The whole file.
    Bytes(Arc<[u8]>),
    /// Opens a fresh reader over the file each time it is called.
    Reader(Arc<dyn Fn() -> std::io::Result<Box<dyn Reader>> + Send + Sync>),
}

/// What a preview draws: the app's key for it, the file's name, and its data.
///
/// Two contents with the same key are the same file as far as a preview is concerned: the
/// app changes the key when the file changes.
#[derive(Clone)]
pub struct Content {
    /// The app's key for this file's current content.
    pub key: String,
    /// The file's name, which tells a preview its type by the extension.
    pub name: String,
    /// The data.
    pub data: Data,
}

impl Content {
    /// A content of bytes in memory.
    pub fn bytes(
        key: impl Into<String>,
        name: impl Into<String>,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Content {
        Content {
            key: key.into(),
            name: name.into(),
            data: Data::Bytes(bytes.into()),
        }
    }

    /// A content read through `open`, which gives a fresh reader on each call.
    pub fn reader(
        key: impl Into<String>,
        name: impl Into<String>,
        open: impl Fn() -> std::io::Result<Box<dyn Reader>> + Send + Sync + 'static,
    ) -> Content {
        Content {
            key: key.into(),
            name: name.into(),
            data: Data::Reader(Arc::new(open)),
        }
    }

    /// A reader opener of the local file at `path`, named by its file name.
    pub fn file(key: impl Into<String>, path: impl Into<PathBuf>) -> Content {
        let path: PathBuf = path.into();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Content::reader(key, name, move || {
            Ok(Box::new(std::fs::File::open(&path)?) as Box<dyn Reader>)
        })
    }

    /// The extension of `name`, in lower case.
    pub fn ext(&self) -> Option<String> {
        std::path::Path::new(&self.name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
    }

    /// A fresh reader: a cursor over the bytes, or the opener's result. Call only off the UI
    /// thread.
    pub fn open(&self) -> std::io::Result<Box<dyn Reader>> {
        match &self.data {
            Data::Bytes(bytes) => Ok(Box::new(Cursor::new(Arc::clone(bytes)))),
            Data::Reader(open) => open(),
        }
    }
}

impl PartialEq for Content {
    fn eq(&self, other: &Content) -> bool {
        self.key == other.key
    }
}

impl Eq for Content {}

impl std::fmt::Debug for Content {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Content")
            .field("key", &self.key)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Where the app is with a file's content.
#[derive(Clone, Copy)]
pub enum State<'a> {
    /// The app is still getting it.
    Loading,
    /// The app could not get it, for this reason.
    Failed(&'a str),
    /// The app has it.
    Ready(&'a Content),
}

/// What the person asked of a preview in this frame.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
#[must_use]
pub struct PreviewResponse {
    /// The person pressed Retry: the app should get the content again.
    pub retry: bool,
    /// The person asked to open the file with the system's app.
    pub open: bool,
    /// The person clicked this link in a Markdown preview: the app decides what to do with
    /// it. The text is the link's target, as the Markdown parser reads it: a reference-style
    /// link arrives resolved, and a relative target (`other.md`, `#heading`) arrives
    /// unresolved, for the app to resolve against the file. The target comes from the file's
    /// content, so check its scheme before opening it (e.g. with
    /// `platform::open_with_system`).
    pub link: Option<String>,
}

impl PreviewResponse {
    /// This response and `other` together: each flag is set if it is set in either; the link
    /// is `other`'s if it has one, else this one's.
    pub fn merge(self, other: PreviewResponse) -> PreviewResponse {
        PreviewResponse {
            retry: self.retry || other.retry,
            open: self.open || other.open,
            link: other.link.or(self.link),
        }
    }
}

/// The spinner's size, and the space above a failure's reason, in points.
// Only the previews that ask for content draw the status; the feature-less build has none.
#[cfg_attr(
    not(any(feature = "image-preview", feature = "audio-preview")),
    allow(dead_code)
)]
const SPINNER: f32 = 32.0;

/// Draws a spinner for Loading, or the reason and a Retry button for Failed, centred in
/// `ui`, and returns `None`. For Ready, draws nothing and returns the content. Sets
/// `response.retry` when Retry is clicked.
#[cfg_attr(
    not(any(feature = "image-preview", feature = "audio-preview")),
    allow(dead_code)
)]
pub(crate) fn status_ui<'a>(
    ui: &mut egui::Ui,
    state: State<'a>,
    response: &mut PreviewResponse,
) -> Option<&'a Content> {
    let area = ui.available_rect_before_wrap();
    match state {
        State::Ready(content) => return Some(content),
        State::Loading => {
            ui.allocate_rect(area, egui::Sense::hover());
            egui::Spinner::new().paint_at(
                ui,
                egui::Rect::from_center_size(area.center(), egui::vec2(SPINNER, SPINNER)),
            );
        }
        State::Failed(reason) => {
            ui.allocate_rect(area, egui::Sense::hover());
            let mut inner = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
            );
            inner.add_space((area.height() / 2.0 - SPINNER).max(0.0));
            inner.add(egui::Label::new(reason).wrap());
            if retry_button(&mut inner) {
                response.retry = true;
            }
        }
    }
    None
}

/// The Retry button every failure offers; whether it was clicked.
#[cfg_attr(
    not(any(feature = "image-preview", feature = "audio-preview")),
    allow(dead_code)
)]
pub(crate) fn retry_button(ui: &mut egui::Ui) -> bool {
    ui.button(tr!(ui, "corundum-preview-retry")).clicked()
}
