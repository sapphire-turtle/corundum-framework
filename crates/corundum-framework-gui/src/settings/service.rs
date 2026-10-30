//! [`ServiceRow`]: an About-page row for one of the app's background services.

use corundum_framework_service::{ServiceSpec, ServiceState};

use crate::service::{Job, ServiceOps};
use crate::tr;

/// Where **Open Login Items** takes the person: System Settings → General → Login Items.
const LOGIN_ITEMS: &str = "x-apple.systempreferences:com.apple.LoginItems-Settings.extension";

/// The About-page row for one of the app's background services: what it is doing, and the
/// buttons that change it. Not registered, it offers **Keep running in the background**;
/// running or stopped, it says so and offers **Stop** or **Start**, and **Remove**; waiting
/// for the person's approval (macOS), **Open Login Items**; broken, the reason and
/// **Repair**, which takes the registration away and makes it again.
///
/// The state is found on a thread, as is every change, after which the state is found
/// again: a frame never waits on the service manager. Hold the row on the page and call
/// [`ui`](Self::ui) every frame.
pub struct ServiceRow {
    service: fn() -> ServiceSpec,
    /// The service's name, as its spec gives it.
    name: &'static str,
    /// How the service is reached: [`ServiceOps::SYSTEM`], or stand-ins in the tests.
    ops: ServiceOps,
    /// The state as last found, or why it could not be; `None` while it is being found.
    state: Option<Result<ServiceState, String>>,
    /// The change or lookup running now.
    job: Option<Job<Looked>>,
    /// What the last click did not do, and why, shown under the row.
    error: Option<(Act, String)>,
    /// The message ids of the buttons and notes last drawn, for the tests.
    #[cfg(test)]
    shown: Vec<&'static str>,
    /// The button the next frame clicks, as a person would, for the tests.
    #[cfg(test)]
    pressed: Option<&'static str>,
}

/// What a click on one of the row's buttons does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Act {
    Install,
    Stop,
    Start,
    Remove,
    /// Take the registration away and make it again: making it over one still there is not
    /// something every manager accepts (launchd refuses to load a label already loaded).
    Repair,
    OpenLoginItems,
}

/// A job's answer: what the change did, if there was one, and the state after it.
struct Looked {
    acted: Option<(Act, Result<(), String>)>,
    state: Result<ServiceState, String>,
}

impl ServiceRow {
    /// The row for the service `service` builds the spec of.
    pub fn new(service: fn() -> ServiceSpec) -> ServiceRow {
        ServiceRow::with_ops(service, ServiceOps::SYSTEM)
    }

    /// The row for `service`, reaching it through `ops` instead of this machine's service
    /// manager.
    pub(crate) fn with_ops(service: fn() -> ServiceSpec, ops: ServiceOps) -> ServiceRow {
        ServiceRow {
            service,
            name: service().app_name,
            ops,
            state: None,
            job: None,
            error: None,
            #[cfg(test)]
            shown: Vec::new(),
            #[cfg(test)]
            pressed: None,
        }
    }

