//! One running copy of a desktop app: a second start hands over to the first and exits, as
//! a browser or an editor does, and as a phone's app is anyway.
//!
//! [`acquire`] takes the app's lock, `instance.lock` in its data directory, held with the
//! OS's file lock ([`File::try_lock`]), which the OS lets go of when the process ends, crash
//! or not. The start that gets it is the app's [`Instance`], and listens on a local socket
//! (an abstract Unix socket on Linux, a named pipe on Windows, a socket file elsewhere); the
//! start that does not is told [`AlreadyRunning`], whose [`activate`](AlreadyRunning::activate)
//! asks the first, over that socket, to come to the front. The first hears it through
//! [`Instance::on_activate`].
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
use std::path::{Path, PathBuf};

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
    /// Ask the running start to come to the front. Nothing is reported: the first may be
    /// closing, and this start exits either way.
    pub fn activate(&self) {
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        if let Some(socket) = &self.socket {
            desktop::send(socket, Message::Activate);
        }
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
        match bytes.trim_ascii() {
            b"activate" => Some(Message::Activate),
            _ => None,
        }
    }
}

/// Where the instance listens: a name in the OS's namespace for sockets where it has one
/// (Linux's abstract sockets, Windows' named pipes), a socket file elsewhere.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Clone, PartialEq, Eq, Debug)]
enum SocketName {
    Namespaced(String),
    Path(PathBuf),
}

/// The socket of `app_name`, whose data directory is `data_dir`, or `None` where no socket
/// file's path is short enough.
///
/// The name carries a hash of `data_dir`: abstract sockets and named pipes are shared by
/// every user of the machine, and each user's app is its own instance. On the other Unixes
/// the socket is in `$XDG_RUNTIME_DIR`, else beside the lock.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn socket_name(app_name: &str, data_dir: &Path) -> Option<SocketName> {
    let name = format!(
        "{app_name}.{:016x}",
        fnv1a(data_dir.as_os_str().as_encoded_bytes())
    );
    if cfg!(any(windows, target_os = "linux")) {
        return Some(SocketName::Namespaced(format!("{name}.instance")));
    }
    let path = match std::env::var_os("XDG_RUNTIME_DIR").filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir).join(format!("{name}.sock")),
        None => data_dir.join("instance.sock"),
    };
    // `sun_path` is 104 bytes on macOS and the BSDs, the terminating nul included.
    if path.as_os_str().len() < 104 {
        Some(SocketName::Path(path))
    } else {
        tracing::warn!(
            "{}: too long for a socket; a second start cannot bring this one to the front",
            path.display()
        );
        None
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
    let listener = socket.as_ref().and_then(desktop::listen);
    Ok(Instance {
        inner: desktop::Held {
            lock,
            socket,
            listener,
            accepting: None,
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
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use interprocess::local_socket::prelude::*;
    use interprocess::local_socket::{
        GenericFilePath, GenericNamespaced, Listener, ListenerOptions, Name, Stream,
    };

    use super::{LOCK_FILE, MAX_MESSAGE, Message, SocketName};

    /// What an instance holds: the lock, unless it could not be taken, and its socket, whose
    /// listener moves to the accepting thread once the app listens.
    #[derive(Default)]
    pub(super) struct Held {
        /// Never read: holding the open file is holding the lock.
        #[allow(dead_code)]
        pub(super) lock: Option<File>,
        pub(super) socket: Option<SocketName>,
        pub(super) listener: Option<Listener>,
        pub(super) accepting: Option<Accepting>,
    }

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
            SocketName::Namespaced(name) => name.as_str().to_ns_name::<GenericNamespaced>(),
            SocketName::Path(path) => path.as_path().to_fs_name::<GenericFilePath>(),
        }
    }

    /// A listener on `socket`, or `None`, logged, where there cannot be one.
    pub(super) fn listen(socket: &SocketName) -> Option<Listener> {
        let listener = name(socket).and_then(|name| {
            ListenerOptions::new()
                .name(name)
                // A socket file left by a start that crashed: the lock is this start's, so
                // no other listens there.
                .try_overwrite(true)
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
                    move || accept(&listener, &stop, &f)
                });
            match thread {
                Ok(thread) => self.accepting = Some(Accepting { stop, thread }),
                Err(err) => tracing::warn!("could not start the instance's listener: {err}"),
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

    /// Accept connections until told to stop, reading each on a thread of its own: one that
    /// sends nothing does not hold up the next.
    fn accept<F: Fn() + Send + 'static>(listener: &Listener, stop: &AtomicBool, f: &Arc<Mutex<F>>) {
        loop {
            let connection = listener.accept();
            if stop.load(Ordering::SeqCst) {
                return;
            }
            match connection {
                Ok(stream) => {
                    let f = f.clone();
                    let _ = std::thread::Builder::new()
                        .name("instance message".to_owned())
                        .spawn(move || {
                            if read(stream) == Some(Message::Activate) {
                                let f = f.lock().unwrap_or_else(|e| e.into_inner());
                                f();
                            }
                        });
                }
                Err(err) => {
                    tracing::warn!("the instance's listener stopped: {err}");
                    return;
                }
            }
        }
    }

    /// The message sent on `stream`: what was read until the sender closed it, at most
    /// [`MAX_MESSAGE`] bytes.
    fn read(stream: Stream) -> Option<Message> {
        let mut bytes = Vec::new();
        // A named pipe whose writer has gone reads as an error, not as the end: what came
        // before it is the message.
        let _ = stream.take(MAX_MESSAGE).read_to_end(&mut bytes);
        Message::decode(&bytes)
    }

    /// Send `message` on `socket`. A failure is only logged: the first start may be closing.
    pub(super) fn send(socket: &SocketName, message: Message) {
        let sent = name(socket)
            .and_then(Stream::connect)
            .and_then(|mut stream| {
                stream.write_all(message.encode())?;
                stream.flush()
            });
        if let Err(err) = sent {
            tracing::debug!("could not reach the running instance on {socket:?}: {err}");
        }
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
        acquire_in(dir.path()).unwrap_err().activate();
        rx.recv_timeout(Duration::from_secs(10))
            .expect("the first start was asked to come to the front");
    }

    #[test]
    fn an_ask_made_before_the_first_listens_is_heard_once_it_does() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = acquire_in(dir.path()).unwrap();
        acquire_in(dir.path()).unwrap_err().activate();
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
        acquire_in(dir.path()).unwrap_err().activate();
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
        acquire_with(dir.path(), socket()).unwrap_err().activate();
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
    fn asking_when_no_one_listens_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        AlreadyRunning {
            socket: socket_name("corundum-test", dir.path()),
        }
        .activate();
    }
}
