//! The default preview of one file: its picture or its player, and its details.

#[cfg(feature = "audio-preview")]
use std::sync::Arc;

#[cfg(feature = "audio-preview")]
use corundum_framework_audio::Player;

#[cfg(feature = "audio-preview")]
use super::AudioPreview;
#[cfg(feature = "audio-preview")]
use super::Content;
#[cfg(feature = "image-preview")]
use super::ImagePreview;
#[cfg(any(feature = "image-preview", feature = "audio-preview"))]
use super::Kind;
#[cfg(feature = "text-preview")]
use super::TextPreview;
use super::{FileInfo, InfoCard, PreviewResponse, State, kind_of};

/// The default for one file: its picture or its player at the top, its details under
/// them; the info card for a file with neither.
///
/// Make one and keep it across frames: it holds the parts' threads and what they decoded.
#[derive(Default)]
pub struct FilePreview {
    #[cfg(feature = "image-preview")]
    image: ImagePreview,
    #[cfg(feature = "audio-preview")]
    audio: AudioPreview,
    #[cfg(feature = "audio-preview")]
    player: Option<Arc<Player<Content>>>,
    #[cfg(feature = "text-preview")]
    text: TextPreview,
}

impl FilePreview {
    /// A preview with no player: sounds get the info card.
    pub fn new() -> FilePreview {
        FilePreview::default()
    }

    /// This preview with `player` to play sounds with.
    #[cfg(feature = "audio-preview")]
    pub fn with_player(self, player: Arc<Player<Content>>) -> FilePreview {
        FilePreview {
            player: Some(player),
            ..self
        }
    }

    /// Whether this preview draws `name`'s content. An image `corundum-framework-image`
    /// cannot decode (SVG, HEIC, AVIF, BMP) is not drawn, so it gets the info card and the
    /// app does not fetch it. When it is false, the app need not fetch the content: the info
    /// card needs none.
    pub fn wants_content(&self, name: &str) -> bool {
        match kind_of(name) {
            #[cfg(feature = "image-preview")]
            Kind::Image => decodable_image(name),
            #[cfg(feature = "audio-preview")]
            Kind::Audio => self.player.is_some(),
            _ => is_text(name),
        }
    }

    /// Draw `file`, with its content as `state`. `open_button` is passed to the card.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        file: &FileInfo,
        state: State<'_>,
        open_button: bool,
    ) -> PreviewResponse {
        if !self.wants_content(&file.name) {
            return InfoCard::new(file).open_button(open_button).ui(ui);
        }
        let part = self.part(ui, file, state);
        part.merge(
            InfoCard::new(file)
                .icon(false)
                .open_button(open_button)
                .ui(ui),
        )
    }

    /// Draw the picture or the player of `file`, at the top of `ui`.
    #[cfg_attr(
        not(any(
            feature = "image-preview",
            feature = "audio-preview",
            feature = "text-preview"
        )),
        allow(unused_variables)
    )]
    fn part(&mut self, ui: &mut egui::Ui, file: &FileInfo, state: State<'_>) -> PreviewResponse {
        #[allow(unused_mut)]
        let mut response = PreviewResponse::default();
        #[cfg(any(
            feature = "image-preview",
            feature = "audio-preview",
            feature = "text-preview"
        ))]
        {
            let width = ui.available_width();
            let bounded = width.min(ui.available_height() * 0.6);
            let origin = ui.available_rect_before_wrap().min;
            let draw = |ui: &mut egui::Ui, height: f32| {
                let rect = egui::Rect::from_min_size(origin, egui::vec2(width, height));
                let child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                (child, rect)
            };
            match kind_of(&file.name) {
                #[cfg(feature = "image-preview")]
                Kind::Image => {
                    let (mut child, rect) = draw(ui, bounded);
                    response = self.image.ui(&mut child, state);
                    ui.allocate_rect(rect, egui::Sense::hover());
                }
                #[cfg(feature = "audio-preview")]
                Kind::Audio => {
                    if let Some(player) = &self.player {
                        // A sound is as tall as it is; Loading and Failed need room to centre in.
                        let height = match state {
                            State::Ready(_) => ui.available_height(),
                            _ => bounded,
                        };
                        let (mut child, _) = draw(ui, height);
                        response = self.audio.ui(&mut child, state, player);
                        let used = child.min_rect();
                        ui.allocate_rect(used, egui::Sense::hover());
                    }
                }
                _ => {}
            }
            #[cfg(feature = "text-preview")]
            if is_text(&file.name) {
                let (mut child, rect) = draw(ui, bounded);
                response = self.text.ui(&mut child, state);
                ui.allocate_rect(rect, egui::Sense::hover());
            }
        }
        response
    }
}

/// Whether `corundum-framework-image` decodes the image called `name`, by its extension:
/// PNG, JPEG, GIF, WebP and TIFF. SVG, HEIC, AVIF and BMP it does not.
#[cfg(feature = "image-preview")]
fn decodable_image(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        ["png", "jpg", "jpeg", "gif", "webp", "tif", "tiff"]
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
}

/// The extensions of the files `FilePreview` shows as text, matched without regard to case.
#[cfg(feature = "text-preview")]
const TEXT_EXTENSIONS_SHOWN: &[&str] = &[
    "txt", "text", "md", "markdown", "cue", "log", "m3u", "m3u8", "json", "toml", "yaml", "yml",
    "csv", "tsv", "ini", "cfg", "conf", "xml", "srt", "vtt", "nfo", "rs", "py", "js", "sh",
];

/// Whether `FilePreview` shows the file called `name` as text, by its extension.
#[cfg(feature = "text-preview")]
fn is_text(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        TEXT_EXTENSIONS_SHOWN
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
}

#[cfg(not(feature = "text-preview"))]
fn is_text(_: &str) -> bool {
    false
}
