//! The thread that owns the [`Engine`]: commands in, state out.
//!
//! The loop waits for a command for as long as `pump` asked (~10 ms while playing, so the
//! 200 ms ring in `output` holds twenty rounds), pumps after every command and every
//! timeout, and publishes the state when it changed. The repaint callback is called on every
//! change but the position, which only asks every 250 ms.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::{Command, Engine, Opener, OutputFactory, PlayerState, QueueItem};

/// How often a moving position repaints the window at most.
const POSITION_REPAINT: Duration = Duration::from_millis(250);

enum Message<T> {
    Command(Command<T>),
    Quit,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The playback engine on a thread of its own.
pub struct Player<T: QueueItem> {
    tx: mpsc::Sender<Message<T>>,
    state: Arc<Mutex<PlayerState<T>>>,
    thread: Option<JoinHandle<()>>,
}

impl<T: QueueItem> Player<T> {
    /// Runs `Engine::new(opener, outputs)` on a thread named "corundum-audio-player".
    /// `repaint` is called when the state changes (the position at most every 250 ms).
    pub fn spawn(
        opener: Box<dyn Opener<T>>,
        outputs: Box<dyn OutputFactory>,
        repaint: impl Fn() + Send + 'static,
    ) -> std::io::Result<Player<T>> {
        let (tx, rx) = mpsc::channel();
        let engine = Engine::new(opener, outputs);
        let state = Arc::new(Mutex::new(engine.state().clone()));
        let shared = state.clone();
        let thread = std::thread::Builder::new()
            .name("corundum-audio-player".into())
            .spawn(move || run(engine, rx, shared, repaint))?;
        Ok(Player {
            tx,
            state,
            thread: Some(thread),
        })
    }

    pub fn send(&self, command: Command<T>) {
        let _ = self.tx.send(Message::Command(command));
    }

    /// A copy of the state as of the last publish.
    pub fn state(&self) -> PlayerState<T> {
        lock(&self.state).clone()
    }
}

impl<T: QueueItem> Drop for Player<T> {
    fn drop(&mut self) {
        let _ = self.tx.send(Message::Command(Command::Stop));
        let _ = self.tx.send(Message::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn run<T: QueueItem>(
    mut engine: Engine<T>,
    rx: mpsc::Receiver<Message<T>>,
    shared: Arc<Mutex<PlayerState<T>>>,
    repaint: impl Fn(),
) {
    let mut last_repaint = Instant::now();
    let mut wait: Option<Duration> = None;
    loop {
        let message = match wait {
            Some(d) => match rx.recv_timeout(d) {
                Ok(m) => Some(m),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match rx.recv() {
                Ok(m) => Some(m),
                Err(_) => return,
            },
        };
        let handled = matches!(message, Some(Message::Command(_)));
        match message {
            Some(Message::Quit) => return,
            Some(Message::Command(c)) => engine.handle(c),
            None => {}
        }
        wait = engine.pump();
        publish(&engine, &shared, &repaint, &mut last_repaint, handled);
    }
}

/// Copies the engine's state to `shared` when it changed, and repaints. The queue only
/// changes when a command is handled, so after a plain pump (`handled` false) it is neither
/// compared nor cloned: the other fields are copied one by one.
fn publish<T: QueueItem>(
    engine: &Engine<T>,
    shared: &Mutex<PlayerState<T>>,
    repaint: &impl Fn(),
    last_repaint: &mut Instant,
    handled: bool,
) {
    let now = engine.state();
    let mut s = lock(shared);
    let only_position;
    if handled {
        if *s == *now {
            return;
        }
        only_position = s.queue == now.queue
            && s.current == now.current
            && s.status == now.status
            && s.duration_ms == now.duration_ms
            && s.volume == now.volume
            && s.error == now.error;
        *s = now.clone();
    } else {
        only_position = s.current == now.current
            && s.status == now.status
            && s.duration_ms == now.duration_ms
            && s.volume == now.volume
            && s.error == now.error;
        if only_position && s.position_ms == now.position_ms {
            return;
        }
        s.current = now.current;
        s.status = now.status;
        s.position_ms = now.position_ms;
        s.duration_ms = now.duration_ms;
        s.volume = now.volume;
        s.error.clone_from(&now.error);
    }
    drop(s);
    if !only_position || last_repaint.elapsed() >= POSITION_REPAINT {
        *last_repaint = Instant::now();
        repaint();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::Status;
    use crate::fakes::{FakeItem, FakeOpener, FakeOutputs, item};

    fn player() -> (Player<FakeItem>, FakeOutputs, Arc<AtomicUsize>) {
        let outs = FakeOutputs::new();
        let repaints = Arc::new(AtomicUsize::new(0));
        let counter = repaints.clone();
        let p = Player::spawn(
            Box::new(FakeOpener::new()),
            Box::new(outs.clone()),
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();
        (p, outs, repaints)
    }

    fn wait_for(
        p: &Player<FakeItem>,
        ok: impl Fn(&PlayerState<FakeItem>) -> bool,
    ) -> PlayerState<FakeItem> {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            let s = p.state();
            if ok(&s) || Instant::now() > end {
                return s;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn commands_reach_the_engine_and_the_state_comes_back() {
        let (p, outs, repaints) = player();
        p.send(Command::Replace {
            items: vec![item("a"), item("b")],
            start: 1,
        });
        let s = wait_for(&p, |s| s.status == Status::Playing);
        assert_eq!(s.status, Status::Playing);
        assert_eq!(s.queue.len(), 2);
        p.send(Command::Append(vec![item("c")]));
        let s = wait_for(&p, |s| s.queue.len() == 3);
        assert_eq!(s.queue.len(), 3);
        assert_eq!(s.current, Some(1));
        assert!(outs.current().is_some());
        p.send(Command::SetVolume(0.5));
        let s = wait_for(&p, |s| s.volume == 0.5);
        assert_eq!(s.volume, 0.5);
        p.send(Command::Pause);
        let s = wait_for(&p, |s| s.status == Status::Paused);
        assert_eq!(s.status, Status::Paused);
        assert!(
            repaints.load(Ordering::SeqCst) > 0,
            "the window was never asked to repaint"
        );
    }

    #[test]
    fn dropping_the_player_joins_its_thread() {
        let (p, _outs, _repaints) = player();
        p.send(Command::Replace {
            items: vec![item("a")],
            start: 0,
        });
        let start = Instant::now();
        drop(p);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
