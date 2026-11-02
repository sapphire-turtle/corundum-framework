//! A track decoded on a thread of its own, from any [`Source`].

use std::io::{self, Read, Seek, SeekFrom};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};

use crate::decode::{self, DecodeError, Source};
use crate::resample::Converter;
use crate::{Feed, FeedEvent, OutputConfig, TrackError, Wait};

/// Messages the decoder thread can hold before it waits for the player.
const CHANNEL_CAPACITY: usize = 32;
/// No duration known yet.
const NO_DURATION: u64 = u64::MAX;

/// Running decode jobs.
static LIVE: AtomicUsize = AtomicUsize::new(0);

/// How many decode threads are running (or about to), for tests.
#[cfg(any(test, feature = "test-support"))]
pub fn live_feeds() -> usize {
    LIVE.load(Ordering::SeqCst)
}

/// Counts a job for as long as it lives, a panic included.
struct LiveGuard;

impl LiveGuard {
    fn new() -> LiveGuard {
        LIVE.fetch_add(1, Ordering::SeqCst);
        LiveGuard
    }
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

/// What a fetch under a source says about itself.
#[derive(Clone, PartialEq, Debug)]
pub enum SourceStatus {
    Ready,
    Waiting(Wait),
    Failed(TrackError),
}

/// How a fetch under a feed reports itself. A file has none.
pub trait Watch: Send + Sync {
    fn status(&self) -> SourceStatus;
}

/// A feed that is already over.
pub(crate) struct FailedFeed(pub(crate) TrackError);

impl Feed for FailedFeed {
    fn poll(&mut self) -> FeedEvent {
        FeedEvent::Failed(self.0.clone())
    }

    fn duration_ms(&self) -> Option<u64> {
        None
    }
}

enum Message {
    Pcm(Vec<f32>),
    End,
    Failed(TrackError),
}

/// One track being decoded on its own thread.
struct TrackFeed {
    rx: Receiver<Message>,
    watch: Option<Arc<dyn Watch>>,
    stop: Arc<AtomicBool>,
    canceller: Option<Box<dyn Fn() + Send + Sync>>,
    duration: Arc<AtomicU64>,
    /// How the feed ended, repeated by any later poll.
    finished: Option<FeedEvent>,
}

/// Decodes `source` from `at_ms` on a thread named "corundum-audio-decode", converting to
/// `target`. `hint` is the file's extension. While nothing is decoded yet, `poll` reports
/// what `watch` says (else buffering). Dropping the feed stops the thread and calls the
/// source's canceller. A thread that cannot start, or a panic in it, is `Unplayable`.
pub fn spawn_feed(
    source: Box<dyn Source>,
    watch: Option<Arc<dyn Watch>>,
    hint: Option<String>,
    at_ms: u64,
    target: OutputConfig,
) -> Box<dyn Feed> {
    let (tx, rx) = sync_channel(CHANNEL_CAPACITY);
    let stop = Arc::new(AtomicBool::new(false));
    let duration = Arc::new(AtomicU64::new(NO_DURATION));
    let canceller = source.canceller();
    let job = Job {
        source: Some(source),
        watch: watch.clone(),
        hint,
        at_ms,
        target,
        tx,
        stop: stop.clone(),
        duration: duration.clone(),
    };
    let guard = LiveGuard::new();
    let started = std::thread::Builder::new()
        .name("corundum-audio-decode".into())
        .spawn(move || {
            let _guard = guard;
            let tx = job.tx.clone();
            if catch_unwind(AssertUnwindSafe(|| job.run())).is_err() {
                let _ = tx.send(Message::Failed(TrackError::Unplayable(
                    "the decoder crashed".into(),
                )));
            }
        });
    if let Err(e) = started {
        return Box::new(FailedFeed(TrackError::Unplayable(format!(
            "cannot start the decoder: {e}"
        ))));
    }
    Box::new(TrackFeed {
        rx,
        watch,
        stop,
        canceller,
        duration,
        finished: None,
    })
}

impl Drop for TrackFeed {
    fn drop(&mut self) {
        // The thread leaves at once, even from a wait for bytes.
        self.stop.store(true, Ordering::Release);
        if let Some(cancel) = &self.canceller {
            cancel();
        }
    }
}

impl Feed for TrackFeed {
    fn poll(&mut self) -> FeedEvent {
        if let Some(done) = &self.finished {
            return done.clone();
        }
        let done = match self.rx.try_recv() {
            Ok(Message::Pcm(p)) => return FeedEvent::Pcm(p),
            Ok(Message::End) => FeedEvent::End,
            Ok(Message::Failed(e)) => FeedEvent::Failed(e),
            Err(TryRecvError::Empty) => {
                return FeedEvent::Waiting(match self.watch.as_ref().map(|w| w.status()) {
                    Some(SourceStatus::Waiting(w)) => w,
                    _ => Wait::Buffering,
                });
            }
            Err(TryRecvError::Disconnected) => {
                FeedEvent::Failed(TrackError::Unplayable("the decoder stopped".into()))
            }
        };
        self.finished = Some(done.clone());
        done
    }

