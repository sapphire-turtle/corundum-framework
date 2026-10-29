//! The app's command line on `PATH`.

/// The command line a package puts on `PATH`.
#[derive(Clone, Debug)]
pub struct CliSpec {
    /// The command's name: `sapphire-journal`.
    pub command: &'static str,
}
