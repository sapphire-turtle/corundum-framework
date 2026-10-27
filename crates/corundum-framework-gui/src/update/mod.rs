//! The Settings → About update panel, without egui: the state of each row (the app, its
//! server, a bridge) as it checks for a release, offers it, installs it and restarts.

mod panel;
mod state;
mod worker;

pub use panel::{AfterUpdate, UpdateTarget, UpdatesPanel};
/// The version type [`UpdateTarget`] uses for the running version.
pub use semver;
pub use state::{RestartOutcome, Retry, RowState, Step};
