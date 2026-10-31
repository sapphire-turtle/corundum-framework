//! What a registered service is doing, as a value the GUI can branch on.

/// The state of an app's service registration, as [`state`](fn@crate::state) reports it.
///
/// Non-exhaustive: a later version may tell more states apart, so a match outside this
/// crate needs a wildcard arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ServiceState {
    /// Nothing is registered with the OS service manager.
    NotInstalled,
    /// Registered, and not running right now.
    Stopped,
    /// Registered and running.
    Running,
    /// Registered, but it cannot work as it is, for the reason given — such as a program
    /// that was moved or deleted after the install.
    Broken {
        /// A sentence saying what is wrong, fit to show the user.
        reason: String,
    },
}