    fn duration_ms(&self) -> Option<u64> {
        match self.duration.load(Ordering::Acquire) {
            NO_DURATION => None,
            ms => Some(ms),
        }
    }
}

/// Everything the decoder thread needs.
struct Job {
    source: Option<Box<dyn Source>>,
    watch: Option<Arc<dyn Watch>>,
    hint: Option<String>,
    at_ms: u64,
    target: OutputConfig,
    tx: SyncSender<Message>,
    stop: Arc<AtomicBool>,
    duration: Arc<AtomicU64>,
}

impl Job {
    fn run(mut self) {
        let message = match self.decode() {
            Ok(()) => Message::End,
            Err(Some(e)) => Message::Failed(e),
            // Cancelled: nobody is listening.
            Err(None) => return,
        };
        let _ = self.tx.send(message);
    }

    /// The watch's failure, if it has one.
    fn watch_failure(&self) -> Option<TrackError> {
        match self.watch.as_ref()?.status() {
            SourceStatus::Failed(e) => Some(e),
            _ => None,
        }
    }

    /// `Err(None)` when the feed was dropped.
    fn decode(&mut self) -> Result<(), Option<TrackError>> {
        let source = Cancellable {
            inner: self.source.take().expect("decoded once"),
            stop: self.stop.clone(),
        };
        let mut decoder =
            decode::open(Box::new(source), self.hint.as_deref()).map_err(|e| self.failure(e))?;
        if let Some(ms) = decoder.duration_ms() {
            self.duration.store(ms, Ordering::Release);
        }
        if self.at_ms > 0 {
            decoder.seek(self.at_ms).map_err(|e| self.failure(e))?;
        }
        let mut converter = Converter::new(decoder.sample_rate(), decoder.channels(), self.target)
            .map_err(|e| Some(TrackError::Unplayable(e)))?;
        loop {
            if self.stop.load(Ordering::Acquire) {
                return Err(None);
            }
            match decoder.next_block() {
                Ok(Some(block)) => {
                    let pcm = converter.process(&block);
                    self.send(pcm)?;
                }
                Ok(None) => break,
                Err(e) => return Err(self.failure(e)),
            }
        }
        // A fetch that failed under a decoder that took the cut-off for the end.
        if let Some(e) = self.watch_failure() {
            return Err((!self.stop.load(Ordering::Acquire)).then_some(e));
        }
        let rest = converter.flush();
        self.send(rest)
    }

    fn send(&self, pcm: Vec<f32>) -> Result<(), Option<TrackError>> {
        if pcm.is_empty() {
            return Ok(());
        }
        self.tx.send(Message::Pcm(pcm)).map_err(|_| None)
    }

    /// The watch's own failure if it has one, else the decoder's reason.
    fn failure(&self, e: DecodeError) -> Option<TrackError> {
        if self.stop.load(Ordering::Acquire) {
            return None;
        }
        Some(
            self.watch_failure()
                .unwrap_or_else(|| TrackError::Unplayable(e.to_string())),
        )
    }
}

/// A source that fails once its feed is dropped.
struct Cancellable {
    inner: Box<dyn Source>,
    stop: Arc<AtomicBool>,
}

impl Cancellable {
    fn check(&self) -> io::Result<()> {
        if self.stop.load(Ordering::Acquire) {
            Err(io::Error::other("the feed was dropped"))
        } else {
            Ok(())
        }
    }
}

impl Read for Cancellable {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.check()?;
        let n = self.inner.read(buf)?;
        self.check()?;
        Ok(n)
    }
}

impl Seek for Cancellable {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        self.check()?;
        self.inner.seek(to)
    }
}

impl Source for Cancellable {
    fn byte_len(&self) -> Option<u64> {
        self.inner.byte_len()
    }
}
