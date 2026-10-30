//! The Settings → About update panel, without egui: the state of each row (the app, its
//! server, a bridge) as it checks for a release, offers it, installs it and restarts.

#[cfg(all(
    feature = "package",
    not(any(target_os = "ios", target_os = "android"))
))]
mod badge;
mod package_row;
mod panel;
mod state;
mod target;
mod worker;

#[cfg(all(
    feature = "package",
    not(any(target_os = "ios", target_os = "android"))
))]
pub use badge::ReadyBadge;
pub use panel::{AfterUpdate, UpdateTarget, UpdatesPanel};
/// The version type [`UpdateTarget`] uses for the running version.
pub use semver;
pub use state::{RestartOutcome, Retry, RowState, Step};
#[cfg(all(
    feature = "package",
    not(any(target_os = "ios", target_os = "android"))
))]
pub use target::PackageTarget;
pub use target::{DelegatedTarget, Target};
