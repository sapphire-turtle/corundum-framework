//! Updates: found and downloaded by a thread of the updater's own, applied only when the
//! app asks, after its `before_apply`.

use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use corundum_framework_platform::package;
use corundum_framework_update::{PackageState, PackageStatus};
use semver::Version;
use velopack::UpdateInfo;

use crate::backend::{Applied, Backend, Velopack};
use crate::error::Error;
use crate::start::{self, PackageSpec};

/// Where a package's updates are published.
#[derive(Clone, Debug)]
pub enum Source {
    /// A GitHub repository's releases, by its URL: `https://github.com/owner/name`.
    /// Velopack reads only the 10 newest releases.
    Github { repo: String, prerelease: bool },
    /// A feed of the app's own, by its base URL.
    Http(String),
}

/// What happens once an update is applied, and who ends the process for it.
#[derive(Clone, Debug)]
pub enum After {
    /// Start the app again without arguments: the GUI. The caller ends the process.
    RestartApp,
    /// Start the app again with these arguments. The caller ends the process.
    RestartWith(Vec<OsString>),
    /// Exit; something else starts it again (a service manager), or nothing. The process
    /// exits in [`Updater::apply`]: a daemon or a command line has nothing to close.
    Exit,
}

/// A package's updates: one thread checks and downloads, the state is shared, and nothing
/// is applied until [`Updater::apply`]. Clones share the thread and the state; the thread
/// ends with the last clone.
#[derive(Clone)]
pub struct Updater {
    shared: Arc<Shared>,
}

type Listener = Arc<dyn Fn() + Send + Sync>;

struct Shared {
    spec: &'static PackageSpec,
    backend: Arc<dyn Backend>,
    current: Version,
    state: Mutex<State>,
    listeners: Mutex<Vec<Listener>>,
    /// The worker's requests, `None` until the first check. Dropping it with the last
    /// clone ends the worker.
    worker: Mutex<Option<Sender<Request>>>,
    /// Whether an apply runs: a status-bar badge and the update panel may both ask at
    /// once. It stays set once an update is handed to Velopack, as the process is ending.
    applying: AtomicBool,
}

/// The backend of [`Updater::for_tests`]: nothing to find, nothing to apply.
#[cfg(feature = "test-support")]
struct Inert;

#[cfg(feature = "test-support")]
impl Backend for Inert {
    fn current(&self) -> Version {
        Version::new(1, 0, 0)
    }

    fn check(&self) -> Result<Option<UpdateInfo>, Error> {
        Ok(None)
    }

    fn download(&self, _info: &UpdateInfo, _progress: &dyn Fn(u8)) -> Result<(), Error> {
        Ok(())
    }

    fn pending(&self) -> Option<Version> {
        None
    }

    fn apply(&self, _applied: Applied, _after: &After) -> Result<(), Error> {
        Ok(())
    }
}

struct State {
    state: PackageState,
    /// What `Ready` stands for.
    downloaded: Option<UpdateInfo>,
}

enum Request {
    Check,
    Every(Duration),
}

impl Updater {
    /// `Error::NotPackaged` outside a package.
    pub fn new(spec: &'static PackageSpec, source: Source) -> Result<Updater, Error> {
        // Told from the files first, as `start` does, so a development build never asks
        // Velopack to find a manifest that is not there.
        if package::current().is_none() {
            return Err(Error::NotPackaged);
        }
        Ok(Updater::with_backend(
            spec,
            Arc::new(Velopack::new(&source)?),
        ))
    }

    pub(crate) fn with_backend(spec: &'static PackageSpec, backend: Arc<dyn Backend>) -> Updater {
        Updater {
            shared: Arc::new(Shared {
                spec,
                current: backend.current(),
                backend,
                state: Mutex::new(State {
                    state: PackageState::Idle,
                    downloaded: None,
                }),
                listeners: Mutex::new(Vec::new()),
                worker: Mutex::new(None),
                applying: AtomicBool::new(false),
            }),
        }
    }

