# corundum-framework-package

A desktop app as a [Velopack](https://velopack.io) package, for Windows, macOS and Linux.

`start` is the first thing the app's `main` runs. Started by the Windows installer or
uninstaller for a hook, it runs the app's hook and exits; otherwise it tells the app whether
this start is its first, the first after an update, or neither, from the version the app
last ran with. Not in a package (`cargo run`), it says so and does nothing else.

```rust,ignore
use corundum_framework_package::{PackageSpec, Start, start};

fn main() {
    let spec = PackageSpec {
        app_name: "sapphire-journal",
        on_installed: None,
        on_uninstalling: None,
        before_apply: None,
        cli: None,
    };
    if let Start::Updated { from, to } = start(&spec) {
        println!("updated from {from} to {to}");
    }
}
```

On iOS and Android the crate is empty: the store installs and updates the app there.

## License

Licensed under either of the Apache License, Version 2.0 or the MIT license, at your option.
