//! [`ReadyBadge`]: the status-bar icon that says an update is downloaded and restarts into it.

use std::sync::{Arc, Mutex};

use corundum_framework_package::{After, Updater};
use corundum_framework_update::PackageState;

use crate::{icons, tr};

/// A status-bar icon shown only once the app's update is downloaded and ready; a click
/// restarts the app into it. Put it in the shell's status bar, hold it there, and call
/// [`ui`](Self::ui) every frame: it draws nothing in any other state, so the bar stays
/// quiet until there is something to do.
///
/// The restart runs [`Updater::apply`] on a thread of its own, so the window does not
/// freeze while the app's `before_apply` stops its background processes. On success the
/// process exits; if the apply returns, its error is the icon's tooltip.
pub struct ReadyBadge {
    updater: Updater,
    /// Why the last click did nothing; written by the apply thread.
    error: Arc<Mutex<Option<String>>>,
    /// Whether an apply is running, so a second click waits for it.
    applying: Arc<Mutex<bool>>,
    /// Whether the badge was drawn on the last frame, for the tests.
    #[cfg(test)]
    shown: bool,
}

impl ReadyBadge {
    /// The badge for `updater`, the one the app's [`Target`](super::Target) reads.
    pub fn new(updater: Updater) -> ReadyBadge {
        ReadyBadge {
            updater,
            error: Arc::new(Mutex::new(None)),
            applying: Arc::new(Mutex::new(false)),
            #[cfg(test)]
            shown: false,
        }
    }

    /// Render the icon when an update is ready, nothing otherwise.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let PackageState::Ready { version } = self.updater.status().state else {
            #[cfg(test)]
            {
                self.shown = false;
            }
            return;
        };
        #[cfg(test)]
        {
            self.shown = true;
        }
        let error = self.error.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let hover = error
            .unwrap_or_else(|| tr!(ui, "corundum-update-badge", version = version.to_string()));
        let response = ui
            .add(icons::button(icons::download(), 14.0))
            .on_hover_text(hover);
        if response.clicked() {
            self.restart(ui.ctx());
        }
    }

    /// Apply the update on a thread; it exits the process unless it fails.
    fn restart(&self, ctx: &egui::Context) {
        {
            let mut applying = self.applying.lock().unwrap_or_else(|e| e.into_inner());
            if *applying {
                return;
            }
            *applying = true;
        }
        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let (updater, error, applying, ctx) = (
            self.updater.clone(),
            Arc::clone(&self.error),
            Arc::clone(&self.applying),
            ctx.clone(),
        );
        std::thread::spawn(move || {
            // Infallible on success: the process has exited, so this is the error.
            let Err(err) = updater.apply(After::RestartApp);
            *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(err.to_string());
            *applying.lock().unwrap_or_else(|e| e.into_inner()) = false;
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;
    use corundum_framework_package::Updater;
    use corundum_framework_update::PackageState;

    fn shown(state: PackageState) -> bool {
        let mut badge = ReadyBadge::new(Updater::for_tests(state));
        render([600.0, 400.0], |ui| badge.ui(ui));
        badge.shown
    }

    #[test]
    fn an_up_to_date_package_draws_no_badge() {
        assert!(!shown(PackageState::UpToDate));
    }

    #[test]
    fn a_downloading_package_draws_no_badge() {
        assert!(!shown(PackageState::Downloading { percent: 40 }));
    }

    #[test]
    fn a_ready_update_draws_the_badge() {
        assert!(shown(PackageState::Ready {
            version: "1.1.0".parse().unwrap()
        }));
    }
}
