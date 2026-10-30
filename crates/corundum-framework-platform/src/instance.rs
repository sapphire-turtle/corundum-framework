//! One running copy of a desktop app: a second start hands over to the first and exits, as
//! a browser or an editor does, and as a phone's app is anyway.
//!
//! [`acquire`] takes the app's lock, `instance.lock` in its data directory, held with the
//! OS's file lock ([`File::try_lock`](std::fs::File::try_lock)), which the OS lets go of
//! when the process ends, crash or not. The start that gets it is the app's [`Instance`], and
//! listens on a local socket; the start that does not is told [`AlreadyRunning`], whose
//! [`activate`](AlreadyRunning::activate) asks the first, over that socket, to come to the
//! front. The first hears it through [`Instance::on_activate`]. [`acquire_or_activate`] is
//! the two together, for a start that opens a window: it also waits a moment for a first
//! start that is closing, and opens in its place.
//!
//! The socket, the user's own on every OS:
//!
//! - **Windows**: a named pipe, named by the app and a hash of its data directory, as the
//!   machine's pipes are shared by its users.
//! - **macOS**: `instance.sock` in the app's data directory; where that path is too long for
//!   a socket (104 bytes), a file named by the same hash in `$TMPDIR`, which macOS gives
//!   each user. Not `$XDG_RUNTIME_DIR`: a start from Finder seldom has it.
//! - **Linux and the other Unixes**: a file named by the app and the hash in
//!   `$XDG_RUNTIME_DIR` (the user's, mode 0700) where it is set, else `instance.sock` in
//!   the app's data directory.
//!
//! A socket file a crashed start left is removed by the next instance, and only while it
//! holds the lock, so that no running instance loses its socket.
//!
//! Only a start that opens the app's window takes the lock: a server or a command line run
//! from the same binary runs beside the window. And a lock that cannot be taken for any other
//! reason than another start holding it (no data directory, a file system without locks) is
//! logged and the start goes on as the instance: the app is never kept from opening.
//!
//! The lock is also what keeps a second start from applying an update under the first: it
//! exits before it gets that far. On iOS and Android, whose apps the OS runs once,
//! [`acquire`] always succeeds and does nothing.

use std::fmt;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::path::Path;
#[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
use std::path::PathBuf;

/// The app's one running copy: holds its lock, and the socket a second start reaches it on,
/// until dropped. Keep it as long as the app's window is open.
pub struct Instance {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    inner: desktop::Held,
}

/// Another start of the app holds its lock. [`activate`](Self::activate) asks that one to
/// come to the front; this start then exits.
#[derive(Debug)]
pub struct AlreadyRunning {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    socket: Option<SocketName>,
}

/// Take `app_name`'s lock, in its data directory ([`AppDirs`](crate::dirs::AppDirs)), or
/// learn that another start holds it. Where the lock cannot be taken for another reason, the
/// reason is logged and this start is the instance.
pub fn acquire(app_name: &str) -> Result<Instance, AlreadyRunning> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = app_name;
        Ok(Instance {})
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let Some(dirs) = crate::dirs::AppDirs::new(app_name) else {
            tracing::warn!("{app_name}: no data directory to keep the instance lock in");
            return Ok(Instance {
                inner: desktop::Held::default(),
            });
        };
        let socket = socket_name(app_name, dirs.data());
        acquire_with(dirs.data(), socket)
    }
}

/// [`acquire`], for a start that opens the app's window: `None` when another start runs,
/// which has been asked to come to the front, and this start should exit.
///
/// A first start that is closing, or has no socket, cannot be asked: this start then tries
/// the lock again for about two seconds, and is the instance if it frees, so that a relaunch
/// while the window closes still opens one.
pub fn acquire_or_activate(app_name: &str) -> Option<Instance> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = app_name;
        Some(Instance {})
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let Some(dirs) = crate::dirs::AppDirs::new(app_name) else {
            return acquire(app_name).ok();
        };
        let socket = socket_name(app_name, dirs.data());
        acquire_or_activate_with(dirs.data(), socket, CLOSING_WAIT)
    }
}

/// [`acquire_or_activate`] with the lock in `data_dir`: for the tests of a crate built on
/// this one, which must not touch the app's real data directory.
#[doc(hidden)]
pub fn acquire_or_activate_in(app_name: &str, data_dir: &std::path::Path) -> Option<Instance> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        let _ = (app_name, data_dir);
        Some(Instance {})
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        let socket = socket_name(app_name, data_dir);
        acquire_or_activate_with(data_dir, socket, CLOSING_WAIT)
    }
}