    /// Render the row: its label, the service's state, and the buttons the state calls for.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        #[cfg(test)]
        self.shown.clear();
        if self.state.is_none() && self.job.is_none() {
            self.run(ui.ctx(), None);
        }
        if let Some(answer) = self.job.as_ref().and_then(Job::poll) {
            self.job = None;
            match answer {
                Ok(looked) => {
                    self.error = looked
                        .acted
                        .and_then(|(act, done)| done.err().map(|err| (act, err)));
                    self.state = Some(looked.state);
                }
                Err(err) => self.state = Some(Err(err)),
            }
        }
        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.label(tr!(ui, "corundum-service-row", name = self.name));
            match self.state.clone() {
                None => {
                    ui.spinner();
                }
                Some(Ok(ServiceState::NotInstalled)) => {
                    clicked = clicked.or(self.button(ui, "corundum-service-run", Act::Install));
                }
                Some(Ok(state @ (ServiceState::Running | ServiceState::Stopped))) => {
                    let (id, act, act_id) = if state == ServiceState::Running {
                        (
                            "corundum-service-running",
                            Act::Stop,
                            "corundum-service-stop",
                        )
                    } else {
                        (
                            "corundum-service-stopped",
                            Act::Start,
                            "corundum-service-start",
                        )
                    };
                    self.note(ui, id);
                    clicked = clicked.or(self.button(ui, act_id, act));
                    clicked = clicked.or(self.button(ui, "corundum-service-remove", Act::Remove));
                }
                Some(Ok(ServiceState::NeedsApproval)) => {
                    self.note(ui, "corundum-service-needs-approval");
                    clicked = clicked.or(self.button(
                        ui,
                        "corundum-service-open-login-items",
                        Act::OpenLoginItems,
                    ));
                }
                Some(Ok(ServiceState::Broken { reason })) => {
                    self.noted("corundum-service-broken");
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        tr!(ui, "corundum-service-broken", reason = reason.as_str()),
                    );
                    clicked = clicked.or(self.button(ui, "corundum-service-repair", Act::Repair));
                }
                Some(Err(err)) => {
                    ui.colored_label(ui.visuals().error_fg_color, err);
                }
            }
        });
        if let Some(act) = clicked {
            self.run(ui.ctx(), Some(act));
        }
        if let Some((act, error)) = &self.error {
            let text = if matches!(act, Act::Install | Act::Repair) {
                tr!(ui, "corundum-service-failed", error = error.as_str())
            } else {
                error.clone()
            };
            ui.colored_label(ui.visuals().error_fg_color, text);
        }
    }

    /// Do `act`, if any, on a thread, and find the state after it.
    fn run(&mut self, ctx: &egui::Context, act: Option<Act>) {
        let (service, ops) = (self.service, self.ops);
        self.state = None;
        self.job = Some(Job::spawn(ctx, "service state", move || {
            let spec = service();
            let acted = act.map(|act| (act, perform(&ops, act, &spec)));
            Looked {
                acted,
                state: (ops.state)(&spec),
            }
        }));
    }

    /// Draw the button `id`; `act` when it was clicked.
    fn button(&mut self, ui: &mut egui::Ui, id: &'static str, act: Act) -> Option<Act> {
        let clicked = ui.button(tr!(ui, id)).clicked();
        #[cfg(test)]
        {
            self.shown.push(id);
            if self.pressed == Some(id) {
                self.pressed = None;
                return Some(act);
            }
        }
        clicked.then_some(act)
    }

    /// Draw the note `id`.
    fn note(&mut self, ui: &mut egui::Ui, id: &'static str) {
        self.noted(id);
        ui.label(tr!(ui, id));
    }

    /// Note that the note `id` was drawn, for the tests.
    fn noted(&mut self, _id: &'static str) {
        #[cfg(test)]
        self.shown.push(_id);
    }

    /// Click the button `id` on the next frame, as a person would.
    #[cfg(test)]
    fn press(&mut self, id: &'static str) {
        self.pressed = Some(id);
    }
}

/// Carry out `act` on the service `spec` describes.
fn perform(ops: &ServiceOps, act: Act, spec: &ServiceSpec) -> Result<(), String> {
    match act {
        Act::Install => (ops.install)(spec),
        Act::Stop => (ops.stop)(spec),
        Act::Start => (ops.start)(spec),
        Act::Remove => (ops.uninstall)(spec),
        Act::Repair => {
            (ops.uninstall)(spec)?;
            (ops.install)(spec)
        }
        Act::OpenLoginItems => open_login_items(),
    }
}

