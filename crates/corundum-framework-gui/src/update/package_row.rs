//! What a package row shows for its updater's state: a pure function, so the table of
//! states is tested without egui.

use corundum_framework_update::{PackageState, PackageStatus};

/// A package row's text, button, progress bar and spinner.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowView {
    /// The message id, and its arguments.
    pub text: (&'static str, Vec<(&'static str, String)>),
    pub button: Option<RowButton>,
    /// The download's progress, from 0 to 1.
    pub progress: Option<f32>,
    pub spinner: bool,
}

/// What a package row's button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowButton {
    /// Apply the downloaded update; the app (or the delegated package) starts again.
    RestartToUpdate,
    /// Check again.
    Retry,
}

/// The row for `status`.
pub(crate) fn package_row(status: &PackageStatus) -> RowView {
    let current = status.current.as_ref().map(|version| version.to_string());
    let text = |id, args: Vec<_>| RowView {
        text: (id, args),
        button: None,
        progress: None,
        spinner: false,
    };
    match &status.state {
        PackageState::Idle => match current {
            Some(version) => text(
                "corundum-update-package-current",
                vec![("version", version)],
            ),
            None => text("corundum-update-not-running", vec![]),
        },
        PackageState::Checking => RowView {
            spinner: true,
            ..text("corundum-update-checking", vec![])
        },
        PackageState::UpToDate => match current {
            Some(version) => text(
                "corundum-update-package-uptodate",
                vec![("version", version)],
            ),
            None => text("corundum-update-up-to-date", vec![]),
        },
        PackageState::Downloading { percent } => RowView {
            progress: Some(f32::from(*percent) / 100.0),
            ..text("corundum-update-package-downloading", vec![])
        },
        PackageState::Ready { version } => RowView {
            button: Some(RowButton::RestartToUpdate),
            ..text(
                "corundum-update-package-ready",
                vec![("version", version.to_string())],
            )
        },
        PackageState::Failed(error) => RowView {
            button: Some(RowButton::Retry),
            ..text("corundum-update-failed", vec![("error", error.clone())])
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(current: Option<&str>, state: PackageState) -> PackageStatus {
        PackageStatus {
            current: current.map(|v| v.parse().unwrap()),
            state,
        }
    }

    fn view(id: &'static str, args: &[(&'static str, &str)], button: Option<RowButton>) -> RowView {
        RowView {
            text: (
                id,
                args.iter().map(|(k, v)| (*k, (*v).to_owned())).collect(),
            ),
            button,
            progress: None,
            spinner: false,
        }
    }

    #[test]
    fn a_checking_package_spins_without_a_button() {
        let row = package_row(&status(Some("1.0.0"), PackageState::Checking));
        assert!(row.spinner);
        assert_eq!(row.button, None);
        assert_eq!(row.text.0, "corundum-update-checking");
    }

    #[test]
    fn an_up_to_date_package_names_its_version() {
        assert_eq!(
            package_row(&status(Some("1.0.0"), PackageState::UpToDate)),
            view(
                "corundum-update-package-uptodate",
                &[("version", "1.0.0")],
                None
            )
        );
    }

    #[test]
    fn a_download_shows_its_progress() {
        let row = package_row(&status(
            Some("1.0.0"),
            PackageState::Downloading { percent: 40 },
        ));
        assert_eq!(row.progress, Some(0.4));
        assert_eq!(row.button, None);
        assert!(!row.spinner);
    }

    #[test]
    fn a_ready_update_offers_a_restart() {
        let ready = PackageState::Ready {
            version: "1.1.0".parse().unwrap(),
        };
        assert_eq!(
            package_row(&status(Some("1.0.0"), ready)),
            view(
                "corundum-update-package-ready",
                &[("version", "1.1.0")],
                Some(RowButton::RestartToUpdate)
            )
        );
    }

    #[test]
    fn a_failure_shows_the_error_and_a_retry() {
        assert_eq!(
            package_row(&status(Some("1.0.0"), PackageState::Failed("x".into()))),
            view(
                "corundum-update-failed",
                &[("error", "x")],
                Some(RowButton::Retry)
            )
        );
    }

    #[test]
    fn an_idle_package_shows_its_current_version() {
        assert_eq!(
            package_row(&status(Some("1.0.0"), PackageState::Idle)),
            view(
                "corundum-update-package-current",
                &[("version", "1.0.0")],
                None
            )
        );
    }

    #[test]
    fn an_unknown_version_is_left_out() {
        assert_eq!(
            package_row(&status(None, PackageState::Idle)),
            view("corundum-update-not-running", &[], None)
        );
        assert_eq!(
            package_row(&status(None, PackageState::UpToDate)),
            view("corundum-update-up-to-date", &[], None)
        );
    }
}
