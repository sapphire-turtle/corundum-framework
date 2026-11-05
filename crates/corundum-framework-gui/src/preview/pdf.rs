//! [`PdfPreview`]: a PDF the app hands in, its pages rendered off the UI thread one at a
//! time and fitted in the space given.

use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use corundum_framework_pdf::{Document, Error, FallbackFont, MAX_INPUT, MAX_PAGES};

use super::content::{Content, Data, PreviewResponse, State, retry_button, status_ui, target};
use crate::{icons, tr};

/// How many rendered pages are kept.
const KEPT: usize = 8;

/// How many files' place (the page shown, the page count) is remembered.
const REMEMBERED: usize = 64;

/// The icon's size above a message, in points.
const ICON: f32 = 48.0;

/// The size of the page buttons' icons, in points.
const BUTTON: f32 = 16.0;

/// The spinner drawn over a page that is being replaced, in points.
const SMALL_SPINNER: f32 = 20.0;

/// The spinner of a page not yet drawn, in points.
const SPINNER: f32 = 32.0;

/// A request to the worker.
struct Request {
    content: Content,
    /// The page to render, from 0.
    page: usize,
    /// The longest side to render to.
    longest: u32,
}

/// What the worker made of a [`Request`].
struct Answer {
    /// The key of the content rendered.
    key: String,
    page: usize,
    longest: u32,
    rendered: Rendered,
}

/// A render's result, made ready for the UI thread to upload.
enum Rendered {
    Page {
        /// How many pages the file has.
        count: usize,
        pixels: egui::ColorImage,
    },
    Encrypted,
    TooLarge,
    Unsupported,
    /// The file is not a readable PDF.
    Broken(String),
    /// The content could not be read at all.
    Failed(String),
}

impl Rendered {
    fn of_error(error: Error) -> Rendered {
        match error {
            Error::Encrypted => Rendered::Encrypted,
            Error::TooLarge => Rendered::TooLarge,
            Error::Unsupported(_) => Rendered::Unsupported,
            Error::Broken(reason) => Rendered::Broken(reason),
            // The error is non-exhaustive.
            other => Rendered::Failed(other.to_string()),
        }
    }
}

/// What a render left to draw.
enum Outcome {
    Page(egui::TextureHandle),
    Encrypted,
    TooLarge,
    Unsupported,
    Broken(String),
    Failed(String),
}

/// A rendered page, kept.
struct Kept {
    /// The app's key of the content.
    key: String,
    page: usize,
    /// The longest side it was asked to be rendered to.
    longest: u32,
    outcome: Outcome,
}

impl Kept {
    /// Whether this serves a request for `longest` pixels: rendered at least that large, or
    /// as large as the page is.
    fn serves(&self, longest: u32) -> bool {
        match &self.outcome {
            Outcome::Page(texture) => {
                let [w, h] = texture.size();
                self.longest >= longest || (w.max(h) as u32) < self.longest
            }
            _ => true,
        }
    }
}

/// Where a file is: the page shown and, once the worker has opened it, how many it has.
struct Place {
    key: String,
    /// The page shown, from 0.
    page: usize,
    count: Option<usize>,
}

impl Place {
    /// How many pages can be paged through.
    fn shown(&self) -> Option<usize> {
        self.count.map(|c| c.min(MAX_PAGES))
    }
}

/// The worker thread's end: requests out, answers in.
struct Worker {
    requests: Sender<Request>,
    answers: Receiver<Answer>,
    /// Requests sent and not yet answered or passed over. The thread holds a clone for as
    /// long as it runs, which is how the tests see that it ended.
    pending: Arc<AtomicUsize>,
}