/// Open System Settings at Login Items, where the person approves the service. Only macOS
/// asks for that approval, so elsewhere there is nothing to open.
fn open_login_items() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg(LOGIN_ITEMS)
            .status()
            .map_err(|err| err.to_string())?;
        if !status.success() {
            return Err(format!("open {LOGIN_ITEMS}: {status}"));
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = LOGIN_ITEMS;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use corundum_framework_service::{ServiceSpec, ServiceState};

    use super::*;
    use crate::service::ServiceOps;
    use crate::test_support::render_in;

    fn demo() -> ServiceSpec {
        ServiceSpec {
            app_name: "demo",
            description: "Demo".into(),
            args: Vec::new(),
            legacy_labels: Vec::new(),
            post_install: None,
        }
    }

    fn done(_: &ServiceSpec) -> Result<(), String> {
        Ok(())
    }

    /// Ops whose state is `state` and whose actions succeed and do nothing.
    fn ops(state: fn(&ServiceSpec) -> Result<ServiceState, String>) -> ServiceOps {
        ServiceOps {
            state,
            install: done,
            uninstall: done,
            stop: done,
            start: done,
        }
    }

    /// Render frames until `until` holds, for up to five seconds.
    fn settle(row: &mut ServiceRow, ctx: &egui::Context, until: impl Fn(&ServiceRow) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            render_in(ctx, [600.0, 400.0], |ui| row.ui(ui));
            if until(row) || Instant::now() > deadline {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// The buttons and notes shown once `state` has answered.
    fn shown(state: fn(&ServiceSpec) -> Result<ServiceState, String>) -> Vec<&'static str> {
        let mut row = ServiceRow::with_ops(demo, ops(state));
        settle(&mut row, &egui::Context::default(), |row| {
            !row.shown.is_empty()
        });
        row.shown.clone()
    }

    #[test]
    fn a_service_not_installed_offers_to_run_it() {
        assert_eq!(
            shown(|_| Ok(ServiceState::NotInstalled)),
            ["corundum-service-run"]
        );
    }

    #[test]
    fn a_running_service_offers_stop_and_remove() {
        assert_eq!(
            shown(|_| Ok(ServiceState::Running)),
            [
                "corundum-service-running",
                "corundum-service-stop",
                "corundum-service-remove"
            ]
        );
    }

    #[test]
    fn a_stopped_service_offers_start_and_remove() {
        assert_eq!(
            shown(|_| Ok(ServiceState::Stopped)),
            [
                "corundum-service-stopped",
                "corundum-service-start",
                "corundum-service-remove"
            ]
        );
    }

    #[test]
    fn a_service_waiting_for_approval_offers_login_items() {
        assert_eq!(
            shown(|_| Ok(ServiceState::NeedsApproval)),
            [
                "corundum-service-needs-approval",
                "corundum-service-open-login-items"
            ]
        );
    }

    #[test]
    fn a_broken_service_offers_repair() {
        assert_eq!(
            shown(|_| Ok(ServiceState::Broken {
                reason: "gone".into()
            })),
            ["corundum-service-broken", "corundum-service-repair"]
        );
    }

    #[test]
    fn state_is_not_asked_on_the_ui_thread() {
        static RELEASED: AtomicBool = AtomicBool::new(false);
        fn blocking(_: &ServiceSpec) -> Result<ServiceState, String> {
            // Held until the test lets it go, for five seconds at most, so a row that asked
            // on the UI thread fails the timing below instead of hanging the test.
            let deadline = Instant::now() + Duration::from_secs(5);
            while !RELEASED.load(Ordering::SeqCst) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(ServiceState::Running)
        }
        let ctx = egui::Context::default();
        let mut row = ServiceRow::with_ops(demo, ops(blocking));
        let started = Instant::now();
        render_in(&ctx, [600.0, 400.0], |ui| row.ui(ui));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(row.shown.is_empty());
        RELEASED.store(true, Ordering::SeqCst);
        settle(&mut row, &ctx, |row| !row.shown.is_empty());
        assert_eq!(
            row.shown,
            [
                "corundum-service-running",
                "corundum-service-stop",
                "corundum-service-remove"
            ]
        );
    }

    #[test]
    fn repair_uninstalls_then_installs_and_looks_again() {
        static CALLS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
        fn record(call: &'static str) {
            CALLS.lock().unwrap().push(call);
        }
        let ops = ServiceOps {
            state: |_| {
                record("state");
                Ok(ServiceState::Broken {
                    reason: "gone".into(),
                })
            },
            install: |_| {
                record("install");
                Ok(())
            },
            uninstall: |_| {
                record("uninstall");
                Ok(())
            },
            stop: done,
            start: done,
        };
        let ctx = egui::Context::default();
        let mut row = ServiceRow::with_ops(demo, ops);
        settle(&mut row, &ctx, |row| !row.shown.is_empty());
        row.press("corundum-service-repair");
        settle(&mut row, &ctx, |_| CALLS.lock().unwrap().len() >= 4);
        assert_eq!(
            *CALLS.lock().unwrap(),
            ["state", "uninstall", "install", "state"]
        );
    }
}
