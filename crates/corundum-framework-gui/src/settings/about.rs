//! [`AboutRows`]: the rows an app's About page shows about how it is installed.

use corundum_framework_platform::launcher::LauncherSpec;

use super::LauncherRow;

/// The rows of an app's About page that are about its install, drawn in this order: the
/// package's updates, a row per background service, the command line on `PATH`, and the
/// app's entry in the OS's app menu. Each is there only when it is asked for; the app draws
/// its own name, version and licences around them.
///
/// Make it as the app is built, hold it on the About page, and call [`ui`](Self::ui) every
/// frame the page is shown. Each row draws nothing where it does not apply (a row for the
/// app menu in a Windows or macOS package, say), so the same rows go on every platform.
///
/// The app-menu row matters most in an AppImage: its window does not write an entry of its
/// own ([`WindowOptions::launcher`](crate::window::WindowOptions::launcher)), so without the
/// row the app gets no icon in the dock or the taskbar.
#[derive(Default)]
pub struct AboutRows {
    #[cfg(feature = "update")]
    updates: Option<Updates>,
    #[cfg(all(
        feature = "service",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    services: Vec<super::ServiceRow>,
    #[cfg(all(
        feature = "package",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    cli: Option<super::CliRow>,
    launcher: Option<LauncherRow>,
}

/// The update panel, or what makes it on the first frame.
#[cfg(feature = "update")]
enum Updates {
    Ready(crate::update::UpdatesPanel),
    Later(Box<dyn FnOnce() -> crate::update::UpdatesPanel>),
}

impl AboutRows {
    /// No rows: add them with the builder's methods.
    pub fn new() -> AboutRows {
        AboutRows::default()
    }

    /// The rows the window started from `ctx` asks for: the app-menu row for the spec it was
    /// given with [`WindowOptions::launcher`](crate::window::WindowOptions::launcher), if
    /// any. Add the others with the builder's methods.
    pub fn for_window(ctx: &egui::Context) -> AboutRows {
        AboutRows::new().launcher_opt(super::launcher::window_spec(ctx))
    }

    /// The row that puts the app in the OS's app menu as `spec`, or takes it out.
    pub fn launcher(self, spec: LauncherSpec) -> AboutRows {
        self.launcher_opt(Some(spec))
    }

    /// [`launcher`](Self::launcher) when `spec` is given; `None` takes the row away.
    pub fn launcher_opt(mut self, spec: Option<LauncherSpec>) -> AboutRows {
        self.launcher = spec.map(LauncherRow::new);
        self
    }

    /// The update panel, drawn first.
    #[cfg(feature = "update")]
    pub fn updates(mut self, panel: crate::update::UpdatesPanel) -> AboutRows {
        self.updates = Some(Updates::Ready(panel));
        self
    }

    /// The update panel, made by `make` on the first frame the rows are drawn: nothing looks
    /// for a release before the page is opened.
    #[cfg(feature = "update")]
    pub fn updates_later(
        mut self,
        make: impl FnOnce() -> crate::update::UpdatesPanel + 'static,
    ) -> AboutRows {
        self.updates = Some(Updates::Later(Box::new(make)));
        self
    }

    /// A row for the background service `service` describes; one per call, in order.
    #[cfg(all(
        feature = "service",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    pub fn service(
        mut self,
        service: fn() -> corundum_framework_service::ServiceSpec,
    ) -> AboutRows {
        self.services.push(super::ServiceRow::new(service));
        self
    }

    /// The row for the command line the package puts on `PATH`.
    #[cfg(all(
        feature = "package",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    pub fn cli(mut self, spec: corundum_framework_package::cli_path::CliSpec) -> AboutRows {
        self.cli = Some(super::CliRow::new(spec));
        self
    }

    /// Draw the rows.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        #[cfg(feature = "update")]
        {
            if matches!(self.updates, Some(Updates::Later(_)))
                && let Some(Updates::Later(make)) = self.updates.take()
            {
                self.updates = Some(Updates::Ready(make()));
            }
            if let Some(Updates::Ready(panel)) = &mut self.updates {
                panel.ui(ui);
                ui.add_space(8.0);
            }
        }
        #[cfg(all(
            feature = "service",
            not(any(target_os = "ios", target_os = "android"))
        ))]
        for service in &mut self.services {
            service.ui(ui);
        }
        #[cfg(all(
            feature = "package",
            not(any(target_os = "ios", target_os = "android"))
        ))]
        if let Some(cli) = &mut self.cli {
            cli.ui(ui);
        }
        if let Some(launcher) = &mut self.launcher {
            launcher.ui(ui);
        }
    }

    /// Whether the rows hold an app-menu row, for the tests.
    #[cfg(test)]
    pub(crate) fn has_launcher(&self) -> bool {
        self.launcher.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: LauncherSpec = LauncherSpec {
        id: "net.example.Test",
        name: "Test",
        description: "A test app",
        category: "Utility",
        icon_svg: b"<svg/>",
        bundle_prefix: "net.example",
    };

    #[test]
    fn the_app_menu_row_is_there_only_when_asked_for() {
        assert!(!AboutRows::new().has_launcher());
        assert!(AboutRows::new().launcher(SPEC).has_launcher());
        assert!(
            !AboutRows::new()
                .launcher(SPEC)
                .launcher_opt(None)
                .has_launcher()
        );
    }

    // The window keeps the spec it was given, so About need not be handed it again.
    #[test]
    fn for_window_takes_the_windows_spec() {
        let ctx = egui::Context::default();
        assert!(!AboutRows::for_window(&ctx).has_launcher());
        super::super::launcher::set_window_spec(&ctx, SPEC);
        assert!(AboutRows::for_window(&ctx).has_launcher());
    }

    #[cfg(feature = "update")]
    #[test]
    fn a_later_update_panel_is_made_on_the_first_frame_only() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static MADE: AtomicUsize = AtomicUsize::new(0);
        let mut rows = AboutRows::new().updates_later(|| {
            MADE.fetch_add(1, Ordering::SeqCst);
            crate::update::UpdatesPanel::new(Vec::new(), "test")
        });
        assert_eq!(MADE.load(Ordering::SeqCst), 0);
        crate::test_support::render([400.0, 300.0], |ui| rows.ui(ui));
        crate::test_support::render([400.0, 300.0], |ui| rows.ui(ui));
        assert_eq!(MADE.load(Ordering::SeqCst), 1);
    }
}
