//! [`MarkdownPreview`]: a Markdown file decoded like a text file and shown formatted.

use super::{Outcome, TextSource, cut_note, message_ui, pending_ui};
use crate::markdown::MarkdownEditor;
use crate::preview::content::{PreviewResponse, State, status_ui};

/// A Markdown file in the detail pane: decoded like a [`TextPreview`](super::TextPreview),
/// shown formatted and read-only by alumina-markdown; a link clicked is reported, not
/// opened.
///
/// The decoding, the 256 KiB limit with its note, the kept texts, Retry and the worker
/// thread are the text preview's. A link the person clicks is in
/// [`PreviewResponse::link`], as the Markdown parser reads its target (a relative one
/// comes unresolved): the app decides what it means. There
/// is no toolbar, and typing does nothing.
///
/// Make one and keep it across frames; the thread it starts ends when it is dropped.
#[derive(Default)]
pub struct MarkdownPreview {
    source: TextSource,
}

impl MarkdownPreview {
    /// An empty preview; its thread starts on the first frame that needs it.
    pub fn new() -> MarkdownPreview {
        MarkdownPreview::default()
    }

    /// Draw the Markdown of `state` in the space `ui` has left. For Loading and Failed it
    /// draws the spinner, or the reason and a Retry button, and reports a click on Retry.
    pub fn ui(&mut self, ui: &mut egui::Ui, state: State<'_>) -> PreviewResponse {
        let mut response = PreviewResponse::default();
        let Some(content) = status_ui(ui, state, &mut response) else {
            return response;
        };
        let area = ui.available_rect_before_wrap();
        let Some(outcome) = self.source.get(ui.ctx(), content) else {
            pending_ui(ui, area);
            return response;
        };
        if let Outcome::Text { text, cut } = outcome {
            egui::ScrollArea::vertical()
                .id_salt(("markdown-preview-scroll", content.key.as_str()))
                .auto_shrink(false)
                .show(ui, |ui| {
                    if *cut {
                        cut_note(ui);
                    }
                    let output = MarkdownEditor::new(text)
                        .read_only(true)
                        .id(ui
                            .make_persistent_id(("markdown-preview-editor", content.key.as_str())))
                        .show(ui);
                    response.link = output.link_clicked;
                });
        } else if message_ui(ui, area, outcome) {
            self.source.retry();
            response.retry = true;
        }
        response
    }
}
