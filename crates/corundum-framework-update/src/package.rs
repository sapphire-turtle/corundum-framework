//! The state of a package this app updates itself, shared by the crates that show or drive it.

use serde::{Deserialize, Serialize};

/// Where a self-updating package stands: the running version and what the updater is doing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageStatus {
    /// The running version, `None` when it is unknown.
    pub current: Option<semver::Version>,
    /// What the updater is doing.
    pub state: PackageState,
}

/// What a package's updater is doing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PackageState {
    /// Nothing asked yet.
    Idle,
    /// Looking for a newer version.
    Checking,
    /// The running version is the newest.
    UpToDate,
    /// Fetching a newer version.
    Downloading {
        /// How much is fetched, 0 to 100.
        percent: u8,
    },
    /// A newer version is staged; restarting applies it.
    Ready {
        /// The staged version.
        version: semver::Version,
    },
    /// The last check or download failed, with the reason.
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_round_trips_through_json() {
        for state in [
            PackageState::Idle,
            PackageState::Downloading { percent: 40 },
            PackageState::Ready {
                version: semver::Version::new(1, 2, 3),
            },
            PackageState::Failed("offline".to_owned()),
        ] {
            let status = PackageStatus {
                current: Some(semver::Version::new(1, 0, 0)),
                state,
            };
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(
                serde_json::from_str::<PackageStatus>(&json).unwrap(),
                status
            );
        }
    }
}
