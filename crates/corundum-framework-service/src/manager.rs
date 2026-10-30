//! The install, uninstall and status flows, and the manager they go through.
//!
//! Everything that touches the machine — writing a unit file, running a manager command,
//! removing one — goes through the [`ServiceManager`] trait. The real implementation writes
//! real files and runs real commands; [`RecordingManager`] records what it was asked and
//! answers from canned output, so the flows can be driven by tests that never touch the
//! host's own service manager.
//!
//! [`install`] renders the platform's file ([`crate::systemd`], [`crate::launchd`],
//! [`crate::windows`]), writes it through the manager and runs the activation commands. When
//! activation fails, the half-written unit is removed before the error returns: a unit file
//! that was written but never enabled is invisible to `status` and springs to life at the
//! next reboot. [`uninstall`] is idempotent — stopping, disabling or removing something that
//! is not there is not an error — and [`status`] reports whatever the manager said.
//!
//! Every install is user level: Linux goes through `systemctl --user` with the unit from
//! [`crate::systemd`], macOS through `launchctl` with the LaunchAgent from
//! [`crate::launchd`], Windows through `schtasks` with the task XML from [`crate::windows`].
//! The activation hint for a user unit on a machine that should run it without a login
//! ([`crate::systemd::linger_hint`]) is not carried here: an install returns an
//! [`InstallContext`], and the app CLI asks for the hint itself.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::error::{Error, Result};
use crate::launchd::{
    MacBackend, agent_path, bundled_agent_path, label, labelled_agent_path, mac_backend, relabel,
    render_launch_agent,
};
use crate::scope::{Environment, InstallContext, Os, ServiceSpec};
use crate::state::ServiceState;
use crate::systemd::{activation, render_unit, unit_path, words};
use crate::windows::render_task;

/// The commands a service manager is driven with.
///
/// The one place that runs anything is the real implementation below; every flow takes a
/// `&dyn ServiceManager` so a test can hand it a [`RecordingManager`] instead and leave the
/// host's manager alone.
pub trait ServiceManager {
    /// Write a unit file, creating its directory if it is missing.
    fn write_unit(&self, path: &Path, body: &str) -> Result<()>;

    /// Run a manager command and return its standard output.
    ///
    /// The command's first element is the program, the rest its arguments; nothing here
    /// goes through a shell, so no word needs quoting.
    fn run(&self, command: &[String]) -> Result<String>;

    /// Remove a unit file. Removing one that is not there is not an error.
    fn remove_unit(&self, path: &Path) -> Result<()>;

    /// Whether a file exists — the program a unit starts, for [`state`] to tell a working
    /// registration from one whose program has since gone.
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    /// Read a unit file; `None` when there is none.
    fn read_unit(&self, path: &Path) -> Result<Option<String>> {
        match std::fs::read_to_string(path) {
            Ok(body) => Ok(Some(body)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Ask macOS's SMAppService to act on the agent whose plist the running `.app` carries
    /// as `Contents/Library/LaunchAgents/<plist_name>`, and return its status afterwards.
    fn app_service(&self, plist_name: &str, action: AppServiceAction) -> Result<AppServiceStatus>;
}

/// What [`ServiceManager::app_service`] asks SMAppService to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppServiceAction {
    /// Register the agent, which starts it once the person has allowed it.
    Register,
    /// Unregister the agent, which stops it.
    Unregister,
    /// Only ask for the status.
    Status,
}

/// SMAppService's status of an agent, as [`ServiceManager::app_service`] reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppServiceStatus {
    /// Not registered, or unregistered since.
    NotRegistered,
    /// Registered and allowed to run.
    Enabled,
    /// Registered, waiting for the person to allow it in System Settings › Login Items.
    RequiresApproval,
    /// SMAppService could not find the agent's plist in the bundle.
    NotFound,
}

/// The real service manager: real files, real commands.
///
/// Only [`install`], [`uninstall`] and [`status`] construct it — the CLI in an app does —
/// and no test ever does.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemManager;

impl ServiceManager for SystemManager {
    fn write_unit(&self, path: &Path, body: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            // A user unit's directory (`~/.config/systemd/user`) may not exist yet on a
            // machine that never had one.
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, body)?;
        Ok(())
    }

    fn run(&self, command: &[String]) -> Result<String> {
        let Some((program, args)) = command.split_first() else {
            return Err(Error::Manager("an empty command".to_owned()));
        };
        let mut process = std::process::Command::new(program);
        process.args(args);
        // `CREATE_NO_WINDOW`: a GUI app (no console of its own) running `schtasks` would
        // otherwise flash a console window for each command.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            process.creation_flags(0x0800_0000);
        }
        let output = process.output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(Error::Manager(format!(
                "{program} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )))
        }
    }

    fn remove_unit(&self, path: &Path) -> Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            // An uninstall of something that was never installed has nothing to remove.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    #[cfg(target_os = "macos")]
    fn app_service(&self, plist_name: &str, action: AppServiceAction) -> Result<AppServiceStatus> {
        crate::smappservice::app_service(plist_name, action)
    }

    #[cfg(not(target_os = "macos"))]
    fn app_service(
        &self,
        _plist_name: &str,
        _action: AppServiceAction,
    ) -> Result<AppServiceStatus> {
        Err(Error::Manager("SMAppService is macOS only".to_owned()))
    }
}

/// What a [`RecordingManager`] was asked to do, in order.
#[derive(Clone, Debug, Default)]
pub struct Calls {
    /// Every unit file written, with the body it was written with.
    pub units: Vec<(PathBuf, String)>,
    /// Every command run, each as its program and arguments.
    pub commands: Vec<Vec<String>>,
    /// Every unit file removed.
    pub removed: Vec<PathBuf>,
    /// Every unit file read.
    pub read: Vec<PathBuf>,
    /// Every SMAppService request, with the plist name it was made for.
    pub app_service: Vec<(String, AppServiceAction)>,
}

/// A service manager that records instead of acting.
///
/// It answers from canned output and never touches the machine, which is what lets the
/// install flows be tested on a laptop without enrolling it in a service. [`failing_on`]
/// makes one command fail, so the failure paths — a half-written unit, an uninstall of
/// something absent — are testable too.
///
/// [`failing_on`]: RecordingManager::failing_on
#[derive(Debug, Default)]
pub struct RecordingManager {
    calls: Mutex<Calls>,
    fail_on: Option<String>,
    output: String,
    order: Option<Arc<Mutex<Vec<String>>>>,
    existing: Vec<(PathBuf, String)>,
    files: Vec<PathBuf>,
    answers: Vec<(String, String)>,
    app_service_status: Option<AppServiceStatus>,
    app_service_fail_on: Option<AppServiceAction>,
}

impl RecordingManager {
    /// A manager that succeeds at everything and answers with empty output.
    pub fn new() -> Self {
        Self::default()
    }