/// How long [`acquire_or_activate`] waits for a first start that cannot be asked.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const CLOSING_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

impl Instance {
    /// Call `f` whenever a second start asks this one to come to the front, from a thread of
    /// its own. Asks made before this is called wait for it. Only the first call listens.
    pub fn on_activate(&mut self, f: impl Fn() + Send + 'static) {
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let _ = f;
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        self.inner.on_activate(f);
    }
}

impl fmt::Debug for Instance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Instance").finish_non_exhaustive()
    }
}

impl AlreadyRunning {
    /// Ask the running start to come to the front: whether it answered that it did, within
    /// a second. It does not where it is closing, has no socket, or has not started
    /// listening yet ([`acquire_or_activate`] then tries the lock again); one that has not
    /// started listening still hears the ask once it does.
    pub fn activate(&self) -> bool {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        if let Some(socket) = &self.socket {
            return desktop::send(socket, Message::Activate);
        }
        false
    }
}

impl fmt::Display for AlreadyRunning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("another start of the app is running")
    }
}

impl std::error::Error for AlreadyRunning {}

/// What a second start says to the first: one line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Message {
    /// Come to the front.
    Activate,
}

/// The first start's answer to [`Message::Activate`], once it has come to the front.
#[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
const REPLY: &[u8] = b"ok\n";

/// The most a message is read to: anything longer is not one.
#[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
const MAX_MESSAGE: u64 = 64;

impl Message {
    /// The bytes sent for this message.
    #[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
    fn encode(self) -> &'static [u8] {
        match self {
            Message::Activate => b"activate\n",
        }
    }

    /// The message `bytes` are, all that was sent on one connection, if any.
    #[cfg_attr(any(target_os = "ios", target_os = "android"), allow(dead_code))]
    fn decode(bytes: &[u8]) -> Option<Message> {
        if bytes.len() as u64 > MAX_MESSAGE {
            return None;
        }
        match bytes.trim_ascii() {
            b"activate" => Some(Message::Activate),
            _ => None,
        }
    }
}

/// Where the instance listens: a named pipe on Windows, a socket file on the Unixes.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Clone, PartialEq, Eq, Debug)]
enum SocketName {
    #[cfg(windows)]
    Pipe(String),
    #[cfg(unix)]
    Path(PathBuf),
}

/// The longest socket file path, its terminating nul left out: `sun_path` is 108 bytes on
/// Linux, 104 on macOS and the BSDs.
#[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
const MAX_SOCKET_PATH: usize = if cfg!(target_os = "linux") { 107 } else { 103 };

/// The socket of `app_name`, whose data directory is `data_dir` (the module's docs have
/// where), or `None` where no socket file's path is short enough.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn socket_name(app_name: &str, data_dir: &Path) -> Option<SocketName> {
    let hash = format!("{:016x}", fnv1a(data_dir.as_os_str().as_encoded_bytes()));
    #[cfg(windows)]
    {
        Some(SocketName::Pipe(format!("{app_name}.{hash}.instance")))
    }
    #[cfg(unix)]
    {
        let beside_lock = Some(data_dir.join("instance.sock"));
        let env_dir = |var| {
            std::env::var_os(var)
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
        };
        #[cfg(target_os = "macos")]
        let candidates = [
            beside_lock,
            env_dir("TMPDIR").map(|dir| dir.join(format!("{hash}.sock"))),
        ];
        #[cfg(not(target_os = "macos"))]
        let candidates = [
            env_dir("XDG_RUNTIME_DIR").map(|dir| dir.join(format!("{app_name}.{hash}.sock"))),
            beside_lock,
        ];
        #[cfg(target_os = "macos")]
        let _ = app_name;
        let found = candidates
            .into_iter()
            .flatten()
            .find(|path| path.as_os_str().len() <= MAX_SOCKET_PATH);
        if found.is_none() {
            tracing::warn!(
                "{}: no socket path short enough; a second start cannot bring this one to \
                 the front",
                data_dir.display()
            );
        }
        found.map(SocketName::Path)
    }
}

