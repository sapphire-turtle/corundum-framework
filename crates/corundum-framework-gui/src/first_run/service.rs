//! [`ServicePage`]: the first-run question whether to keep the app's services running in
//! the background.

use corundum_framework_platform::package;
use corundum_framework_service::ServiceSpec;

use super::{FirstRunPage, PageAction};
use crate::service::{Job, ServiceOps};
use crate::store;
use crate::tr;

/// The first-run page asking whether to keep the app's services running in the background.
///
/// The app gives it the services and the words: the question, which is the page's title,
/// and the reason, shown under it. **Yes** registers each service in order, on a thread,
/// with a spinner meanwhile, and moves on; a failure shows the error and offers **Try
/// again** or **Continue without it**. **No** registers nothing and moves on. Either answer
/// is kept in the [store] (`services_asked`). The footer's **Next** stays
/// disabled until the question is answered.
///
/// A build running from cargo's `target` directory registers nothing, as a service must not
/// point into it: the page says so and leaves **Next** to the footer.
pub struct ServicePage {
    services: Vec<fn() -> ServiceSpec>,
    question: String,
    reason: String,
    /// How the services are registered: [`ServiceOps::SYSTEM`], or stand-ins in the tests.
    ops: ServiceOps,
    /// Whether the app is a cargo build, which registers nothing.
    is_dev: bool,
    phase: Phase,
    /// The message ids of the buttons and notes last drawn, for the tests.
    #[cfg(test)]
    shown: Vec<&'static str>,
    /// The button the next frame clicks, as a person would, for the tests.
    #[cfg(test)]
    pressed: Option<&'static str>,
}

/// Where the page is in its question.
enum Phase {
    /// Not answered yet.
    Asking,
    /// **Yes** was pressed, and the services are being registered.
    Installing(Job<Result<(), String>>),
    /// A service could not be registered, for the reason given.
    Failed(String),
    /// Answered: the page may be left.
    Answered,
}

impl ServicePage {
    /// The page for `services`, each a function building its spec, asking `question` (the
    /// page's title) for `reason` (the text under it).
    pub fn new(
        services: Vec<fn() -> ServiceSpec>,
        question: String,
        reason: String,
    ) -> ServicePage {
        let mut page = ServicePage::with_ops(services, question, reason, ServiceOps::SYSTEM);
        page.is_dev = package::current_exe().is_some_and(|exe| package::is_cargo_build(&exe));
        page
    }

    /// The page for `services`, registering them through `ops` instead of this machine's
    /// service manager.
    pub(crate) fn with_ops(
        services: Vec<fn() -> ServiceSpec>,
        question: String,
        reason: String,
        ops: ServiceOps,
    ) -> ServicePage {
        ServicePage {
            services,
            question,
            reason,
            ops,
            is_dev: false,
            phase: Phase::Asking,
            #[cfg(test)]
            shown: Vec::new(),
            #[cfg(test)]
            pressed: None,
        }
    }

    /// Register each service in order on a thread, stopping at the first that fails.
    fn install(&mut self, ctx: &egui::Context) {
        let services = self.services.clone();
        let install = self.ops.install;
        let named = services.len() > 1;
        self.phase = Phase::Installing(Job::spawn(ctx, "service install", move || {
            for service in services {
                let spec = service();
                install(&spec).map_err(|err| {
                    // With more than one service, say which failed.
                    if named {
                        format!("{}: {err}", spec.app_name)
                    } else {
                        err
                    }
                })?;
            }
            Ok(())
        }));
    }

    /// Keep the answer, and leave the page.
    fn answer(&mut self, ctx: &egui::Context, yes: bool) -> PageAction {
        store::update(ctx, |stored| stored.services_asked = Some(yes));
        self.phase = Phase::Answered;
        PageAction::Next
    }