    /// A manager whose commands containing `needle` fail.
    ///
    /// The needle is matched against the command's own words, so `failing_on("enable")`
    /// fails the `enable --now` command alone and leaves `daemon-reload` working.
    pub fn failing_on(needle: &str) -> Self {
        Self {
            fail_on: Some(needle.to_owned()),
            ..Self::default()
        }
    }

    /// A manager that answers every command with `text`.
    pub fn returning(text: &str) -> Self {
        Self {
            output: text.to_owned(),
            ..Self::default()
        }
    }

    /// A manager that also appends to `order` as it goes: one entry per unit file written
    /// and per command run, so a test can compare the install flow's own steps against
    /// steps a post-install hook records into the same list.
    pub fn ordered(order: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            order: Some(order),
            ..Self::default()
        }
    }

    /// The same manager, answering a read of `path` with `body`, as if the file were there.
    pub fn with_unit(mut self, path: impl Into<PathBuf>, body: &str) -> Self {
        self.existing.push((path.into(), body.to_owned()));
        self
    }

    /// The same manager, saying that a file exists at `path`, as `exists` is asked.
    pub fn with_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.files.push(path.into());
        self
    }

    /// The same manager, answering commands containing the word `needle` with `text` rather
    /// than its default output. May be chained: the first matching answer wins.
    pub fn answering(mut self, needle: &str, text: &str) -> Self {
        self.answers.push((needle.to_owned(), text.to_owned()));
        self
    }

    /// The same manager, answering every SMAppService request with `status` rather than
    /// [`AppServiceStatus::NotRegistered`].
    pub fn with_app_service(mut self, status: AppServiceStatus) -> Self {
        self.app_service_status = Some(status);
        self
    }

    /// The same manager, failing the SMAppService request `action` (after recording it), as
    /// SMAppService refuses an unsigned app or an unregister of an unknown agent.
    pub fn failing_app_service_on(mut self, action: AppServiceAction) -> Self {
        self.app_service_fail_on = Some(action);
        self
    }

    /// Everything recorded so far.
    pub fn calls(&self) -> Calls {
        self.calls
            .lock()
            .expect("the recording manager's lock is never held across a panic")
            .clone()
    }

    /// Whether a command is the one asked to fail.
    fn fails(&self, command: &[String]) -> bool {
        match &self.fail_on {
            Some(needle) => command.iter().any(|word| word == needle),
            None => false,
        }
    }
}

impl ServiceManager for RecordingManager {
    fn write_unit(&self, path: &Path, body: &str) -> Result<()> {
        self.calls
            .lock()
            .expect("the recording manager's lock is never held across a panic")
            .units
            .push((path.to_owned(), body.to_owned()));
        if let Some(order) = &self.order {
            order
                .lock()
                .expect("the shared order list's lock is never held across a panic")
                .push(format!("write_unit {}", path.display()));
        }
        Ok(())
    }

    fn run(&self, command: &[String]) -> Result<String> {
        // Record first: a command that failed was still run, and a test asking what was
        // attempted wants to see it.
        self.calls
            .lock()
            .expect("the recording manager's lock is never held across a panic")
            .commands
            .push(command.to_vec());
        if let Some(order) = &self.order {
            order
                .lock()
                .expect("the shared order list's lock is never held across a panic")
                .push(format!("run {}", command.join(" ")));
        }
        if self.fails(command) {
            return Err(Error::Manager(format!("refused: {}", command.join(" "))));
        }
        let answer = self
            .answers
            .iter()
            .find(|(needle, _)| command.iter().any(|word| word == needle));
        Ok(answer.map_or_else(|| self.output.clone(), |(_, text)| text.clone()))
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.iter().any(|file| file == path)
    }

    fn remove_unit(&self, path: &Path) -> Result<()> {
        self.calls
            .lock()
            .expect("the recording manager's lock is never held across a panic")
            .removed
            .push(path.to_owned());
        Ok(())
    }

    fn read_unit(&self, path: &Path) -> Result<Option<String>> {
        self.calls
            .lock()
            .expect("the recording manager's lock is never held across a panic")
            .read
            .push(path.to_owned());
        Ok(self
            .existing
            .iter()
            .find(|(existing, _)| existing == path)
            .map(|(_, body)| body.clone()))
    }

    fn app_service(&self, plist_name: &str, action: AppServiceAction) -> Result<AppServiceStatus> {
        self.calls
            .lock()
            .expect("the recording manager's lock is never held across a panic")
            .app_service
            .push((plist_name.to_owned(), action));
        if self.app_service_fail_on == Some(action) {
            return Err(Error::Manager(format!("refused: {action:?} {plist_name}")));
        }
        Ok(self
            .app_service_status
            .unwrap_or(AppServiceStatus::NotRegistered))
    }
}

/// Which kind of install a `service` invocation asks for.
#[cfg(feature = "cli")]
#[derive(Clone, Debug, clap::Subcommand)]
pub enum ServiceCommand {
    /// Install the service and start it.
    Install,
    /// Stop, disable and remove the service.
    Uninstall,
    /// Report what the service manager says about the service.
    Status,
}

#[cfg(feature = "cli")]
impl ServiceCommand {
    /// Carry out the command against `manager`, returning the process exit code.
    ///
    /// `env` is the machine this runs on — read by an application's CLI with
    /// [`Environment::detect`] — and `manager` is the real one there, while every test hands
    /// in a [`RecordingManager`] instead. What the user is told is printed here, so
    /// both the app servers' and the bridge's CLIs say the same thing about the same act.
    ///
    /// [`Environment::detect`]: crate::scope::Environment::detect
    pub fn run(
        &self,
        spec: &ServiceSpec,
        env: &Environment,
        manager: &dyn ServiceManager,
    ) -> Result<i32> {
        match self {
            ServiceCommand::Install => {
                let context = install(spec, env, manager)?;
                println!(
                    "installed the {} service ({})",
                    spec.app_name,
                    context.unit_path.display()
                );
                // A user unit dies with its login session; the hint to make it survive one
                // is Linux's own (`loginctl`), so it is only offered there.
                if env.os == Os::Linux
                    && let Some(hint) = crate::systemd::linger_hint()
                {
                    println!("{hint}");
                }
                Ok(0)
            }
            ServiceCommand::Uninstall => {
                uninstall(spec, env, manager)?;
                println!("removed the {} service", spec.app_name);
                Ok(0)
            }
            ServiceCommand::Status => {
                let report = status(spec, env, manager)?;
                // Another tool's output, printed as it came: it is what the user asked for.
                print!("{report}");
                if !report.ends_with('\n') {
                    println!();
                }
                Ok(0)
            }
        }
    }
}

