//! The app's background services across an update: the running ones are stopped before it
//! is applied, noted in the data directory, and started again at the next start (or at
//! once, when the apply fails).

/// An app's background services, for [`PackageSpec::services`](crate::PackageSpec::services):
/// [`Services::NONE`], or with the `service` feature a list given to `Services::new`.
/// Functions, as a service's spec holds owned values a static cannot.
#[derive(Clone, Copy, Debug)]
pub struct Services {
    #[cfg(feature = "service")]
    list: &'static [fn() -> corundum_framework_service::ServiceSpec],
}

impl Services {
    /// No background services.
    pub const NONE: Services = Services {
        #[cfg(feature = "service")]
        list: &[],
    };

    /// These services: each function builds one's spec.
    #[cfg(feature = "service")]
    pub const fn new(
        services: &'static [fn() -> corundum_framework_service::ServiceSpec],
    ) -> Services {
        Services { list: services }
    }

    /// Whether there are none.
    #[cfg(feature = "service")]
    pub(crate) fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Each service's spec, built.
    #[cfg(feature = "service")]
    pub(crate) fn specs(&self) -> impl Iterator<Item = corundum_framework_service::ServiceSpec> {
        self.list.iter().map(|service| service())
    }
}

/// What the updater asks of the app's services around an apply. The real one drives the
/// OS's service manager; without the `service` feature, and in an updater's tests, `()`
/// stands in and does nothing.
pub(crate) trait ServiceDriver: Send + Sync {
    /// Stops the running services and notes them, before an update is applied. An error
    /// stops the update.
    fn stop_running(&self) -> Result<(), String>;
    /// Starts the services noted by [`stop_running`](Self::stop_running) again, as an
    /// apply failed and nothing will start them otherwise.
    fn restart_stopped(&self);
}

/// No services: nothing to stop or start.
impl ServiceDriver for () {
    fn stop_running(&self) -> Result<(), String> {
        Ok(())
    }

    fn restart_stopped(&self) {}
}

#[cfg(feature = "service")]
pub(crate) use with_services::*;

#[cfg(not(feature = "service"))]
/// The driver for `spec`'s services: none without the `service` feature.
pub(crate) fn driver(
    _spec: &'static crate::start::PackageSpec,
) -> std::sync::Arc<dyn ServiceDriver> {
    std::sync::Arc::new(())
}

#[cfg(feature = "service")]
mod with_services {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use corundum_framework_platform::dirs::AppDirs;
    use corundum_framework_service::{
        Environment, ServiceManager, ServiceState, SystemManager, start, state, stop,
    };

    use super::ServiceDriver;
    use crate::start::PackageSpec;

    /// The file, in the app's data directory, naming the services an update stopped, one
    /// per line.
    pub(crate) const RESTART_MARKER: &str = "services-to-restart";

    /// The driver for `spec`'s services, through the OS's service manager.
    pub(crate) fn driver(spec: &'static PackageSpec) -> Arc<dyn ServiceDriver> {
        Arc::new(System {
            spec,
            data: AppDirs::new(spec.app_name).map(|dirs| dirs.data().to_owned()),
        })
    }

    /// The services of an app, driven through the OS's service manager.
    struct System {
        spec: &'static PackageSpec,
        /// Where the marker goes: `None` when the OS gives no data directory.
        data: Option<PathBuf>,
    }

    impl ServiceDriver for System {
        fn stop_running(&self) -> Result<(), String> {
            if self.spec.services.is_empty() {
                return Ok(());
            }
            // Without the marker an update would leave the services stopped: better not to
            // update.
            let data = self
                .data
                .as_deref()
                .ok_or("no data directory to note the services to restart in")?;
            stop_running(self.spec, data, &Environment::detect(), &SystemManager).map(|_| ())
        }

        fn restart_stopped(&self) {
            if let Some(data) = &self.data {
                restart_stopped(self.spec, data, &Environment::detect(), &SystemManager);
            }
        }
    }

