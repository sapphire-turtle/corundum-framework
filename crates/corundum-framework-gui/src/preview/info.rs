//! The info card: what is known of a file, for the files no preview draws.

use std::time::SystemTime;

use super::{Kind, PreviewResponse, kind_of};
use crate::explorer::{Entry, format_size, format_time, icon_for};
use crate::tr;

/// The size of the card's large icon.
const ICON: f32 = 64.0;

/// What the card says of a file. Built from an explorer [`Entry`], or by hand.
#[derive(Clone, Debug, Default)]
pub struct FileInfo {
    /// The file's name, with its extension.
    pub name: String,
    /// The size in bytes, where known.
    pub size: Option<u64>,
    /// When it last changed, where known.
    pub modified: Option<SystemTime>,
    /// When it was made, where known.
    pub created: Option<SystemTime>,
    /// The icon to show; else none is drawn.
    pub icon: Option<egui::ImageSource<'static>>,
}

impl FileInfo {
    /// A file called `name`, of which nothing else is known.
    pub fn new(name: impl Into<String>) -> FileInfo {
        FileInfo {
            name: name.into(),
            ..FileInfo::default()
        }
    }
}

impl From<&Entry> for FileInfo {
    /// Its name, size, modified time and icon (else the explorer's icon for it).
    fn from(entry: &Entry) -> FileInfo {
        FileInfo {
            name: entry.name.clone(),
            size: entry.size,
            modified: entry.modified,
            created: None,
            icon: Some(entry.icon.clone().unwrap_or_else(|| icon_for(entry))),
        }
    }
}

/// The file's icon, name, size, kind and dates; and "Open with the system's app" when asked.
pub struct InfoCard<'a> {
    file: &'a FileInfo,
    open: bool,
    icon: bool,
    compact: bool,
}

impl<'a> InfoCard<'a> {
    /// A card of `file`, with its icon, without the open button, not compact.
    pub fn new(file: &'a FileInfo) -> InfoCard<'a> {
        InfoCard {
            file,
            open: false,
            icon: true,
            compact: false,
        }
    }

    /// Whether to offer "Open with the system's app": the app's say. Off by default.
    pub fn open_button(mut self, on: bool) -> Self {
        self.open = on;
        self
    }

    /// Whether to draw the large icon above the name and details. On by default; the name is
    /// drawn either way.
    pub fn icon(mut self, on: bool) -> Self {
        self.icon = on;
        self
    }

    /// Compact, for under a preview: a separator, then the name and the details as small weak
    /// lines, with no icon.
    pub fn footer(mut self) -> Self {
        self.compact = true;
        self
    }

    /// Draw the card. `open` is set on the response when the button is clicked.
    pub fn ui(self, ui: &mut egui::Ui) -> PreviewResponse {
        let file = self.file;
        let mut response = PreviewResponse::default();
        let mut rows = Vec::new();
        if let Some(size) = file.size {
            rows.push((tr!(ui, "corundum-preview-size"), format_size(size)));
        }
        rows.push((tr!(ui, "corundum-preview-kind"), kind_label(ui, &file.name)));
        if let Some(at) = file.modified {
            rows.push((tr!(ui, "corundum-preview-modified"), format_time(at)));
        }
        if let Some(at) = file.created {
            rows.push((tr!(ui, "corundum-preview-created"), format_time(at)));
        }

        if self.compact {
            ui.separator();
            ui.label(egui::RichText::new(&file.name).small().weak());
            for (label, value) in rows {
                ui.label(
                    egui::RichText::new(format!("{label}: {value}"))
                        .small()
                        .weak(),
                );
            }
        } else {
            ui.vertical_centered(|ui| {
                if let (true, Some(icon)) = (self.icon, &file.icon) {
                    ui.add(
                        egui::Image::new(icon.clone()).fit_to_exact_size(egui::Vec2::splat(ICON)),
                    );
                }
                ui.add(egui::Label::new(egui::RichText::new(&file.name).heading()).wrap());
            });
            ui.add_space(8.0);
            egui::Grid::new(ui.next_auto_id())
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, value) in rows {
                        ui.label(egui::RichText::new(label).weak());
                        ui.label(value);
                        ui.end_row();
                    }
                });
        }
        if self.open {
            ui.add_space(8.0);
            if ui.button(tr!(ui, "corundum-preview-open")).clicked() {
                response.open = true;
            }
        }
        response
    }
}

/// "PNG image", "ZIP file", or "File" for a name with no extension.
fn kind_label(ui: &egui::Ui, name: &str) -> String {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return tr!(ui, "corundum-preview-kind-unknown");
    };
    if stem.is_empty() || extension.is_empty() {
        return tr!(ui, "corundum-preview-kind-unknown");
    }
    let ext = extension.to_uppercase();
    match kind_of(name) {
        Kind::Image => tr!(ui, "corundum-preview-kind-image", ext = ext),
        Kind::Audio => tr!(ui, "corundum-preview-kind-audio", ext = ext),
        Kind::Video => tr!(ui, "corundum-preview-kind-video", ext = ext),
        Kind::Text => tr!(ui, "corundum-preview-kind-text", ext = ext),
        Kind::Other => tr!(ui, "corundum-preview-kind-other", ext = ext),
    }
}
