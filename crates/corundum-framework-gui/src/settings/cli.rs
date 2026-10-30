//! [`CliRow`]: whether the app's command line is on `PATH`, and the button that changes it.

use corundum_framework_package::cli_path::{self, CliSpec, CliStatus};

use crate::tr;

/// The About-page row for the app's command line. On macOS and Linux it says the command is
/// on `PATH` (**Remove**), is not (**Install**), or is a link gone stale (**Repair**), and
/// the click changes it; where another program already has the command's name, it says so
/// and offers nothing, as the install would never replace it. On Windows the install hook
/// owns the entry, so the row only shows its status. Hold it on the page and call
/// [`ui`](Self::ui) every frame; it draws nothing outside a package
/// ([`CliStatus::Unsupported`]).
///
/// On Linux it also says so when `~/.local/bin`, where the link goes, is not on the `PATH`
/// a terminal gets. That is asked of the user's login shell on a thread, and the note shows
/// only once the shell has said no: a shell that is slow to start says nothing.
pub struct CliRow {
    spec: CliSpec,
    /// How the command's state is found: [`cli_path::status`], or a stand-in in the tests.
    status_of: fn(&CliSpec) -> CliStatus,
    /// The state as last found; `None` until the first frame, and after a click.
    status: Option<CliStatus>,
    /// Whether `~/.local/bin` is on a terminal's `PATH`: `None` until the login shell has
    /// answered (and off Linux, where it is never asked), filled in by a thread.
    on_path: std::sync::Arc<std::sync::Mutex<Option<bool>>>,
    /// Whether the login shell has been asked.
    asked: bool,
    /// Why the last click did nothing, shown under the row.
    error: Option<String>,
    /// The message id of the button or note last shown, for the tests.
    #[cfg(test)]
    shown: Option<&'static str>,
    /// Whether the not-on-`PATH` note was drawn last frame, for the tests.
    #[cfg(test)]
    noted: bool,
}

/// What a click on the row's button does.
#[cfg(unix)]
#[derive(Clone, Copy)]
enum Act {
    Install,
    Remove,
}

impl CliRow {
    /// The row for `spec`, the command line the app's package puts on `PATH`.
    pub fn new(spec: CliSpec) -> CliRow {
        CliRow::with_status(spec, cli_path::status)
    }

    /// The row for `spec`, finding the command's state with `status_of` instead of looking
    /// on the host.
    pub(crate) fn with_status(spec: CliSpec, status_of: fn(&CliSpec) -> CliStatus) -> CliRow {
        CliRow {
            spec,
            status_of,
            status: None,
            on_path: Default::default(),
            asked: false,
            error: None,
            #[cfg(test)]
            shown: None,
            #[cfg(test)]
            noted: false,
        }
    }

