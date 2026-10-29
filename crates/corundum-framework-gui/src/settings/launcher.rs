//! [`LauncherRow`]: whether the app is in the OS's app menu, and the button that changes it.

use corundum_framework_platform::launcher::{self, LauncherSpec, Status};

use crate::tr;

/// The settings row that says the app is in the OS's app menu, with a button that takes it
/// out (**Remove**) or puts it back (**Add back**). Put it on one of the app's
/// [`Settings`](super::Settings) pages, its general one, say; hold it there, and call
/// [`ui`](Self::ui) every frame the page is shown.
///
/// The entry is the one [`WindowOptions::launcher`](crate::window::WindowOptions::launcher)
/// registers as the window opens. With the `serde` feature and a [store](crate::store), the
/// person's choice is kept there, so a removed entry is not written back on the next start;
/// without a store, a removal lasts only for the session.
/// Where there is no app menu to look in ([`Status::Unsupported`]: on iOS and Android, or
/// without a home directory, or for a build [`launcher::register`] skips) and in a Windows
/// or macOS package, whose installer owns the entry, the row draws nothing.
pub struct LauncherRow {
    spec: LauncherSpec,
    /// How the entry's state is found: [`launcher::status`], or a stand-in in the tests.
    status_of: fn(&LauncherSpec) -> Status,
    /// The entry's state as last found; `None` until the first frame, and after a click.
    status: Option<Status>,
    /// Why the last click did nothing, shown under the row.
    error: Option<String>,
    /// The message id of the button last shown, for the tests.
    #[cfg(test)]
    shown: Option<&'static str>,
}

/// What a click on the row's button does.
#[derive(Clone, Copy)]
enum Act {
    Remove,
    Add,
}

impl LauncherRow {
    /// The row for `spec`, the spec the app gives its window.
    pub fn new(spec: LauncherSpec) -> LauncherRow {
        LauncherRow::with_status(spec, launcher::status)
    }

    /// The row for `spec`, finding the entry's state with `status_of` instead of looking on
    /// the host.
    pub(crate) fn with_status(
        spec: LauncherSpec,
        status_of: fn(&LauncherSpec) -> Status,
    ) -> LauncherRow {
        LauncherRow {
            spec,
            status_of,
            status: None,
            error: None,
            #[cfg(test)]
            shown: None,
        }
    }

    /// Render the row: its label, and the button its state calls for.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let status = *self
            .status
            .get_or_insert_with(|| (self.status_of)(&self.spec));
        let (act, id) = match status {
            Status::Registered | Status::Stale => (Act::Remove, "corundum-launcher-remove"),
            Status::Absent => (Act::Add, "corundum-launcher-add"),
            Status::Unsupported => {
                #[cfg(test)]
                {
                    self.shown = None;
                }
                return;
            }
        };
        #[cfg(test)]
        {
            self.shown = Some(id);
        }
        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.label(tr!(ui, "corundum-launcher-in-menu"));
            if ui.button(tr!(ui, id)).clicked() {
                clicked = Some(act);
            }
        });
        if let Some(act) = clicked {
            self.act(ui.ctx(), act);
        }
        if let Some(error) = &self.error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr!(ui, "corundum-launcher-failed", error = error.as_str()),
            );
        }
    }

    /// Remove or add the entry, keep the choice, and look at the entry again next frame.
    #[cfg_attr(not(feature = "serde"), allow(unused_variables))]
    fn act(&mut self, ctx: &egui::Context, act: Act) {
        let done = match act {
            Act::Remove => launcher::unregister(&self.spec),
            Act::Add => launcher::register(&self.spec).map(|_| ()),
        };
        match done {
            Ok(()) => {
                self.error = None;
                #[cfg(feature = "serde")]
                crate::store::update(ctx, |s| {
                    s.launcher = Some(matches!(act, Act::Add));
                });
            }
            Err(err) => self.error = Some(err.to_string()),
        }
        self.status = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const SPEC: LauncherSpec = LauncherSpec {
        id: "net.example.Test",
        name: "Test",
        description: "A test app",
        category: "Utility",
        icon_svg: b"<svg/>",
        bundle_prefix: "net.example",
    };

    fn shown(status_of: fn(&LauncherSpec) -> Status) -> Option<&'static str> {
        let mut row = LauncherRow::with_status(SPEC, status_of);
        render([600.0, 400.0], |ui| row.ui(ui));
        row.shown
    }

    #[test]
    fn an_absent_entry_offers_add_back() {
        assert_eq!(shown(|_| Status::Absent), Some("corundum-launcher-add"));
    }

    #[test]
    fn a_registered_or_stale_entry_offers_remove() {
        assert_eq!(
            shown(|_| Status::Registered),
            Some("corundum-launcher-remove")
        );
        assert_eq!(shown(|_| Status::Stale), Some("corundum-launcher-remove"));
    }

    #[test]
    fn without_an_app_menu_there_is_no_button() {
        assert_eq!(shown(|_| Status::Unsupported), None);
    }

    #[test]
    fn without_an_app_menu_the_row_is_not_drawn() {
        let drawn = |status_of: fn(&LauncherSpec) -> Status| {
            let mut row = LauncherRow::with_status(SPEC, status_of);
            let mut moved = false;
            render([600.0, 400.0], |ui| {
                let before = ui.cursor().min;
                row.ui(ui);
                moved = ui.cursor().min != before;
            });
            moved
        };
        assert!(drawn(|_| Status::Absent));
        assert!(!drawn(|_| Status::Unsupported));
    }

    #[test]
    fn the_status_is_looked_up_once() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let mut row = LauncherRow::with_status(SPEC, |_| {
            CALLS.fetch_add(1, Ordering::Relaxed);
            Status::Absent
        });
        render([600.0, 400.0], |ui| row.ui(ui));
        render([600.0, 400.0], |ui| row.ui(ui));
        assert_eq!(CALLS.load(Ordering::Relaxed), 1);
    }
}