    /// Stops each of `spec`'s services that is running, after noting their names in
    /// `data`'s [`RESTART_MARKER`]; returns them.
    ///
    /// The marker is written before anything is stopped, so a service is never left
    /// stopped without a note to start it again. A service whose state cannot be read may
    /// be running: it is stopped and noted all the same, rather than refusing every update
    /// until the cause goes away. `stop` cannot fail ("not running" is stopped), so only a
    /// marker that cannot be written refuses the update.
    pub(crate) fn stop_running(
        spec: &PackageSpec,
        data: &Path,
        env: &Environment,
        manager: &dyn ServiceManager,
    ) -> Result<Vec<&'static str>, String> {
        let mut running = Vec::new();
        for service in spec.services.specs() {
            match state(&service, env, manager) {
                Ok(ServiceState::Running) => running.push(service),
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(
                        "could not tell whether {} runs, stopping it anyway: {e}",
                        service.app_name
                    );
                    running.push(service);
                }
            }
        }
        if running.is_empty() {
            return Ok(Vec::new());
        }
        let names: String = running
            .iter()
            .map(|service| format!("{}\n", service.app_name))
            .collect();
        let marker = data.join(RESTART_MARKER);
        fs::create_dir_all(data)
            .and_then(|()| fs::write(&marker, names))
            .map_err(|e| format!("could not write {}: {e}", marker.display()))?;
        for service in &running {
            // Never an error: see above.
            let _ = stop(service, env, manager);
        }
        Ok(running.iter().map(|service| service.app_name).collect())
    }

    /// Starts each service named in `data`'s [`RESTART_MARKER`] that is still one of
    /// `spec`'s, then removes the file.
    ///
    /// Never fails: a start that fails, a name the app no longer lists, or a file that is
    /// garbage is logged, and the app goes on starting.
    pub(crate) fn restart_stopped(
        spec: &PackageSpec,
        data: &Path,
        env: &Environment,
        manager: &dyn ServiceManager,
    ) {
        let marker = data.join(RESTART_MARKER);
        let names = match fs::read(&marker) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                tracing::warn!("could not read {}: {e}", marker.display());
                String::new()
            }
        };
        for name in names.lines().map(str::trim).filter(|name| !name.is_empty()) {
            let Some(service) = spec
                .services
                .specs()
                .find(|service| service.app_name == name)
            else {
                tracing::warn!(
                    "{name:?} was stopped for an update but is no service of the app now"
                );
                continue;
            };
            if let Err(e) = start(&service, env, manager) {
                tracing::warn!("could not start {name} again after the update: {e}");
            }
        }
        if let Err(e) = fs::remove_file(&marker) {
            tracing::warn!("could not remove {}: {e}", marker.display());
        }
    }

    /// Uninstalls each of `spec`'s services, logging what fails: from the Windows uninstall
    /// hook, which has 30 seconds and no window.
    #[cfg(any(windows, test))]
    pub(crate) fn uninstall_all(
        spec: &PackageSpec,
        env: &Environment,
        manager: &dyn ServiceManager,
    ) {
        for service in spec.services.specs() {
            if let Err(e) = corundum_framework_service::uninstall(&service, env, manager) {
                tracing::warn!("could not uninstall {}: {e}", service.app_name);
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use std::fs;
        use std::path::PathBuf;

        use corundum_framework_service::{
            Environment, Os, RecordingManager, ServiceSpec, unit_path,
        };

        use super::*;
        use crate::services::Services;

        fn demo() -> ServiceSpec {
            ServiceSpec {
                app_name: "demo",
                description: "Demo daemon".into(),
                args: vec!["daemon".into()],
                legacy_labels: vec![],
                post_install: None,
            }
        }

        fn idle() -> ServiceSpec {
            ServiceSpec {
                app_name: "idle",
                ..demo()
            }
        }

        fn package(services: &'static [fn() -> ServiceSpec]) -> PackageSpec {
            PackageSpec {
                app_name: "demo",
                on_installed: None,
                on_uninstalling: None,
                before_apply: None,
                cli: None,
                services: Services::new(services),
            }
        }

        fn home() -> PathBuf {
            PathBuf::from("/home/user")
        }

        fn linux() -> Environment {
            Environment {
                euid: 1000,
                os: Os::Linux,
                home: Some(home()),
            }
        }

        fn command(parts: &[&str]) -> Vec<String> {
            parts.iter().map(|part| (*part).to_owned()).collect()
        }

        /// A manager on which both services are installed, `demo` running and `idle`
        /// stopped.
        fn installed() -> RecordingManager {
            RecordingManager::new()
                .with_unit(
                    unit_path("demo", &home()),
                    "[Service]\nExecStart=/usr/bin/demo daemon\n",
                )
                .with_unit(
                    unit_path("idle", &home()),
                    "[Service]\nExecStart=/usr/bin/idle daemon\n",
                )
                .with_file("/usr/bin/demo")
                .with_file("/usr/bin/idle")
                // The first matching answer wins: `idle`'s `is-active` is inactive.
                .answering("idle", "inactive")
                .answering("is-active", "active")
        }

        fn marker(data: &Path) -> PathBuf {
            data.join(RESTART_MARKER)
        }

        #[test]
        fn a_running_service_is_stopped_and_marked() {
            let data = tempfile::tempdir().unwrap();
            let manager = installed();
            let stopped = stop_running(&package(&[demo]), data.path(), &linux(), &manager).unwrap();
            assert_eq!(stopped, ["demo"]);
            assert_eq!(fs::read_to_string(marker(data.path())).unwrap(), "demo\n");
            assert!(manager.calls().commands.contains(&command(&[
                "systemctl",
                "--user",
                "stop",
                "demo"
            ])));
        }

        #[test]
        fn only_the_spec_s_running_services_are_stopped() {
            let data = tempfile::tempdir().unwrap();
            let manager = installed();
            let stopped =
                stop_running(&package(&[demo, idle]), data.path(), &linux(), &manager).unwrap();
            assert_eq!(stopped, ["demo"]);
            assert_eq!(fs::read_to_string(marker(data.path())).unwrap(), "demo\n");
            let commands = manager.calls().commands;
            assert!(!commands.contains(&command(&["systemctl", "--user", "stop", "idle"])));

            // A service the app no longer lists (installed by an earlier version) is neither
            // asked about nor stopped, and with no services no marker is written.
            let data = tempfile::tempdir().unwrap();
            let manager = installed();
            let stopped = stop_running(&package(&[]), data.path(), &linux(), &manager).unwrap();
            assert!(stopped.is_empty());
            assert!(manager.calls().commands.is_empty());
            assert!(!marker(data.path()).exists());
        }

        #[test]
        fn the_marked_services_start_and_the_marker_goes() {
            let data = tempfile::tempdir().unwrap();
            fs::write(marker(data.path()), "demo\n").unwrap();
            let manager = RecordingManager::new();
            restart_stopped(&package(&[demo, idle]), data.path(), &linux(), &manager);
            assert_eq!(
                manager.calls().commands,
                [command(&["systemctl", "--user", "start", "demo"])]
            );
            assert!(!marker(data.path()).exists());
        }

        #[test]
        fn a_marker_naming_an_unknown_service_is_dropped() {
            let data = tempfile::tempdir().unwrap();
            fs::write(marker(data.path()), b"gone\n\xff\n").unwrap();
            let manager = RecordingManager::new();
            restart_stopped(&package(&[demo]), data.path(), &linux(), &manager);
            assert!(manager.calls().commands.is_empty());
            assert!(!marker(data.path()).exists());

            // No marker at all: nothing to do, and nothing goes wrong.
            restart_stopped(&package(&[demo]), data.path(), &linux(), &manager);
            assert!(manager.calls().commands.is_empty());
        }

        #[test]
        fn a_failed_start_is_logged_and_the_marker_still_goes() {
            let data = tempfile::tempdir().unwrap();
            fs::write(marker(data.path()), "demo\nidle\n").unwrap();
            let manager = RecordingManager::failing_on("start");
            restart_stopped(&package(&[demo, idle]), data.path(), &linux(), &manager);
            // A failure starting one does not keep the next from starting.
            assert_eq!(manager.calls().commands.len(), 2);
            assert!(!marker(data.path()).exists());
        }

        #[test]
        fn a_service_whose_state_cannot_be_read_is_stopped_and_marked() {
            // Without a home directory the unit cannot be found: the service may be
            // running, so it is stopped and marked, and the update goes on.
            let data = tempfile::tempdir().unwrap();
            let env = Environment {
                home: None,
                ..linux()
            };
            let manager = installed();
            let stopped = stop_running(&package(&[demo]), data.path(), &env, &manager).unwrap();
            assert_eq!(stopped, ["demo"]);
            assert_eq!(fs::read_to_string(marker(data.path())).unwrap(), "demo\n");
            assert!(manager.calls().commands.contains(&command(&[
                "systemctl",
                "--user",
                "stop",
                "demo"
            ])));
        }

        #[test]
        fn a_marker_that_cannot_be_written_stops_nothing() {
            // The marker is written first: one that cannot be written fails the stop
            // before anything is stopped, so no service is left stopped and unnoted.
            let data = tempfile::tempdir().unwrap();
            let unwritable = data.path().join("file");
            fs::write(&unwritable, "").unwrap();
            let manager = installed();
            let err = stop_running(&package(&[demo]), &unwritable, &linux(), &manager).unwrap_err();
            assert!(err.contains(RESTART_MARKER), "{err}");
            assert!(!manager.calls().commands.contains(&command(&[
                "systemctl",
                "--user",
                "stop",
                "demo"
            ])));
        }

        #[test]
        fn uninstall_all_uninstalls_each_service() {
            let manager = RecordingManager::new();
            uninstall_all(&package(&[demo, idle]), &linux(), &manager);
            assert_eq!(
                manager.calls().removed,
                [unit_path("demo", &home()), unit_path("idle", &home())]
            );
        }
    }
}