    /// Render the row: its label, and the button or note its state calls for.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if self.status.is_none() {
            self.status = Some((self.status_of)(&self.spec));
        }
        if !self.asked {
            self.asked = true;
            // Only Linux's `~/.local/bin` is left off some terminals' `PATH`.
            #[cfg(target_os = "linux")]
            self.ask_login_path(ui.ctx());
        }
        let status = self.status.unwrap_or(CliStatus::Unsupported);
        #[cfg(test)]
        {
            self.shown = None;
            self.noted = false;
        }
        if status == CliStatus::Unsupported {
            return;
        }
        #[cfg(unix)]
        let mut clicked = None;
        ui.horizontal(|ui| {
            ui.label(tr!(ui, "corundum-cli-row", command = self.spec.command));
            #[cfg(unix)]
            {
                let button = match status {
                    CliStatus::Missing => Some((Act::Install, "corundum-cli-install")),
                    CliStatus::Broken => Some((Act::Install, "corundum-cli-repair")),
                    CliStatus::Installed => Some((Act::Remove, "corundum-cli-remove")),
                    CliStatus::Taken | CliStatus::Unsupported => None,
                };
                if let Some((act, id)) = button {
                    #[cfg(test)]
                    {
                        self.shown = Some(id);
                    }
                    if ui.button(tr!(ui, id)).clicked() {
                        clicked = Some(act);
                    }
                }
            }
            #[cfg(windows)]
            {
                let id = match status {
                    CliStatus::Installed => "corundum-cli-status-installed",
                    CliStatus::Taken => "corundum-cli-taken",
                    _ => "corundum-cli-status-missing",
                };
                ui.label(tr!(ui, id, command = self.spec.command));
            }
            if status == CliStatus::Taken {
                #[cfg(test)]
                {
                    self.shown = Some("corundum-cli-taken");
                }
                #[cfg(unix)]
                ui.label(tr!(ui, "corundum-cli-taken", command = self.spec.command));
            }
        });
        let off_path = *self.on_path.lock().unwrap_or_else(|e| e.into_inner()) == Some(false);
        if off_path && status != CliStatus::Taken {
            #[cfg(test)]
            {
                self.noted = true;
            }
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr!(ui, "corundum-cli-not-on-path"),
            );
        }
        #[cfg(unix)]
        if let Some(act) = clicked {
            self.act(act);
        }
        if let Some(error) = &self.error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                tr!(ui, "corundum-cli-failed", error = error.as_str()),
            );
        }
    }

    /// Ask the login shell whether `~/.local/bin` is on a terminal's `PATH`, on a thread: a
    /// shell can take seconds to start. The answer repaints the window.
    #[cfg(target_os = "linux")]
    fn ask_login_path(&self, ctx: &egui::Context) {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let on_path = std::sync::Arc::clone(&self.on_path);
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("login PATH".to_owned())
            .spawn(move || {
                let answer =
                    cli_path::on_login_path(&std::path::Path::new(&home).join(".local/bin"));
                *on_path.lock().unwrap_or_else(|e| e.into_inner()) = answer;
                ctx.request_repaint();
            });
        if let Err(err) = spawned {
            tracing::warn!("cannot ask the login shell for its PATH: {err}");
        }
    }

    /// Install or remove the command, and look at it again next frame.
    #[cfg(unix)]
    fn act(&mut self, act: Act) {
        let done = match act {
            Act::Install => cli_path::install(&self.spec),
            Act::Remove => cli_path::uninstall(&self.spec),
        };
        self.error = done.err().map(|err| err.to_string());
        self.status = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    const SPEC: CliSpec = CliSpec { command: "demo" };

    fn shown(status_of: fn(&CliSpec) -> CliStatus) -> Option<&'static str> {
        let mut row = CliRow::with_status(SPEC, status_of);
        render([600.0, 400.0], |ui| row.ui(ui));
        row.shown
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_command_offers_install() {
        assert_eq!(shown(|_| CliStatus::Missing), Some("corundum-cli-install"));
    }

    #[cfg(unix)]
    #[test]
    fn an_installed_command_offers_remove() {
        assert_eq!(shown(|_| CliStatus::Installed), Some("corundum-cli-remove"));
    }

    #[cfg(unix)]
    #[test]
    fn a_broken_link_offers_repair() {
        assert_eq!(shown(|_| CliStatus::Broken), Some("corundum-cli-repair"));
    }

    #[cfg(unix)]
    #[test]
    fn a_taken_name_says_so_and_offers_no_install() {
        assert_eq!(shown(|_| CliStatus::Taken), Some("corundum-cli-taken"));
    }

    #[test]
    fn the_path_note_waits_for_the_shell_s_answer() {
        let mut row = CliRow::with_status(SPEC, |_| CliStatus::Missing);
        row.asked = true;
        assert!(!note_shown(&mut row));
        *row.on_path.lock().unwrap() = Some(true);
        assert!(!note_shown(&mut row));
        *row.on_path.lock().unwrap() = Some(false);
        assert!(note_shown(&mut row));
    }

    /// Whether the not-on-PATH note is drawn this frame.
    fn note_shown(row: &mut CliRow) -> bool {
        render([600.0, 400.0], |ui| row.ui(ui));
        row.noted
    }

    #[test]
    fn outside_a_package_the_row_is_not_drawn() {
        assert_eq!(shown(|_| CliStatus::Unsupported), None);
    }
}
