# Releasing

The libraries — `corundum-framework-platform`, `corundum-framework-gui` and the facade
`corundum-framework` — are released with [release-plz](https://release-plz.dev/). The three
share one version (`version_group = "corundum"` in `release-plz.toml`) and go to crates.io
together. The facade makes the one `v{version}` tag and GitHub release, and writes
`CHANGELOG.md` for all three, as `sapphire-framework-workspace` does in sapphire-framework.
The demo app is not released.

## Normal flow

1. On every push to `main`, release-plz opens or updates a release PR (labelled `release`):
   the next version in each crate's `Cargo.toml` and the internal dependencies, the
   updated `Cargo.lock`, and a section for the version in `CHANGELOG.md`, from the
   conventional commits since the last release.
2. Review the release PR: the version, `CHANGELOG.md`, `Cargo.toml`.
3. Merge it: release-plz publishes the three crates to crates.io, tags `v{version}`, and
   makes the GitHub release, its notes the version's `CHANGELOG.md` section.

Pull requests do not touch `CHANGELOG.md`: their commit messages are what it is written
from, so write them for a reader of the changelog.

### The first release

0.1.0 is the version the crates already carry, so the first release PR keeps it. Its
section has the commits made in this repository, and under them what `CHANGELOG.md`'s
"Unreleased" held when it was written by hand — the move from sapphire-framework, in more
detail. Tidy the two into one section in that release PR, once.

## Version bump rules

release-plz infers the version from conventional commits (pre-1.0, `0.x.y`):

- `feat:`, `fix:` and other non-breaking changes → patch bump.
- `feat!:` / `fix!:`, or a `BREAKING CHANGE:` footer → minor bump.
- A change to the public API (a signature, a public field, a removed item) is breaking:
  mark its commit with `!`.

The apps must use the same egui major as these crates, so a bump of egui or eframe is a
breaking change too: make it a minor bump.

## Checking that the crates publish

```sh
cargo publish --workspace --exclude corundum-framework-demo --dry-run
```

packages and builds the three on their own, as crates.io will. `release-plz update` makes
the release PR's changes in the working tree, to see them before a push.

## Secrets

The workflow uses the sapphire-turtle organization's `RELEASE_PLZ_TOKEN` (a fine-grained PAT
with Contents and Pull requests read/write; the release PR is opened with it, not
`GITHUB_TOKEN`, so CI runs on it) and `CARGO_REGISTRY_TOKEN`.
