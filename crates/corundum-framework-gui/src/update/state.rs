//! One row's state, and how checking and installing move it.

use corundum_framework_update::{InstallMethod, Release, ReleaseSource, advice, asset_name};
use semver::Version;

/// What an update row shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowState {
    /// Asking for the latest release.
    Checking,
    /// Running the latest release, or newer.
    UpToDate {
        /// The latest published version.
        latest: Version,
    },
    /// A newer release this build can install (`self-update` only): offer it.
    Newer {
        /// The release.
        release: Release,
    },
    /// A newer release that is not installed from here: installed by cargo, binstall, a
    /// package manager or a store; or without a file for this platform.
    NotHere {
        /// The release.
        release: Release,
        /// What to do about it.
        advice: String,
    },
    /// Installing.
    Working {
        /// The release.
        release: Release,
        /// What is being done.
        step: Step,
    },
    /// Installed and running again.
    Done {
        /// The installed version.
        version: Version,
    },
    /// Installed; the user restarts it.
    RestartByHand {
        /// The installed version.
        version: Version,
    },
    /// Something went wrong.
    Failed {
        /// What.
        message: String,
        /// What the retry button does.
        retry: Retry,
    },
}

/// What an installing row is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Fetching and replacing the file.
    Downloading,
    /// Restarting the program.
    Restarting,
}

/// What retrying a failed row does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Retry {
    /// Look for the release again.
    Check,
    /// Install this release again.
    Update(Release),
}

/// How a restart went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestartOutcome {
    /// It runs again.
    Restarted,
    /// It cannot be restarted from here.
    ByHand,
    /// Restarting failed.
    Failed(String),
}

/// What happens to a row after a check began.
#[derive(Clone, Debug)]
pub(crate) enum Event {
    /// The lookup finished.
    Checked(Result<Release, String>),
    /// The download and replacement finished.
    Downloaded(Result<(), String>),
    /// The restart finished.
    Restarted(RestartOutcome),
}

/// The row after the lookup. `current` is the running version, `None` when it is unknown;
/// `has_asset` whether the release holds this platform's file (`asset_name`);
/// `can_install` whether this build has `self-update`.
pub(crate) fn on_check(
    current: Option<&Version>,
    method: &InstallMethod,
    has_asset: bool,
    can_install: bool,
    source: &ReleaseSource,
    found: Result<Release, String>,
) -> RowState {
    let release = match found {
        Ok(release) => release,
        Err(message) => {
            return RowState::Failed {
                message,
                retry: Retry::Check,
            };
        }
    };
    // Never a downgrade, nor a reinstall of the same version.
    if current.is_some_and(|current| release.version <= *current) {
        return RowState::UpToDate {
            latest: release.version,
        };
    }
    if *method == InstallMethod::Release && can_install && has_asset {
        return RowState::Newer { release };
    }
    // `advice` names the release page for a hand-placed executable, which is also the way
    // when the build cannot install or the release has no file for this platform.
    let advice = advice(source, &release, method);
    RowState::NotHere { release, advice }
}

/// The row after `event`. An event that does not fit the state leaves it as it is.
pub(crate) fn on_event(state: RowState, event: Event) -> RowState {
    match (state, event) {
        (
            RowState::Working {
                release,
                step: Step::Downloading,
            },
            Event::Downloaded(result),
        ) => match result {
            Ok(()) => RowState::Working {
                release,
                step: Step::Restarting,
            },
            Err(message) => RowState::Failed {
                message,
                retry: Retry::Update(release),
            },
        },
        (
            RowState::Working {
                release,
                step: Step::Restarting,
            },
            Event::Restarted(outcome),
        ) => match outcome {
            RestartOutcome::Restarted => RowState::Done {
                version: release.version,
            },
            RestartOutcome::ByHand => RowState::RestartByHand {
                version: release.version,
            },
            RestartOutcome::Failed(message) => RowState::Failed {
                message,
                retry: Retry::Check,
            },
        },
        (state, _) => state,
    }
}