    /// Draw the button `id`; whether it was clicked.
    fn button(&mut self, ui: &mut egui::Ui, id: &'static str) -> bool {
        let clicked = ui.button(tr!(ui, id)).clicked();
        #[cfg(test)]
        {
            self.shown.push(id);
            if self.pressed == Some(id) {
                self.pressed = None;
                return true;
            }
        }
        clicked
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

impl FirstRunPage for ServicePage {
    fn title(&self, _ctx: &egui::Context) -> String {
        self.question.clone()
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> PageAction {
        #[cfg(test)]
        self.shown.clear();
        ui.label(&self.reason);
        ui.add_space(12.0);
        if self.is_dev {
            self.noted("corundum-service-dev-build");
            ui.label(tr!(ui, "corundum-service-dev-build"));
            return PageAction::Stay;
        }
        if let Phase::Installing(job) = &self.phase
            && let Some(answer) = job.poll()
        {
            match answer.and_then(|installed| installed) {
                Ok(()) => return self.answer(ui.ctx(), true),
                Err(err) => self.phase = Phase::Failed(err),
            }
        }
        match &self.phase {
            Phase::Installing(_) => {
                ui.spinner();
                PageAction::Stay
            }
            Phase::Failed(error) => {
                let error = error.clone();
                self.noted("corundum-service-failed");
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    tr!(ui, "corundum-service-failed", error = error.as_str()),
                );
                ui.add_space(8.0);
                let (mut again, mut without) = (false, false);
                ui.horizontal(|ui| {
                    again = self.button(ui, "corundum-service-try-again");
                    without = self.button(ui, "corundum-service-continue-without");
                });
                if again {
                    self.install(ui.ctx());
                } else if without {
                    return self.answer(ui.ctx(), false);
                }
                PageAction::Stay
            }
            Phase::Asking | Phase::Answered => {
                let (mut yes, mut no) = (false, false);
                ui.horizontal(|ui| {
                    yes = self.button(ui, "corundum-service-yes");
                    no = self.button(ui, "corundum-service-no");
                });
                if yes {
                    self.install(ui.ctx());
                } else if no {
                    return self.answer(ui.ctx(), false);
                }
                PageAction::Stay
            }
        }
    }

    fn can_continue(&self) -> bool {
        self.is_dev || matches!(self.phase, Phase::Answered)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use corundum_framework_service::{ServiceSpec, ServiceState};

    use super::*;
    use crate::service::ServiceOps;
    use crate::store;
    use crate::test_support::render_in;

    fn first() -> ServiceSpec {
        spec("first")
    }

    fn second() -> ServiceSpec {
        spec("second")
    }

    fn spec(app_name: &'static str) -> ServiceSpec {
        ServiceSpec {
            app_name,
            description: app_name.into(),
            args: Vec::new(),
            legacy_labels: Vec::new(),
            post_install: None,
        }
    }

    fn done(_: &ServiceSpec) -> Result<(), String> {
        Ok(())
    }

    /// Ops that install with `install` and do nothing else.
    fn ops(install: fn(&ServiceSpec) -> Result<(), String>) -> ServiceOps {
        ServiceOps {
            state: |_| Ok(ServiceState::NotInstalled),
            install,
            uninstall: done,
            stop: done,
            start: done,
        }
    }

    fn page(install: fn(&ServiceSpec) -> Result<(), String>, is_dev: bool) -> ServicePage {
        let mut page = ServicePage::with_ops(
            vec![first, second],
            "Keep it running?".into(),
            "So it syncs.".into(),
            ops(install),
        );
        page.is_dev = is_dev;
        page
    }

    /// Render frames until the page asks for something other than staying, or `until`
    /// holds, for up to five seconds; what the page asked for last.
    fn settle(
        page: &mut ServicePage,
        ctx: &egui::Context,
        until: impl Fn(&ServicePage) -> bool,
    ) -> PageAction {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut action = PageAction::Stay;
            render_in(ctx, [600.0, 400.0], |ui| {
                let asked = page.ui(ui);
                if asked != PageAction::Stay {
                    action = asked;
                }
            });
            if action != PageAction::Stay || until(page) || Instant::now() > deadline {
                return action;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn yes_installs_each_service_and_moves_on() {
        static INSTALLED: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
        let ctx = egui::Context::default();
        let mut page = page(
            |spec| {
                INSTALLED.lock().unwrap().push(spec.app_name);
                Ok(())
            },
            false,
        );
        settle(&mut page, &ctx, |_| true);
        assert!(page.shown.contains(&"corundum-service-yes"));
        page.press("corundum-service-yes");
        assert_eq!(settle(&mut page, &ctx, |_| false), PageAction::Next);
        assert_eq!(*INSTALLED.lock().unwrap(), ["first", "second"]);
        assert_eq!(store::state(&ctx).services_asked, Some(true));
        assert!(page.can_continue());
    }

    #[test]
    fn no_registers_nothing_and_moves_on() {
        static INSTALLS: AtomicUsize = AtomicUsize::new(0);
        let ctx = egui::Context::default();
        let mut page = page(
            |_| {
                INSTALLS.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            false,
        );
        page.press("corundum-service-no");
        assert_eq!(settle(&mut page, &ctx, |_| false), PageAction::Next);
        assert_eq!(INSTALLS.load(Ordering::SeqCst), 0);
        assert_eq!(store::state(&ctx).services_asked, Some(false));
    }

    #[test]
    fn a_failed_install_offers_try_again_and_continue() {
        static INSTALLS: AtomicUsize = AtomicUsize::new(0);
        let ctx = egui::Context::default();
        let mut page = page(
            |_| {
                INSTALLS.fetch_add(1, Ordering::SeqCst);
                Err("refused".into())
            },
            false,
        );
        page.press("corundum-service-yes");
        let offered = |page: &ServicePage| page.shown.contains(&"corundum-service-try-again");
        assert_eq!(settle(&mut page, &ctx, offered), PageAction::Stay);
        assert!(page.shown.contains(&"corundum-service-failed"));
        assert!(page.shown.contains(&"corundum-service-continue-without"));
        // The first service failed, so the second was not tried.
        assert_eq!(INSTALLS.load(Ordering::SeqCst), 1);
        assert!(!page.can_continue());

        page.press("corundum-service-try-again");
        settle(&mut page, &ctx, |page| {
            INSTALLS.load(Ordering::SeqCst) == 2 && offered(page)
        });
        assert_eq!(INSTALLS.load(Ordering::SeqCst), 2);
        assert!(offered(&page));

        page.press("corundum-service-continue-without");
        assert_eq!(settle(&mut page, &ctx, |_| false), PageAction::Next);
        assert_eq!(store::state(&ctx).services_asked, Some(false));
    }

    #[test]
    fn it_cannot_continue_until_answered() {
        let ctx = egui::Context::default();
        let mut page = page(done, false);
        settle(&mut page, &ctx, |_| true);
        assert!(!page.can_continue());
        page.press("corundum-service-no");
        settle(&mut page, &ctx, |_| false);
        assert!(page.can_continue());
    }

    #[test]
    fn a_cargo_build_registers_nothing() {
        static INSTALLS: AtomicUsize = AtomicUsize::new(0);
        let ctx = egui::Context::default();
        let mut page = page(
            |_| {
                INSTALLS.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
            true,
        );
        assert_eq!(settle(&mut page, &ctx, |_| true), PageAction::Stay);
        assert_eq!(page.shown, ["corundum-service-dev-build"]);
        assert!(page.can_continue());
        // Its buttons are not drawn, so a click on one does nothing.
        page.press("corundum-service-yes");
        settle(&mut page, &ctx, |_| true);
        assert_eq!(INSTALLS.load(Ordering::SeqCst), 0);
        assert_eq!(store::state(&ctx).services_asked, None);
    }
}
