//! [`ImagePreview`]: a picture the app hands in, decoded off the UI thread and fitted in the
//! space given.

use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use corundum_framework_image::{Error, Image, Limits, Source, thumbnail};

pub(super) use super::content::target;
use super::content::{Content, Data, PreviewResponse, State, retry_button, status_ui};
use crate::{icons, tr};

/// How many decoded pictures are kept.
const KEPT: usize = 8;

/// The most pixels a picture may have, by default: 64 megapixels, a large camera's photo.
const MAX_PIXELS: u64 = 64_000_000;

/// The most bytes of a file read whole to be decoded; a longer one is too large.
const READ_MAX: u64 = 256 * 1024 * 1024;

/// The icon's size above a message, in points.
const ICON: f32 = 48.0;

/// A request to the worker.
struct Request {
    content: Content,
    /// The longest side to decode to.
    longest: u32,
    /// How big a picture may be.
    limits: Limits,
}

/// What the worker made of a [`Request`].
struct Answer {
    /// The key of the content decoded.
    key: String,
    longest: u32,
    decoded: Decoded,
}

/// A decode's result, made ready for the UI thread to upload.
enum Decoded {
    Picture(egui::ColorImage),
    Unsupported,
    TooLarge,
    Failed(String),
}

impl Decoded {
    fn of(result: Result<Image, Error>) -> Decoded {
        match result {
            Ok(image) => {
                let size = [image.width as usize, image.height as usize];
                Decoded::Picture(egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba))
            }
            Err(Error::Unsupported) => Decoded::Unsupported,
            Err(Error::TooLarge) => Decoded::TooLarge,
            Err(error) => Decoded::Failed(error.to_string()),
        }
    }
}

/// What a decode left to draw.
enum Outcome {
    Picture(egui::TextureHandle),
    Unsupported,
    TooLarge,
    Failed(String),
}

/// A decoded content, kept.
struct Kept {
    /// The app's key of the content.
    key: String,
    /// The longest side it was asked to be decoded to.
    longest: u32,
    outcome: Outcome,
}

impl Kept {
    /// Whether this serves a request for `longest` pixels: decoded at least that large, or
    /// as large as the picture is.
    fn serves(&self, longest: u32) -> bool {
        match &self.outcome {
            Outcome::Picture(texture) => {
                let [w, h] = texture.size();
                self.longest >= longest || (w.max(h) as u32) < self.longest
            }
            _ => true,
        }
    }
}

/// The worker thread's end: requests out, answers in.
struct Worker {
    requests: Sender<Request>,
    answers: Receiver<Answer>,
    /// Requests sent and not yet answered or passed over. The thread holds a clone for as
    /// long as it runs, which is how the tests see that it ended, and when it is idle.
    pending: Arc<AtomicUsize>,
}

impl Worker {
    /// Start the thread. It takes the newest request it has, passing over older ones, and
    /// ends when the [`ImagePreview`] is dropped.
    fn spawn(ctx: egui::Context) -> Worker {
        let (requests, inbox) = channel::<Request>();
        let (outbox, answers) = channel::<Answer>();
        let pending = Arc::new(AtomicUsize::new(0));
        let count = pending.clone();
        std::thread::Builder::new()
            .name("image preview".into())
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
            .expect("spawn the image preview's thread");
        Worker {
            requests,
            answers,
            pending,
        }
    }
}

/// The worker's answer to `request`: the content decoded.
fn answer(request: Request) -> Answer {
    let Request {
        content,
        longest,
        limits,
    } = request;
    // The decoder guards itself, but a panic elsewhere (the resizer, say) would end the
    // thread, and the next frame would start another to decode the content again: it fails
    // instead, once, like a broken file.
    let decoded = catch_unwind(AssertUnwindSafe(|| decode(&content, longest, &limits)))
        .unwrap_or_else(|_| Decoded::Failed("the image preview panicked".into()));
    Answer {
        key: content.key,
        longest,
        decoded,
    }
}