/// Whether `release` holds this platform's file, named `asset` plus the platform.
pub(crate) fn has_asset(release: &Release, base: &str) -> bool {
    let name = asset_name(base);
    release.assets.iter().any(|asset| asset.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use corundum_framework_update::{Asset, BinstallSource, StoreKind};

    fn v(s: &str) -> Version {
        s.parse().unwrap()
    }

    fn release(version: &str) -> Release {
        Release {
            version: v(version),
            url: "https://example.test/r".to_owned(),
            assets: vec![Asset {
                name: asset_name("app"),
                download_url: "https://example.test/a".to_owned(),
            }],
        }
    }

    fn source() -> ReleaseSource {
        ReleaseSource::github("o", "r").cargo_crate("app-crate")
    }

    fn check(
        current: Option<&str>,
        method: InstallMethod,
        has_asset: bool,
        can_install: bool,
        found: Result<Release, String>,
    ) -> RowState {
        let current = current.map(v);
        on_check(
            current.as_ref(),
            &method,
            has_asset,
            can_install,
            &source(),
            found,
        )
    }

    #[test]
    fn newer_release_is_offered() {
        let state = check(
            Some("1.0.0"),
            InstallMethod::Release,
            true,
            true,
            Ok(release("1.1.0")),
        );
        assert_eq!(
            state,
            RowState::Newer {
                release: release("1.1.0")
            }
        );
    }

    #[test]
    fn same_or_older_is_up_to_date() {
        for latest in ["1.0.0", "0.9.0"] {
            let state = check(
                Some("1.0.0"),
                InstallMethod::Release,
                true,
                true,
                Ok(release(latest)),
            );
            assert_eq!(state, RowState::UpToDate { latest: v(latest) });
        }
    }

    #[test]
    fn unknown_current_offers_the_release() {
        let state = check(
            None,
            InstallMethod::Release,
            true,
            true,
            Ok(release("1.0.0")),
        );
        assert!(matches!(state, RowState::Newer { .. }));
    }

    #[test]
    fn other_install_methods_get_advice() {
        let git = InstallMethod::Binstall {
            crate_name: "app-crate".to_owned(),
            source: BinstallSource::Git("https://github.com/o/r".to_owned()),
        };
        for method in [
            InstallMethod::Cargo,
            git,
            InstallMethod::Store(StoreKind::Flatpak),
        ] {
            let r = release("1.1.0");
            let want = advice(&source(), &r, &method);
            let state = check(Some("1.0.0"), method, true, true, Ok(r.clone()));
            assert_eq!(
                state,
                RowState::NotHere {
                    release: r,
                    advice: want
                }
            );
        }
        let git = InstallMethod::Binstall {
            crate_name: "app-crate".to_owned(),
            source: BinstallSource::Git("https://github.com/o/r".to_owned()),
        };
        let RowState::NotHere { advice, .. } =
            check(Some("1.0.0"), git, true, true, Ok(release("1.1.0")))
        else {
            panic!("not NotHere");
        };
        assert!(advice.contains("cargo binstall --git https://github.com/o/r app-crate"));
    }

    #[test]
    fn missing_asset_names_the_release_page() {
        let state = check(
            Some("1.0.0"),
            InstallMethod::Release,
            false,
            true,
            Ok(release("1.1.0")),
        );
        let RowState::NotHere { advice, .. } = state else {
            panic!("not NotHere");
        };
        assert!(advice.contains("https://example.test/r"));
    }

    #[test]
    fn without_self_update_says_to_download() {
        let state = check(
            Some("1.0.0"),
            InstallMethod::Release,
            true,
            false,
            Ok(release("1.1.0")),
        );
        let RowState::NotHere { advice, .. } = state else {
            panic!("not NotHere");
        };
        assert!(advice.contains("download it from https://example.test/r"));
    }

    #[test]
    fn failed_check_retries_the_check() {
        let state = check(
            Some("1.0.0"),
            InstallMethod::Release,
            true,
            true,
            Err("down".into()),
        );
        assert_eq!(
            state,
            RowState::Failed {
                message: "down".into(),
                retry: Retry::Check
            }
        );
    }

    fn working(step: Step) -> RowState {
        RowState::Working {
            release: release("1.1.0"),
            step,
        }
    }

    #[test]
    fn failed_download_retries_the_update() {
        let state = on_event(
            working(Step::Downloading),
            Event::Downloaded(Err("disk".into())),
        );
        assert_eq!(
            state,
            RowState::Failed {
                message: "disk".into(),
                retry: Retry::Update(release("1.1.0"))
            }
        );
    }

    #[test]
    fn download_then_restart() {
        let state = on_event(working(Step::Downloading), Event::Downloaded(Ok(())));
        assert_eq!(state, working(Step::Restarting));
    }

    #[test]
    fn restart_outcomes() {
        let done = on_event(
            working(Step::Restarting),
            Event::Restarted(RestartOutcome::Restarted),
        );
        assert_eq!(
            done,
            RowState::Done {
                version: v("1.1.0")
            }
        );
        let by_hand = on_event(
            working(Step::Restarting),
            Event::Restarted(RestartOutcome::ByHand),
        );
        assert_eq!(
            by_hand,
            RowState::RestartByHand {
                version: v("1.1.0")
            }
        );
        let failed = on_event(
            working(Step::Restarting),
            Event::Restarted(RestartOutcome::Failed("no".into())),
        );
        assert!(matches!(failed, RowState::Failed { ref message, .. } if message == "no"));
    }

    #[test]
    fn has_asset_matches_the_platform_file() {
        assert!(has_asset(&release("1.0.0"), "app"));
        assert!(!has_asset(&release("1.0.0"), "other"));
    }
}
