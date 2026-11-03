# corundum-framework-package

A desktop app as a [cargo-packager](https://github.com/crabnebula-dev/cargo-packager)
package, for Windows, macOS and Linux, with updates signed with minisign.

`start` is the first thing the app's `main` runs. Started by the Windows installer or
uninstaller for a hook, it runs the app's hook and exits; otherwise it tells the app whether
this start is its first, the first after an update, or neither, from the version the app
last ran with. Not in a package (`cargo run`), it says so and does nothing else.

```rust,ignore
use corundum_framework_package::{PackageSpec, Services, Start, start};

static SPEC: PackageSpec = PackageSpec {
    app_name: "my-app",
    version: env!("CARGO_PKG_VERSION"),
    // `cargo packager signer generate`'s public key.
    update_pubkey: "dW50cnVzdGVkIGNvbW1lbnQ6...",
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
  The feed is the release's `<app_name>-latest.json` (`Source::Github`: the latest
  release's, or with `prerelease` the newest one that has it), as cargo-packager-updater
  reads it: the version, and per platform the package's URL, its minisign signature and its
  format. A package whose signature does not match `PackageSpec::update_pubkey` is not
  installed. `apply` installs the update: on macOS and Linux at once, on Windows by a
  silent installer that waits for the process to end. With `After::Exit` (a daemon, a
  command) it exits the process; with a restart it returns, and the app ends the process
  itself, a GUI by closing its window; the app starts again once it has. `apply_pending_at_start`
  applies, at a GUI's start, an update downloaded earlier and never applied.
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

The packages are cargo-packager's (sapphire-turtle/.github's `release-packager.yml` makes
them): an AppImage on Linux, a signed and notarized `.app` on macOS (and a `.tar.gz` of it
for updates), an NSIS installer on Windows, installing for the current user. Each package
file has its minisign signature (`.sig`), and the feed `<app_name>-latest.json` lists them.

- Windows: the installer is made from this crate's NSIS template (`nsis/installer.nsi`,
  cargo-packager's with the hooks added), which runs `<app> --corundum-package installed`
  after every install and `--corundum-package uninstalling` before an uninstall. The
  console shim is installed as `<command>.exe` beside the GUI executable, and calls
  `console::forward("<app>-desktop.exe")`. Without the shim (`cargo install`, or before
  packaging) the GUI executable prints in the terminal through
  `corundum_framework_platform::console::attach_parent`, which the shell does not wait for:
  no exit code, no typed input.
- macOS and Linux: the command on `PATH` (`CliSpec::command`) links to the running
  executable, whatever its name.
- The product name equals `LauncherSpec::name`, so the installer's shortcut and the
  launcher's path coincide.

On iOS and Android the crate is empty: the store installs and updates the app there.

## License

Licensed under either of the Apache License, Version 2.0 or the MIT license, at your option.
