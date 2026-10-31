# corundum-framework-service

Registers an app's background service with the OS service manager, at user level: a systemd
user unit on Linux, a LaunchAgent on macOS, a scheduled task on Windows.

An app describes itself with a `ServiceSpec`; `install`, `uninstall`, `status`, `state`,
`stop` and `start` turn that into the platform's unit file and drive the manager; `state`
answers with a `ServiceState` (not installed, stopped, running, or broken with a reason). Everything goes through the
`ServiceManager` trait, whose real implementation is the only code that writes to the machine
or runs a command, so tests use `RecordingManager`. With the `cli` feature, `ServiceCommand`
is the `service install | uninstall | status` subcommand for a clap command line.

The executable the manager starts is the running one, or inside a Velopack package the file
that lasts: the AppImage itself on Linux.

Before uninstalling the app on macOS or Linux, remove its background service (Settings ›
About › Remove). Windows' uninstaller removes it.

Desktops only; on iOS and Android the crate is empty.