    /// An updater standing in `state`, over a backend that checks nothing and never applies:
    /// [`apply`](Self::apply) returns `Ok` without doing anything when `state` is `Ready`,
    /// and an error (nothing downloaded) otherwise. For a GUI's tests.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn for_tests(state: PackageState) -> Updater {
        static SPEC: PackageSpec = PackageSpec {
            app_name: "test",
            on_installed: None,
            on_uninstalling: None,
            before_apply: None,
            cli: None,
        };
        let updater = Updater::with_backend(&SPEC, Arc::new(Inert));
        let downloaded = match &state {
            PackageState::Ready { version } => Some(UpdateInfo {
                TargetFullRelease: velopack::VelopackAsset {
                    Version: version.to_string(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            _ => None,
        };
        *lock(&updater.shared.state) = State { state, downloaded };
        updater
    }

    /// Called after every change of state: a GUI asks for a repaint here. Runs on the
    /// updater's thread, or on the thread calling `apply` when `before_apply` refuses.
    pub fn on_change(&self, f: impl Fn() + Send + Sync + 'static) {
        lock(&self.shared.listeners).push(Arc::new(f));
    }

    /// The running version and what the updater is doing. Never waits for a check.
    pub fn status(&self) -> PackageStatus {
        PackageStatus {
            current: Some(self.shared.current.clone()),
            state: lock(&self.shared.state).state.clone(),
        }
    }

    /// Look now (the Check button, `<app> update`), and download what is found.
    pub fn check_now(&self) {
        self.send(Request::Check);
    }

    /// Look at once and then every `every`, downloading what is found. A second call
    /// replaces the interval.
    pub fn run_in_background(&self, every: Duration) {
        self.send(Request::Every(every));
    }

    /// Runs `before_apply`, then hands the downloaded update to Velopack's updater, which
    /// waits for this process to end before it replaces the files.
    ///
    /// Who ends the process: for [`After::Exit`] this call exits it and returns only on an
    /// error. For [`After::RestartApp`] and [`After::RestartWith`] it returns `Ok`, and the
    /// caller must end the process soon (Velopack waits 60 seconds), a GUI by closing its
    /// window so that what it saves on exit is saved.
    ///
    /// An error: nothing downloaded, an apply already running on another thread
    /// ([`Error::Applying`]), `before_apply` refused (nothing is applied, and a later call
    /// tries again), or Velopack failed.
    pub fn apply(&self, after: After) -> Result<(), Error> {
        let applying = self.shared.begin_apply()?;
        // Whatever the state shows: after a refused `before_apply` it is `Failed`, and the
        // update is still there to apply.
        let info = lock(&self.shared.state)
            .downloaded
            .clone()
            .ok_or_else(|| Error::Velopack("nothing downloaded".into()))?;
        self.shared.prepare()?;
        self.shared
            .backend
            .apply(Applied::Downloaded(info), &after)?;
        applying.keep();
        Ok(())
    }

    /// For a GUI's start only, before its window opens: apply an update downloaded earlier
    /// and never applied, and start the app again; this call then exits the process itself,
    /// as there is no window to close. Looks at the downloaded packages only, never the
    /// network, so the start is not delayed. `Ok` when there is none, and on a start by
    /// Velopack's updater: an update it failed to apply would otherwise be tried at every
    /// start.
    ///
    /// Only the app's single GUI instance may call this: an app that can run two must
    /// prevent the second first.
    pub fn apply_pending_at_start(&self) -> Result<(), Error> {
        if self.apply_pending(start::restarted())? {
            std::process::exit(0);
        }
        Ok(())
    }

    /// [`apply_pending_at_start`](Self::apply_pending_at_start) but the exit: whether an
    /// update was handed to Velopack.
    fn apply_pending(&self, restarted: bool) -> Result<bool, Error> {
        if restarted {
            return Ok(false);
        }
        match self.shared.backend.pending() {
            Some(pending) if pending > self.shared.current => {
                let applying = self.shared.begin_apply()?;
                self.shared.prepare()?;
                self.shared
                    .backend
                    .apply(Applied::Pending, &After::RestartApp)?;
                applying.keep();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Hands `request` to the worker, started on the first request.
    fn send(&self, request: Request) {
        let mut worker = lock(&self.shared.worker);
        let sender = worker.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel();
            let shared = Arc::downgrade(&self.shared);
            let backend = Arc::clone(&self.shared.backend);
            std::thread::Builder::new()
                .name("package-updater".into())
                .spawn(move || work(shared, backend, rx))
                .expect("could not start the updater's thread");
            tx
        });
        // The worker ends only once the sender is dropped, so it is listening.
        let _ = sender.send(request);
    }
}

/// An apply under way: dropped, it lets the next one run, unless [`keep`](Self::keep)
/// says the update was handed to Velopack.
struct ApplyGuard<'a>(Option<&'a AtomicBool>);

impl ApplyGuard<'_> {
    fn keep(mut self) {
        self.0 = None;
    }
}

impl Drop for ApplyGuard<'_> {
    fn drop(&mut self) {
        if let Some(applying) = self.0 {
            applying.store(false, Ordering::SeqCst);
        }
    }
}

impl Shared {
    /// Marks an apply as running, or [`Error::Applying`] when one already is.
    fn begin_apply(&self) -> Result<ApplyGuard<'_>, Error> {
        self.applying
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| Error::Applying)?;
        Ok(ApplyGuard(Some(&self.applying)))
    }

    /// Runs the app's `before_apply`; its error is the state and stops the update.
    fn prepare(&self) -> Result<(), Error> {
        let Some(before_apply) = self.spec.before_apply else {
            return Ok(());
        };
        before_apply().map_err(|e| {
            self.set(PackageState::Failed(e.clone()));
            Error::Prepare(e)
        })
    }

    /// Sets the state and tells the listeners, if it changed.
    fn set(&self, new: PackageState) {
        {
            let mut state = lock(&self.state);
            if state.state == new {
                return;
            }
            state.state = new;
        }
        self.notify();
    }

    fn notify(&self) {
        // Called without the lock, so a listener may register another.
        let listeners = lock(&self.listeners).clone();
        for listener in listeners {
            listener();
        }
    }

    /// One check, and the download of what it finds. Once an update is downloaded, later
    /// checks run quietly: the state stays `Ready` (what the badge and the button show)
    /// unless one finds a newer update and has downloaded it, and their errors are only
    /// logged. A refused `before_apply` left `Failed`: a check (a GUI's Retry) is `Ready`
    /// again for the update still downloaded.
    fn check(&self, backend: &dyn Backend) {
        let ready = {
            let state = lock(&self.state);
            match (&state.state, &state.downloaded) {
                (PackageState::Failed(_), Some(info)) => {
                    Version::parse(&info.TargetFullRelease.Version).ok()
                }
                _ => None,
            }
        };
        if let Some(version) = ready {
            self.set(PackageState::Ready { version });
        }
        let quiet = lock(&self.state).downloaded.is_some();
        let fail = |msg: String| {
            if quiet {
                tracing::warn!("update check failed, keeping the downloaded update: {msg}");
            } else {
                self.set(PackageState::Failed(msg));
            }
        };
        if !quiet {
            self.set(PackageState::Checking);
        }
        let info = match backend.check() {
            Ok(Some(info)) => info,
            Ok(None) => {
                if !quiet {
                    self.set(PackageState::UpToDate);
                }
                return;
            }
            Err(e) => return fail(e.to_string()),
        };
        let version = match Version::parse(&info.TargetFullRelease.Version) {
            Ok(version) => version,
            Err(e) => {
                return fail(format!(
                    "the update's version {:?} is not a version: {e}",
                    info.TargetFullRelease.Version
                ));
            }
        };
        if !quiet {
            self.set(PackageState::Downloading { percent: 0 });
        }
        let progress = |percent: u8| {
            if !quiet {
                self.set(PackageState::Downloading { percent });
            }
        };
        match backend.download(&info, &progress) {
            Ok(()) => {
                {
                    let mut state = lock(&self.state);
                    state.state = PackageState::Ready { version };
                    state.downloaded = Some(info);
                }
                self.notify();
            }
            Err(e) => fail(e.to_string()),
        }
    }
}

