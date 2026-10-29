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

    /// Render the icon when an update is ready, or when the last click failed, nothing
    /// otherwise.
    ///
    /// A refused `before_apply` leaves the updater `Failed`, so the icon stays for as long
    /// as it holds its click's error: shown in the warning colour with the error as its
    /// hover text, and a click tries again.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let state = self.updater.status().state;
        let ready = match &state {
            PackageState::Ready { version } => Some(version.to_string()),
            _ => None,
        };
        let mut error = self.error.lock().unwrap_or_else(|e| e.into_inner());
        // Checking again, or a newer state, ends the failure the icon was showing.
        if ready.is_none()
            && !matches!(state, PackageState::Failed(_))
            && !*self.applying.lock().unwrap_or_else(|e| e.into_inner())
        {
            *error = None;
        }
        let error = error.clone();
        let shown = ready.is_some() || error.is_some();
        #[cfg(test)]
        {
            self.shown = shown;
        }
        if !shown {
            return;
        }
        let failed = error.is_some();
        let hover = error.unwrap_or_else(|| {
            tr!(
                ui,
                "corundum-update-badge",
                version = ready.unwrap_or_default()
            )
        });
        let response = ui
            .scope(|ui| {
                if failed {
                    ui.visuals_mut().override_text_color = Some(ui.visuals().warn_fg_color);
                }
                ui.add(icons::button(icons::download(), 14.0))
            })
            .inner
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
            // Put the flag back even if the apply panics.
            let _reset = ResetOnDrop(applying, ctx.clone());
            // Infallible on success: the process has exited, so this is the error.
            let Err(err) = updater.apply(After::RestartApp);
            *error.lock().unwrap_or_else(|e| e.into_inner()) = Some(err.to_string());
        });
    }
}

/// Clears the badge's `applying` flag and asks for a repaint when the apply thread ends,
/// however it ends.
struct ResetOnDrop(Arc<Mutex<bool>>, egui::Context);

impl Drop for ResetOnDrop {
    fn drop(&mut self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.1.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

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

    /// A badge whose click failed: the updater is `Failed`, as after a refused
    /// `before_apply`, and has nothing downloaded, so the apply returns an error.
    fn clicked_failing() -> ReadyBadge {
        let badge = ReadyBadge::new(Updater::for_tests(PackageState::Failed("stop".into())));
        let ctx = egui::Context::default();
        badge.restart(&ctx);
        while *badge.applying.lock().unwrap() {
            std::thread::yield_now();
        }
        badge
    }

    #[test]
    fn a_failed_restart_keeps_the_badge_with_its_error() {
        let mut badge = clicked_failing();
        render([600.0, 400.0], |ui| badge.ui(ui));
        assert!(badge.shown);
        let error = badge.error.lock().unwrap().clone();
        assert!(error.is_some_and(|e| e.contains("nothing downloaded")));
    }

    #[test]
    fn a_new_check_ends_the_failure_the_badge_showed() {
        let badge = clicked_failing();
        let mut ready = ReadyBadge::new(Updater::for_tests(PackageState::Checking));
        ready.error = Arc::clone(&badge.error);
        render([600.0, 400.0], |ui| ready.ui(ui));
        assert!(!ready.shown);
        assert!(badge.error.lock().unwrap().is_none());
    }

    #[test]
    fn a_ready_badge_is_an_enabled_button() {
        let mut badge = ReadyBadge::new(Updater::for_tests(PackageState::Ready {
            version: "1.1.0".parse().unwrap(),
        }));
        let mut enabled = false;
        render([600.0, 400.0], |ui| {
            badge.ui(ui);
            enabled = ui.is_enabled() && badge.shown;
        });
        assert!(enabled);
    }
}
