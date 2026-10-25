# Releasing

The libraries — `corundum-framework-platform`, `corundum-framework-gui` and the facade
`corundum-framework` — are released with [release-plz](https://release-plz.dev/), to
crates.io only: no tags, no GitHub releases, as the sapphire-turtle libraries are. The
three share one version (`version_group = "corundum"` in `release-plz.toml`). The demo app
is not released.

## Normal flow

1. On every push to `main`, release-plz opens or updates a release PR (labelled `release`)
   with the next version in each crate's `Cargo.toml` and the internal dependencies.
2. In the release PR, move `CHANGELOG.md`'s "Unreleased" section under the new version:
   rename its heading to `## [x.y.z] - YYYY-MM-DD`, and leave an empty `## [Unreleased]`
   above it. release-plz does not write the changelog (`changelog_update = false`): it is
   written by hand as each change lands.
3. Merge the release PR: release-plz publishes the three crates to crates.io.

The first release, 0.1.0, is the version the crates already carry: the first release PR
keeps it and only asks for the changelog.

## Version bump rules

release-plz infers the version from conventional commits (pre-1.0, `0.x.y`):

- `feat:`, `fix:` and other non-breaking changes → patch bump.
- `feat!:` / `fix!:`, or a `BREAKING CHANGE:` footer → minor bump.
- A change to the public API (a signature, a public field, a removed item) is breaking:
  mark its commit with `!`.

The apps must use the same egui major as these crates, so a bump of egui or eframe is a
breaking change too: make it a minor bump.

## Before the first release

Check that the crates package and build on their own, as crates.io will:

```sh
cargo publish --workspace --exclude corundum-framework-demo --dry-run
```

## Required secrets

- `RELEASE_PLZ_TOKEN`: a fine-grained PAT with Contents and Pull requests read/write. The
  release PR is opened with it, not `GITHUB_TOKEN`, so CI runs on it.
- `CARGO_REGISTRY_TOKEN`: a crates.io token able to publish the three crates.