/// Install a service: write the platform's file, activate it, and run the spec's
/// post-install hook on what was installed.
///
/// The unit's `ExecStart` (or its platform's equivalent) is the absolute path of the
/// running executable — the app that called this, or the AppImage it runs from
/// ([`service_exe`](crate::service_exe)) — plus the spec's own arguments, because a service
/// manager starts one file and nothing else. A desktop app that carries its server as a
/// subcommand registers itself this way: started from its console shim on Windows, the
/// running executable is the GUI-subsystem binary, which opens no console window.
///
/// The hook runs last, with the [`InstallContext`] the install resolved, so what it sees is
/// what was installed. When the hook fails, the install reports the hook's own error and
/// that the service is installed and running — the files exist and the manager has started
/// the service, which is exactly why a failed hook should be able to wait for a fix rather
/// than undo an otherwise good install.
pub fn install(
    spec: &ServiceSpec,
    env: &Environment,
    manager: &dyn ServiceManager,
) -> Result<InstallContext> {
    retire_legacy_agent(spec, env, manager)?;
    if let Some((plist_name, unit_path)) = bundled_agent(spec, env, manager) {
        // An agent file left by an install from outside the package would run a second
        // copy under the same label; it goes before SMAppService registers the bundle's.
        retire_agent_file(spec, env, manager)?;
        let context = InstallContext {
            unit_path,
            exe: crate::exe::service_exe()?,
        };
        manager.app_service(&plist_name, AppServiceAction::Register)?;
        run_post_install(spec, &context)?;
        return Ok(context);
    }
    let context = InstallContext {
        unit_path: install_path(spec, env)?,
        exe: crate::exe::service_exe()?,
    };
    let body = render_install(spec, &context, env.os);
    manager.write_unit(&context.unit_path, &body)?;
    for command in install_commands(spec, &context, env) {
        if let Err(error) = manager.run(&command) {
            if let Err(cleanup) = manager.remove_unit(&context.unit_path) {
                return Err(Error::Manager(format!(
                    "{error}; the partially written unit at {} could not be removed either \
                     ({cleanup})",
                    context.unit_path.display()
                )));
            }
            return Err(error);
        }
    }
    if env.os == Os::Windows {
        // The XML was a hand-over: `/create` copies the task into the scheduler, and the
        // copy left under the temporary directory has done its job.
        let _ = manager.remove_unit(&context.unit_path);
    }
    run_post_install(spec, &context)?;
    Ok(context)
}

/// Run the spec's post-install hook, last, on an installed, running service, with what the
/// install resolved.
fn run_post_install(spec: &ServiceSpec, context: &InstallContext) -> Result<()> {
    if let Some(hook) = spec.post_install.as_ref()
        && let Err(error) = hook(context)
    {
        return Err(Error::Manager(format!(
            "{error}; the service is installed and running, so fix what the hook needs \
             and rerun the uninstall and install of your choice"
        )));
    }
    Ok(())
}

/// Uninstall a service: stop and disable it, then remove its file.
///
/// Idempotent by design: a service that was never installed, or was already removed, is not
/// an error. Stopping or disabling something the manager does not know about is exactly the
/// state an uninstall wants to reach, so those failures say nothing worth reporting; a
/// failure to remove the file does, and is returned.
pub fn uninstall(
    spec: &ServiceSpec,
    env: &Environment,
    manager: &dyn ServiceManager,
) -> Result<()> {
    retire_legacy_agent(spec, env, manager)?;
    if let Some((plist_name, _)) = bundled_agent(spec, env, manager) {
        retire_agent_file(spec, env, manager)?;
        // SMAppService refuses to unregister an agent it does not have, which is the state
        // an uninstall wants to reach anyway; it is only asked when there is one.
        let status = manager.app_service(&plist_name, AppServiceAction::Status)?;
        if matches!(
            status,
            AppServiceStatus::Enabled | AppServiceStatus::RequiresApproval
        ) {
            manager.app_service(&plist_name, AppServiceAction::Unregister)?;
        }
        return Ok(());
    }
    let path = install_path(spec, env)?;

    for command in uninstall_commands(spec, env) {
        // Idempotence: "not running" and "not enabled" are what we came for.
        let _ = manager.run(&command);
    }
    manager.remove_unit(&path)
}

/// Report what the service manager says about the service.
pub fn status(
    spec: &ServiceSpec,
    env: &Environment,
    manager: &dyn ServiceManager,
) -> Result<String> {
    adopt_legacy_agent(spec, env, manager)?;
    manager.run(&status_command(spec, env))
}

/// What the service is doing now: not installed, stopped, running, or broken.
///
/// Reads the registration's own file for the program it starts, so a service whose program
/// was moved or deleted is [`Broken`](ServiceState::Broken) rather than merely stopped.
/// A legacy macOS agent is adopted first, as [`status`] does.
pub fn state(
    spec: &ServiceSpec,
    env: &Environment,
    manager: &dyn ServiceManager,
) -> Result<ServiceState> {
    adopt_legacy_agent(spec, env, manager)?;
    if let Some((plist_name, _)) = bundled_agent(spec, env, manager) {
        return Ok(
            match manager.app_service(&plist_name, AppServiceAction::Status)? {
                AppServiceStatus::RequiresApproval => ServiceState::NeedsApproval,
                AppServiceStatus::NotRegistered | AppServiceStatus::NotFound => {
                    ServiceState::NotInstalled
                }
                // SMAppService says only that it may run; launchd says whether it does.
                AppServiceStatus::Enabled => match manager.run(&status_command(spec, env)) {
                    Ok(output) if crate::launchd::is_running(&output) => ServiceState::Running,
                    _ => ServiceState::Stopped,
                },
            },
        );
    }
    let (program, running_command, is_running): (_, _, fn(&str) -> bool) = match env.os {
        Os::Linux => {
            let Some(unit) = manager.read_unit(&install_path(spec, env)?)? else {
                return Ok(ServiceState::NotInstalled);
            };
            (
                crate::systemd::exec_of(&unit),
                words(&["systemctl", "--user", "is-active", spec.app_name]),
                crate::systemd::is_running,
            )
        }
        Os::MacOs => {
            let Some(plist) = manager.read_unit(&install_path(spec, env)?)? else {
                return Ok(ServiceState::NotInstalled);
            };
            (
                crate::launchd::program_of(&plist),
                status_command(spec, env),
                crate::launchd::is_running,
            )
        }
        Os::Windows => {
            // The scheduler keeps the task, not a file; ask it for the XML it holds.
            let Ok(xml) = manager.run(&words(&[
                "schtasks",
                "/query",
                "/tn",
                spec.app_name,
                "/xml",
            ])) else {
                return Ok(ServiceState::NotInstalled);
            };
            (
                crate::windows::command_of(&xml),
                // `schtasks` prints a localised state; the task object's own is not.
                words(&[
                    "powershell",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &format!("(Get-ScheduledTask -TaskName '{}').State", spec.app_name),
                ]),
                crate::windows::is_running,
            )
        }
    };
    let Some(program) = program else {
        return Ok(ServiceState::Broken {
            reason: "the registration names no program to run".to_owned(),
        });
    };
    if !manager.exists(&program) {
        return Ok(ServiceState::Broken {
            reason: format!("the program {} no longer exists", program.display()),
        });
    }
    // A command that fails — `is-active` on an inactive unit, `print` of an unloaded
    // agent — is how the manager says "not running".
    Ok(match manager.run(&running_command) {
        Ok(output) if is_running(&output) => ServiceState::Running,
        _ => ServiceState::Stopped,
    })
}

