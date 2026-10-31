# Activation Message and Registered Program Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A second start can tell the running window which page to show, and an app can find the program another app's OS service starts — for sapphire-bridge's window (sapphire-framework#363).

**Architecture:** The single-instance socket's one-line message grows an optional payload (`activate <message>`); a sender falls back to the plain form when an older window does not answer. `corundum-framework-service` exposes the program its `state` already reads from each OS's registration.

**Tech Stack:** Rust 2024; corundum-framework-platform (`instance`), corundum-framework-gui (`window`), corundum-framework-service.

**Spec:** sapphire-framework's `docs/superpowers/specs/2026-10-31-bridge-desktop-design.md`, section "corundum-framework 0.6".

## Global Constraints

- Additive only: `activate`, `on_activate`, `acquire_or_activate` keep their signatures and behaviour.
- Wire: `activate\n` or `activate <message>\n`; message is UTF-8, 1..=1024 bytes, no `\n` or `\r`.
- The message is opaque to corundum.
- Release: release-plz picks the version; no manual bump.

## Review Focus

- An old window (plain `activate` only) receiving `activate page` must still come to the front: sender falls back.
- A message with a newline or over 1024 bytes: `activate_with` returns false without sending a malformed line.
- A listener registered with `on_activate` (no message) still fires for `activate <message>`.
- `registered_program` on an unregistered service: `Ok(None)`, not an error.
- Windows `schtasks /query` failing (no task): `Ok(None)`.

---

### Task 1: Activation message (platform + gui)

**Files:**
- Modify: `crates/corundum-framework-platform/src/instance.rs` (`Message`, `AlreadyRunning`, `Instance`, the desktop `Held` listener, the private `acquire_or_activate_with` renamed `acquire_or_activate_at`)
- Modify: `crates/corundum-framework-gui/src/window/mod.rs` (`WindowOptions::on_activate`)

**Interfaces:**
- Produces:
  - `enum Message { Activate(Option<String>) }`; `fn encode(&self) -> Vec<u8>`; `fn decode(&[u8]) -> Option<Message>`; `MAX_MESSAGE = 1024 + 16`.
  - `pub const MAX_ACTIVATION_MESSAGE: usize = 1024;`
  - `impl AlreadyRunning { pub fn activate_with(&self, message: &str) -> bool }` — false for an invalid message (nothing sent); sends `activate <message>`, and if unanswered within the existing wait, sends plain `activate` and returns its answer.
  - `impl Instance { pub fn on_activate_with(&mut self, f: impl Fn(Option<&str>) + Send + 'static) }`; `on_activate(f)` becomes `on_activate_with(move |_| f())`.
  - `pub fn acquire_or_activate_with(app_name: &str, message: Option<&str>) -> Option<Instance>` (and `acquire_or_activate_with_in(app_name, data_dir, message)` `#[doc(hidden)]`); `acquire_or_activate` = `acquire_or_activate_with(app_name, None)`.
  - `impl WindowOptions { pub fn on_activate(self, f: impl Fn(Option<String>) + Send + 'static) -> WindowOptions }` (feature `single-instance`): called after corundum brings the window to the front, for both a `single_instance` lock and a held `instance`.

- [ ] **Step 1: Failing tests** in `instance.rs` tests:
  - `message_round_trips`: `decode(encode(Activate(None)))` and `Activate(Some("workgroup"))` round-trip; `decode(b"activate workgroup\n") == Some(Activate(Some("workgroup")))`; `decode(b"activate \n") == None`; a 1025-byte payload decodes to `None`.
  - `activate_with_delivers_the_message`: as the existing two-start test, first `on_activate_with` records into a channel; second `activate_with("devices")` → true; channel receives `Some("devices")`.
  - `activate_delivers_none`: plain `activate()` → `None` received.
  - `invalid_messages_are_not_sent`: `activate_with("a\nb")` and a 1025-byte string → false, and the listener receives nothing within 500 ms.
  - `on_activate_fires_for_a_message`: listener via `on_activate(|| tx.send(()))`; `activate_with("x")` → fired.
  - `an_old_listener_still_comes_to_front`: a raw listener on the socket that answers `ok\n` only to exactly `activate\n` (simulating the old decode); `activate_with("x")` → true, and the raw listener saw two connections, the second `activate\n`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-platform --features single-instance instance` — Expected: FAIL (does not compile / assertions).
- [ ] **Step 3: Implement** the Produces list; the listener decodes, calls the callback with the payload, replies `ok\n`.
- [ ] **Step 4: Failing test** in `window/mod.rs` tests: `on_activate_is_kept_for_the_instance` — `WindowOptions::new("x").on_activate(|_| {})` stores the callback (assert via a `#[cfg(test)]` accessor `has_on_activate()`).
- [ ] **Step 5: Implement** `WindowOptions::on_activate`: store `Option<Box<dyn Fn(Option<String>) + Send>>`; `come_to_front_on_activate` uses `on_activate_with`, brings to front, then calls it with `msg.map(str::to_owned)`.
- [ ] **Step 6: Run** `cargo test -p corundum-framework-platform --features single-instance` and `cargo test -p corundum-framework-gui --no-default-features --features single-instance` — Expected: PASS.
- [ ] **Step 7: Commit** `feat(instance): a message with the activation`

### Task 2: `registered_program`

**Files:**
- Modify: `crates/corundum-framework-service/src/manager.rs` (extract from `state`), `src/lib.rs` (re-export)

**Interfaces:**
- Produces: `pub fn registered_program(spec: &ServiceSpec, env: &Environment, manager: &dyn ServiceManager) -> Result<Option<PathBuf>>` — `Ok(None)` when not registered or the registration names no program; `state` uses it.

- [ ] **Step 1: Failing tests** (with `RecordingManager`, as `state`'s tests do):
  - `registered_program_reads_each_os`: Linux unit `ExecStart=/opt/b/sapphire-bridge serve` → `Some("/opt/b/sapphire-bridge")`; macOS plist with ProgramArguments `/Applications/B.app/Contents/MacOS/b` → that path; Windows task XML `<Command>C:\b\b.exe</Command>` → that path.
  - `registered_program_is_none_when_not_registered`: no unit (Linux, macOS); `schtasks` failing (Windows) → `Ok(None)`.
- [ ] **Step 2: Run** `cargo test -p corundum-framework-service registered_program` — Expected: FAIL.
- [ ] **Step 3: Implement**; refactor `state` to call it (existing `state` tests must still pass).
- [ ] **Step 4: Run** `cargo test -p corundum-framework-service` — Expected: PASS.
- [ ] **Step 5: Commit** `feat(service): registered_program`

### Task 3: Docs, CI, release notes

**Files:**
- Modify: `CHANGELOG.md` `[Unreleased]` → `### Added` lines for `instance::{activate_with, on_activate_with, acquire_or_activate_with}`, `WindowOptions::on_activate`, `service::registered_program` (sapphire-framework#363).

- [ ] **Step 1:** `cargo fmt --all --check`, the CI clippy lines (`.github/workflows/ci.yml`, incl. `single-instance` features), `cargo test --workspace --all-features --locked` — Expected: clean.
- [ ] **Step 2: Commit** `docs: changelog for the activation message and registered_program`
