//! [`Target`]: what a row of the update panel keeps up to date, and how.

#[cfg(all(
    feature = "package",
    not(any(target_os = "ios", target_os = "android"))
))]
use corundum_framework_package::{After, Updater};
use corundum_framework_update::PackageStatus;

use super::panel::UpdateTarget;

/// One row of the update panel.
// A panel has a few targets, made once: a binary's left unboxed keeps `.into()` the whole
// change for the callers.
#[allow(clippy::large_enum_variant)]
pub enum Target {
    /// A binary replaced from a GitHub release: the app, its server, a bridge outside a
    /// package.
    Binary(UpdateTarget),
    /// This app's own Velopack package, updated by its [`Updater`].
    #[cfg(all(
        feature = "package",
        not(any(target_os = "ios", target_os = "android"))
    ))]
    Package(PackageTarget),
    /// Another package, which updates itself when told: sapphire's bridge, behind its own
    /// API.
    Delegated(Box<dyn DelegatedTarget>),
}

impl From<UpdateTarget> for Target {
    fn from(target: UpdateTarget) -> Target {
        Target::Binary(target)
    }
}

/// This app's package: its row reads the updater's state every frame.
#[cfg(all(
    feature = "package",
    not(any(target_os = "ios", target_os = "android"))
))]
pub struct PackageTarget {
    /// What the row says: `Sapphire Journal`.
    pub label: String,
    /// The app's updater.
    pub updater: Updater,
}

#[cfg(all(
    feature = "package",
    not(any(target_os = "ios", target_os = "android"))
))]
impl Target {
    /// The app's own row: its package's when it runs from one and has an `updater`, else
    /// `binary`, the release it was installed from.
    pub fn this_app(label: String, updater: Option<Updater>, binary: UpdateTarget) -> Target {
        match updater {
            Some(updater) if corundum_framework_platform::package::current().is_some() => {
                Target::Package(PackageTarget { label, updater })
            }
            _ => Target::Binary(binary),
        }
    }
}

/// A package that updates itself when told, behind an API of its own. Called on the
/// panel's worker thread, where each call may block; but `label`, which the panel asks
/// once, when it is made.
pub trait DelegatedTarget: Send + Sync + 'static {
    /// What the row says: `sapphire-bridge`.
    fn label(&self) -> String;
    /// Its running version and what its updater is doing. Asked every 5 seconds while the
    /// panel is shown.
    fn status(&self) -> Result<PackageStatus, String>;
    /// Look for an update now, and download what is found: the panel's Check button.
    fn check(&self) -> Result<(), String>;
    /// Apply the downloaded update and restart.
    fn apply(&self) -> Result<(), String>;
}

/// A package row's calls, the same for this app's package and a delegated one.
#[cfg(all(
    feature = "package",
    not(any(target_os = "ios", target_os = "android"))
))]
impl DelegatedTarget for PackageTarget {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn status(&self) -> Result<PackageStatus, String> {
        Ok(self.updater.status())
    }

    fn check(&self) -> Result<(), String> {
        self.updater.check_now();
        Ok(())
    }

    /// On success Velopack's updater waits for this process to end, and the panel closes
    /// the window (see [`Target::closes_on_apply`]).
    fn apply(&self) -> Result<(), String> {
        self.updater
            .apply(After::RestartApp)
            .map_err(|e| e.to_string())
    }
}

impl Target {
    /// What the row says.
    pub(crate) fn label(&self) -> String {
        match self {
            Target::Binary(target) => target.label.clone(),
            #[cfg(all(
                feature = "package",
                not(any(target_os = "ios", target_os = "android"))
            ))]
            Target::Package(target) => target.label.clone(),
            Target::Delegated(target) => target.label(),
        }
    }

    /// The binary row's target, `None` for a package.
    pub(crate) fn binary(&self) -> Option<&UpdateTarget> {
        match self {
            Target::Binary(target) => Some(target),
            _ => None,
        }
    }

    /// A package row's calls, `None` for a binary.
    pub(crate) fn package(&self) -> Option<&dyn DelegatedTarget> {
        match self {
            Target::Binary(_) => None,
            #[cfg(all(
                feature = "package",
                not(any(target_os = "ios", target_os = "android"))
            ))]
            Target::Package(target) => Some(target),
            Target::Delegated(target) => Some(&**target),
        }
    }

    /// This app's package's state, read every frame as it costs nothing; `None` for a row
    /// whose state is asked on the worker.
    pub(crate) fn live_status(&self) -> Option<PackageStatus> {
        match self {
            #[cfg(all(
                feature = "package",
                not(any(target_os = "ios", target_os = "android"))
            ))]
            Target::Package(target) => Some(target.updater.status()),
            _ => None,
        }
    }

    /// Whether an apply ends this app, so the panel closes the window once it succeeds:
    /// this app's package's. A delegated package restarts itself, not this app.
    pub(crate) fn closes_on_apply(&self) -> bool {
        match self {
            #[cfg(all(
                feature = "package",
                not(any(target_os = "ios", target_os = "android"))
            ))]
            Target::Package(_) => true,
            _ => false,
        }
    }

    /// Whether the row's state is asked every few seconds: a delegated package's.
    pub(crate) fn is_polled(&self) -> bool {
        matches!(self, Target::Delegated(_))
    }

    /// Repaint `ctx` whenever this app's package changes state. Only the context is kept by
    /// the updater, not the panel, so the two do not hold each other.
    pub(crate) fn repaint_on_change(&self, _ctx: &egui::Context) {
        #[cfg(all(
            feature = "package",
            not(any(target_os = "ios", target_os = "android"))
        ))]
        if let Target::Package(target) = self {
            let ctx = _ctx.clone();
            target.updater.on_change(move || ctx.request_repaint());
        }
    }
}