/// Stop the service. A service that is not running is already stopped, so that is not an
/// error.
pub fn stop(spec: &ServiceSpec, env: &Environment, manager: &dyn ServiceManager) -> Result<()> {
    let _ = manager.run(&stop_command(spec, env));
    Ok(())
}

/// Start the service. Unlike [`stop`], a failure is an error: the user asked for it to run.
pub fn start(spec: &ServiceSpec, env: &Environment, manager: &dyn ServiceManager) -> Result<()> {
    manager.run(&start_command(spec, env)).map(|_| ())
}

/// The command that stops the service.
fn stop_command(spec: &ServiceSpec, env: &Environment) -> Vec<String> {
    match env.os {
        Os::Linux => words(&["systemctl", "--user", "stop", spec.app_name]),
        // `kill` rather than `bootout`: the agent stays registered and loaded.
        Os::MacOs => words(&[
            "launchctl",
            "kill",
            "SIGTERM",
            &format!("gui/{}/{}", env.euid, label(spec.app_name)),
        ]),
        Os::Windows => words(&["schtasks", "/end", "/tn", spec.app_name]),
    }
}

/// The command that starts the service.
fn start_command(spec: &ServiceSpec, env: &Environment) -> Vec<String> {
    match env.os {
        Os::Linux => words(&["systemctl", "--user", "start", spec.app_name]),
        // `-k` restarts an agent that is running already, so it also works as a restart.
        Os::MacOs => words(&[
            "launchctl",
            "kickstart",
            "-k",
            &format!("gui/{}/{}", env.euid, label(spec.app_name)),
        ]),
        Os::Windows => words(&["schtasks", "/run", "/tn", spec.app_name]),
    }
}

/// Give an agent installed under a legacy label the current one, if none is installed there.
fn adopt_legacy_agent(
    spec: &ServiceSpec,
    env: &Environment,
    manager: &dyn ServiceManager,
) -> Result<()> {
    let retired = retire_legacy_agent(spec, env, manager)?;
    // Under SMAppService the bundle's plist is the service: an adopted agent file would run
    // a second copy, so the legacy agent is only retired.
    if bundled_agent(spec, env, manager).is_some() {
        return Ok(());
    }
    // An agent under the legacy label is the service, renamed: it keeps running, under the
    // current label, unless one is installed there already.
    if let Some((from_label, legacy)) = retired {
        let path = install_path(spec, env)?;
        if manager.read_unit(&path)?.is_none() {
            manager.write_unit(&path, &relabel(&legacy, &from_label, spec.app_name))?;
            manager.run(&bootstrap_command(&path, env))?;
        }
    }
    Ok(())
}

/// Take every agent installed under one of the spec's [`legacy_labels`] off launchd and
/// remove its plist, returning the label and the plist of the first one found; `None` off
/// macOS or when there is none.
///
/// Left in place, it would start a second copy of the service next to the one under the
/// current label.
///
/// [`legacy_labels`]: ServiceSpec::legacy_labels
fn retire_legacy_agent(
    spec: &ServiceSpec,
    env: &Environment,
    manager: &dyn ServiceManager,
) -> Result<Option<(String, String)>> {
    if env.os != Os::MacOs || spec.legacy_labels.is_empty() {
        return Ok(None);
    }
    let home = home_dir(env)?;
    let mut first = None;
    for legacy_label in &spec.legacy_labels {
        let path = labelled_agent_path(legacy_label, &home);
        let Some(body) = manager.read_unit(&path)? else {
            continue;
        };
        // Not loaded (the user booted it out by hand) is what this wants anyway.
        let _ = manager.run(&words(&[
            "launchctl",
            "bootout",
            &format!("gui/{}", env.euid),
            legacy_label,
        ]));
        manager.remove_unit(&path)?;
        first.get_or_insert((legacy_label.clone(), body));
    }
    Ok(first)
}

/// The plist name and path of the agent the running `.app` carries, when the install goes
/// through SMAppService ([`mac_backend`]); `None` off macOS and for the LaunchAgent file.
fn bundled_agent(
    spec: &ServiceSpec,
    env: &Environment,
    manager: &dyn ServiceManager,
) -> Option<(String, PathBuf)> {
    if env.os != Os::MacOs {
        return None;
    }
    let layout = env.package.as_ref()?;
    match mac_backend(Some(layout), |path| manager.exists(path), spec.app_name) {
        MacBackend::AppService { plist_name } => {
            let path = bundled_agent_path(&layout.root, &plist_name);
            Some((plist_name, path))
        }
        MacBackend::AgentFile => None,
    }
}

/// Take a LaunchAgent file under the current label off launchd and remove it, if one is
/// there: one an install from outside the package left.
fn retire_agent_file(
    spec: &ServiceSpec,
    env: &Environment,
    manager: &dyn ServiceManager,
) -> Result<()> {
    // Without a home there is no `~/Library/LaunchAgents` to have left a file in.
    let Some(home) = env.home.as_deref() else {
        return Ok(());
    };
    let path = agent_path(spec.app_name, home);
    if manager.read_unit(&path)?.is_none() {
        return Ok(());
    }
    // Not loaded is what this wants anyway.
    let _ = manager.run(&words(&[
        "launchctl",
        "bootout",
        &format!("gui/{}", env.euid),
        &label(spec.app_name),
    ]));
    manager.remove_unit(&path)
}

/// Where each platform keeps the file an install writes: the invoking user's own directory;
/// a Windows task's hand-over lives in the temporary directory.
fn install_path(spec: &ServiceSpec, env: &Environment) -> Result<PathBuf> {
    match env.os {
        Os::Linux => Ok(unit_path(spec.app_name, &home_dir(env)?)),
        Os::MacOs => Ok(agent_path(spec.app_name, &home_dir(env)?)),
        Os::Windows => Ok(std::env::temp_dir().join(format!("{}-task.xml", spec.app_name))),
    }
}

/// Render the platform's file for a resolved install.
fn render_install(spec: &ServiceSpec, context: &InstallContext, os: Os) -> String {
    match os {
        Os::Linux => render_unit(spec, context),
        Os::MacOs => render_launch_agent(spec, context),
        Os::Windows => render_task(spec, context),
    }
}

/// The commands that activate what was just installed.
fn install_commands(
    spec: &ServiceSpec,
    context: &InstallContext,
    env: &Environment,
) -> Vec<Vec<String>> {
    match env.os {
        Os::Linux => activation(spec.app_name),
        // `gui/<uid>` is the agent session of the user doing the install; a LaunchAgent
        // belongs to them.
        Os::MacOs => vec![bootstrap_command(&context.unit_path, env)],
        // The task name is the app name — the same key an uninstall and a status address.
        Os::Windows => vec![words(&[
            "schtasks",
            "/create",
            "/tn",
            spec.app_name,
            "/xml",
            &context.unit_path.display().to_string(),
            "/f",
        ])],
    }
}

