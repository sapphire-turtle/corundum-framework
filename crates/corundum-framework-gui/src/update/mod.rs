//! The Settings → About update panel, without egui: the state of each row (the app, its
//! server, a bridge) as it checks for a release, offers it, installs it and restarts.

mod panel;
mod state;
mod worker;

pub use panel::{AfterUpdate, UpdateTarget, UpdatesPanel};
pub use state::{RestartOutcome, Retry, RowState, Step};
