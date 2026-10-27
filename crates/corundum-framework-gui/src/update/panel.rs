//! [`UpdatesPanel`]: the rows of Settings → About that check for releases and install them.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::TryRecvError;
#[cfg(test)]
use std::sync::mpsc::{Receiver, Sender};

use corundum_framework_update::{InstallMethod, Release, ReleaseSource, StoreKind, advice_command};
use semver::Version;

use super::state::{Event, RestartOutcome, Retry, RowState, Step, on_check, on_event};
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
/// each release is.
pub struct UpdatesPanel {
    targets: Arc<[UpdateTarget]>,
    rows: Vec<Row>,
    user_agent: String,
    /// Started on the first `ui`.
    worker: Option<Worker>,
    /// Whether the first check went out.
    checked: bool,
    /// Each row's button, and whether it was enabled, for the tests.
    #[cfg(test)]
    buttons: Vec<Option<(egui::Rect, bool)>>,
}

/// A row's state, and how its executable was installed once known.
struct Row {
    state: RowState,
    method: Option<InstallMethod>,
}

/// What a row's button asked for.
enum Action {
    /// Install the release.
    #[cfg(feature = "self-update")]
    Update(Release),
    /// Do what the failed row's retry does.
    Retry(Retry),
}

/// Whether this build installs releases.
const CAN_INSTALL: bool = cfg!(feature = "self-update");

impl UpdatesPanel {
    /// The rows for `targets`; GitHub is asked as `user_agent`.
    pub fn new(targets: Vec<UpdateTarget>, user_agent: &str) -> UpdatesPanel {
        let rows = targets
            .iter()
            .map(|_| Row {
                state: RowState::Checking,
                method: None,
            })
            .collect();
        UpdatesPanel {
            targets: targets.into(),
            rows,
            user_agent: user_agent.to_owned(),
            worker: None,
            checked: false,
            #[cfg(test)]
            buttons: Vec::new(),
        }
    }

    /// A panel whose worker is the other ends of `jobs` and `results`: for the tests.
    #[cfg(test)]
    pub(crate) fn with_worker(
        targets: Vec<UpdateTarget>,
        jobs: Sender<Job>,
        results: Receiver<(usize, Event)>,
    ) -> UpdatesPanel {
        let mut panel = UpdatesPanel::new(targets, "test");
        panel.worker = Some(Worker { jobs, results });
        panel
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
            self.check_all(ui.ctx());
        }
        self.receive(ui.ctx());

        if ui.button(tr!(ui, "corundum-update-check")).clicked() {
            self.check_all(ui.ctx());
        }
        ui.add_space(8.0);
        // One install at a time: while a row works, the others' buttons wait.
        let idle = !self
            .rows
            .iter()
            .any(|row| matches!(row.state, RowState::Working { .. }));
        let mut action = None;
        #[cfg(test)]
        self.buttons.clear();
        egui::Grid::new("corundum_update_rows")
            .num_columns(3)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                for (index, (target, row)) in self.targets.iter().zip(&self.rows).enumerate() {
                    ui.label(&target.label);
                    match &target.current {
                        Some(version) => ui.label(version.to_string()),
                        None => ui.weak(tr!(ui, "corundum-update-not-running")),
                    };
                    let (asked, _button) = ui.horizontal(|ui| row_ui(ui, target, row, idle)).inner;
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
            None => {}
        }
    }

    /// Look every row up again but one installing.
    fn check_all(&mut self, ctx: &egui::Context) {
        for index in 0..self.rows.len() {
            if !matches!(self.rows[index].state, RowState::Working { .. }) {
                self.check(ctx, index);
            }
        }
    }

    fn check(&mut self, ctx: &egui::Context, index: usize) {
        self.rows[index].state = RowState::Checking;
        self.send(ctx, index, Job::Check(index));
    }

    #[cfg(feature = "self-update")]
    fn update(&mut self, ctx: &egui::Context, index: usize, release: Release) {
        self.rows[index].state = RowState::Working {
            release: release.clone(),
            step: Step::Downloading,
        };
        self.send(ctx, index, Job::Update(index, release));
    }

    /// Hand `job` to the worker; if it is gone, `index`'s row says so.
    fn send(&mut self, ctx: &egui::Context, index: usize, job: Job) {
        let sent = self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.jobs.send(job).is_ok());
        if !sent {
            self.rows[index].state = stopped(ctx);
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
            match event {
                // Only a row waiting for it: a late answer to an earlier check must not undo an
                // install begun since.
                Event::Checked {
                    found,
                    method,
                    has_asset,
                } if matches!(row.state, RowState::Checking) => {
                    row.state = on_check(
                        target.current.as_ref(),
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
                if matches!(row.state, RowState::Checking | RowState::Working { .. }) {
                    row.state = stopped(ctx);
                }
            }
        }
    }
}

/// A row whose worker is gone.
fn stopped(ctx: &egui::Context) -> RowState {
    RowState::Failed {
        message: tr!(ctx, "corundum-update-worker-stopped"),
        retry: Retry::Check,
    }
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
    use crate::update::state::Event;
    use corundum_framework_update::{Asset, Release, asset_name};
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
            target("App", AfterUpdate::Relaunch),
            target("Server", AfterUpdate::Manual),
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

    #[cfg(feature = "self-update")]
    mod self_update {
        use super::*;

        impl Fixture {
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
            assert_eq!(f.sent(), vec![Job::Update(0, release("1.1.0"))]);
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