/// The command that loads a LaunchAgent into the agent session of the user doing the install.
fn bootstrap_command(path: &Path, env: &Environment) -> Vec<String> {
    words(&[
        "launchctl",
        "bootstrap",
        &format!("gui/{}", env.euid),
        &path.display().to_string(),
    ])
}

/// The commands that stop and disable an installed service.
fn uninstall_commands(spec: &ServiceSpec, env: &Environment) -> Vec<Vec<String>> {
    match env.os {
        Os::Linux => vec![
            words(&["systemctl", "--user", "stop", spec.app_name]),
            words(&["systemctl", "--user", "disable", spec.app_name]),
        ],
        Os::MacOs => vec![words(&[
            "launchctl",
            "bootout",
            &format!("gui/{}", env.euid),
            &label(spec.app_name),
        ])],
        Os::Windows => vec![words(&["schtasks", "/delete", "/tn", spec.app_name, "/f"])],
    }
}

/// The command that asks the manager about a service.
fn status_command(spec: &ServiceSpec, env: &Environment) -> Vec<String> {
    match env.os {
        // `--no-pager`: the answer is captured, not read off a terminal.
        Os::Linux => words(&["systemctl", "--user", "--no-pager", "status", spec.app_name]),
        Os::MacOs => words(&[
            "launchctl",
            "print",
            &format!("gui/{}/{}", env.euid, label(spec.app_name)),
        ]),
        Os::Windows => words(&["schtasks", "/query", "/tn", spec.app_name]),
    }
}

