//! The Settings → About update panel, without egui: the state of each row (the app, its
//! server, a bridge) as it checks for a release, offers it, installs it and restarts.

// The panel that drives the transitions comes in the next step.
#[allow(dead_code)]
mod state;

pub use state::{RestartOutcome, Retry, RowState, Step};