/// The worker: a check per request, and one every interval once asked to. It holds the
/// state only while it works, so it ends when the last [`Updater`] is dropped.
fn work(shared: Weak<Shared>, backend: Arc<dyn Backend>, requests: Receiver<Request>) {
    let mut every: Option<Duration> = None;
    let mut next: Option<Instant> = None;
    loop {
        let request = match next {
            Some(at) => match requests.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(request) => request,
                Err(RecvTimeoutError::Timeout) => Request::Check,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match requests.recv() {
                Ok(request) => request,
                Err(_) => return,
            },
        };
        if let Request::Every(interval) = request {
            every = Some(interval);
        }
        let Some(shared) = shared.upgrade() else {
            return;
        };
        shared.check(&*backend);
        drop(shared);
        next = every.map(|every| Instant::now() + every);
    }
}

/// Locks `mutex`, through a panic elsewhere: the state is replaced whole, never left
/// half-written.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use corundum_framework_update::{PackageState, PackageStatus};
    use semver::Version;
    use velopack::{UpdateInfo, VelopackAsset};

    use super::*;
    use crate::backend::{Applied, Backend};
    use crate::error::Error;
    use crate::start::PackageSpec;

    thread_local! {
        /// What ran on the thread that applies, in order: `before_apply` and the fake's
        /// `apply` both run on the caller's thread, so each test sees only its own.
        static APPLY_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    fn log(entry: impl Into<String>) {
        APPLY_LOG.with(|l| l.borrow_mut().push(entry.into()));
    }

    fn apply_log() -> Vec<String> {
        APPLY_LOG.with(|l| l.borrow().clone())
    }

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn info(version: &str) -> UpdateInfo {
        UpdateInfo {
            TargetFullRelease: VelopackAsset {
                Version: version.to_owned(),
                FileName: format!("app-{version}-full.nupkg"),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn spec(before_apply: Option<fn() -> Result<(), String>>) -> &'static PackageSpec {
        Box::leak(Box::new(PackageSpec {
            app_name: "updater-test",
            on_installed: None,
            on_uninstalling: None,
            before_apply,
            cli: None,
        }))
    }

    fn stopping() -> Result<(), String> {
        log("before_apply");
        Ok(())
    }

    fn failing_to_stop() -> Result<(), String> {
        log("before_apply");
        Err("stop failed".to_owned())
    }

    /// A Velopack that answers checks from a list, downloads in three steps and records an
    /// apply instead of handing it over.
    struct Fake {
        current: Version,
        checks: Mutex<VecDeque<Result<Option<UpdateInfo>, Error>>>,
        checked: AtomicUsize,
        pending: Option<Version>,
    }

    impl Fake {
        fn new(
            current: &str,
            checks: Vec<Result<Option<UpdateInfo>, Error>>,
            pending: Option<&str>,
        ) -> Arc<Fake> {
            Arc::new(Fake {
                current: v(current),
                checks: Mutex::new(checks.into()),
                checked: AtomicUsize::new(0),
                pending: pending.map(v),
            })
        }
    }

    impl Backend for Fake {
        fn current(&self) -> Version {
            self.current.clone()
        }

        fn check(&self) -> Result<Option<UpdateInfo>, Error> {
            self.checked.fetch_add(1, Ordering::SeqCst);
            self.checks.lock().unwrap().pop_front().unwrap_or(Ok(None))
        }

        fn download(&self, _info: &UpdateInfo, progress: &dyn Fn(u8)) -> Result<(), Error> {
            for percent in [0, 50, 100] {
                progress(percent);
            }
            Ok(())
        }

        fn pending(&self) -> Option<Version> {
            self.pending.clone()
        }

        fn apply(&self, applied: Applied, after: &After) -> Result<(), Error> {
            let what = match applied {
                Applied::Downloaded(info) => info.TargetFullRelease.Version,
                Applied::Pending => "pending".to_owned(),
            };
            log(format!("apply {what} {after:?}"));
            Ok(())
        }
    }

    fn updater(spec: &'static PackageSpec, fake: Arc<Fake>) -> Updater {
        Updater::with_backend(spec, fake)
    }

    /// Polls the state until `done` holds, for 5 seconds at most.
    fn wait_for(updater: &Updater, done: impl Fn(&PackageState) -> bool) -> PackageStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = updater.status();
            if done(&status.state) {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "still {:?} after 5 seconds",
                status.state
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn settled(state: &PackageState) -> bool {
        matches!(
            state,
            PackageState::UpToDate | PackageState::Ready { .. } | PackageState::Failed(_)
        )
    }

    #[test]
    fn a_found_update_is_downloaded_and_ready() {
        let updater = updater(
            spec(None),
            Fake::new("1.0.0", vec![Ok(Some(info("1.1.0")))], None),
        );
        assert_eq!(
            updater.status(),
            PackageStatus {
                current: Some(v("1.0.0")),
                state: PackageState::Idle
            }
        );
        updater.check_now();
        let status = wait_for(&updater, settled);
        assert_eq!(
            status.state,
            PackageState::Ready {
                version: v("1.1.0")
            }
        );
        assert_eq!(status.current, Some(v("1.0.0")));
    }

    #[test]
    fn nothing_found_is_up_to_date() {
        let updater = updater(spec(None), Fake::new("1.0.0", vec![Ok(None)], None));
        updater.check_now();
        assert_eq!(wait_for(&updater, settled).state, PackageState::UpToDate);
    }

    #[test]
    fn a_check_error_is_failed_and_a_later_check_recovers() {
        let updater = updater(
            spec(None),
            Fake::new(
                "1.0.0",
                vec![Err(Error::Network("offline".into())), Ok(None)],
                None,
            ),
        );
        updater.check_now();
        let failed = wait_for(&updater, settled).state;
        assert!(
            matches!(&failed, PackageState::Failed(msg) if msg.contains("offline")),
            "{failed:?}"
        );
        updater.check_now();
        assert_eq!(
            wait_for(&updater, |s| *s == PackageState::UpToDate).state,
            PackageState::UpToDate
        );
    }

    #[test]
    fn apply_runs_before_apply_first() {
        let updater = updater(
            spec(Some(stopping)),
            Fake::new("1.0.0", vec![Ok(Some(info("1.1.0")))], None),
        );
        updater.check_now();
        wait_for(&updater, settled);
        updater.apply(After::RestartApp).unwrap();
        assert_eq!(apply_log(), ["before_apply", "apply 1.1.0 RestartApp"]);
    }

    #[test]
    fn a_second_apply_while_one_runs_is_refused() {
        static UPDATER: std::sync::OnceLock<Updater> = std::sync::OnceLock::new();
        /// Asks for an apply again while the first is still in `before_apply`: the badge
        /// and the panel, clicked together.
        fn applying_again() -> Result<(), String> {
            let again = UPDATER.get().unwrap().apply(After::RestartApp);
            log(format!("again {again:?}"));
            Ok(())
        }
        let updater = updater(
            spec(Some(applying_again)),
            Fake::new("1.0.0", vec![Ok(Some(info("1.1.0")))], None),
        );
        UPDATER.set(updater.clone()).ok().unwrap();
        updater.check_now();
        wait_for(&updater, settled);
        updater.apply(After::RestartApp).unwrap();
        assert_eq!(
            apply_log(),
            ["again Err(Applying)", "apply 1.1.0 RestartApp"]
        );
        // Handed to Velopack: the process is ending, and nothing applies again.
        assert!(matches!(
            updater.apply(After::RestartApp),
            Err(Error::Applying)
        ));
    }

    #[test]
    fn a_failing_before_apply_applies_nothing() {
        let updater = updater(
            spec(Some(failing_to_stop)),
            Fake::new("1.0.0", vec![Ok(Some(info("1.1.0")))], None),
        );
        updater.check_now();
        wait_for(&updater, settled);
        let err = updater.apply(After::Exit).unwrap_err();
        assert!(
            matches!(err, Error::Prepare(ref m) if m == "stop failed"),
            "{err:?}"
        );
        assert_eq!(apply_log(), ["before_apply"]);
        assert_eq!(
            updater.status().state,
            PackageState::Failed("stop failed".to_owned())
        );
    }

    #[test]
    fn apply_without_a_download_is_an_error() {
        let updater = updater(spec(Some(stopping)), Fake::new("1.0.0", vec![], None));
        let err = updater.apply(After::RestartApp).unwrap_err();
        assert!(
            matches!(err, Error::Velopack(ref m) if m == "nothing downloaded"),
            "{err:?}"
        );
        assert!(apply_log().is_empty());
    }

    #[test]
    fn a_pending_update_newer_than_current_is_applied_at_start() {
        let updater = updater(
            spec(Some(stopping)),
            Fake::new("1.0.0", vec![], Some("1.1.0")),
        );
        assert!(updater.apply_pending(false).unwrap());
        assert_eq!(apply_log(), ["before_apply", "apply pending RestartApp"]);
    }

    #[test]
    fn a_pending_package_not_newer_is_left_alone() {
        for pending in [Some("1.0.0"), Some("0.9.0"), None] {
            let updater = updater(spec(Some(stopping)), Fake::new("1.0.0", vec![], pending));
            assert!(!updater.apply_pending(false).unwrap());
        }
        assert!(apply_log().is_empty());
    }

    #[test]
    fn a_start_by_velopack_s_updater_applies_no_pending_package() {
        let updater = updater(
            spec(Some(stopping)),
            Fake::new("1.0.0", vec![], Some("1.1.0")),
        );
        assert!(!updater.apply_pending(true).unwrap());
        assert!(apply_log().is_empty());
    }

    #[test]
    fn on_change_is_called_on_every_state_change() {
        let updater = updater(
            spec(None),
            Fake::new("1.0.0", vec![Ok(Some(info("1.1.0")))], None),
        );
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));
        for counter in [&first, &second] {
            let counter = Arc::clone(counter);
            updater.on_change(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            });
        }
        updater.check_now();
        wait_for(&updater, settled);
        // Checking, Downloading at 0, 50 and 100, Ready. The callbacks run after the state
        // is set, so wait for the last one too.
        let deadline = Instant::now() + Duration::from_secs(5);
        while second.load(Ordering::SeqCst) < 5 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(first.load(Ordering::SeqCst), 5);
        assert_eq!(second.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn a_failing_check_after_a_download_keeps_it_ready() {
        let fake = Fake::new(
            "1.0.0",
            vec![
                Ok(Some(info("1.1.0"))),
                Err(Error::Network("offline".into())),
            ],
            None,
        );
        let updater = updater(spec(Some(stopping)), Arc::clone(&fake));
        let changes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&changes);
        updater.on_change(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        updater.check_now();
        wait_for(&updater, settled);
        // A check that fails, then one that finds nothing: once the fake sees the third
        // check, the worker is done with the failing one.
        updater.check_now();
        updater.check_now();
        let deadline = Instant::now() + Duration::from_secs(5);
        while fake.checked.load(Ordering::SeqCst) < 3 {
            assert!(Instant::now() < deadline, "the checks never ran");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            updater.status().state,
            PackageState::Ready {
                version: v("1.1.0")
            }
        );
        // Checking, Downloading at 0, 50 and 100, Ready; nothing for the later checks.
        assert_eq!(changes.load(Ordering::SeqCst), 5);
        updater.apply(After::RestartApp).unwrap();
        assert_eq!(apply_log(), ["before_apply", "apply 1.1.0 RestartApp"]);
    }

    #[test]
    fn apply_is_retried_after_a_failing_before_apply() {
        static REFUSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
        fn refusing_once() -> Result<(), String> {
            log("before_apply");
            if REFUSE.swap(false, Ordering::SeqCst) {
                Err("stop failed".to_owned())
            } else {
                Ok(())
            }
        }
        let updater = updater(
            spec(Some(refusing_once)),
            Fake::new("1.0.0", vec![Ok(Some(info("1.1.0")))], None),
        );
        updater.check_now();
        wait_for(&updater, settled);
        let err = updater.apply(After::Exit).unwrap_err();
        assert!(matches!(err, Error::Prepare(_)), "{err:?}");
        assert_eq!(
            updater.status().state,
            PackageState::Failed("stop failed".to_owned())
        );
        updater.apply(After::Exit).unwrap();
        assert_eq!(
            apply_log(),
            ["before_apply", "before_apply", "apply 1.1.0 Exit"]
        );
    }

    #[test]
    fn a_check_after_a_refused_before_apply_is_ready_again() {
        let fake = Fake::new(
            "1.0.0",
            vec![
                Ok(Some(info("1.1.0"))),
                Err(Error::Network("offline".into())),
            ],
            None,
        );
        let updater = updater(spec(Some(failing_to_stop)), Arc::clone(&fake));
        updater.check_now();
        wait_for(&updater, settled);
        updater.apply(After::RestartApp).unwrap_err();
        assert_eq!(
            updater.status().state,
            PackageState::Failed("stop failed".to_owned())
        );
        // The GUI's Retry: a check, which fails here, and the update is still downloaded.
        updater.check_now();
        let status = wait_for(&updater, |s| matches!(s, PackageState::Ready { .. }));
        assert_eq!(
            status.state,
            PackageState::Ready {
                version: v("1.1.0")
            }
        );
    }

    #[test]
    fn run_in_background_checks_at_once_and_stops_with_the_last_updater() {
        let fake = Fake::new("1.0.0", vec![Ok(None)], None);
        let updater = updater(spec(None), Arc::clone(&fake));
        updater.run_in_background(Duration::from_secs(3600));
        assert_eq!(wait_for(&updater, settled).state, PackageState::UpToDate);
        let clone = updater.clone();
        drop(updater);
        drop(clone);
        // The worker holds the fake through the updater only: once it ends, the test holds
        // the last reference.
        let deadline = Instant::now() + Duration::from_secs(5);
        while Arc::strong_count(&fake) > 1 {
            assert!(Instant::now() < deadline, "the worker outlived the updater");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
