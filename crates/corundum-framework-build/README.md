# corundum-framework-build

Build-time helpers for corundum apps, for the app's `build.rs`.

`embed_icon` takes the app's SVG icon and, when the app is built for Windows, embeds it in
the executable as its icon. On other targets it only tells Cargo to rerun when the SVG
changes.

```toml
[build-dependencies]
corundum-framework-build = "0.2"
```

```rust,ignore
fn main() {
    corundum_framework_build::embed_icon("assets/icon.svg");
}
```

If the icon cannot be embedded (for example a cross build without a resource compiler), the
build prints a `cargo:warning=` and goes on; it never fails the app's build.

## License

Licensed under either of the Apache License, Version 2.0 or the MIT license, at your option.