/// FNV-1a: a hash that stays the same across Rust's versions, as the app's next version
/// must reach the socket this one made.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// [`acquire_or_activate`], with [`acquire_with`]'s directory and socket, waiting at most
/// `wait` for a first start that cannot be asked.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn acquire_or_activate_with(
    data_dir: &Path,
    socket: Option<SocketName>,
    wait: std::time::Duration,
) -> Option<Instance> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        match acquire_with(data_dir, socket.clone()) {
            Ok(instance) => return Some(instance),
            Err(running) if running.activate() => return None,
            Err(_) if std::time::Instant::now() >= deadline => {
                tracing::warn!(
                    "another start holds {} and cannot be asked to come to the front",
                    data_dir.join(LOCK_FILE).display()
                );
                return None;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    }
}

/// [`acquire`], with the lock in `data_dir` and listening on `socket`: the tests' own
/// directory and name.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn acquire_with(data_dir: &Path, socket: Option<SocketName>) -> Result<Instance, AlreadyRunning> {
    let lock = match desktop::lock(data_dir) {
        Ok(Some(lock)) => Some(lock),
        Ok(None) => return Err(AlreadyRunning { socket }),
        Err(err) => {
            tracing::warn!(
                "could not lock {}; starting as the app's instance: {err}",
                data_dir.join(LOCK_FILE).display()
            );
            None
        }
    };
    // A socket file is taken over only under the lock: without it, another start may be
    // listening there.
    let listener = socket
        .as_ref()
        .and_then(|socket| desktop::listen(socket, lock.is_some()));
    Ok(Instance {
        inner: desktop::Held {
            listener,
            accepting: None,
            socket,
            lock,
        },
    })
}

/// The lock's file, in the app's data directory.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const LOCK_FILE: &str = "instance.lock";