/// Decode the picture in `content` to `longest` pixels on its longest side. A reader is
/// read whole first, up to [`READ_MAX`] bytes.
fn decode(content: &Content, longest: u32, limits: &Limits) -> Decoded {
    #[cfg(test)]
    if content.name == PANIC_NAME {
        PANICS.fetch_add(1, Ordering::SeqCst);
        panic!("a test's panic in the image preview's worker");
    }
    let ext = content.ext();
    let ext = ext.as_deref();
    match &content.data {
        Data::Bytes(bytes) => Decoded::of(thumbnail(Source::Bytes { bytes, ext }, longest, limits)),
        Data::Reader(_) => {
            let mut bytes = Vec::new();
            let read = content
                .open()
                .and_then(|reader| reader.take(READ_MAX + 1).read_to_end(&mut bytes));
            match read {
                Err(e) => Decoded::Failed(e.to_string()),
                Ok(_) if bytes.len() as u64 > READ_MAX => Decoded::TooLarge,
                Ok(_) => Decoded::of(thumbnail(
                    Source::Bytes { bytes: &bytes, ext },
                    longest,
                    limits,
                )),
            }
        }
    }
}

/// A file name the worker panics on, past the decoder's own guard, for a test.
#[cfg(test)]
pub(super) const PANIC_NAME: &str = "corundum-preview-panics.png";

/// How many times the worker panicked on [`PANIC_NAME`].
#[cfg(test)]
static PANICS: AtomicUsize = AtomicUsize::new(0);

/// How many times the worker panicked on [`PANIC_NAME`], for a test.
#[cfg(test)]
pub(super) fn panics() -> usize {
    PANICS.load(Ordering::SeqCst)
}

/// A picture in the detail pane: decoded on a worker thread to the size it is drawn at
/// (2048 pixels at most on its longest side), with a spinner meanwhile, and fitted in the
/// space given, its aspect kept and never drawn larger than its pixels.
///
/// What it draws is the [`Content`] the app hands in with a [`State`]; the app's key names
/// the picture. The last pictures shown are kept by key, so going back to one is instant.
/// The file is never looked at again: when it changes, the app gives it another key. The UI
/// thread never reads the content: the worker does, and decodes it. A file the decoder does
/// not know (HEIC, say) shows the file icon and says so; one too large or broken says why.
///
/// A picture over its [`Limits`] is too large: by default, over 64 megapixels. Peak memory
/// while one is decoded is two of its buffers at a time (the decoded one and the copy an
/// orientation, the RGBA conversion or the colour conversion makes), some 8 to 16 bytes a
/// pixel: up to about 1 GB at 64 megapixels, where the 200 of [`Limits::default`] would
/// take over 3. A failed allocation ends the app rather than failing the decode, so on a
/// phone keep this low; [`ImagePreview::with_limits`] sets another bound.
///
/// Make one and keep it across frames; the thread it starts ends when it is dropped.
pub struct ImagePreview {
    /// Started on the first frame that needs it.
    worker: Option<Worker>,
    /// The key and size of the last request sent, until it is answered, so a frame asks
    /// again only for something new.
    asked: Option<(String, u32)>,
    /// The decoded contents, one per key, the most recently shown last.
    kept: Vec<Kept>,
    /// How big a picture may be.
    limits: Limits,
}

impl Default for ImagePreview {
    fn default() -> ImagePreview {
        ImagePreview {
            worker: None,
            asked: None,
            kept: Vec::new(),
            limits: Limits::default().with_max_pixels(MAX_PIXELS),
        }
    }
}

impl ImagePreview {
    /// An empty preview; its thread starts on the first frame that needs it.
    pub fn new() -> ImagePreview {
        ImagePreview::default()
    }

    /// This preview with `limits` on the pictures it decodes, instead of the default:
    /// [`Limits::default`] with 64 megapixels at most.
    pub fn with_limits(self, limits: Limits) -> ImagePreview {
        ImagePreview { limits, ..self }
    }

    /// Draw the picture of `state` in the space `ui` has left. For Loading and Failed it
    /// draws the spinner, or the reason and a Retry button, and reports a click on Retry.
    pub fn ui(&mut self, ui: &mut egui::Ui, state: State<'_>) -> PreviewResponse {
        let mut response = PreviewResponse::default();
        self.receive(ui.ctx());
        let Some(content) = status_ui(ui, state, &mut response) else {
            return response;
        };
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, egui::Sense::hover());
        let ppp = ui.ctx().pixels_per_point();
        let max_side = ui.ctx().input(|i| i.max_texture_side) as u32;
        let longest = target(area.size(), ppp).min(max_side.max(1));

