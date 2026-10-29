//! [`UpdatesPanel`]: the rows of Settings → About that check for releases and install them.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::TryRecvError;
#[cfg(test)]
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use corundum_framework_update::{
    InstallMethod, PackageState, PackageStatus, Release, ReleaseSource, StoreKind, advice_command,
};
use semver::Version;

use super::package_row::{RowButton, package_row};
use super::state::{Event, RestartOutcome, Retry, RowState, Step, on_check, on_event};
use super::target::Target;
use super::worker::{self, Job, Worker};
use crate::tr;

/// One program the panel keeps up to date: the app, its server, a bridge.
pub struct UpdateTarget {
    /// What the row says: `Sapphire Journal`, `sapphire-journal server`, `sapphire-bridge`.
    pub label: String,
    /// The version running now, or `None` when it is not known (a server not running).
    pub current: Option<Version>,
    /// Where its releases are.
    pub source: ReleaseSource,
    /// The executable to replace.
    pub exe: PathBuf,
    /// What follows the replacement.
    pub after: AfterUpdate,
}

/// What follows replacing a target's executable.
pub enum AfterUpdate {
    /// The app itself: start `exe` again with the same arguments, then close the window.
    Relaunch,
    /// A service: the app's own way to restart it, run on the panel's worker thread.
    Restart(Box<dyn Fn() -> RestartOutcome + Send + Sync>),
    /// Nothing the app can do: the row says to restart it by hand.
    Manual,
}

/// The update rows: each target's running version, its latest release, and the way to it.
///
/// It looks the releases up when first shown and on its Check button, never before; the
/// work runs on a thread of its own, which ends when the panel is dropped. One target is
/// installed at a time. Without the `self-update` feature it only reports, and says where
/// each binary's release is; a package updates itself whatever the features.
pub struct UpdatesPanel {
    targets: Arc<[Target]>,
    /// The targets' labels, asked once.
    labels: Vec<String>,
    rows: Vec<Row>,
    user_agent: String,
    /// Started on the first `ui`.
    worker: Option<Worker>,
    /// Whether the first check went out.
    checked: bool,
    /// When the delegated packages were last asked their state.
    last_poll: Option<Instant>,
    /// Each row's button, and whether it was enabled, for the tests.
    #[cfg(test)]
    buttons: Vec<Option<(egui::Rect, bool)>>,
}

/// A row's state, and how its executable was installed once known.
struct Row {
    /// The version running now; see [`UpdatesPanel::set_current`].
    current: Option<Version>,
    /// A binary's state; a package row's is in `package`.
    state: RowState,
    method: Option<InstallMethod>,
    /// A package row's state, `None` for a binary.
    package: Option<PackageRow>,
}

/// What a package row knows besides its updater's state.
struct PackageRow {
    /// The updater's state: read every frame for this app's package, asked on the worker
    /// for a delegated one.
    status: PackageStatus,
    /// Why the last apply failed, until the next check.
    failure: Option<String>,
    /// Whether an apply is under way.
    applying: bool,
    /// Whether a check or a poll is waiting for the worker.
    asking: bool,
}

/// What a row's button asked for.
enum Action {
    /// Install the release.
    #[cfg(feature = "self-update")]
    Update(Release),
    /// Do what the failed row's retry does.
    Retry(Retry),
    /// Apply a package's downloaded update.
    Apply,
}

/// Whether this build installs releases.
const CAN_INSTALL: bool = cfg!(feature = "self-update");

/// How often a delegated package is asked its state while the panel is shown.
const POLL_EVERY: Duration = Duration::from_secs(5);

impl UpdatesPanel {
    /// The rows for `targets`; GitHub is asked as `user_agent`.
    pub fn new(targets: Vec<Target>, user_agent: &str) -> UpdatesPanel {
        let rows = targets
            .iter()
            .map(|target| Row {
                current: target.binary().and_then(|target| target.current.clone()),
                state: RowState::Checking,
                method: None,
                package: target.package().map(|_| PackageRow {
                    status: PackageStatus {
                        current: None,
                        state: PackageState::Checking,
                    },
                    failure: None,
                    applying: false,
                    asking: false,
                }),
            })
            .collect();
        UpdatesPanel {
            labels: targets.iter().map(Target::label).collect(),
            targets: targets.into(),
            rows,
            user_agent: user_agent.to_owned(),
            worker: None,
            checked: false,
            last_poll: None,
            #[cfg(test)]
            buttons: Vec::new(),
        }
    }

