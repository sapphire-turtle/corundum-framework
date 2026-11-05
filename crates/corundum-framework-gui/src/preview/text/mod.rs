//! [`TextPreview`]: a text file the app hands in, decoded off the UI thread and shown
//! read-only.

mod decode;

use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use decode::{NotText, TEXT_LIMIT, Text, decode_text};

use super::content::{Content, PreviewResponse, State, retry_button, status_ui};
use crate::code::CodeEditor;
use crate::{explorer, icons, tr};

/// How many decoded texts are kept.
const KEPT: usize = 8;

/// The icon's size above a message, in points.
const ICON: f32 = 48.0;

/// What the worker made of a request.
struct Answer {
    /// The key of the content decoded.
    key: String,
    outcome: Outcome,
}

/// What reading a content left to draw.
enum Outcome {
    Text { text: String, cut: bool },
    Binary,
    Failed(String),
}

/// The worker thread's end: requests out, answers in.
struct Worker {
    requests: Sender<Content>,
    answers: Receiver<Answer>,
    /// Requests sent and not yet answered or passed over. The thread holds a clone for as
    /// long as it runs, which is how the tests see that it ended, and when it is idle.
    pending: Arc<AtomicUsize>,
}

impl Worker {
    /// Start the thread. It takes the newest request it has, passing over older ones, and
    /// ends when the [`TextSource`] is dropped.
    fn spawn(ctx: egui::Context) -> Worker {
        let (requests, inbox) = channel::<Content>();
        let (outbox, answers) = channel::<Answer>();
        let pending = Arc::new(AtomicUsize::new(0));
        let count = pending.clone();
        std::thread::Builder::new()
            .name("text preview".into())
            .spawn(move || {
                while let Ok(mut request) = inbox.recv() {
                    let mut taken = 1;
                    while let Ok(newer) = inbox.try_recv() {
                        request = newer;
                        taken += 1;
                    }
                    let sent = outbox.send(answer(request));
                    count.fetch_sub(taken, Ordering::SeqCst);
                    if sent.is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn the text preview's thread");
        Worker {
            requests,
            answers,
            pending,
        }
    }
}

/// The worker's answer to `content`: its first bytes decoded.
fn answer(content: Content) -> Answer {
    // A panic would end the thread, and the next frame would start another to read the
    // content again: it fails instead, once, like an unreadable file.
    let outcome = catch_unwind(AssertUnwindSafe(|| read(&content)))
        .unwrap_or_else(|_| Outcome::Failed("the text preview panicked".into()));
    Answer {
        key: content.key,
        outcome,
    }
}

/// Read the first bytes of `content`, one past the limit to learn whether it was cut, and
/// decode them.
fn read(content: &Content) -> Outcome {
    let mut bytes = Vec::new();
    let read = content
        .open()
        .and_then(|reader| reader.take(TEXT_LIMIT as u64 + 1).read_to_end(&mut bytes));
    if let Err(e) = read {
        return Outcome::Failed(e.to_string());
    }
    match decode_text(&bytes) {
        Ok(Text { text, cut, .. }) => Outcome::Text { text, cut },
        Err(NotText::Binary) => Outcome::Binary,
    }
}

/// The worker, the key it was last asked for, and the texts it has finished: what a preview
/// of a text file needs to decode off the UI thread. The thread ends when it is dropped.
#[derive(Default)]
struct TextSource {
    /// Started on the first frame that needs it.
    worker: Option<Worker>,
    /// The key of the last request sent, until it is answered, so a frame asks again only
    /// for something new.
    asked: Option<String>,
    /// The decoded contents, one per key, the most recently shown last.
    kept: Vec<(String, Outcome)>,
}

impl TextSource {
    /// The outcome for `content`, which becomes the most recently shown; or `None` while
    /// the worker is still reading it (it is asked, once).
    fn get(&mut self, ctx: &egui::Context, content: &Content) -> Option<&mut Outcome> {
        self.receive();
        let Some(at) = self.kept.iter().position(|(k, _)| *k == content.key) else {
            self.ask(ctx, content);
            return None;
        };
        let kept = self.kept.remove(at);
        self.kept.push(kept);
        self.kept.last_mut().map(|(_, outcome)| outcome)
    }

    /// Forget the entry [`TextSource::get`] last returned, a failure the person retries:
    /// the next frame asks the worker again.
    fn retry(&mut self) {
        self.kept.pop();
    }

    /// Send the worker a request for `content`, unless that is the one still unanswered.
    fn ask(&mut self, ctx: &egui::Context, content: &Content) {
        if self.asked.as_ref() == Some(&content.key) {
            return;
        }
        let worker = self
            .worker
            .get_or_insert_with(|| Worker::spawn(ctx.clone()));
        worker.pending.fetch_add(1, Ordering::SeqCst);
        if worker.requests.send(content.clone()).is_err() {
            // The thread is gone (it panicked): start another on the next frame.
            self.worker = None;
            self.asked = None;
            return;
        }
        self.asked = Some(content.key.clone());
    }

    /// Keep what the worker has finished; whether it is shown is the frame's to decide.
    fn receive(&mut self) {
        let Some(worker) = &self.worker else {
            return;
        };
        loop {
            match worker.answers.try_recv() {
                Ok(Answer { key, outcome }) => {
                    if self.asked.as_ref() == Some(&key) {
                        self.asked = None;
                    }
                    self.kept.retain(|(k, _)| *k != key);
                    self.kept.push((key, outcome));
                    if self.kept.len() > KEPT {
                        self.kept.remove(0);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.asked = None;
                    break;
                }
            }
        }
    }
}

/// The spinner of a text still being read, centred in `area`.
fn pending_ui(ui: &mut egui::Ui, area: egui::Rect) {
    ui.allocate_rect(area, egui::Sense::hover());
    egui::Spinner::new().paint_at(
        ui,
        egui::Rect::from_center_size(area.center(), egui::vec2(32.0, 32.0)),
    );
}

/// The note above a text that is only the file's first bytes.
fn cut_note(ui: &mut egui::Ui) {
    let size = explorer::format_size(TEXT_LIMIT as u64);
    ui.label(
        egui::RichText::new(tr!(ui, "corundum-preview-text-cut", size = size.as_str())).weak(),
    );
}

/// For a binary file or a failure: the file icon and the message, centred in `area`, and
/// for a failure Retry too. Whether Retry was clicked.
fn message_ui(ui: &mut egui::Ui, area: egui::Rect, outcome: &Outcome) -> bool {
    let message = match outcome {
        Outcome::Text { .. } => return false,
        Outcome::Binary => tr!(ui, "corundum-preview-unsupported"),
        Outcome::Failed(reason) => tr!(ui, "corundum-preview-failed", reason = reason.as_str()),
    };
    ui.allocate_rect(area, egui::Sense::hover());
    let mut inner = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(area)
            .layout(egui::Layout::top_down(egui::Align::Center)),
    );
    inner.add_space((area.height() / 2.0 - ICON).max(0.0));
    inner.add(icons::image(&inner, icons::file(), ICON));
    inner.add(egui::Label::new(message).wrap());
    matches!(outcome, Outcome::Failed(_)) && retry_button(&mut inner)
}

/// A text file in the detail pane: decoded on a worker thread, shown read-only and
/// monospace in a scroll area, where it can be selected and copied.
///
/// What it draws is the [`Content`] the app hands in with a [`State`]; the app's key names
/// the file. At most the first 256 KiB are read, and a note says when the file is longer.
/// Only Unicode is read: UTF-8, or UTF-16 with a BOM; other bytes show as U+FFFD. The last
/// texts shown are kept by key, so going back to one is instant, and its scroll position
/// stays. The UI thread never reads the content: the worker does. A file that is not text
/// (it has NUL bytes and no BOM) shows the file icon and says it can't be previewed; one
/// that can't be read says why, with Retry.
///
/// Make one and keep it across frames; the thread it starts ends when it is dropped.
#[derive(Default)]
pub struct TextPreview {
    source: TextSource,
}

impl TextPreview {
    /// An empty preview; its thread starts on the first frame that needs it.
    pub fn new() -> TextPreview {
        TextPreview::default()
    }

    /// Draw the text of `state` in the space `ui` has left. For Loading and Failed it draws
    /// the spinner, or the reason and a Retry button, and reports a click on Retry.
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
                .id_salt(("text-preview-scroll", content.key.as_str()))
                .auto_shrink(false)
                .show(ui, |ui| {
                    if *cut {
                        cut_note(ui);
                    }
                    CodeEditor::new(text)
                        .read_only(true)
                        .id(egui::Id::unique((
                            "text-preview-editor",
                            content.key.as_str(),
                        )))
                        .show(ui);
                });
        } else if message_ui(ui, area, outcome) {
            self.source.retry();
            response.retry = true;
        }
        response
    }

    /// Something the worker thread holds while it runs.
    #[cfg(test)]
    pub(super) fn thread_token(&self) -> Option<std::sync::Weak<AtomicUsize>> {
        self.source
            .worker
            .as_ref()
            .map(|w| Arc::downgrade(&w.pending))
    }

    /// Whether the worker has nothing left to do.
    #[cfg(test)]
    pub(super) fn idle(&self) -> bool {
        self.source
            .worker
            .as_ref()
            .is_none_or(|w| w.pending.load(Ordering::SeqCst) == 0)
    }
}
