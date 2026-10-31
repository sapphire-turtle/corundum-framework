//! The demo's first-run flow, shown over the window until it is finished: a welcome, then
//! the question whether to keep the daemon running in the background
//! ([`ServicePage`]). Settings › About runs it again.

use corundum_framework::gui::egui;
use corundum_framework::gui::first_run::{FirstRun, FirstRunPage, PageAction, ServicePage};
use corundum_framework::gui::tr;

/// The flow over [`pages`], headed with the demo's name.
pub(crate) fn flow(ctx: &egui::Context) -> FirstRun {
    FirstRun::new("Corundum Demo", pages(ctx))
}

/// The flow's pages, in order: the welcome and the service question, in the language
/// `ctx` has now.
pub(crate) fn pages(ctx: &egui::Context) -> Vec<Box<dyn FirstRunPage>> {
    vec![
        Box::new(Welcome),
        Box::new(ServicePage::new(
            vec![crate::service::service],
            tr!(ctx, "demo-first-run-service-question"),
            tr!(ctx, "demo-first-run-service-reason"),
        )),
    ]
}

/// The first page: a few words on what the demo is.
struct Welcome;

impl FirstRunPage for Welcome {
    fn title(&self, ctx: &egui::Context) -> String {
        tr!(ctx, "demo-first-run-welcome")
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> PageAction {
        ui.label(tr!(ui, "demo-first-run-welcome-text"));
        PageAction::Stay
    }
}
