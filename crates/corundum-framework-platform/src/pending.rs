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

use std::fmt;
use std::sync::{Arc, Mutex};

/// The app's end: the answer, once the system gives it.
pub struct Pending<T> {
    shared: Arc<Shared<T>>,
}

/// The platform's end, kept until the system answers.
pub struct Sender<T: Default> {
    shared: Arc<Shared<T>>,
    sent: bool,
}

struct Shared<T> {
    answer: Mutex<Option<T>>,
    wake: Box<dyn Fn() + Send + Sync>,
}

/// A request's two ends. `wake` runs, on whichever thread sends, when the answer arrives.
pub fn channel<T: Default>(wake: impl Fn() + Send + Sync + 'static) -> (Pending<T>, Sender<T>) {
    let shared = Arc::new(Shared {
        answer: Mutex::new(None),
        wake: Box::new(wake),
    });
    let sender = Sender {
        shared: Arc::clone(&shared),
        sent: false,
    };
    (Pending { shared }, sender)
}

impl<T> Pending<T> {
    /// A request answered already: a desktop's dialog, which holds the frame until it closes.
    pub fn ready(answer: T) -> Pending<T> {
        Pending {
            shared: Arc::new(Shared {
                answer: Mutex::new(Some(answer)),
                wake: Box::new(|| {}),
            }),
        }
    }

    /// The answer, the first time it is asked for after the system gave it; `None` before,
    /// and again after, as the answer is taken out.
    pub fn poll(&mut self) -> Option<T> {
        self.shared
            .answer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
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
        *self
            .shared
            .answer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(answer);
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
    fn a_ready_request_answers_at_once() {
        assert_eq!(Pending::ready(3).poll(), Some(3));
    }
}