/// The invoking user's home directory, where a user-level install writes.
///
/// Taken from [`Environment::home`]. A user unit has nowhere to go without it, which is
/// worth saying rather than guessing a path.
fn home_dir(env: &Environment) -> Result<PathBuf> {
    env.home.clone().ok_or_else(|| {
        Error::Config(format!(
            "{} is not set; there is nowhere to write a user unit",
            env.os.home_variable()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use corundum_framework_platform::package::{PackageKind, PackageLayout};

    fn spec() -> ServiceSpec {
        ServiceSpec {
            app_name: "sapphire-agent",
            description: "Sapphire agent server".into(),
            args: vec!["server".into(), "run".into()],
            legacy_labels: vec![LEGACY_LABEL.into()],
            post_install: None,
        }
    }

    const LEGACY_LABEL: &str = "net.fireturtle.sapphire.sapphire-bridge";

    fn linux() -> Environment {
        Environment {
            euid: 1000,
            os: Os::Linux,
            home: Some(PathBuf::from("/home/user")),
            package: None,
        }
    }

    fn macos() -> Environment {
        Environment {
            euid: 1000,
            os: Os::MacOs,
            home: Some(PathBuf::from("/home/user")),
            package: None,
        }
    }

    fn windows() -> Environment {
        Environment {
            euid: 1000,
            os: Os::Windows,
            home: Some(PathBuf::from("/home/user")),
            package: None,
        }
    }

    /// macOS, running from a `.app` that carries the agent's plist.
    fn macos_bundle() -> Environment {
        Environment {
            package: Some(PackageLayout {
                kind: PackageKind::MacBundle,
                root: PathBuf::from("/Applications/Sapphire.app"),
                bin_dir: PathBuf::from("/Applications/Sapphire.app/Contents/MacOS"),
            }),
            ..macos()
        }
    }

    const BUNDLED_PLIST_NAME: &str = "net.sapphire-turtle.sapphire-agent.plist";

    /// A manager that sees the bundle's own plist, so the flows pick SMAppService.
    fn bundle_manager() -> RecordingManager {
        RecordingManager::new().with_file(
            "/Applications/Sapphire.app/Contents/Library/LaunchAgents/\
             net.sapphire-turtle.sapphire-agent.plist",
        )
    }

    fn app_service_call(action: AppServiceAction) -> (String, AppServiceAction) {
        (BUNDLED_PLIST_NAME.to_owned(), action)
    }

    fn command(parts: &[&str]) -> Vec<String> {
        words(parts)
    }

    fn unit_file(env: &Environment) -> PathBuf {
        install_path(&spec(), env).unwrap()
    }

    const PROGRAM: &str = "/usr/bin/sapphire-agent";

    fn linux_unit() -> String {
        include_str!("../tests/golden/user.service").to_owned()
    }

    fn macos_plist() -> String {
        include_str!("../tests/golden/launchagent.plist").to_owned()
    }

    fn task_xml() -> String {
        include_str!("../tests/golden/task.xml").to_owned()
    }

    const POWERSHELL_STATE: &str = "(Get-ScheduledTask -TaskName 'sapphire-agent').State";

    fn powershell() -> Vec<String> {
        command(&[
            "powershell",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            POWERSHELL_STATE,
        ])
    }

    #[test]
    fn linux_without_a_unit_is_not_installed() {
        let manager = RecordingManager::new();
        assert_eq!(
            state(&spec(), &linux(), &manager).unwrap(),
            ServiceState::NotInstalled
        );
        assert!(manager.calls().commands.is_empty());
    }

    #[test]
    fn linux_with_a_missing_program_is_broken() {
        let manager = RecordingManager::new().with_unit(unit_file(&linux()), &linux_unit());
        match state(&spec(), &linux(), &manager).unwrap() {
            ServiceState::Broken { reason } => assert!(reason.contains(PROGRAM), "{reason}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn linux_running() {
        let manager = RecordingManager::returning("active\n")
            .with_unit(unit_file(&linux()), &linux_unit())
            .with_file(PROGRAM);
        assert_eq!(
            state(&spec(), &linux(), &manager).unwrap(),
            ServiceState::Running
        );
        assert_eq!(
            manager.calls().commands,
            vec![command(&[
                "systemctl",
                "--user",
                "is-active",
                "sapphire-agent"
            ])]
        );
    }

    #[test]
    fn linux_stopped_when_is_active_fails() {
        let manager = RecordingManager::failing_on("is-active")
            .with_unit(unit_file(&linux()), &linux_unit())
            .with_file(PROGRAM);
        assert_eq!(
            state(&spec(), &linux(), &manager).unwrap(),
            ServiceState::Stopped
        );
    }

    #[test]
    fn macos_without_a_plist_is_not_installed() {
        let manager = RecordingManager::new();
        assert_eq!(
            state(&spec(), &macos(), &manager).unwrap(),
            ServiceState::NotInstalled
        );
    }

    #[test]
    fn macos_running() {
        let manager = RecordingManager::returning("\tstate = running\n")
            .with_unit(unit_file(&macos()), &macos_plist())
            .with_file(PROGRAM);
        assert_eq!(
            state(&spec(), &macos(), &manager).unwrap(),
            ServiceState::Running
        );
        assert_eq!(
            manager.calls().commands,
            vec![command(&[
                "launchctl",
                "print",
                "gui/1000/net.sapphire-turtle.sapphire-agent"
            ])]
        );
    }

    #[test]
    fn macos_not_loaded_is_stopped() {
        let manager = RecordingManager::failing_on("print")
            .with_unit(unit_file(&macos()), &macos_plist())
            .with_file(PROGRAM);
        assert_eq!(
            state(&spec(), &macos(), &manager).unwrap(),
            ServiceState::Stopped
        );
    }

    #[test]
    fn windows_without_a_task_is_not_installed() {
        let manager = RecordingManager::failing_on("/query");
        assert_eq!(
            state(&spec(), &windows(), &manager).unwrap(),
            ServiceState::NotInstalled
        );
    }

    #[test]
    fn windows_running() {
        let manager = RecordingManager::new()
            .answering("/query", &task_xml())
            .answering("powershell", "Running\r\n")
            .with_file(PROGRAM);
        assert_eq!(
            state(&spec(), &windows(), &manager).unwrap(),
            ServiceState::Running
        );
        assert!(manager.calls().commands.contains(&powershell()));
    }

    #[test]
    fn windows_ready_is_stopped() {
        let manager = RecordingManager::new()
            .answering("/query", &task_xml())
            .answering("powershell", "Ready\r\n")
            .with_file(PROGRAM);
        assert_eq!(
            state(&spec(), &windows(), &manager).unwrap(),
            ServiceState::Stopped
        );
    }

    #[test]
    fn stop_and_start_run_each_backend_s_command() {
        let label = "gui/1000/net.sapphire-turtle.sapphire-agent";
        for (env, stop_command, start_command) in [
            (
                linux(),
                command(&["systemctl", "--user", "stop", "sapphire-agent"]),
                command(&["systemctl", "--user", "start", "sapphire-agent"]),
            ),
            (
                macos(),
                command(&["launchctl", "kill", "SIGTERM", label]),
                command(&["launchctl", "kickstart", "-k", label]),
            ),
            (
                windows(),
                command(&["schtasks", "/end", "/tn", "sapphire-agent"]),
                command(&["schtasks", "/run", "/tn", "sapphire-agent"]),
            ),
        ] {
            let manager = RecordingManager::new();
            stop(&spec(), &env, &manager).unwrap();
            start(&spec(), &env, &manager).unwrap();
            assert_eq!(manager.calls().commands, vec![stop_command, start_command]);
        }
    }

    #[test]
    fn stopping_a_stopped_service_is_not_an_error() {
        for (env, needle) in [(linux(), "stop"), (macos(), "kill"), (windows(), "/end")] {
            let manager = RecordingManager::failing_on(needle);
            stop(&spec(), &env, &manager).unwrap();
        }
    }

    #[test]
    fn a_failing_start_is_an_error() {
        let manager = RecordingManager::failing_on("start");
        assert!(start(&spec(), &linux(), &manager).is_err());
    }

    #[test]
    fn installing_writes_a_unit_and_activates_it() {
        let manager = RecordingManager::default();
        install(&spec(), &linux(), &manager).unwrap();

        let calls = manager.calls();
        assert_eq!(calls.units.len(), 1);
        assert!(
            calls.units[0].0.ends_with("sapphire-agent.service"),
            "{:?}",
            calls.units[0].0
        );
        assert!(
            calls
                .commands
                .iter()
                .any(|c| c.contains(&"enable".to_owned())),
            "{:?}",
            calls.commands
        );
    }

    #[test]
    fn an_install_without_a_home_names_the_missing_variable() {
        for env in [linux(), macos()] {
            let env = Environment { home: None, ..env };
            let manager = RecordingManager::default();
            let err = install(&spec(), &env, &manager).unwrap_err().to_string();
            assert!(err.contains("HOME"), "{err}");
            assert!(
                manager.calls().units.is_empty(),
                "nothing is written without a home"
            );
        }
    }

    #[test]
    fn a_user_install_uses_the_user_flag() {
        // The one install kind always goes through the invoking user's own manager: every
        // command carries `--user`, so nothing here can reach the system's units.
        let manager = RecordingManager::default();
        install(&spec(), &linux(), &manager).unwrap();
        assert!(
            manager
                .calls()
                .commands
                .iter()
                .all(|c| c.contains(&"--user".to_owned())),
            "{:?}",
            manager.calls().commands
        );
    }

    #[test]
    fn a_failing_activation_leaves_no_unit_behind() {
        let manager = RecordingManager::failing_on("enable");
        assert!(install(&spec(), &linux(), &manager).is_err());
        assert!(
            !manager.calls().removed.is_empty(),
            "a half-installed service is worse than none: the unit must be cleaned up"
        );
    }

    #[test]
    fn uninstalling_stops_disables_and_removes() {
        let manager = RecordingManager::default();
        uninstall(&spec(), &linux(), &manager).unwrap();

        let calls = manager.calls();
        let flat: Vec<String> = calls.commands.iter().flatten().cloned().collect();
        assert!(flat.contains(&"disable".to_owned()), "{flat:?}");
        assert_eq!(calls.removed.len(), 1);
    }

    #[test]
    fn uninstalling_something_that_is_not_installed_is_not_an_error() {
        let manager = RecordingManager::failing_on("disable");
        uninstall(&spec(), &linux(), &manager).expect("uninstall is idempotent");
    }

    #[test]
    fn status_reports_what_the_manager_said() {
        let manager = RecordingManager::returning("active");
        let text = status(&spec(), &linux(), &manager).unwrap();
        assert!(text.contains("active"), "{text}");
    }

    #[test]
    fn a_macos_install_bootstraps_a_launch_agent() {
        let manager = RecordingManager::default();
        install(&spec(), &macos(), &manager).unwrap();

        let calls = manager.calls();
        assert_eq!(calls.units.len(), 1);
        assert!(
            calls.units[0].0.to_string_lossy().ends_with(".plist"),
            "a LaunchAgent is a plist: {:?}",
            calls.units[0].0
        );
        let flat: Vec<String> = calls.commands.iter().flatten().cloned().collect();
        assert!(flat.contains(&"launchctl".to_owned()), "{flat:?}");
        assert!(flat.contains(&"bootstrap".to_owned()), "{flat:?}");
    }

    // Built the way the install builds them: on Windows, joining onto `/home/user` mixes
    // separators, and the bootstrap command carries the path as text.
    fn legacy_plist() -> PathBuf {
        labelled_agent_path(LEGACY_LABEL, Path::new("/home/user"))
    }

    fn current_plist() -> PathBuf {
        agent_path("sapphire-agent", Path::new("/home/user"))
    }

    fn legacy_body() -> String {
        format!(
            "<plist><dict><key>Label</key>\n  <string>{LEGACY_LABEL}</string>\n\
             <key>ProgramArguments</key><array><string>/opt/old/sapphire-agent</string></array>\
             </dict></plist>\n"
        )
    }

    fn boots_out_legacy(calls: &Calls) -> bool {
        calls
            .commands
            .contains(&words(&["launchctl", "bootout", "gui/1000", LEGACY_LABEL]))
    }

    #[test]
    fn a_macos_install_retires_an_agent_under_the_legacy_label() {
        let manager = RecordingManager::new().with_unit(legacy_plist(), &legacy_body());
        install(&spec(), &macos(), &manager).unwrap();

        let calls = manager.calls();
        assert!(boots_out_legacy(&calls), "{:?}", calls.commands);
        assert_eq!(calls.removed, vec![legacy_plist()]);
        assert_eq!(calls.units.len(), 1);
        assert_eq!(calls.units[0].0, current_plist());
    }

    #[test]
    fn a_macos_uninstall_removes_both_labels() {
        let manager = RecordingManager::new().with_unit(legacy_plist(), &legacy_body());
        uninstall(&spec(), &macos(), &manager).unwrap();

        let calls = manager.calls();
        assert!(boots_out_legacy(&calls), "{:?}", calls.commands);
        assert_eq!(calls.removed, vec![legacy_plist(), current_plist()]);
    }

    #[test]
    fn a_macos_status_moves_a_legacy_agent_to_the_current_label() {
        let manager = RecordingManager::new().with_unit(legacy_plist(), &legacy_body());
        status(&spec(), &macos(), &manager).unwrap();

        let calls = manager.calls();
        assert!(boots_out_legacy(&calls), "{:?}", calls.commands);
        assert_eq!(calls.removed, vec![legacy_plist()]);
        assert_eq!(calls.units.len(), 1);
        let (path, body) = &calls.units[0];
        assert_eq!(path, &current_plist());
        assert!(
            body.contains("<string>net.sapphire-turtle.sapphire-agent</string>"),
            "{body}"
        );
        assert!(
            body.contains("/opt/old/sapphire-agent"),
            "the agent keeps its own program: {body}"
        );
        assert!(
            calls.commands.contains(&words(&[
                "launchctl",
                "bootstrap",
                "gui/1000",
                &current_plist().display().to_string(),
            ])),
            "{:?}",
            calls.commands
        );
    }

    #[test]
    fn a_macos_status_leaves_an_agent_already_under_the_current_label() {
        let manager = RecordingManager::new()
            .with_unit(legacy_plist(), &legacy_body())
            .with_unit(current_plist(), "<plist/>");
        status(&spec(), &macos(), &manager).unwrap();

        let calls = manager.calls();
        assert!(boots_out_legacy(&calls), "{:?}", calls.commands);
        assert!(calls.units.is_empty(), "{:?}", calls.units);
        assert!(
            !calls.commands.iter().flatten().any(|w| w == "bootstrap"),
            "{:?}",
            calls.commands
        );
    }

    #[test]
    fn without_a_legacy_agent_nothing_extra_runs() {
        let manager = RecordingManager::new();
        status(&spec(), &macos(), &manager).unwrap();
        let calls = manager.calls();
        assert_eq!(calls.commands.len(), 1, "{:?}", calls.commands);
        assert!(calls.removed.is_empty());

        // Off macOS, there is no legacy plist to look for.
        let manager = RecordingManager::new();
        status(&spec(), &linux(), &manager).unwrap();
        assert!(manager.calls().read.is_empty());
    }

    #[test]
    fn every_legacy_label_is_retired() {
        let mut spec = spec();
        spec.legacy_labels = vec!["org.old.a".into(), "org.old.b".into()];
        let home = PathBuf::from("/home/user");
        let manager = RecordingManager::new()
            .with_unit(labelled_agent_path("org.old.a", &home), &legacy_body())
            .with_unit(labelled_agent_path("org.old.b", &home), &legacy_body());
        uninstall(&spec, &macos(), &manager).unwrap();
        let calls = manager.calls();
        for old in ["org.old.a", "org.old.b"] {
            assert!(
                calls
                    .commands
                    .iter()
                    .any(|c| c.last().is_some_and(|w| w == old)
                        && c.contains(&"bootout".to_string())),
                "{:?}",
                calls.commands
            );
            assert!(calls.removed.contains(&labelled_agent_path(old, &home)));
        }
    }

    #[test]
    fn without_legacy_labels_no_old_agent_is_looked_for() {
        let manager = RecordingManager::new();
        let mut spec = spec();
        spec.legacy_labels.clear();
        install(&spec, &macos(), &manager).unwrap();
        assert!(manager.calls().read.is_empty());
    }

    #[test]
    fn a_windows_install_registers_a_scheduled_task() {
        let manager = RecordingManager::default();
        install(&spec(), &windows(), &manager).unwrap();

        let flat: Vec<String> = manager.calls().commands.iter().flatten().cloned().collect();
        assert!(flat.contains(&"schtasks".to_owned()), "{flat:?}");
        assert!(flat.contains(&"/create".to_owned()), "{flat:?}");
    }

    #[test]
    fn install_registers_with_smappservice() {
        let manager = bundle_manager().with_app_service(AppServiceStatus::Enabled);
        install(&spec(), &macos_bundle(), &manager).unwrap();

        let calls = manager.calls();
        assert!(
            calls
                .app_service
                .contains(&app_service_call(AppServiceAction::Register)),
            "{:?}",
            calls.app_service
        );
        assert!(
            calls.units.is_empty(),
            "the bundle carries the plist: {:?}",
            calls.units
        );
        assert!(
            !calls.commands.iter().flatten().any(|w| w == "bootout"),
            "no agent file was there to take off: {:?}",
            calls.commands
        );
        assert!(calls.removed.is_empty(), "{:?}", calls.removed);
    }

    #[test]
    fn install_with_smappservice_retires_an_agent_file_under_the_same_label() {
        let manager = bundle_manager()
            .with_app_service(AppServiceStatus::Enabled)
            .with_unit(current_plist(), &macos_plist());
        install(&spec(), &macos_bundle(), &manager).unwrap();

        let calls = manager.calls();
        assert!(
            calls.commands.contains(&words(&[
                "launchctl",
                "bootout",
                "gui/1000",
                "net.sapphire-turtle.sapphire-agent",
            ])),
            "{:?}",
            calls.commands
        );
        assert_eq!(calls.removed, vec![current_plist()]);
        assert!(calls.units.is_empty(), "{:?}", calls.units);
        assert_eq!(
            calls.app_service,
            vec![app_service_call(AppServiceAction::Register)]
        );
    }

    #[test]
    fn a_failed_smappservice_registration_is_an_error() {
        let manager = bundle_manager()
            .with_app_service(AppServiceStatus::NotRegistered)
            .failing_app_service_on(AppServiceAction::Register);
        assert!(install(&spec(), &macos_bundle(), &manager).is_err());
    }

    #[test]
    fn uninstall_unregisters() {
        let manager = bundle_manager().with_app_service(AppServiceStatus::Enabled);
        uninstall(&spec(), &macos_bundle(), &manager).unwrap();

        let calls = manager.calls();
        assert!(
            calls
                .app_service
                .contains(&app_service_call(AppServiceAction::Unregister)),
            "{:?}",
            calls.app_service
        );
        assert!(calls.units.is_empty(), "{:?}", calls.units);
    }

    #[test]
    fn uninstalling_an_unregistered_app_service_is_not_an_error() {
        // SMAppService refuses to unregister what it does not know; an uninstall of a
        // service that is not there must not.
        for status in [AppServiceStatus::NotRegistered, AppServiceStatus::NotFound] {
            let manager = bundle_manager()
                .with_app_service(status)
                .failing_app_service_on(AppServiceAction::Unregister);
            uninstall(&spec(), &macos_bundle(), &manager).unwrap();
            assert!(
                !manager
                    .calls()
                    .app_service
                    .contains(&app_service_call(AppServiceAction::Unregister)),
                "{:?}",
                manager.calls().app_service
            );
        }
    }

    #[test]
    fn state_maps_smappservice() {
        for (status, print_fails, expected) in [
            (
                AppServiceStatus::RequiresApproval,
                false,
                ServiceState::NeedsApproval,
            ),
            (
                AppServiceStatus::NotRegistered,
                false,
                ServiceState::NotInstalled,
            ),
            (
                AppServiceStatus::NotFound,
                false,
                ServiceState::NotInstalled,
            ),
            (AppServiceStatus::Enabled, false, ServiceState::Running),
            (AppServiceStatus::Enabled, true, ServiceState::Stopped),
        ] {
            let manager = if print_fails {
                RecordingManager::failing_on("print")
            } else {
                RecordingManager::returning("\tstate = running\n")
            }
            .with_file(
                "/Applications/Sapphire.app/Contents/Library/LaunchAgents/\
                 net.sapphire-turtle.sapphire-agent.plist",
            )
            .with_app_service(status);
            assert_eq!(
                state(&spec(), &macos_bundle(), &manager).unwrap(),
                expected,
                "{status:?}, print failing: {print_fails}"
            );
            let calls = manager.calls();
            assert_eq!(
                calls.app_service,
                vec![app_service_call(AppServiceAction::Status)]
            );
            assert!(calls.units.is_empty(), "{:?}", calls.units);
        }
    }

    #[test]
    fn a_legacy_agent_is_retired_not_adopted_under_smappservice() {
        // Adopting it would write an agent file next to the registered service: two copies.
        let manager = bundle_manager()
            .with_app_service(AppServiceStatus::Enabled)
            .with_unit(legacy_plist(), &legacy_body());
        state(&spec(), &macos_bundle(), &manager).unwrap();
        let calls = manager.calls();
        assert!(boots_out_legacy(&calls), "{:?}", calls.commands);
        assert_eq!(calls.removed, vec![legacy_plist()]);
        assert!(calls.units.is_empty(), "{:?}", calls.units);
    }

    #[test]
    fn smappservice_stop_and_start_go_through_launchctl() {
        let label = "gui/1000/net.sapphire-turtle.sapphire-agent";
        let manager = bundle_manager().with_app_service(AppServiceStatus::Enabled);
        stop(&spec(), &macos_bundle(), &manager).unwrap();
        start(&spec(), &macos_bundle(), &manager).unwrap();
        assert_eq!(
            manager.calls().commands,
            vec![
                command(&["launchctl", "kill", "SIGTERM", label]),
                command(&["launchctl", "kickstart", "-k", label]),
            ]
        );
    }

    #[test]
    fn a_bundle_without_the_plist_keeps_the_agent_file() {
        let manager = RecordingManager::new();
        install(&spec(), &macos_bundle(), &manager).unwrap();
        let calls = manager.calls();
        assert!(calls.app_service.is_empty(), "{:?}", calls.app_service);
        assert_eq!(calls.units.len(), 1);
        assert_eq!(calls.units[0].0, current_plist());
    }

    #[test]
    fn no_test_touches_the_real_service_manager() {
        // Stated as a test so the intent is visible where it can be read: the real manager
        // is the only type that runs anything, and it is never constructed above.
        //
        // The needle is assembled from two pieces so that counting it does not count its
        // own text; the two remaining occurrences are the type's definition and the trait
        // implementation that follows it.
        let source = include_str!("manager.rs");
        let constructions = source.matches(concat!("System", "Manager")).count();
        assert!(
            constructions <= 2,
            "the real manager appears {constructions} times; tests must use RecordingManager"
        );
    }

    #[test]
    fn post_install_runs_after_activation() {
        let order = Arc::new(Mutex::new(Vec::<String>::new()));
        let recorded = Arc::clone(&order);
        let mut spec = spec();
        spec.post_install = Some(Box::new(move |_| {
            recorded.lock().unwrap().push("post_install".to_owned());
            Ok(())
        }));

        let manager = RecordingManager::ordered(Arc::clone(&order));
        install(&spec, &linux(), &manager).unwrap();

        let order = order.lock().unwrap().clone();
        assert_eq!(
            order.last().map(String::as_str),
            Some("post_install"),
            "{order:?}"
        );
    }

    #[test]
    fn a_failing_post_install_fails_the_install_and_says_what_was_done() {
        let mut spec = spec();
        spec.post_install = Some(Box::new(|_| Err(Error::Config("no room".into()))));

        let err = install(&spec, &linux(), &RecordingManager::default()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("no room"), "{message}");
        assert!(
            message.contains("installed"),
            "the service is installed and running; say so rather than leaving it ambiguous: \
            {message}"
        );
    }

    #[cfg(feature = "cli")]
    #[test]
    fn running_an_install_command_activates_the_service() {
        let manager = RecordingManager::default();
        let code = ServiceCommand::Install
            .run(&spec(), &linux(), &manager)
            .unwrap();

        assert_eq!(code, 0, "a successful install is a zero exit");
        assert_eq!(manager.calls().units.len(), 1);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn running_an_uninstall_command_removes_the_unit() {
        let manager = RecordingManager::default();
        let code = ServiceCommand::Uninstall
            .run(&spec(), &linux(), &manager)
            .unwrap();

        assert_eq!(code, 0);
        assert_eq!(manager.calls().removed.len(), 1);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn running_a_status_command_reports_what_the_manager_said() {
        let manager = RecordingManager::returning("active (running)");
        let code = ServiceCommand::Status
            .run(&spec(), &linux(), &manager)
            .unwrap();
        assert_eq!(code, 0, "a status that answered is a zero exit");
    }

    #[cfg(feature = "cli")]
    #[test]
    fn a_failing_install_through_the_command_is_an_error() {
        let manager = RecordingManager::failing_on("enable");
        let err = ServiceCommand::Install
            .run(&spec(), &linux(), &manager)
            .unwrap_err();
        assert!(err.to_string().contains("enable"), "{err}");
    }
}
