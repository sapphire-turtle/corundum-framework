# Attach Console Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Windows GUI-subsystem app binary prints in the terminal it was started from, without the shim, so it can register itself as a service that opens no console window (#89).

**Architecture:** `corundum_framework_platform::console::attach_parent()`, behind an off-by-default `console` feature, attaches the parent's console for the standard handles a GUI-subsystem process lacks. The umbrella crate forwards the feature; the demo calls it first in `main`. Documentation in the service, package, platform and top-level READMEs stops assuming the shim.

**Tech Stack:** Rust 2024 workspace, `windows` 0.62 (`Win32_System_Console`, `Win32_Storage_FileSystem`, `Win32_Foundation`).

**Spec:** `docs/superpowers/specs/2026-11-01-attach-console-design.md`

## Global Constraints

- Signature: `pub fn attach_parent() -> bool` in `crates/corundum-framework-platform/src/console.rs`; returns `false` at once outside Windows.
- Platform feature: `console = ["windows/Win32_System_Console"]`, not in `default`. `Win32_Foundation` and `Win32_Storage_FileSystem` are already always on.
- Umbrella feature: `console = ["platform", "corundum-framework-platform/console"]`.
- No change to `ServiceSpec`, `render_task` or the task XML golden file.
- Errors are not reported (no console, maybe no logging yet); a handle that cannot be opened stays missing.
- Present handles (shim's, a pipe, a file redirection) are never replaced.
- Doc comments and READMEs follow the repo's prose style: full sentences, no marketing words; `README.ja.md` gets the Japanese counterpart of every `README.md` change.

## Review Focus

- stdout redirected to a file, stderr still missing: only stderr (and stdin) are set; stdout stays the file. Covered by the per-handle rule in Task 1 Step 3; checked by hand (spec's Windows step 2).
- Started by the Task Scheduler or a double-click (no parent console): `AttachConsole` fails, nothing opens, returns `false`. Checked by hand (spec's step 3).
- Called a second time: every handle is present after the first call, so it returns `false` without attaching again. Follows from Step 3's first check.
- Built for Windows with `--no-default-features --features console`: the module must compile with only `Win32_Foundation`, `Win32_Storage_FileSystem` and `Win32_System_Console`. Task 1 Step 5 checks it with the windows-gnu target.
- Through the packaged shim: the shim's inherited handles are present, so `attach_parent` returns `false` and the shim's exit code and Ctrl+C behave as before. Checked by hand (spec's step 4).

---

### Task 1: `platform::console::attach_parent` behind the `console` feature

**Files:**
- Create: `crates/corundum-framework-platform/src/console.rs`
- Modify: `crates/corundum-framework-platform/Cargo.toml` (feature comment block above `[features]`, the `[features]` table)
- Modify: `crates/corundum-framework-platform/src/lib.rs` (module list near `#[cfg(feature = "single-instance")] pub mod instance;`, and the crate doc's feature paragraphs)
- Modify: `crates/corundum-framework/Cargo.toml` (feature after `single-instance`), `crates/corundum-framework/src/lib.rs` (feature table row after `single-instance`)
- Modify: `.github/workflows/ci.yml` (platform clippy lines)

**Interfaces:**
- Produces: `corundum_framework_platform::console::attach_parent() -> bool`, reachable as `corundum_framework::platform::console::attach_parent()` with the umbrella's `console` feature.

- [ ] **Step 1: Write the failing test** at the bottom of `console.rs`, with only `pub fn attach_parent() -> bool { todo!() }` above it and `pub mod console;` declared under the feature:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn with_every_standard_handle_present_it_does_not_attach() {
        // cargo gives a test process its handles (pipes or the terminal), as the shim does.
        assert!(!super::attach_parent());
        assert!(!super::attach_parent(), "a second call changes nothing either");
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p corundum-framework-platform --features console --locked console::`
Expected: FAIL, panicked at `not yet implemented`.

- [ ] **Step 3: Implement `attach_parent`**

Non-Windows: `false`. Windows (a `#[cfg(windows)]` private `fn attach() -> bool`):
1. For `STD_INPUT_HANDLE`, `STD_OUTPUT_HANDLE`, `STD_ERROR_HANDLE`, a handle is missing when `GetStdHandle` returns `Err` (windows 0.62 maps both null and `INVALID_HANDLE_VALUE` to `Err`). None missing → `false`.
2. `AttachConsole(ATTACH_PARENT_PROCESS)`; `Err` → `false`.
3. For each missing handle only: `CreateFileW` on `w!("CONIN$")` for input, `w!("CONOUT$")` for output and error, `GENERIC_READ | GENERIC_WRITE`, `FILE_SHARE_READ | FILE_SHARE_WRITE`, no security attributes, `OPEN_EXISTING`, no flags, no template; on `Ok`, `SetStdHandle`; errors ignored.
4. `true`.

Each `unsafe` block gets a `// SAFETY:` line as `package/src/console.rs` does. Module doc: what it is for (a GUI-subsystem binary that is also a command line, run without the shim), when to call it (first in `main`), and spec decision 3's limits — the shell does not wait for a GUI-subsystem process, so the prompt comes back first, output lands after it, the exit code is lost and typed input goes to the shell; the full command line is the package's shim (`corundum_framework_package::console::forward`).

- [ ] **Step 4: Run the test**

Run: `cargo test -p corundum-framework-platform --features console --locked console::`
Expected: PASS (1 test).

- [ ] **Step 5: Check the Windows code compiles, alone and with the defaults**

Run:
```bash
cargo clippy -p corundum-framework-platform --target x86_64-pc-windows-gnu --all-targets --no-default-features --features console --locked -- -D warnings
cargo clippy -p corundum-framework-platform --target x86_64-pc-windows-gnu --all-targets --features console --locked -- -D warnings
cargo clippy -p corundum-framework-platform --all-targets --features console --locked -- -D warnings
```
Expected: all three finish with no warnings. (If the windows-gnu clippy fails in a C build script unrelated to this crate's code, note it and rely on the Windows CI job.)

- [ ] **Step 6: Wire the feature**

- Platform `Cargo.toml`: a comment paragraph in the feature block (``console`: `console::attach_parent`, a Windows GUI-subsystem binary that prints in the terminal it was started from. Off by default: only a binary that is both a window and a command line wants it.``) and `console = ["windows/Win32_System_Console"]`.
- Platform `lib.rs` crate doc: a paragraph "With the `console` feature, `console` …" beside the `single-instance` one.
- Umbrella `Cargo.toml`: a commented `console = ["platform", "corundum-framework-platform/console"]`.
- Umbrella `lib.rs` table: `| `console` | `platform::console` | a Windows GUI-subsystem binary that also answers in a terminal without the package's shim (brings `platform`) |`.
- `ci.yml`: add `console` to the `for f in media-picker qr-scan` platform loop, and after the `--no-default-features --features launcher` line add `cargo clippy -p corundum-framework-platform --all-targets --no-default-features --features console --locked -- -D warnings`.

- [ ] **Step 7: Verify**

Run: `cargo clippy -p corundum-framework --all-targets --features console --locked -- -D warnings && cargo test -p corundum-framework-platform --all-features --locked`
Expected: no warnings; all tests pass.

- [ ] **Step 8: Commit**

```bash
git add crates/corundum-framework-platform crates/corundum-framework .github/workflows/ci.yml
git commit -m "feat(platform): console::attach_parent, the parent's console for a GUI binary (#89)"
```

### Task 2: The demo attaches, and the documentation stops assuming the shim

**Files:**
- Modify: `demo/Cargo.toml` (desktop `corundum-framework` dependency's features)
- Modify: `demo/src/main.rs` (first line of the desktop `main`)
- Modify: `crates/corundum-framework-service/src/exe.rs` (module doc / `service_exe` doc), `crates/corundum-framework-service/src/manager.rs:356-362` (`install` doc), `crates/corundum-framework-service/README.md`
- Modify: `crates/corundum-framework-package/src/console.rs` (module doc), `crates/corundum-framework-package/README.md` (near line 73, the shim)
- Modify: `README.md` (around line 84, the `windows_subsystem` note), `README.ja.md` (line 35, same note), `demo/README.md` if it describes the command line on Windows

**Interfaces:**
- Consumes: `corundum_framework::platform::console::attach_parent() -> bool` (Task 1), umbrella feature `console`.

- [ ] **Step 1: Demo**

Add `"console"` to the desktop dependency's features in `demo/Cargo.toml` (and to the comment above it if it lists them). In `demo/src/main.rs`, first statement of the desktop `main`, before `command_line` (whose `--help` prints):

```rust
    // Without the package's shim (a `cargo install`ed demo), `--help` and `--version` reach
    // the terminal through its console; the daemon's task, with no parent console, opens none.
    corundum_framework::platform::console::attach_parent();
```

- [ ] **Step 2: Verify the demo builds on Linux and for Windows**

Run:
```bash
cargo build -p corundum-framework-demo --bin corundum-framework-demo --locked
cargo clippy -p corundum-framework-demo --target x86_64-pc-windows-gnu --bins --locked -- -D warnings
```
Expected: both succeed (same build-script caveat as Task 1 Step 5).

- [ ] **Step 3: Documentation**

What each place says (wording is the implementer's, in the repo's style):
- `service_exe` / `exe.rs` and `install`: on Windows the registered executable is started by a logon task; a console-subsystem binary opens a console window at every logon, and closing it stops the service. So an app that registers itself is a GUI-subsystem binary (`windows_subsystem = "windows"`), with its command line through the package's shim or `platform::console::attach_parent`. Replace the sentence "started from its console shim on Windows, the running executable is the GUI-subsystem binary, which opens no console window."
- Service README: one sentence of the same.
- Package `console.rs` module doc and README: a binary without the shim (`cargo install`, before packaging) uses `platform::console::attach_parent` for its output; the shim stays the full command line (waits, exit code, input).
- `README.md` / `README.ja.md`: after the `windows_subsystem` note, that a binary which is also a command line calls `platform::console::attach_parent()` first in `main` (`console` feature) and is shipped with the shim for the full command line; and that a console-subsystem binary registered as a service opens a window at every logon.

- [ ] **Step 4: Verify docs build and the workspace passes**

Run: `cargo doc --workspace --all-features --no-deps --locked 2>&1 | grep -E "warning|error" ; cargo test --workspace --all-features --locked && cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
Expected: no doc warnings (intra-doc links resolve), all tests pass, no clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add demo crates/corundum-framework-service crates/corundum-framework-package README.md README.ja.md
git commit -m "docs: a service executable on Windows is a GUI binary; the demo attaches its console (#89)"
```

### Task 3: Hand check on Windows (the user, before merge)

Not automatable (needs a GUI-subsystem process started from a console). Listed in the PR description as a checklist, from the spec's Testing section:

- [ ] `cargo install --path demo`, then in `cmd`: `corundum-framework-demo --help` prints its usage.
- [ ] `corundum-framework-demo --help > out.txt` writes the file and prints nothing.
- [ ] The daemon registered from the demo (first run's service page) starts at the next logon with no console window.
- [ ] Through the packaged shim (`demo-release.yml` build): output, exit code and Ctrl+C behave as before.
