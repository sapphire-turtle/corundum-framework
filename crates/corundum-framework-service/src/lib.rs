//! Registering an app with the OS service manager.
//!
//! An application describes itself with a [`ServiceSpec`]; this crate turns that into a unit
//! file and hands it to the OS service manager — one real implementation per platform, plus
//! a recording one for tests, so no test ever touches the host's service manager.
//!
//! The one unit kind each platform offers, and how this crate installs it:
//!
//! | Platform | Unit | Activation |
//! |---|---|---|
//! | Linux | `~/.config/systemd/user/<app>.service` | `systemctl --user enable --now` |
//! | macOS | `~/Library/LaunchAgents/<label>.plist` | `launchctl bootstrap gui/<uid>` |
//! | Windows | a scheduled task, handed over from a temporary XML | `schtasks /create` |
//!
//! Every install is user level: the service runs as the user who installed it, with their
//! own home directory and their own directories. LaunchDaemons and real Windows services
//! are per-machine services this crate does not install — a service the machine runs
//! without a user is outside what it offers.
//!
//! The install flow itself lives in [`manager`]: [`install`], [`uninstall`] and [`status`]
//! drive everything through the [`ServiceManager`] trait, whose real implementation is the
//! only thing in the crate that writes to the machine or runs a command.

#![warn(missing_docs)]

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod error;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod exe;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod launchd;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod manager;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod scope;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod state;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod systemd;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod windows;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use error::{Error, Result};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use exe::service_exe;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use launchd::{agent_path, label, labelled_agent_path, relabel, render_launch_agent};
#[cfg(all(feature = "cli", not(any(target_os = "ios", target_os = "android"))))]
pub use manager::ServiceCommand;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use manager::{
    Calls, RecordingManager, ServiceManager, SystemManager, install, registered_program, start,
    state, status, stop, uninstall,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use scope::{Environment, InstallContext, Os, PostInstall, ServiceSpec};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use state::ServiceState;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use systemd::{activation, linger_hint, render_unit, unit_path};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use windows::render_task;
