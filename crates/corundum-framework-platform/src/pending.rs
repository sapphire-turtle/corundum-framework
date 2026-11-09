//! An answer the system gives later: a picker or a scanner the app opened answers in a
//! callback, on another thread or a later turn of the event loop, while the app goes on
//! drawing frames.
//!
//! [`channel`] makes the two ends. The app keeps the [`Pending`] and asks it each frame
//! with [`Pending::poll`]; the platform code keeps the [`Sender`] until the system answers.
//! Sending calls the `wake` given to [`channel`], so that a window that only draws on input
//! draws again for the answer. A sender dropped without sending answers `T::default()`
//! (nothing picked, nothing read): the app's request always ends, even when the platform
//! code loses it (another request took its place, the activity went away).
//!
//! A worker thread may instead block on the answer with [`Pending::wait`]; the UI thread
//! never does. [`Pending::map`] turns the answer into another as it is taken.

use std::fmt;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// The app's end: the answer, once the system gives it.
pub struct Pending<T> {
    source: Source<T>,
}

enum Source<T> {
    /// A channel's end, or an answer given already.
    Shared(Arc<Shared<T>>),
    /// Another request's answer, turned into this one's as it is taken.
    Mapped(Box<dyn Mapped<T> + Send>),
}

/// The platform's end, kept until the system answers.
pub struct Sender<T: Default> {
    shared: Arc<Shared<T>>,
    sent: bool,
}

struct Shared<T> {
    answer: Mutex<Option<T>>,
    /// Notified when `answer` is set, for [`Pending::wait`].
    answered: Condvar,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl<T> Shared<T> {
    fn lock(&self) -> MutexGuard<'_, Option<T>> {
        self.answer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A [`Pending`] whose answer goes through a function before it is given.
trait Mapped<T> {
    fn poll(&mut self) -> Option<T>;
    fn wait(self: Box<Self>) -> T;
}

struct Map<S, F> {
    from: Pending<S>,
    f: Option<F>,
}

impl<S, T, F: FnOnce(S) -> T> Mapped<T> for Map<S, F> {
    fn poll(&mut self) -> Option<T> {
        let answer = self.from.poll()?;
        self.f.take().map(|f| f(answer))
    }

    fn wait(self: Box<Self>) -> T {
        let Map { from, f } = *self;
        // Without `f`, the answer was taken already, and `from.wait()` never returns.
        let answer = from.wait();
        f.expect("the answer is taken once")(answer)
    }
}

/// A request's two ends. `wake` runs, on whichever thread sends, when the answer arrives.
pub fn channel<T: Default>(wake: impl Fn() + Send + Sync + 'static) -> (Pending<T>, Sender<T>) {
    let shared = Arc::new(Shared {
        answer: Mutex::new(None),
        answered: Condvar::new(),
        wake: Box::new(wake),
    });
    let sender = Sender {
        shared: Arc::clone(&shared),
        sent: false,
    };
    (
        Pending {
            source: Source::Shared(shared),
        },
        sender,
    )
}

impl<T> Pending<T> {
    /// A request answered already: a desktop's dialog, which holds the frame until it closes.
    pub fn ready(answer: T) -> Pending<T> {
        Pending {
            source: Source::Shared(Arc::new(Shared {
                answer: Mutex::new(Some(answer)),
                answered: Condvar::new(),
                wake: Box::new(|| {}),
            })),
        }
    }

    /// The answer, the first time it is asked for after the system gave it; `None` before,
    /// and again after, as the answer is taken out.
    pub fn poll(&mut self) -> Option<T> {
        match &mut self.source {
            Source::Shared(shared) => shared.lock().take(),
            Source::Mapped(mapped) => mapped.poll(),
        }
    }

    /// Blocks until the answer comes, and gives it. A sender dropped without sending gives
    /// `T::default()`, as [`poll`](Pending::poll) does.
    ///
    /// For a worker thread: never on the UI thread, whose frames would stop. Once
    /// [`poll`](Pending::poll) has taken the answer, there is none to wait for, and this
    /// never returns.
    pub fn wait(self) -> T {
        match self.source {
            Source::Shared(shared) => {
                let mut answer = shared.lock();
                loop {
                    if let Some(answer) = answer.take() {
                        return answer;
                    }
                    answer = shared
                        .answered
                        .wait(answer)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
            }
            Source::Mapped(mapped) => mapped.wait(),
        }
    }

    /// This request with its answer turned by `f`, on whichever thread takes it (the one
    /// that polls or waits). The window is woken as for this request.
    pub fn map<U>(self, f: impl FnOnce(T) -> U + Send + 'static) -> Pending<U>
    where
        T: Send + 'static,
    {
        Pending {
            source: Source::Mapped(Box::new(Map {
                from: self,
                f: Some(f),
            })),
        }
    }
}

impl<T> fmt::Debug for Pending<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pending").finish_non_exhaustive()
    }
}

impl<T: Default> Sender<T> {
    /// Give the app `answer`, and wake its window.
    pub fn send(mut self, answer: T) {
        self.deliver(answer);
    }

    fn deliver(&mut self, answer: T) {
        self.sent = true;
        *self.shared.lock() = Some(answer);
        self.shared.answered.notify_all();
        (self.shared.wake)();
    }
}

impl<T: Default> Drop for Sender<T> {
    fn drop(&mut self) {
        if !self.sent {
            self.deliver(T::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn the_answer_comes_once_and_wakes_the_window() {
        let woken = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&woken);
        let (mut pending, sender) = channel(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(pending.poll(), None);
        std::thread::spawn(move || sender.send(vec![1, 2]))
            .join()
            .unwrap();
        assert_eq!(woken.load(Ordering::SeqCst), 1);
        assert_eq!(pending.poll(), Some(vec![1, 2]));
        assert_eq!(pending.poll(), None);
    }

    #[test]
    fn a_lost_sender_answers_nothing() {
        let (mut pending, sender) = channel::<Option<String>>(|| {});
        drop(sender);
        assert_eq!(pending.poll(), Some(None));
    }

    #[test]
    fn wait_blocks_until_the_answer() {
        let (pending, sender) = channel(|| {});
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(20));
            sender.send(7);
        });
        assert_eq!(pending.wait(), 7);
        t.join().unwrap();
    }

    #[test]
    fn wait_on_a_dropped_sender_is_the_default() {
        let (pending, sender) = channel::<Option<String>>(|| {});
        std::thread::spawn(move || drop(sender));
        assert_eq!(pending.wait(), None);
    }

    #[test]
    fn a_mapped_request_answers_mapped() {
        let (pending, sender) = channel::<u32>(|| {});
        let mut doubled = pending.map(|n| n * 2);
        assert_eq!(doubled.poll(), None);
        sender.send(4);
        assert_eq!(doubled.poll(), Some(8));
        assert_eq!(doubled.poll(), None);
        let (pending, sender) = channel::<u32>(|| {});
        std::thread::spawn(move || sender.send(5));
        assert_eq!(pending.map(|n| n + 1).wait(), 6);
    }

    #[test]
    fn a_ready_request_answers_at_once() {
        assert_eq!(Pending::ready(3).poll(), Some(3));
    }
}
