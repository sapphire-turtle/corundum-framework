//! [`ImagePreview`]: a picture file, decoded off the UI thread and fitted in the space given.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::SystemTime;

use corundum_framework_image::{Error, Image, Limits, Source, thumbnail};

use crate::{icons, tr};

/// The sizes a picture is decoded to, in pixels on its longest side: the space it is drawn
/// in, rounded up to one of these, so a pane resized a little does not decode it again.
const SIZES: [u32; 4] = [256, 512, 1024, 2048];

/// How many decoded pictures are kept.
const KEPT: usize = 8;

/// The icon's size above a message, in points.
const ICON: f32 = 48.0;

/// A file as it was when asked for: a file changed on disk is another picture.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Key {
    path: PathBuf,
    modified: Option<SystemTime>,
    size: u64,
}

impl Key {
    fn of(path: &Path) -> Key {
        let meta = std::fs::metadata(path).ok();
        Key {
            path: path.to_owned(),
            modified: meta.as_ref().and_then(|m| m.modified().ok()),
            size: meta.map_or(0, |m| m.len()),
        }
    }
}

/// A request to the worker: the file, and the longest side to decode it to.
type Request = (Key, u32);

/// What the worker made of a [`Request`].
type Answer = (Key, u32, Result<Image, Error>);

/// What a decode left to draw.
enum Outcome {
    Picture(egui::TextureHandle),
    Unsupported,
    TooLarge,
    Failed(String),
}

/// A decoded file, kept.
struct Kept {
    key: Key,
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
    /// Requests sent and not yet answered or passed over.
    pending: Arc<AtomicUsize>,
}

impl Worker {
    /// Start the thread. It decodes the newest request it has, passing over older ones, and
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
                    let (key, longest) = request;
                    let decoded = thumbnail(Source::Path(&key.path), longest, &Limits::default());
                    let sent = outbox.send((key, longest, decoded));
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

/// A picture file in the detail pane: decoded on a worker thread to the size it is drawn
/// at (2048 pixels at most on its longest side), with a spinner meanwhile, and fitted in
/// the space given, its aspect kept and never drawn larger than its pixels.
///
/// The last pictures shown are kept, by path, modified time and size, so going back to
/// one is instant. A file the decoder does not know (HEIC, say) shows the file icon and
/// says so; one too large or broken says why.
///
/// Make one and keep it across frames; the thread it starts ends when it is dropped.
#[derive(Default)]
pub struct ImagePreview {
    /// Started on the first frame that needs it.
    worker: Option<Worker>,
    /// The last request sent, so a frame asks again only for something new.
    asked: Option<Request>,
    /// The decoded files, the most recently shown last.
    kept: Vec<Kept>,
}

impl ImagePreview {
    /// An empty preview; its thread starts on the first frame that needs it.
    pub fn new() -> ImagePreview {
        ImagePreview::default()
    }

    /// Draw `path` in the space `ui` has left.
    pub fn ui(&mut self, ui: &mut egui::Ui, path: &Path) {
        self.receive(ui.ctx());
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, egui::Sense::hover());
        let ppp = ui.ctx().pixels_per_point();
        let max_side = ui.ctx().input(|i| i.max_texture_side) as u32;
        let longest = target(area.size(), ppp).min(max_side.max(1));
        let key = Key::of(path);

        match self
            .kept
            .iter()
            .position(|k| k.key == key && k.serves(longest))
        {
            Some(at) => {
                let kept = self.kept.remove(at);
                self.kept.push(kept);
                self.draw(ui, area, self.kept.len() - 1);
            }
            None => {
                // An older decode of the same file, smaller, is better than a spinner.
                let older = self.kept.iter().rposition(|k| k.key == key);
                self.ask(ui.ctx(), key, longest);
                match older {
                    Some(at) => self.draw(ui, area, at),
                    None => egui::Spinner::new().paint_at(
                        ui,
                        egui::Rect::from_center_size(area.center(), egui::vec2(32.0, 32.0)),
                    ),
                }
            }
        }
    }

    /// Draw the kept decode at `at` in `area`.
    fn draw(&self, ui: &mut egui::Ui, area: egui::Rect, at: usize) {
        let message = match &self.kept[at].outcome {
            Outcome::Picture(texture) => {
                let ppp = ui.ctx().pixels_per_point();
                let full = texture.size_vec2() / ppp;
                let scale = (area.width() / full.x).min(area.height() / full.y).min(1.0);
                let shown = egui::Rect::from_center_size(area.center(), full * scale);
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter()
                    .image(texture.id(), shown, uv, egui::Color32::WHITE);
                return;
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
    }

    /// Send the worker `key` at `longest` pixels, unless it was the last thing asked.
    fn ask(&mut self, ctx: &egui::Context, key: Key, longest: u32) {
        let request = (key, longest);
        if self.asked.as_ref() == Some(&request) {
            return;
        }
        let worker = self
            .worker
            .get_or_insert_with(|| Worker::spawn(ctx.clone()));
        worker.pending.fetch_add(1, Ordering::SeqCst);
        if worker.requests.send(request.clone()).is_err() {
            // The thread is gone (it panicked): start another on the next frame.
            self.worker = None;
            self.asked = None;
            return;
        }
        self.asked = Some(request);
    }

    /// Keep what the worker has finished; whether it is shown is the frame's to decide.
    fn receive(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        loop {
            match worker.answers.try_recv() {
                Ok((key, longest, decoded)) => {
                    let outcome = match decoded {
                        Ok(image) => Outcome::Picture(upload(ctx, &key, image)),
                        Err(Error::Unsupported) => Outcome::Unsupported,
                        Err(Error::TooLarge) => Outcome::TooLarge,
                        Err(error) => Outcome::Failed(error.to_string()),
                    };
                    if self
                        .asked
                        .as_ref()
                        .is_some_and(|(k, l)| *k == key && *l == longest)
                    {
                        self.asked = None;
                    }
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

/// `image` as a texture of `ctx`'s, named after `key`'s file.
fn upload(ctx: &egui::Context, key: &Key, image: Image) -> egui::TextureHandle {
    let size = [image.width as usize, image.height as usize];
    let pixels = egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba);
    ctx.load_texture(
        format!("corundum-preview {}", key.path.display()),
        pixels,
        egui::TextureOptions::LINEAR,
    )
}

/// The longest side to decode to for a `size`-point space at `ppp` pixels per point: the
/// first of [`SIZES`] at least that large, or the largest.
pub(super) fn target(size: egui::Vec2, ppp: f32) -> u32 {
    let wanted = (size.x.max(size.y) * ppp).ceil().max(0.0) as u32;
    SIZES
        .into_iter()
        .find(|&s| s >= wanted)
        .unwrap_or(SIZES[SIZES.len() - 1])
}