    /// A panel whose worker is the other ends of `jobs` and `results`: for the tests.
    #[cfg(test)]
    pub(crate) fn with_worker(
        targets: Vec<Target>,
        jobs: Sender<Job>,
        results: Receiver<(usize, Event)>,
    ) -> UpdatesPanel {
        let mut panel = UpdatesPanel::new(targets, "test");
        panel.worker = Some(Worker { jobs, results });
        panel
    }

    /// Set the running version of the target at `index`: call it when the app learns it
    /// (a server that connected, or reports its new version after an update). It shows from
    /// the next frame and is what the next check compares with; an `index` out of range does
    /// nothing.
    pub fn set_current(&mut self, index: usize, current: Option<Version>) {
        if let Some(row) = self.rows.get_mut(index) {
            row.current = current;
        }
    }

    /// The Check button and the rows.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if self.worker.is_none() {
            let targets = self.targets.clone();
            let ctx = ui.ctx().clone();
            self.worker = Some(worker::spawn(targets, self.user_agent.clone(), ctx));
        }
        // Looked up when first shown, not when the app starts.
        if !self.checked {
            self.checked = true;
            for target in self.targets.iter() {
                target.repaint_on_change(ui.ctx());
            }
            self.check_all(ui.ctx());
            self.last_poll = Some(Instant::now());
        }
        self.poll(ui.ctx());
        self.receive(ui.ctx());
        for (target, row) in self.targets.iter().zip(&mut self.rows) {
            if let (Some(status), Some(package)) = (target.live_status(), &mut row.package) {
                package.status = status;
            }
        }

        if ui.button(tr!(ui, "corundum-update-check")).clicked() {
            self.check_all(ui.ctx());
        }
        ui.add_space(8.0);
        // One install at a time: while a row works, the others' buttons wait.
        let idle = !self.rows.iter().any(|row| {
            matches!(row.state, RowState::Working { .. })
                || row.package.as_ref().is_some_and(|package| package.applying)
        });
        let mut action = None;
        #[cfg(test)]
        self.buttons.clear();
        egui::Grid::new("corundum_update_rows")
            .num_columns(3)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                for (index, (target, row)) in self.targets.iter().zip(&self.rows).enumerate() {
                    ui.label(&self.labels[index]);
                    let current = match &row.package {
                        Some(package) => package.status.current.as_ref(),
                        None => row.current.as_ref(),
                    };
                    match current {
                        Some(version) => ui.label(version.to_string()),
                        None => ui.weak(tr!(ui, "corundum-update-not-running")),
                    };
                    let (asked, _button) = ui
                        .horizontal(|ui| match (target.binary(), &row.package) {
                            (Some(target), _) => row_ui(ui, target, row, idle),
                            (None, Some(package)) => package_ui(ui, package, idle),
                            (None, None) => (None, None),
                        })
                        .inner;
                    if let Some(asked) = asked {
                        action = Some((index, asked));
                    }
                    #[cfg(test)]
                    self.buttons.push(_button.map(|b| (b.rect, b.enabled())));
                    ui.end_row();
                }
            });
        match action {
            #[cfg(feature = "self-update")]
            Some(
                (index, Action::Update(release)) | (index, Action::Retry(Retry::Update(release))),
            ) => self.update(ui.ctx(), index, release),
            #[cfg(not(feature = "self-update"))]
            Some((index, Action::Retry(Retry::Update(_)))) => self.check(ui.ctx(), index),
            Some((index, Action::Retry(Retry::Check))) => self.check(ui.ctx(), index),
            Some((index, Action::Apply)) => self.apply(ui.ctx(), index),
            None => {}
        }
        // Asked again while shown: `ui` runs at least that often.
        if self.targets.iter().any(Target::is_polled) {
            ui.ctx().request_repaint_after(POLL_EVERY);
        }
    }

    /// Look every row up again but one installing.
    fn check_all(&mut self, ctx: &egui::Context) {
        for index in 0..self.rows.len() {
            let row = &self.rows[index];
            let busy = match &row.package {
                Some(package) => package.applying,
                None => matches!(row.state, RowState::Working { .. }),
            };
            if !busy {
                self.check(ctx, index);
            }
        }
    }

    fn check(&mut self, ctx: &egui::Context, index: usize) {
        let row = &mut self.rows[index];
        match &mut row.package {
            Some(package) => {
                package.failure = None;
                package.asking = true;
                // This app's package shows its own state from the next frame.
                package.status.state = PackageState::Checking;
            }
            None => row.state = RowState::Checking,
        }
        self.send(ctx, index, Job::Check(index));
    }

    /// Ask the delegated packages their state, if it has been [`POLL_EVERY`] since the last
    /// time; but not one already waiting for an answer, or applying.
    fn poll(&mut self, ctx: &egui::Context) {
        if self
            .last_poll
            .is_some_and(|last| last.elapsed() < POLL_EVERY)
        {
            return;
        }
        self.last_poll = Some(Instant::now());
        for index in 0..self.rows.len() {
            let waiting = self.rows[index]
                .package
                .as_ref()
                .is_none_or(|package| package.asking || package.applying);
            if self.targets[index].is_polled() && !waiting {
                if let Some(package) = &mut self.rows[index].package {
                    package.asking = true;
                }
                self.send(ctx, index, Job::Poll(index));
            }
        }
    }

    /// Apply the package's downloaded update: on the worker, as the app's `before_apply`
    /// may take a while. When it succeeds for this app's package, the window closes.
    fn apply(&mut self, ctx: &egui::Context, index: usize) {
        if let Some(package) = &mut self.rows[index].package {
            package.applying = true;
            package.failure = None;
        }
        self.send(ctx, index, Job::Apply(index));
    }

    #[cfg(feature = "self-update")]
    fn update(&mut self, ctx: &egui::Context, index: usize, release: Release) {
        self.rows[index].state = RowState::Working {
            release: release.clone(),
            step: Step::Downloading,
        };
        let current = self.rows[index].current.clone();
        self.send(ctx, index, Job::Update(index, release, current));
    }

    /// Hand `job` to the worker; if it is gone, `index`'s row says so.
    fn send(&mut self, ctx: &egui::Context, index: usize, job: Job) {
        let sent = self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.jobs.send(job).is_ok());
        if !sent {
            stop(ctx, &mut self.rows[index]);
        }
    }

    /// Take the worker's results into the rows; close the window once the app relaunched.
    fn receive(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        let mut results = Vec::new();
        let mut gone = false;
        loop {
            match worker.results.try_recv() {
                Ok(result) => results.push(result),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    gone = true;
                    break;
                }
            }
        }
        for (index, event) in results {
            let (Some(target), Some(row)) = (self.targets.get(index), self.rows.get_mut(index))
            else {
                continue;
            };
            if let Some(package) = &mut row.package {
                match event {
                    Event::Status(status) => {
                        package.asking = false;
                        match status {
                            Ok(status) => package.status = status,
                            Err(message) => package.status.state = PackageState::Failed(message),
                        }
                    }
                    Event::Applied(applied) => {
                        package.applying = false;
                        // Velopack's updater waits for this app to end: the window closes,
                        // as for a relaunched binary, so what the app saves on exit is saved.
                        if applied.is_ok() && target.closes_on_apply() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        package.failure = applied.err();
                    }
                    _ => {}
                }
                continue;
            }
            let Some(target) = target.binary() else {
                continue;
            };
            match event {
                // Only a row waiting for it: a late answer to an earlier check must not undo an
                // install begun since.
                Event::Checked {
                    found,
                    method,
                    has_asset,
                } if matches!(row.state, RowState::Checking) => {
                    row.state = on_check(
                        row.current.as_ref(),
                        &method,
                        has_asset,
                        CAN_INSTALL,
                        &target.source,
                        found,
                    );
                    row.method = Some(method);
                }
                Event::Checked { .. } => {}
                event => {
                    let was_working = matches!(row.state, RowState::Working { .. });
                    row.state = on_event(row.state.clone(), event);
                    // The new process runs: this one makes way.
                    if was_working
                        && matches!(row.state, RowState::Done { .. })
                        && matches!(target.after, AfterUpdate::Relaunch)
                    {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
        // The worker ended (a restart that panicked, say): what waits on it never comes.
        if gone {
            for row in &mut self.rows {
                let waiting = match &row.package {
                    Some(package) => package.asking || package.applying,
                    None => matches!(row.state, RowState::Checking | RowState::Working { .. }),
                };
                if waiting {
                    stop(ctx, row);
                }
            }
        }
    }
}

/// Fail `row`, whose worker is gone.
fn stop(ctx: &egui::Context, row: &mut Row) {
    let message = tr!(ctx, "corundum-update-worker-stopped");
    match &mut row.package {
        Some(package) => {
            package.asking = false;
            package.applying = false;
            package.failure = Some(message);
        }
        None => {
            row.state = RowState::Failed {
                message,
                retry: Retry::Check,
            }
        }
    }
}

/// A package row's state and button, from [`package_row`]: what its button asked for, and
/// the button.
fn package_ui(
    ui: &mut egui::Ui,
    package: &PackageRow,
    idle: bool,
) -> (Option<Action>, Option<egui::Response>) {
    if package.applying {
        ui.spinner();
        ui.label(tr!(ui, "corundum-update-restarting"));
        return (None, None);
    }
    let status = match &package.failure {
        Some(failure) => PackageStatus {
            current: package.status.current.clone(),
            state: PackageState::Failed(failure.clone()),
        },
        None => package.status.clone(),
    };
    let view = package_row(&status);
    if view.spinner {
        ui.spinner();
    }
    if let Some(progress) = view.progress {
        ui.add(
            egui::ProgressBar::new(progress)
                .desired_width(120.0)
                .show_percentage(),
        );
    }
    let (id, args) = view.text;
    let args: Vec<_> = args
        .into_iter()
        .map(|(name, value)| (name, crate::i18n::Arg::from(value)))
        .collect();
    ui.label(crate::i18n::tr_args(ui.ctx(), id, &args));
    let Some(button) = view.button else {
        return (None, None);
    };
    let (text, action) = match button {
        RowButton::RestartToUpdate => (tr!(ui, "corundum-update-package-restart"), Action::Apply),
        RowButton::Retry => (
            tr!(ui, "corundum-update-retry"),
            Action::Retry(Retry::Check),
        ),
    };
    let response = ui.add_enabled(idle, egui::Button::new(text));
    let action = response.clicked().then_some(action);
    (action, Some(response))
}

/// A row's state, button and release-notes link; what its button asked for, and the
/// button.
fn row_ui(
    ui: &mut egui::Ui,
    target: &UpdateTarget,
    row: &Row,
    idle: bool,
) -> (Option<Action>, Option<egui::Response>) {
    let mut action = None;
    let mut button = None;
    let newest = |release: &Release| release.version.to_string();
    match &row.state {
        RowState::Checking => {
            ui.spinner();
            ui.label(tr!(ui, "corundum-update-checking"));
        }
        RowState::UpToDate { .. } => {
            ui.label(tr!(ui, "corundum-update-up-to-date"));
        }
        RowState::Newer { release } => {
            ui.label(tr!(ui, "corundum-update-newer", version = newest(release)));
            #[cfg(feature = "self-update")]
            {
                let response =
                    ui.add_enabled(idle, egui::Button::new(tr!(ui, "corundum-update-update")));
                if response.clicked() {
                    action = Some(Action::Update(release.clone()));
                }
                button = Some(response);
            }
            notes_link(ui, release);
        }
        RowState::NotHere { release, .. } => {
            ui.label(tr!(ui, "corundum-update-newer", version = newest(release)));
            button = how_ui(ui, &target.source, row.method.as_ref(), idle);
            notes_link(ui, release);
        }
        RowState::Working { release, step } => {
            ui.spinner();
            ui.label(match step {
                Step::Downloading => {
                    tr!(ui, "corundum-update-downloading", version = newest(release))
                }
                Step::Restarting => tr!(ui, "corundum-update-restarting"),
            });
            notes_link(ui, release);
        }
        RowState::Done { version } => {
            ui.label(tr!(
                ui,
                "corundum-update-done",
                version = version.to_string()
            ));
        }
        RowState::RestartByHand { version } => {
            ui.label(tr!(
                ui,
                "corundum-update-restart-by-hand",
                version = version.to_string()
            ));
        }
        RowState::Failed { message, retry } => {
            ui.label(tr!(ui, "corundum-update-failed", error = message.as_str()));
            let response =
                ui.add_enabled(idle, egui::Button::new(tr!(ui, "corundum-update-retry")));
            if response.clicked() {
                action = Some(Action::Retry(retry.clone()));
            }
            button = Some(response);
        }
    }
    (action, button)
}

/// How a release this app does not install is had, by how the executable was installed;
/// with a Copy button for a command to run.
fn how_ui(
    ui: &mut egui::Ui,
    source: &ReleaseSource,
    method: Option<&InstallMethod>,
    idle: bool,
) -> Option<egui::Response> {
    let command = method.and_then(|method| advice_command(source, method));
    let text = match (method, command.as_deref()) {
        (_, Some(command)) => {
            ui.label(tr!(ui, "corundum-update-run"));
            ui.code(command);
            let response = ui.add_enabled(idle, egui::Button::new(tr!(ui, "corundum-update-copy")));
            if response.clicked() {
                ui.ctx().copy_text(command.to_owned());
            }
            return Some(response);
        }
        (Some(InstallMethod::Cargo), None) => {
            tr!(ui, "corundum-update-by-cargo", tool = "cargo install")
        }
        (Some(InstallMethod::Binstall { .. }), None) => {
            tr!(ui, "corundum-update-by-cargo", tool = "cargo binstall")
        }
        (Some(InstallMethod::PackageManager), None) => {
            tr!(ui, "corundum-update-by-package-manager")
        }
        (Some(InstallMethod::Velopack), None) => tr!(ui, "corundum-update-by-the-app"),
        (Some(InstallMethod::Store(store)), None) => tr!(
            ui,
            match store {
                StoreKind::Flatpak => "corundum-update-by-flathub",
                StoreKind::Snap => "corundum-update-by-snap",
                StoreKind::Msix => "corundum-update-by-microsoft-store",
                StoreKind::MacAppStore => "corundum-update-by-mac-app-store",
            }
        ),
        // Placed by hand, but this build does not install, or the release has no file for
        // this platform.
        (Some(InstallMethod::Release) | None, None) => {
            tr!(ui, "corundum-update-from-release-page")
        }
    };
    ui.label(text);
    None
}

/// A link to `release`'s notes.
fn notes_link(ui: &mut egui::Ui, release: &Release) {
    ui.hyperlink_to(tr!(ui, "corundum-update-release-notes"), &release.url);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::DelegatedTarget;
    use crate::update::state::Event;
    use corundum_framework_update::{Asset, Release, asset_name};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    fn release(version: &str) -> Release {
        Release {
            version: version.parse().unwrap(),
            url: "https://example.test/r".to_owned(),
            assets: vec![Asset {
                name: asset_name("app"),
                download_url: "https://example.test/a".to_owned(),
            }],
        }
    }

    fn target(label: &str, after: AfterUpdate) -> UpdateTarget {
        UpdateTarget {
            label: label.to_owned(),
            current: Some("1.0.0".parse().unwrap()),
            source: ReleaseSource::github("o", "r").asset("app"),
            exe: PathBuf::from("/nonexistent/app"),
            after,
        }
    }

    struct Fixture {
        ctx: egui::Context,
        panel: UpdatesPanel,
        jobs: Receiver<Job>,
        results: Sender<(usize, Event)>,
    }

    fn fixture() -> Fixture {
        let (job_tx, jobs) = mpsc::channel();
        let (results, result_rx) = mpsc::channel();
        let targets = vec![
            target("App", AfterUpdate::Relaunch).into(),
            target("Server", AfterUpdate::Manual).into(),
        ];
        Fixture {
            ctx: egui::Context::default(),
            panel: UpdatesPanel::with_worker(targets, job_tx, result_rx),
            jobs,
            results,
        }
    }

    impl Fixture {
        /// One frame with `events`; what egui gave back.
        fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let panel = &mut self.panel;
            let mut output = self.ctx.run_ui(input, |ui| panel.ui(ui));
            output.textures_delta.clear();
            output
        }

        fn sent(&self) -> Vec<Job> {
            self.jobs.try_iter().collect()
        }

        fn click(&mut self, pos: egui::Pos2) {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![egui::Event::PointerMoved(pos)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.frame(vec![]);
        }

        /// Carry out the package rows' jobs sent, as the worker would, and send back its
        /// replies; the jobs carried out. A binary row's are dropped.
        fn work(&self) -> Vec<Job> {
            let mut done = Vec::new();
            for job in self.sent() {
                let index = match job {
                    Job::Check(index) | Job::Poll(index) | Job::Apply(index) => index,
                    #[cfg(feature = "self-update")]
                    Job::Update(index, ..) => index,
                };
                let Some(package) = self.panel.targets[index].package() else {
                    continue;
                };
                for event in worker::package_job(package, &job) {
                    self.results.send((index, event)).unwrap();
                }
                done.push(job);
            }
            done
        }

        /// Whether a frame asked for the window to close.
        fn closes(output: &egui::FullOutput) -> bool {
            output.viewport_output.values().any(|v| {
                v.commands
                    .iter()
                    .any(|c| matches!(c, egui::ViewportCommand::Close))
            })
        }
    }

    fn newer(version: &str) -> Event {
        Event::Checked {
            found: Ok(release(version)),
            method: InstallMethod::Release,
            has_asset: true,
        }
    }

    #[test]
    fn checks_every_row_on_the_first_frame_only() {
        let mut f = fixture();
        f.frame(vec![]);
        assert_eq!(f.sent(), vec![Job::Check(0), Job::Check(1)]);
        f.frame(vec![]);
        assert_eq!(f.sent(), vec![]);
    }

    #[test]
    fn a_set_current_is_what_a_check_compares_with() {
        let mut f = fixture();
        f.frame(vec![]);
        f.panel.set_current(1, Some("1.1.0".parse().unwrap()));
        f.panel.set_current(9, None);
        // As after an update: the row is done, the app learns the new version, checks again.
        f.panel.rows[1].state = RowState::Done {
            version: "1.1.0".parse().unwrap(),
        };
        f.panel.check(&f.ctx.clone(), 1);
        f.results.send((1, newer("1.1.0"))).unwrap();
        f.frame(vec![]);
        assert_eq!(
            f.panel.rows[1].state,
            RowState::UpToDate {
                latest: "1.1.0".parse().unwrap()
            }
        );
    }

    #[test]
    fn rows_render() {
        let mut f = fixture();
        crate::test_support::render_in(&f.ctx.clone(), [900.0, 600.0], |ui| f.panel.ui(ui));
        assert_eq!(f.sent().len(), 2);
    }

    #[test]
    fn a_check_result_sets_the_row() {
        let mut f = fixture();
        f.frame(vec![]);
        f.results.send((1, newer("0.9.0"))).unwrap();
        f.frame(vec![]);
        assert_eq!(
            f.panel.rows[1].state,
            RowState::UpToDate {
                latest: "0.9.0".parse().unwrap()
            }
        );
        assert_eq!(f.panel.rows[1].method, Some(InstallMethod::Release));
    }

    #[test]
    fn a_command_to_run_gets_a_copy_button() {
        let mut f = fixture();
        f.frame(vec![]);
        let method = InstallMethod::Binstall {
            crate_name: "app-crate".to_owned(),
            source: corundum_framework_update::BinstallSource::Registry,
        };
        let event = Event::Checked {
            found: Ok(release("1.1.0")),
            method: method.clone(),
            has_asset: true,
        };
        f.results.send((0, event)).unwrap();
        let store = Event::Checked {
            found: Ok(release("1.1.0")),
            method: InstallMethod::PackageManager,
            has_asset: true,
        };
        f.results.send((1, store)).unwrap();
        f.frame(vec![]);
        f.frame(vec![]);
        assert!(matches!(f.panel.rows[0].state, RowState::NotHere { .. }));
        assert!(f.panel.buttons[0].is_some(), "Copy");
        assert!(f.panel.buttons[1].is_none(), "nothing to copy");
    }

    #[test]
    fn a_late_check_leaves_a_working_row() {
        let mut f = fixture();
        f.frame(vec![]);
        let working = RowState::Working {
            release: release("1.1.0"),
            step: crate::update::Step::Downloading,
        };
        f.panel.rows[0].state = working.clone();
        f.results.send((0, newer("1.1.0"))).unwrap();
        f.frame(vec![]);
        assert_eq!(f.panel.rows[0].state, working);
    }

    #[test]
    fn a_stopped_worker_fails_the_waiting_rows() {
        let mut f = fixture();
        f.frame(vec![]);
        f.panel.rows[0].state = RowState::Working {
            release: release("1.1.0"),
            step: crate::update::Step::Restarting,
        };
        // The worker's end of the results goes away.
        f.results = mpsc::channel().0;
        f.frame(vec![]);
        for row in &f.panel.rows {
            assert!(matches!(
                row.state,
                RowState::Failed {
                    retry: Retry::Check,
                    ..
                }
            ));
        }
    }

    #[test]
    fn a_failed_restart_keeps_the_window() {
        let mut f = fixture();
        f.frame(vec![]);
        f.panel.rows[0].state = RowState::Working {
            release: release("1.1.0"),
            step: crate::update::Step::Restarting,
        };
        f.results
            .send((0, Event::Restarted(RestartOutcome::Failed("no".into()))))
            .unwrap();
        let output = f.frame(vec![]);
        assert!(!Fixture::closes(&output));
        assert!(matches!(f.panel.rows[0].state, RowState::Failed { .. }));
    }

    #[test]
    fn the_relaunched_app_closes_the_window() {
        let mut f = fixture();
        f.frame(vec![]);
        f.panel.rows[0].state = RowState::Working {
            release: release("1.1.0"),
            step: crate::update::Step::Restarting,
        };
        f.results
            .send((0, Event::Restarted(RestartOutcome::Restarted)))
            .unwrap();
        let output = f.frame(vec![]);
        assert!(Fixture::closes(&output));
    }

    /// A delegated package that is always ready with 1.1.0, and counts its calls.
    #[derive(Default)]
    struct Bridge {
        checks: AtomicUsize,
        polls: AtomicUsize,
        applies: AtomicUsize,
    }

    impl DelegatedTarget for Arc<Bridge> {
        fn label(&self) -> String {
            "Bridge".to_owned()
        }

        fn status(&self) -> Result<PackageStatus, String> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Ok(PackageStatus {
                current: Some("1.0.0".parse().unwrap()),
                state: PackageState::Ready {
                    version: "1.1.0".parse().unwrap(),
                },
            })
        }

        fn check(&self) -> Result<(), String> {
            self.checks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn apply(&self) -> Result<(), String> {
            self.applies.fetch_add(1, Ordering::SeqCst);
            Err("the bridge is busy".to_owned())
        }
    }

    fn bridge_fixture() -> (Fixture, Arc<Bridge>) {
        let (job_tx, jobs) = mpsc::channel();
        let (results, result_rx) = mpsc::channel();
        let bridge = Arc::new(Bridge::default());
        let targets = vec![
            Target::Delegated(Box::new(Arc::clone(&bridge))),
            target("App", AfterUpdate::Manual).into(),
        ];
        let fixture = Fixture {
            ctx: egui::Context::default(),
            panel: UpdatesPanel::with_worker(targets, job_tx, result_rx),
            jobs,
            results,
        };
        (fixture, bridge)
    }

    #[test]
    fn a_delegated_row_shows_its_status_and_applies() {
        let (mut f, bridge) = bridge_fixture();
        f.frame(vec![]);
        assert_eq!(f.work(), vec![Job::Check(0)]);
        // The binary row's check is the test's to answer; it is left waiting.
        f.frame(vec![]);
        f.frame(vec![]);
        assert_eq!(bridge.checks.load(Ordering::SeqCst), 1);
        assert_eq!(f.panel.labels[0], "Bridge");
        let package = f.panel.rows[0].package.as_ref().unwrap();
        assert!(matches!(package.status.state, PackageState::Ready { .. }));
        assert_eq!(package.status.current, Some("1.0.0".parse().unwrap()));
        let (rect, enabled) = f.panel.buttons[0].expect("Restart to update");
        assert!(enabled);

        f.click(rect.center());
        assert_eq!(f.work(), vec![Job::Apply(0)]);
        assert_eq!(bridge.applies.load(Ordering::SeqCst), 1);
        f.frame(vec![]);
        let package = f.panel.rows[0].package.as_ref().unwrap();
        assert!(!package.applying);
        assert_eq!(package.failure.as_deref(), Some("the bridge is busy"));
        // Retry checks again, and clears the failure.
        let (rect, _) = f.panel.buttons[0].expect("Retry");
        f.click(rect.center());
        assert_eq!(f.work(), vec![Job::Check(0)]);
        assert_eq!(bridge.checks.load(Ordering::SeqCst), 2);
        assert_eq!(bridge.applies.load(Ordering::SeqCst), 1);
        assert_eq!(f.panel.rows[0].package.as_ref().unwrap().failure, None);
    }

    #[test]
    fn a_delegated_row_is_asked_its_state_every_5_seconds() {
        let (mut f, bridge) = bridge_fixture();
        f.frame(vec![]);
        f.work();
        f.frame(vec![]);
        assert_eq!(f.sent(), vec![], "not again so soon");
        f.panel.last_poll = Some(Instant::now() - POLL_EVERY);
        f.frame(vec![]);
        assert_eq!(f.work(), vec![Job::Poll(0)]);
        assert_eq!(bridge.polls.load(Ordering::SeqCst), 2);
        // Not while it is still waiting for the last answer.
        f.panel.rows[0].package.as_mut().unwrap().asking = true;
        f.panel.last_poll = Some(Instant::now() - POLL_EVERY);
        f.frame(vec![]);
        assert_eq!(f.sent(), vec![]);
    }

    /// A panel whose first row is this app's package, its updater standing in `state`.
    #[cfg(feature = "package")]
    fn package_fixture(state: PackageState) -> Fixture {
        let (job_tx, jobs) = mpsc::channel();
        let (results, result_rx) = mpsc::channel();
        let targets = vec![
            Target::Package(crate::update::PackageTarget {
                label: "App".to_owned(),
                updater: corundum_framework_package::Updater::for_tests(state),
            }),
            target("Server", AfterUpdate::Manual).into(),
        ];
        Fixture {
            ctx: egui::Context::default(),
            panel: UpdatesPanel::with_worker(targets, job_tx, result_rx),
            jobs,
            results,
        }
    }

    #[cfg(feature = "package")]
    #[test]
    fn an_applied_package_of_this_app_closes_the_window() {
        let mut f = package_fixture(PackageState::Ready {
            version: "1.1.0".parse().unwrap(),
        });
        f.frame(vec![]);
        f.sent();
        let (rect, enabled) = f.panel.buttons[0].expect("Restart to update");
        assert!(enabled);
        f.click(rect.center());
        assert_eq!(f.work(), vec![Job::Apply(0)]);
        let output = f.frame(vec![]);
        assert!(Fixture::closes(&output));
        assert_eq!(f.panel.rows[0].package.as_ref().unwrap().failure, None);
    }

    #[cfg(feature = "package")]
    #[test]
    fn a_failed_package_apply_keeps_the_window() {
        let mut f = package_fixture(PackageState::Failed("stop failed".into()));
        f.frame(vec![]);
        f.sent();
        f.panel.apply(&f.ctx.clone(), 0);
        assert_eq!(f.work(), vec![Job::Apply(0)]);
        let output = f.frame(vec![]);
        assert!(!Fixture::closes(&output));
        let failure = f.panel.rows[0].package.as_ref().unwrap().failure.clone();
        assert!(failure.is_some_and(|f| f.contains("nothing downloaded")));
    }

    #[test]
    fn a_delegated_row_fails_when_the_worker_stops() {
        let (mut f, _bridge) = bridge_fixture();
        f.frame(vec![]);
        f.sent();
        f.results = mpsc::channel().0;
        f.frame(vec![]);
        let package = f.panel.rows[0].package.as_ref().unwrap();
        assert!(package.failure.is_some());
        assert!(!package.asking);
    }

    #[cfg(feature = "self-update")]
    mod self_update {
        use super::*;

        #[test]
        fn an_update_carries_the_set_version() {
            let mut f = fixture();
            f.frame(vec![]);
            f.sent();
            f.panel.set_current(0, Some("1.0.5".parse().unwrap()));
            f.panel.update(&f.ctx.clone(), 0, release("1.1.0"));
            assert_eq!(
                f.sent(),
                vec![Job::Update(
                    0,
                    release("1.1.0"),
                    Some("1.0.5".parse().unwrap())
                )]
            );
        }

        #[test]
        fn a_newer_release_is_offered_and_installed_alone() {
            let mut f = fixture();
            f.frame(vec![]);
            f.sent();
            f.results.send((0, newer("1.1.0"))).unwrap();
            f.results.send((1, newer("1.1.0"))).unwrap();
            f.frame(vec![]);
            f.frame(vec![]);
            let (rect, enabled) = f.panel.buttons[0].expect("row 0 has a button");
            assert!(enabled);
            assert!(f.panel.buttons[1].is_some_and(|(_, enabled)| enabled));

            f.click(rect.center());
            assert_eq!(
                f.sent(),
                vec![Job::Update(
                    0,
                    release("1.1.0"),
                    Some("1.0.0".parse().unwrap())
                )]
            );
            assert!(
                f.panel.buttons[0].is_none(),
                "the working row has no button"
            );
            assert!(
                f.panel.buttons[1].is_some_and(|(_, enabled)| !enabled),
                "the other rows wait"
            );
        }
    }
}