impl Worker {
    /// Start the thread. It takes the newest request it has, passing over older ones, and
    /// ends when the [`PdfPreview`] is dropped.
    fn spawn(ctx: egui::Context) -> Worker {
        let (requests, inbox) = channel::<Request>();
        let (outbox, answers) = channel::<Answer>();
        let pending = Arc::new(AtomicUsize::new(0));
        let count = pending.clone();
        std::thread::Builder::new()
            .name("pdf preview".into())
            .spawn(move || {
                // A `Document` must stay on the thread that opened it.
                let mut state = Opened::default();
                while let Ok(mut request) = inbox.recv() {
                    let mut taken = 1;
                    while let Ok(newer) = inbox.try_recv() {
                        request = newer;
                        taken += 1;
                    }
                    let sent = outbox.send(answer(request, &mut state));
                    count.fetch_sub(taken, Ordering::SeqCst);
                    if sent.is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("spawn the pdf preview's thread");
        Worker {
            requests,
            answers,
            pending,
        }
    }
}

/// What the worker keeps between requests.
#[derive(Default)]
struct Opened {
    /// The document of the last content, under its key.
    document: Option<(String, Document)>,
    /// The system's CJK font, looked up on the first open: `Some(None)` if there is none.
    cjk: Option<Option<FallbackFont>>,
}

/// The worker's answer to `request`: the page rendered.
fn answer(request: Request, state: &mut Opened) -> Answer {
    let Request {
        content,
        page,
        longest,
    } = request;
    // The renderer guards itself, but a panic elsewhere would end the thread, and the next
    // frame would start another to render the page again: it fails instead, once.
    let rendered = catch_unwind(AssertUnwindSafe(|| render(&content, page, longest, state)))
        .unwrap_or_else(|_| {
            state.document = None;
            Rendered::Failed("the PDF preview panicked".into())
        });
    Answer {
        key: content.key,
        page,
        longest,
        rendered,
    }
}

/// Render page `page` of the PDF in `content` to `longest` pixels on its longest side,
/// opening it first unless it is the one the worker holds.
fn render(content: &Content, page: usize, longest: u32, state: &mut Opened) -> Rendered {
    if state
        .document
        .as_ref()
        .is_none_or(|(k, _)| *k != content.key)
    {
        state.document = None;
        let bytes = match read(content) {
            Ok(bytes) => bytes,
            Err(rendered) => return rendered,
        };
        let cjk = state.cjk.get_or_insert_with(system_cjk).clone();
        // A file that does not open is not kept: Retry opens it again.
        match Document::open(bytes, cjk) {
            Ok(document) => state.document = Some((content.key.clone(), document)),
            Err(error) => return Rendered::of_error(error),
        }
    }
    let Some((_, document)) = &state.document else {
        return Rendered::Failed("the PDF preview lost its document".into());
    };
    let count = document.page_count();
    let page = page.min(count.min(MAX_PAGES).saturating_sub(1));
    match document.render(page, longest) {
        Ok(rendered) => {
            let size = [rendered.width as usize, rendered.height as usize];
            Rendered::Page {
                count,
                pixels: egui::ColorImage::from_rgba_unmultiplied(size, &rendered.rgba),
            }
        }
        Err(error) => Rendered::of_error(error),
    }
}

/// The file's bytes: the content's own, or a reader's read whole, at most [`MAX_INPUT`] of
/// them.
fn read(content: &Content) -> Result<Arc<[u8]>, Rendered> {
    match &content.data {
        Data::Bytes(bytes) => Ok(bytes.clone()),
        Data::Reader(_) => {
            let mut bytes = Vec::new();
            content
                .open()
                .and_then(|reader| reader.take(MAX_INPUT as u64 + 1).read_to_end(&mut bytes))
                .map_err(|e| Rendered::Failed(e.to_string()))?;
            if bytes.len() > MAX_INPUT {
                return Err(Rendered::TooLarge);
            }
            Ok(bytes.into())
        }
    }
}

/// The system's CJK font, for the PDFs that use one without embedding it.
fn system_cjk() -> Option<FallbackFont> {
    crate::fonts::system_cjk_font().map(|font| FallbackFont {
        data: Arc::new(font.data),
        index: font.index,
    })
}

/// A PDF in the detail pane: its pages rendered on a worker thread, one at a time, fitted in
/// the space given (the page's aspect kept, never drawn larger than its pixels), with a
/// button either side of the page number to page through them.
///
/// What it draws is the [`Content`] the app hands in with a [`State`]; the app's key names
/// the file. The UI thread never reads the content: the worker does, a reader read whole up
/// to 64 MiB. A page is rendered to at most 2048 pixels on its longest side, and only the
/// first 500 pages can be paged to. The last 8 pages shown are kept, so going back to one
/// is instant; the file's place is remembered by key, so a file starts on its first page
/// and one gone back to is where it was left.
///
/// A password-protected, damaged, or too large file says so, and a damaged one offers
/// Retry. Text in a CJK font a PDF does not embed is drawn in the system's, where there is
/// one.
///
/// Make one and keep it across frames; the thread it starts ends when it is dropped.
#[derive(Default)]
pub struct PdfPreview {
    /// Started on the first frame that needs it.
    worker: Option<Worker>,
    /// The key, page and size of the last request sent, until it is answered, so a frame
    /// asks again only for something new.
    asked: Option<(String, usize, u32)>,
    /// The rendered pages, the most recently shown last.
    kept: Vec<Kept>,
    /// Where each file is, the most recently shown last.
    places: Vec<Place>,
}

impl PdfPreview {
    /// An empty preview; its thread starts on the first frame that needs it.
    pub fn new() -> PdfPreview {
        PdfPreview::default()
    }

    /// Draw the page of `state` in the space `ui` has left, with the page buttons under it.
    /// For Loading and Failed it draws the spinner, or the reason and a Retry button, and
    /// reports a click on Retry.
    pub fn ui(&mut self, ui: &mut egui::Ui, state: State<'_>) -> PreviewResponse {
        let mut response = PreviewResponse::default();
        self.receive(ui.ctx());
        let Some(content) = status_ui(ui, state, &mut response) else {
            return response;
        };
        let whole = ui.available_rect_before_wrap();
        ui.allocate_rect(whole, egui::Sense::hover());

        let place = self.place(&content.key);
        let mut area = whole;
        if let Some(shown) = self.places[place].shown() {
            // The buttons first, so that a click shows the new page in this frame.
            let row = ui.spacing().interact_size.y;
            let bar = egui::Rect::from_min_max(
                egui::pos2(whole.left(), (whole.bottom() - row).max(whole.top())),
                whole.max,
            );
            area.max.y = bar.top();
            self.bar(ui, bar, place, shown);
        }

        let page = self.places[place].page;
        let ppp = ui.ctx().pixels_per_point();
        let max_side = ui.ctx().input(|i| i.max_texture_side) as u32;
        let longest = target(area.size(), ppp).min(max_side.max(1));

        let found = self
            .kept
            .iter()
            .position(|k| k.key == content.key && k.page == page);
        let Some(at) = found else {
            self.ask(ui.ctx(), content, page, longest);
            // The page before is drawn until this one comes, with a small spinner over it.
            let previous = self
                .kept
                .iter()
                .rposition(|k| k.key == content.key && matches!(k.outcome, Outcome::Page(_)));
            if let Some(previous) = previous {
                self.draw(ui, area, previous);
            }
            let side = if previous.is_some() {
                SMALL_SPINNER
            } else {
                SPINNER
            };
            egui::Spinner::new().paint_at(
                ui,
                egui::Rect::from_center_size(area.center(), egui::vec2(side, side)),
            );
            return response;
        };
        let kept = self.kept.remove(at);
        self.kept.push(kept);
        let last = self.kept.len() - 1;
        if !self.kept[last].serves(longest) {
            // The smaller render is drawn until the larger one comes.
            self.ask(ui.ctx(), content, page, longest);
        }
        if self.draw(ui, area, last) {
            // Forget the failure: the next frame asks the worker again.
            self.kept.pop();
            response.retry = true;
        }
        response
    }

    /// The index in `places` of the file `key`, made if new, and moved to the end.
    fn place(&mut self, key: &str) -> usize {
        let place = match self.places.iter().position(|p| p.key == key) {
            Some(at) => self.places.remove(at),
            None => Place {
                key: key.to_owned(),
                page: 0,
                count: None,
            },
        };
        self.places.push(place);
        if self.places.len() > REMEMBERED {
            self.places.remove(0);
        }
        self.places.len() - 1
    }

    /// Draw the page buttons and number in `bar`, and turn the page of `places[place]` if
    /// one is clicked. `shown` is how many pages there are to turn to.
    fn bar(&mut self, ui: &mut egui::Ui, bar: egui::Rect, place: usize, shown: usize) {
        let Place { page, count, .. } = self.places[place];
        let count = count.unwrap_or(shown);
        let label = if count > shown {
            tr!(
                ui,
                "corundum-preview-pdf-page-of",
                page = page + 1,
                shown = shown,
                count = count
            )
        } else {
            tr!(
                ui,
                "corundum-preview-pdf-page",
                page = page + 1,
                count = count
            )
        };
        let mut row = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
        ));
        let mut turn = 0isize;
        // A centred row of three: the label in the middle, a button either side of it.
        let text_width = {
            let font = egui::TextStyle::Body.resolve(row.style());
            row.fonts_mut(|f| {
                f.layout_no_wrap(label.clone(), font, egui::Color32::WHITE)
                    .size()
                    .x
            })
        };
        let gap = row.spacing().item_spacing.x;
        let button = BUTTON + 2.0 * row.spacing().button_padding.x;
        let total = 2.0 * button + 2.0 * gap + text_width;
        let left = bar.center().x - total / 2.0;
        let place_at = |x: f32, width: f32| {
            egui::Rect::from_min_size(egui::pos2(x, bar.top()), egui::vec2(width, bar.height()))
        };
        let previous = place_at(left, button);
        let middle = place_at(left + button + gap, text_width);
        let next = place_at(left + button + gap + text_width + gap, button);
        for (rect, icon, enabled, key, step) in [
            (
                previous,
                icons::chevron_left(),
                page > 0,
                "corundum-preview-pdf-previous",
                -1,
            ),
            (
                next,
                icons::chevron_right(),
                page + 1 < shown,
                "corundum-preview-pdf-next",
                1,
            ),
        ] {
            let name = tr!(ui, key);
            let response = row
                .scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.add_enabled(enabled, icons::button(icon, BUTTON))
                })
                .inner;
            let response = response.on_hover_text(name.clone());
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, enabled, name.clone())
            });
            if response.clicked() {
                turn = step;
            }
        }
        row.scope_builder(egui::UiBuilder::new().max_rect(middle), |ui| {
            ui.add(egui::Label::new(label).wrap_mode(egui::TextWrapMode::Extend));
        });
        if turn != 0 {
            self.places[place].page = page.saturating_add_signed(turn).min(shown - 1);
        }
    }

    /// Draw the kept render at `at` in `area`. True if Retry, offered for a failure to read,
    /// was clicked.
    fn draw(&self, ui: &mut egui::Ui, area: egui::Rect, at: usize) -> bool {
        let message = match &self.kept[at].outcome {
            Outcome::Page(texture) => {
                let ppp = ui.ctx().pixels_per_point();
                let full = texture.size_vec2() / ppp;
                let scale = (area.width() / full.x).min(area.height() / full.y).min(1.0);
                let shown = egui::Rect::from_center_size(area.center(), full * scale);
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter()
                    .image(texture.id(), shown, uv, egui::Color32::WHITE);
                return false;
            }
            Outcome::Encrypted => tr!(ui, "corundum-preview-pdf-encrypted"),
            Outcome::TooLarge => tr!(ui, "corundum-preview-pdf-too-large"),
            Outcome::Unsupported => tr!(ui, "corundum-preview-unsupported"),
            Outcome::Broken(reason) => {
                tr!(ui, "corundum-preview-pdf-broken", reason = reason.as_str())
            }
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
        matches!(
            self.kept[at].outcome,
            Outcome::Broken(_) | Outcome::Failed(_)
        ) && retry_button(&mut inner)
    }

    /// Send the worker a request for `page` of `content` at `longest` pixels, unless that is
    /// the one still unanswered.
    fn ask(&mut self, ctx: &egui::Context, content: &Content, page: usize, longest: u32) {
        if self
            .asked
            .as_ref()
            .is_some_and(|(k, p, l)| *k == content.key && *p == page && *l == longest)
        {
            return;
        }
        let worker = self
            .worker
            .get_or_insert_with(|| Worker::spawn(ctx.clone()));
        worker.pending.fetch_add(1, Ordering::SeqCst);
        let request = Request {
            content: content.clone(),
            page,
            longest,
        };
        if worker.requests.send(request).is_err() {
            // The thread is gone (it panicked): start another on the next frame.
            self.worker = None;
            self.asked = None;
            return;
        }
        self.asked = Some((content.key.clone(), page, longest));
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
                    page,
                    longest,
                    rendered,
                }) => {
                    if self
                        .asked
                        .as_ref()
                        .is_some_and(|(k, p, l)| *k == key && *p == page && *l == longest)
                    {
                        self.asked = None;
                    }
                    let outcome = match rendered {
                        Rendered::Page { count, pixels } => {
                            if let Some(place) = self.places.iter_mut().find(|p| p.key == key) {
                                place.count = Some(count);
                            }
                            Outcome::Page(ctx.load_texture(
                                format!("corundum-preview {key} {page}"),
                                pixels,
                                egui::TextureOptions::LINEAR,
                            ))
                        }
                        Rendered::Encrypted => Outcome::Encrypted,
                        Rendered::TooLarge => Outcome::TooLarge,
                        Rendered::Unsupported => Outcome::Unsupported,
                        Rendered::Broken(reason) => Outcome::Broken(reason),
                        Rendered::Failed(reason) => Outcome::Failed(reason),
                    };
                    self.kept.retain(|k| !(k.key == key && k.page == page));
                    self.kept.push(Kept {
                        key,
                        page,
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
}