        let Some(at) = self.kept.iter().position(|k| k.key == content.key) else {
            self.ask(ui.ctx(), content, longest);
            egui::Spinner::new().paint_at(
                ui,
                egui::Rect::from_center_size(area.center(), egui::vec2(32.0, 32.0)),
            );
            return response;
        };
        let kept = self.kept.remove(at);
        self.kept.push(kept);
        let kept = &self.kept[self.kept.len() - 1];
        if !kept.serves(longest) {
            // The smaller decode is drawn until the larger one comes.
            self.ask(ui.ctx(), content, longest);
        }
        if self.draw(ui, area, self.kept.len() - 1) {
            // Forget the failure: the next frame asks the worker again.
            self.kept.pop();
            response.retry = true;
        }
        response
    }

    /// Draw the kept decode at `at` in `area`. True if Retry, offered for a failure to read,
    /// was clicked.
    fn draw(&self, ui: &mut egui::Ui, area: egui::Rect, at: usize) -> bool {
        let message = match &self.kept[at].outcome {
            Outcome::Picture(texture) => {
                let ppp = ui.ctx().pixels_per_point();
                let full = texture.size_vec2() / ppp;
                let scale = (area.width() / full.x).min(area.height() / full.y).min(1.0);
                let shown = egui::Rect::from_center_size(area.center(), full * scale);
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter()
                    .image(texture.id(), shown, uv, egui::Color32::WHITE);
                return false;
            }
            Outcome::Unsupported => tr!(ui, "corundum-preview-unsupported"),
            Outcome::TooLarge => tr!(ui, "corundum-preview-too-large"),
            Outcome::Failed(reason) => tr!(ui, "corundum-preview-failed", reason = reason.as_str()),
        };
        let mut inner = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );
        inner.add_space((area.height() / 2.0 - ICON).max(0.0));
        inner.add(icons::image(&inner, icons::file(), ICON));
        inner.add(egui::Label::new(message).wrap());
        matches!(self.kept[at].outcome, Outcome::Failed(_)) && retry_button(&mut inner)
    }

    /// Send the worker a request for `content` at `longest` pixels, unless that is the one
    /// still unanswered.
    fn ask(&mut self, ctx: &egui::Context, content: &Content, longest: u32) {
        if self
            .asked
            .as_ref()
            .is_some_and(|(k, l)| *k == content.key && *l == longest)
        {
            return;
        }
        let worker = self
            .worker
            .get_or_insert_with(|| Worker::spawn(ctx.clone()));
        worker.pending.fetch_add(1, Ordering::SeqCst);
        let request = Request {
            content: content.clone(),
            longest,
            limits: self.limits,
        };
        if worker.requests.send(request).is_err() {
            // The thread is gone (it panicked): start another on the next frame.
            self.worker = None;
            self.asked = None;
            return;
        }
        self.asked = Some((content.key.clone(), longest));
    }

    /// Keep what the worker has finished; whether it is shown is the frame's to decide.
    fn receive(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        loop {
            match worker.answers.try_recv() {
                Ok(Answer {
                    key,
                    longest,
                    decoded,
                }) => {
                    answered(&mut self.asked, &key, longest);
                    let outcome = match decoded {
                        Decoded::Picture(pixels) => Outcome::Picture(ctx.load_texture(
                            format!("corundum-preview {key}"),
                            pixels,
                            egui::TextureOptions::LINEAR,
                        )),
                        Decoded::Unsupported => Outcome::Unsupported,
                        Decoded::TooLarge => Outcome::TooLarge,
                        Decoded::Failed(reason) => Outcome::Failed(reason),
                    };
                    self.kept.retain(|k| k.key != key);
                    self.kept.push(Kept {
                        key,
                        longest,
                        outcome,
                    });
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

    /// The limits on the pictures decoded, for a test.
    #[cfg(test)]
    pub(super) fn limits(&self) -> Limits {
        self.limits
    }

    /// Something the worker thread holds while it runs.
    #[cfg(test)]
    pub(super) fn thread_token(&self) -> Option<std::sync::Weak<AtomicUsize>> {
        self.worker.as_ref().map(|w| Arc::downgrade(&w.pending))
    }

    /// Whether the worker has nothing left to do.
    #[cfg(test)]
    pub(super) fn idle(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(|w| w.pending.load(Ordering::SeqCst) == 0)
    }
}

/// Forget the request for `key` at `longest` as unanswered, if it was the last sent.
fn answered(asked: &mut Option<(String, u32)>, key: &str, longest: u32) {
    if asked
        .as_ref()
        .is_some_and(|(k, l)| k == key && *l == longest)
    {
        *asked = None;
    }
}