#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod desktop {
    use std::fs::{File, OpenOptions, TryLockError};
    use std::io::{self, Read as _, Write as _};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread::JoinHandle;
    use std::time::Duration;

    #[cfg(unix)]
    use interprocess::local_socket::GenericFilePath;
    #[cfg(windows)]
    use interprocess::local_socket::GenericNamespaced;
    use interprocess::local_socket::prelude::*;
    use interprocess::local_socket::{Listener, ListenerOptions, Name, Stream};

    use super::{LOCK_FILE, MAX_MESSAGE, Message, REPLY, SocketName};

    /// What an instance holds: the lock, unless it could not be taken, and its socket, whose
    /// listener moves to the accepting thread once the app listens.
    ///
    /// Fields drop in this order: the listener lets go of the socket before the lock is let
    /// go of, so the next instance finds the socket free.
    #[derive(Default)]
    pub(super) struct Held {
        pub(super) listener: Option<Listener>,
        pub(super) accepting: Option<Accepting>,
        pub(super) socket: Option<SocketName>,
        /// Never read: holding the open file is holding the lock.
        #[allow(dead_code)]
        pub(super) lock: Option<File>,
    }

    /// How long a connection may take to send its message, and a message to be sent.
    const IO_TIMEOUT: Duration = Duration::from_secs(1);

    /// The thread that accepts the second starts' connections, and how to stop it.
    pub(super) struct Accepting {
        stop: Arc<AtomicBool>,
        thread: JoinHandle<()>,
    }

    /// The lock in `data_dir`, made if need be: `None` when another start holds it.
    pub(super) fn lock(data_dir: &Path) -> io::Result<Option<File>> {
        std::fs::create_dir_all(data_dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(data_dir.join(LOCK_FILE))?;
        match file.try_lock() {
            Ok(()) => Ok(Some(file)),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(err)) => Err(err),
        }
    }

    /// `socket` as interprocess names it.
    fn name(socket: &SocketName) -> io::Result<Name<'_>> {
        match socket {
            #[cfg(windows)]
            SocketName::Pipe(name) => name.as_str().to_ns_name::<GenericNamespaced>(),
            #[cfg(unix)]
            SocketName::Path(path) => path.as_path().to_fs_name::<GenericFilePath>(),
        }
    }

    /// A listener on `socket`, or `None`, logged, where there cannot be one. `locked`: this
    /// start holds the lock, so a socket file already there was left by a start that
    /// crashed, and is replaced.
    pub(super) fn listen(socket: &SocketName, locked: bool) -> Option<Listener> {
        let listener = name(socket).and_then(|name| {
            ListenerOptions::new()
                .name(name)
                .try_overwrite(locked)
                .max_spin_time(Duration::from_secs(1))
                .create_sync()
        });
        match listener {
            Ok(listener) => Some(listener),
            Err(err) => {
                tracing::warn!(
                    "could not listen on {socket:?}; a second start cannot bring this one \
                     to the front: {err}"
                );
                None
            }
        }
    }

    impl Held {
        pub(super) fn on_activate(&mut self, f: impl Fn() + Send + 'static) {
            let Some(listener) = self.listener.take() else {
                return;
            };
            let stop = Arc::new(AtomicBool::new(false));
            let f = Arc::new(Mutex::new(f));
            let thread = std::thread::Builder::new()
                .name("instance".to_owned())
                .spawn({
                    let stop = stop.clone();
                    move || accept(&listener, stop, &f)
                });
            match thread {
                Ok(thread) => self.accepting = Some(Accepting { stop, thread }),
                Err(err) => tracing::warn!("could not start the instance's listener: {err}"),
            }
        }
    }

    #[cfg(test)]
    impl Held {
        /// Tell the accepting thread to stop without waking it, as a drop has begun to.
        pub(super) fn stop_accepting(&self) {
            if let Some(accepting) = &self.accepting {
                accepting.stop.store(true, Ordering::SeqCst);
            }
        }
    }

    impl Drop for Held {
        /// Stop the accepting thread, so its listener lets go of the socket before the
        /// lock is: the next instance listens there.
        fn drop(&mut self) {
            let Some(Accepting { stop, thread }) = self.accepting.take() else {
                return;
            };
            stop.store(true, Ordering::SeqCst);
            // A connection wakes the thread from `accept`. Without one it may still be
            // waiting there, and is left to the process's end rather than waited for.
            let woken = self
                .socket
                .as_ref()
                .is_some_and(|socket| name(socket).and_then(Stream::connect).is_ok());
            if woken || thread.is_finished() {
                let _ = thread.join();
            }
        }
    }

    /// Accept connections until told to stop, answering each on a thread of its own: one
    /// that sends nothing does not hold up the next. A failed accept (out of file
    /// descriptors, say) is waited out, not taken as the end.
    fn accept<F: Fn() + Send + 'static>(
        listener: &Listener,
        stop: Arc<AtomicBool>,
        f: &Arc<Mutex<F>>,
    ) {
        let mut failing = false;
        loop {
            let connection = listener.accept();
            if stop.load(Ordering::SeqCst) {
                return;
            }
            match connection {
                Ok(stream) => {
                    failing = false;
                    let (f, stop) = (f.clone(), stop.clone());
                    let _ = std::thread::Builder::new()
                        .name("instance message".to_owned())
                        .spawn(move || answer(stream, &stop, &f));
                }
                Err(err) => {
                    // Once per run of failures: they may come many times a second.
                    if !failing {
                        tracing::warn!("the instance's listener could not accept: {err}");
                    }
                    failing = true;
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    }

    /// Read the message on `stream`, and for an activation call `f` and reply that it was.
    /// Once the instance is told to stop, it neither calls nor replies: the asking start
    /// then takes the lock once it frees.
    fn answer<F: Fn()>(mut stream: Stream, stop: &AtomicBool, f: &Mutex<F>) {
        if let Err(err) = stream.set_recv_timeout(Some(IO_TIMEOUT)) {
            tracing::debug!("no read timeout on the instance's connection: {err}");
        }
        let _ = stream.set_send_timeout(Some(IO_TIMEOUT));
        if Message::decode(&read_line(&mut stream)) != Some(Message::Activate) {
            return;
        }
        if stop.load(Ordering::SeqCst) {
            return;
        }
        f.lock().unwrap_or_else(|e| e.into_inner())();
        if stop.load(Ordering::SeqCst) {
            return;
        }
        let _ = stream.write_all(REPLY).and_then(|()| stream.flush());
    }

    /// One line from `stream`, its newline included: read a byte at a time, so nothing past
    /// it is taken, until the newline, the end, an error or a timeout, or [`MAX_MESSAGE`]
    /// bytes and one more, which is no message.
    fn read_line(stream: &mut Stream) -> Vec<u8> {
        let mut line = Vec::new();
        let mut byte = [0];
        while (line.len() as u64) <= MAX_MESSAGE {
            match stream.read(&mut byte) {
                Ok(1) => {
                    line.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
                // The end, or a named pipe whose writer has gone, or a timeout.
                _ => break,
            }
        }
        line
    }

    /// Send `message` on `socket` and wait for the reply: whether the running start
    /// answered. A failure is only logged: the first start may be closing.
    pub(super) fn send(socket: &SocketName, message: Message) -> bool {
        let sent = name(socket)
            .and_then(Stream::connect)
            .and_then(|mut stream| {
                let _ = stream.set_send_timeout(Some(IO_TIMEOUT));
                let _ = stream.set_recv_timeout(Some(IO_TIMEOUT));
                stream.write_all(message.encode())?;
                stream.flush()?;
                Ok(stream)
            });
        let mut stream = match sent {
            Ok(stream) => stream,
            Err(err) => {
                tracing::debug!("could not reach the running instance on {socket:?}: {err}");
                return false;
            }
        };
        // On a thread, waited for no longer than the timeout: a named pipe may not take
        // the read timeout, and a first start that is closing may never answer.
        let (tx, rx) = mpsc::channel();
        let reading = std::thread::Builder::new()
            .name("instance reply".to_owned())
            .spawn(move || {
                let _ = tx.send(read_line(&mut stream));
            });
        if reading.is_err() {
            return false;
        }
        let answered = rx
            .recv_timeout(IO_TIMEOUT)
            .is_ok_and(|reply| reply.trim_ascii() == REPLY.trim_ascii());
        if !answered {
            tracing::debug!("the running instance on {socket:?} did not answer");
        }
        answered
    }
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    fn acquire_in(dir: &Path) -> Result<Instance, AlreadyRunning> {
        acquire_with(dir, socket_name("corundum-test", dir))
    }

    #[test]
    fn messages_are_one_line_read_back_as_sent() {
        assert_eq!(
            Message::decode(Message::Activate.encode()),
            Some(Message::Activate)
        );
        assert_eq!(Message::decode(b"activate"), Some(Message::Activate));
        assert_eq!(Message::decode(b""), None);
        assert_eq!(Message::decode(b"activated\n"), None);
        assert!(Message::Activate.encode().len() as u64 <= MAX_MESSAGE);
        let long = [b' '; MAX_MESSAGE as usize + 1];
        assert_eq!(Message::decode(&long), None);
    }

    #[test]
    fn each_data_directory_has_a_socket_of_its_own() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let name_a = socket_name("corundum-test", a.path());
        assert!(name_a.is_some());
        assert_eq!(name_a, socket_name("corundum-test", a.path()));
        assert_ne!(name_a, socket_name("corundum-test", b.path()));
    }

    #[test]
    fn a_second_start_finds_the_first_running() {
        let dir = tempfile::tempdir().unwrap();
        let _first = acquire_in(dir.path()).expect("the first start is the instance");
        assert!(dir.path().join("instance.lock").is_file());
        acquire_in(dir.path()).expect_err("the second start finds the first");
    }

    #[test]
    fn the_second_start_brings_the_first_to_the_front() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = acquire_in(dir.path()).unwrap();
        let (tx, rx) = mpsc::channel();
        first.on_activate(move || tx.send(()).unwrap());
        assert!(acquire_in(dir.path()).unwrap_err().activate());
        rx.recv_timeout(Duration::from_secs(10))
            .expect("the first start was asked to come to the front");
    }

    #[test]
    fn an_ask_made_before_the_first_listens_is_heard_once_it_does() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = acquire_in(dir.path()).unwrap();
        // Not answered in time: the first is not listening yet.
        assert!(!acquire_in(dir.path()).unwrap_err().activate());
        let (tx, rx) = mpsc::channel();
        first.on_activate(move || tx.send(()).unwrap());
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    #[test]
    fn the_lock_is_let_go_of_when_the_instance_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = acquire_in(dir.path()).unwrap();
        first.on_activate(|| {});
        drop(first);
        // A process another test starts at that moment holds a copy of the lock's file until
        // it execs (the file is close-on-exec, but a fork copies it first), and so the lock
        // a little longer: a later start is the instance once that copy is gone.
        let mut third = (0..100)
            .find_map(|_| {
                acquire_in(dir.path())
                    .inspect_err(|_| std::thread::sleep(Duration::from_millis(50)))
                    .ok()
            })
            .expect("a later start is the instance again");
        // And listens where the first did.
        let (tx, rx) = mpsc::channel();
        third.on_activate(move || tx.send(()).unwrap());
        assert!(acquire_in(dir.path()).unwrap_err().activate());
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    // Socket files are the macOS and BSD form; Windows has named pipes alone.
    #[cfg(unix)]
    #[test]
    fn a_socket_file_left_by_a_crash_is_taken_over() {
        let dir = tempfile::tempdir().unwrap();
        let socket = || Some(SocketName::Path(dir.path().join("instance.sock")));
        std::fs::write(dir.path().join("instance.sock"), b"").unwrap();
        let mut first = acquire_with(dir.path(), socket()).unwrap();
        let (tx, rx) = mpsc::channel();
        first.on_activate(move || tx.send(()).unwrap());
        assert!(acquire_with(dir.path(), socket()).unwrap_err().activate());
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    #[test]
    fn a_lock_that_cannot_be_made_does_not_keep_the_app_from_starting() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the data directory should be: neither it nor the lock can be made.
        let data = dir.path().join("data");
        std::fs::write(&data, b"").unwrap();
        let _first = acquire_in(&data).expect("the start goes on without a lock");
    }

    #[test]
    fn asking_when_no_one_listens_is_not_delivered() {
        let dir = tempfile::tempdir().unwrap();
        let running = AlreadyRunning {
            socket: socket_name("corundum-test", dir.path()),
        };
        assert!(!running.activate());
        assert!(!AlreadyRunning { socket: None }.activate());
    }

    #[cfg(unix)]
    #[test]
    fn a_start_without_the_lock_leaves_another_s_socket_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let socket = || Some(SocketName::Path(dir.path().join("instance.sock")));
        let mut first = acquire_with(&dir.path().join("first"), socket()).unwrap();
        let (tx, rx) = mpsc::channel();
        first.on_activate(move || tx.send(()).unwrap());
        // A start whose lock cannot be made (a file where its directory should be) does not
        // take the socket over.
        let blocked = dir.path().join("blocked");
        std::fs::write(&blocked, b"").unwrap();
        let _other = acquire_with(&blocked, socket()).unwrap();
        assert!(AlreadyRunning { socket: socket() }.activate());
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    #[test]
    fn a_running_first_start_is_asked_and_the_second_exits() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = acquire_in(dir.path()).unwrap();
        let (tx, rx) = mpsc::channel();
        first.on_activate(move || tx.send(()).unwrap());
        let socket = socket_name("corundum-test", dir.path());
        assert!(acquire_or_activate_with(dir.path(), socket, Duration::from_secs(2)).is_none());
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
    }

    #[test]
    fn a_relaunch_while_the_first_closes_opens_in_its_place() {
        let dir = tempfile::tempdir().unwrap();
        // Closing: its listener is gone, its lock not yet.
        let first = acquire_with(dir.path(), None).unwrap();
        let closing = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(first);
        });
        let socket = socket_name("corundum-test", dir.path());
        let second = acquire_or_activate_with(dir.path(), socket, Duration::from_secs(5));
        assert!(
            second.is_some(),
            "the relaunch is the instance once the first is gone"
        );
        closing.join().unwrap();
    }

    #[test]
    fn a_first_start_that_has_stopped_listening_does_not_answer_and_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = acquire_in(dir.path()).unwrap();
        let (tx, rx) = mpsc::channel();
        first.on_activate(move || tx.send(()).unwrap());
        // Closing: told to stop, its listener and lock not yet let go of.
        first.inner.stop_accepting();
        assert!(!acquire_in(dir.path()).unwrap_err().activate());
        let closing = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(first);
        });
        let socket = socket_name("corundum-test", dir.path());
        let second = acquire_or_activate_with(dir.path(), socket, Duration::from_secs(5));
        assert!(
            second.is_some(),
            "the relaunch is the instance once the first is gone"
        );
        closing.join().unwrap();
        assert!(
            rx.try_recv().is_err(),
            "the closing first was not brought to the front"
        );
    }

    #[test]
    fn a_first_start_that_cannot_be_asked_is_given_up_on() {
        let dir = tempfile::tempdir().unwrap();
        let _first = acquire_with(dir.path(), None).unwrap();
        let socket = socket_name("corundum-test", dir.path());
        let started = std::time::Instant::now();
        let wait = Duration::from_millis(300);
        assert!(acquire_or_activate_with(dir.path(), socket, wait).is_none());
        assert!(started.elapsed() >= wait);
    }
}
