//! The panel's worker: one thread, with a current-thread tokio runtime, that looks the
//! releases up and installs them, one job at a time.

#[cfg(feature = "self-update")]
use std::io;
#[cfg(feature = "self-update")]
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};

#[cfg(feature = "self-update")]
use corundum_framework_update::{InstallMethod, Release};
use corundum_framework_update::{cargo_home, client, install_method, latest_release};

#[cfg(feature = "self-update")]
use super::panel::AfterUpdate;
use super::panel::UpdateTarget;
#[cfg(feature = "self-update")]
use super::state::RestartOutcome;
use super::state::{Event, has_asset};

/// What the panel asks of the worker, by the row's index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Job {
    /// Look the row's latest release up.
    Check(usize),
    /// Download and install the release, then restart.
    #[cfg(feature = "self-update")]
    Update(usize, Release),
}

/// The panel's ends of the worker's channels.
pub(crate) struct Worker {
    pub(crate) jobs: Sender<Job>,
    pub(crate) results: Receiver<(usize, Event)>,
}

/// Start the worker for `targets`. It asks `ctx` for a repaint after each result, and ends
/// when the [`Worker`] (its job sender) is dropped.
pub(crate) fn spawn(
    targets: Arc<[UpdateTarget]>,
    user_agent: String,
    ctx: egui::Context,
) -> Worker {
    let (jobs, job_rx) = mpsc::channel();
    let (result_tx, results) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("update worker".to_owned())
        .spawn(move || {
            let reply = |index: usize, event: Event| {
                // The panel is gone when this fails; the next `recv` ends the loop.
                let _ = result_tx.send((index, event));
                ctx.request_repaint();
            };
            run(&targets, &user_agent, &job_rx, reply);
        });
    if let Err(e) = spawned {
        // The job receiver went with the closure: the panel's sends fail, and it says so.
        tracing::warn!("cannot start the update worker: {e}");
    }
    Worker { jobs, results }
}

/// Carry out the jobs from `jobs` until the panel hangs up.
fn run(
    targets: &[UpdateTarget],
    user_agent: &str,
    jobs: &Receiver<Job>,
    reply: impl Fn(usize, Event),
) {
    let setup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("no async runtime: {e}"))
        .and_then(|runtime| {
            client(user_agent)
                .map(|client| (runtime, client))
                .map_err(|e| e.to_string())
        });
    for job in jobs {
        match job {
            Job::Check(index) => {
                let target = &targets[index];
                let found = match &setup {
                    Ok((runtime, client)) => runtime
                        .block_on(latest_release(client, &target.source))
                        .map_err(|e| e.to_string()),
                    Err(message) => Err(message.clone()),
                };
                let has_asset = match (&found, target.source.asset_base()) {
                    (Ok(release), Some(base)) => has_asset(release, base),
                    _ => false,
                };
                let method = install_method(&target.exe, cargo_home().as_deref());
                reply(
                    index,
                    Event::Checked {
                        found,
                        method,
                        has_asset,
                    },
                );
            }
            #[cfg(feature = "self-update")]
            Job::Update(index, release) => {
                let target = &targets[index];
                let installed = match &setup {
                    Ok((runtime, client)) => install(runtime, client, target, &release),
                    Err(message) => Err(message.clone()),
                };
                let ok = installed.is_ok();
                reply(index, Event::Downloaded(installed));
                if ok {
                    reply(index, Event::Restarted(restart(target)));
                }
            }
        }
    }
}

/// Put `release`'s file in place of `target`'s executable.
#[cfg(feature = "self-update")]
fn install(
    runtime: &tokio::runtime::Runtime,
    client: &reqwest::Client,
    target: &UpdateTarget,
    release: &Release,
) -> Result<(), String> {
    // The panel only offers these; checked again here, where the file is written.
    let method = install_method(&target.exe, cargo_home().as_deref());
    if method != InstallMethod::Release {
        return Err(format!(
            "{} was not installed from a release",
            target.exe.display()
        ));
    }
    // An unknown current version (a server not running) installs: there is nothing to
    // downgrade from.
    if target
        .current
        .as_ref()
        .is_some_and(|current| release.version <= *current)
    {
        return Err(format!(
            "{} is not newer than the running version",
            release.version
        ));
    }
    let bytes = runtime
        .block_on(corundum_framework_update::download_asset(
            client,
            &target.source,
            release,
        ))
        .map_err(|e| e.to_string())?;
    match target.after {
        AfterUpdate::Relaunch => corundum_framework_update::install(&bytes, &target.exe, |new| {
            self_replace::self_replace(new)
        }),
        _ => corundum_framework_update::replace_file(&target.exe, &bytes),
    }
    .map_err(|e| e.to_string())
}

/// Run `target`'s new executable.
#[cfg(feature = "self-update")]
fn restart(target: &UpdateTarget) -> RestartOutcome {
    match &target.after {
        AfterUpdate::Relaunch => match relaunch(&target.exe) {
            Ok(()) => RestartOutcome::Restarted,
            Err(e) => RestartOutcome::Failed(format!("cannot start {}: {e}", target.exe.display())),
        },
        AfterUpdate::Restart(restart) => restart(),
        AfterUpdate::Manual => RestartOutcome::ByHand,
    }
}

/// Start `exe` with this process's arguments: the app, after replacing itself. The window
/// closes only once this succeeded.
#[cfg(feature = "self-update")]
pub(crate) fn relaunch(exe: &Path) -> io::Result<()> {
    // Not waited for: it outlives this process.
    #[allow(clippy::zombie_processes)]
    std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .spawn()
        .map(drop)
}

#[cfg(all(test, feature = "self-update"))]
mod tests {
    use super::*;
    use corundum_framework_update::ReleaseSource;
    use std::path::PathBuf;

    #[test]
    fn relaunching_a_missing_file_fails() {
        let target = UpdateTarget {
            label: "App".to_owned(),
            current: None,
            source: ReleaseSource::github("o", "r"),
            exe: PathBuf::from("/nonexistent/app"),
            after: AfterUpdate::Relaunch,
        };
        assert!(relaunch(&target.exe).is_err());
        assert!(matches!(restart(&target), RestartOutcome::Failed(_)));
    }
}
