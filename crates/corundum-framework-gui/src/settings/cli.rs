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
/// On Linux it also says so when `~/.local/bin`, where the link goes, is not on `PATH`.
pub struct CliRow {
    spec: CliSpec,
    /// How the command's state is found: [`cli_path::status`], or a stand-in in the tests.
    status_of: fn(&CliSpec) -> CliStatus,
    /// The state as last found; `None` until the first frame, and after a click.
    status: Option<CliStatus>,
    /// Whether `~/.local/bin` is on `PATH`, found with the status; always true off Linux.
    on_path: bool,
    /// Why the last click did nothing, shown under the row.
    error: Option<String>,
    /// The message id of the button or note last shown, for the tests.
    #[cfg(test)]
    shown: Option<&'static str>,
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
            on_path: true,
            error: None,
            #[cfg(test)]
            shown: None,
        }
    }

    /// Render the row: its label, and the button or note its state calls for.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        if self.status.is_none() {
            self.status = Some((self.status_of)(&self.spec));
            #[cfg(target_os = "linux")]
            {
                self.on_path = std::env::var_os("HOME")
                    .map(|home| {
                        cli_path::on_login_path(&std::path::Path::new(&home).join(".local/bin"))
                    })
                    .unwrap_or(true);
            }
        }
        let status = self.status.unwrap_or(CliStatus::Unsupported);
        #[cfg(test)]
        {
            self.shown = None;
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
        if cfg!(target_os = "linux") && !self.on_path && status != CliStatus::Taken {
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
    fn outside_a_package_the_row_is_not_drawn() {
        assert_eq!(shown(|_| CliStatus::Unsupported), None);
    }
}
