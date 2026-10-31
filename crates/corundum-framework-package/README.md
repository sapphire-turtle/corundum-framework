# corundum-framework-package

A desktop app as a [Velopack](https://velopack.io) package, for Windows, macOS and Linux.

`start` is the first thing the app's `main` runs. Started by the Windows installer or
uninstaller for a hook, it runs the app's hook and exits; otherwise it tells the app whether
this start is its first, the first after an update, or neither, from the version the app
last ran with. Not in a package (`cargo run`), it says so and does nothing else.

```rust,ignore
use corundum_framework_package::{PackageSpec, Services, Start, start};

static SPEC: PackageSpec = PackageSpec {
    app_name: "my-app",
    on_installed: None,
    on_uninstalling: None,
    before_apply: None,
    cli: None,
    services: Services::NONE,
};

fn main() {
    if let Start::Updated { from, to } = start(&SPEC) {
        println!("updated from {from} to {to}");
    }
}
```

The crate's other parts:

- `Updater` finds and downloads the app's updates on a thread of its own, and applies one
  only when asked, after the app's `before_apply` has stopped its background processes.
  `apply` hands the update to Velopack's updater: with `After::Exit` (a daemon, a command)
  it exits the process; with a restart it returns, and the app ends the process itself, a
  GUI by closing its window. `apply_pending_at_start` applies, at a GUI's start, an update
  downloaded earlier and never applied.
- `cli_path` puts the app's command line on `PATH`: the user `Path` on Windows, from the
  install hook; a link in `/usr/local/bin` on macOS and in `~/.local/bin` on Linux, when
  the user asks. Another program's command of the same name is left alone.
- `console` is the Windows console shim: a GUI binary has no console, so a small console
  binary on `PATH` starts it with the terminal's arguments and streams.

## Background services

`PackageSpec::services` names the app's background services: `Services::NONE`, or with the
`service` feature `Services::new(&[my_service])`, each a function building the service's
`corundum_framework_service::ServiceSpec`. With them:

- `Updater::apply` and `apply_pending_at_start` stop the services that are running after
  the app's `before_apply`, and note them in `services-to-restart` in the app's data
  directory first. A service whose state cannot be read is stopped and noted all the same;
  only a note that cannot be written refuses the update. When the apply fails, they are
  started again at once.
- `start` starts the services an update stopped, on every start, packaged or not, and
  removes the note: whichever of the app's programs starts first after the update does it.
- On Windows, the uninstall hook uninstalls each service before the files go. On macOS and
  Linux nothing runs at uninstall: remove the background service first (Settings › About ›
  Remove), or its unit is left pointing at a program
  that is gone.

## Packaging

The command on `PATH` (`CliSpec::command`) need not be the cargo binary's name. On macOS and
Linux it links to the running executable, whatever `vpk --mainExe` named it; on Windows it
is the shim `<command>.exe` beside the GUI executable in `current\`.

Cargo cannot build two binaries of one name in a workspace (cargo#6313), so an app's desktop
package is built as `<app>-desktop`, while its headless build keeps `<app>`. Pack it so:

- macOS and Linux: rename the desktop binary to `<app>` in the pack directory, and run
  `vpk pack --mainExe <app>`.
- Windows: keep the GUI executable's name, add the console shim as `<app>.exe`, and run
  `vpk pack --mainExe <app>-desktop.exe`. The shim calls `console::forward("<app>-desktop.exe")`.
- `--packTitle` equals `LauncherSpec::name`, so the installer's shortcut and the launcher's
  path coincide.
- Velopack's `Update.exe` (`UpdateMac` on macOS) stays beside the executable; vpk puts it
  there, and the package's detection looks for it.

On iOS and Android the crate is empty: the store installs and updates the app there.

## License

Licensed under either of the Apache License, Version 2.0 or the MIT license, at your option.
