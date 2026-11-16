# View Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let any `ShellApp` refresh its open screen. The shell provides a status bar
button, F5 / Cmd+R, and a pull-down helper, and all three go through one callback.

**Architecture:** `ShellApp` gets `refresh_state(screen) -> Refresh` and
`refresh(screen)`, both with defaults. In each frame the shell:

1. reads the state and stores it in `ctx` temp data for the pull helper;
2. collects requests from the keys, the status bar button and the helper;
3. calls `refresh` once, and only when the state is `Idle`.

The plumbing goes in a new `shell/refresh.rs` and the helper in `shell/pull.rs`, so the
2000-line `shell/mod.rs` gets only the calls into them.

**Tech Stack:** Rust, egui 0.37, and the crate's existing raw-event tests
(`ctx.run_ui` with `egui::RawInput`).

**Spec:** `docs/superpowers/specs/2026-11-16-view-refresh-design.md`

## Global Constraints

- Existing `ShellApp` implementors must compile unchanged. Both new methods have
  defaults (`Refresh::Unsupported`, and a no-op).
- `refresh` is called at most once per frame, and only while `refresh_state` for the
  open screen is `Idle`.
- Keys: F5, and `Modifiers::COMMAND` + R. Not on `target_arch = "wasm32"`. Not while
  `ctx.wants_keyboard_input()`. Not consumed unless the state is `Idle`.
- The button sits on the status bar only (wide layout). It is absent for `Unsupported`, an
  `icons::refresh_cw()` button for `Idle`, and an `egui::Spinner` for `Running`.
- Message `corundum-shell-refresh` = "Refresh" (en) / "再読み込み" (ja).
- Pull threshold: about 64 px. Mouse drags count as well as touch.
- Keep to the crate's doc-comment voice (plain sentences on every pub item).

## Review Focus

1. **Holding F5 down.** Key-repeat events must not refresh again on every repeat frame
   for a screen whose refresh is instant. Only `repeat: false` presses count.
   → Task 1 test `a_held_key_refreshes_once`.
2. **A drag that starts below the top.** If the person scrolls back up to the top and
   keeps dragging down, that must not refresh. Only a press that starts while the offset
   is 0 can pull. → Task 3 test `a_drag_begun_scrolled_down_does_not_pull`.
3. **A mostly sideways swipe** (a carousel, a horizontal list) that drifts down past
   64 px must not refresh. The pull counts only while the drag is more down than
   sideways. → Task 3 test `a_sideways_drag_does_not_pull`.
4. **Switching screens while one is `Running`.** The button follows the new screen's
   state, not the old one's. → Task 2 test `the_button_follows_the_open_screen`.
5. **Ctrl+R in a text field.** It belongs to the field. → Task 1 test
   `keys_are_left_to_a_focused_text_field`.

---

### Task 1: `Refresh`, the trait methods, the per-frame dispatch, and the keys

**Files:**
- Create: `crates/corundum-framework-gui/src/shell/refresh.rs` (the enum, the ctx
  plumbing, keys, and dispatch; its own `#[cfg(test)] mod tests`)
- Modify: `crates/corundum-framework-gui/src/shell/mod.rs`. Add `mod refresh;` and
  `pub use refresh::Refresh;`. Add the two trait methods after `action` (around line
  263). Call the dispatch from `frame_ui` (around line 705).
- Modify: `crates/corundum-framework-gui/src/lib.rs:87`. Add `Refresh` to the
  `pub use shell::{…}`.

**Interfaces:**
- Produces:
  - `pub enum Refresh { #[default] Unsupported, Idle, Running }`, deriving
    `Clone, Copy, PartialEq, Eq, Debug, Default`.
  - `ShellApp::refresh_state(&self, screen: D) -> Refresh` (default `Unsupported`).
  - `ShellApp::refresh(&mut self, screen: D)` (default no-op).
  - `pub(crate) fn begin(ctx: &egui::Context, state: Refresh)`: stores the state for this
    frame and clears any request left over.
  - `pub(crate) fn state(ctx: &egui::Context) -> Refresh`: what `begin` stored
    (`Unsupported` if nothing was stored).
  - `pub(crate) fn request(ctx: &egui::Context)`: marks a request for this frame.
  - `pub(crate) fn keys(ctx: &egui::Context, state: Refresh)`: marks a request on a key
    press (rules in Global Constraints).
  - `pub(crate) fn take(ctx: &egui::Context) -> bool`: whether a request was marked, and
    clears it.
  - Temp-data ids: `egui::Id::new("corundum-shell-refresh-state")` and
    `egui::Id::new("corundum-shell-refresh-request")`.

- [ ] **Step 1: Write the failing tests** in `refresh.rs`. The fixture is a
  `ShellApp<Screen>` with `states: HashMap<Screen, Refresh>`, `refreshed: Vec<Screen>`
  and a `focus_text: bool`. When `focus_text` is set, its `screen_ui` draws a `TextEdit`
  and calls `request_focus()` on it. Drive it with `Shell::show` over raw frames, as
  `shell/mod.rs`'s `show` helper does. Key events are
  `egui::Event::Key { key, physical_key: None, pressed: true, repeat, modifiers }`.

  ```rust
  #[test] fn by_default_nothing_is_refreshable()      // F5 on a default app: refreshed empty
  #[test] fn f5_refreshes_the_open_screen_once()       // Idle on Entries: F5 → refreshed == [Entries]
  #[test] fn command_r_refreshes_too()                 // Modifiers::COMMAND + Key::R → [Entries]
  #[test] fn nothing_refreshes_while_running()         // Running: F5 → []
  #[test] fn a_held_key_refreshes_once()               // one press, then 5 frames of repeat: true → [Entries]
  #[test] fn keys_are_left_to_a_focused_text_field()   // focus_text, Idle: Ctrl+R and F5 → []
  #[test] fn keys_work_narrow_too()                    // size [400, 700]: F5 → [Entries]
  ```

- [ ] **Step 2: Run them to see them fail.**
  Run: `cargo test -p corundum-framework-gui shell::refresh`
  Expected: compile errors (`Refresh` and `refresh_state` are not defined).

- [ ] **Step 3: Implement.** Put the enum and the free functions in `refresh.rs`, and the
  trait methods in `mod.rs`. In `frame_ui`, once `self.open` is known for the frame:
  `let state = self.open.map_or(Refresh::Unsupported, |s| app.refresh_state(s));`, then
  `refresh::begin`, then `refresh::keys`. At the end of `frame_ui`, before `sync_side`:
  if `refresh::take(ctx)`, the state is `Idle`, and a screen is open, call
  `app.refresh(open)` and `ctx.request_repaint()`. For `keys`: read
  `i.events` for `Key { pressed: true, repeat: false, .. }` with F5, or R with
  `modifiers.command`. Remove the matching event only when the state is `Idle`. Gate the
  whole function body with `#[cfg(not(target_arch = "wasm32"))]`.

- [ ] **Step 4: Run them to see them pass.**
  Run: `cargo test -p corundum-framework-gui shell::` (the existing shell tests must
  pass too)
  Expected: PASS.

- [ ] **Step 5: Commit.**
  `git commit -m "feat(corundum-framework-gui): let a ShellApp refresh its open screen with F5 or Cmd+R"`

---

### Task 2: The status bar button

**Files:**
- Modify: `crates/corundum-framework-gui/src/shell/refresh.rs`. Add `button_ui`.
- Modify: `crates/corundum-framework-gui/src/shell/mod.rs`. In `status_bar_ui` (around
  line 1072), call `button_ui` right after `foot_ui` in the right-to-left row. Add the
  `#[cfg(test)] refresh_rect: Option<egui::Rect>` field on `Shell`, set from the response.
- Modify: `crates/corundum-framework-gui/locales/en/corundum.ftl` and
  `locales/ja/corundum.ftl`. Add `corundum-shell-refresh`, next to
  `corundum-explorer-refresh`.

**Interfaces:**
- Consumes: `Refresh`, `refresh::state`, `refresh::request` (Task 1).
- Produces: `pub(crate) fn button_ui(ui: &mut egui::Ui) -> Option<egui::Response>`.
  It reads `refresh::state(ui.ctx())` and draws nothing for `Unsupported`, a
  `crate::icons::button(crate::icons::refresh_cw(), …)` for `Idle` (same size as
  `toggle_ui`'s, hover text `tr!(ui, "corundum-shell-refresh")`, and a click calls
  `refresh::request`), or an `egui::Spinner` of that size for `Running`. It returns the
  response drawn.

- [ ] **Step 1: Write the failing tests** in `refresh.rs`, with the Task 1 fixture:

  ```rust
  #[test] fn the_button_is_absent_when_unsupported()  // wide: shell.refresh_rect == None
  #[test] fn clicking_the_button_refreshes_once()     // Idle, wide: click refresh_rect.center() → [Entries]
  #[test] fn a_running_screen_shows_a_spinner_not_a_button() // Running: rect is Some, click → []
  #[test] fn the_button_follows_the_open_screen()     // Entries Running, Summary Idle; open Summary, click → [Summary]
  #[test] fn narrow_there_is_no_button()              // [400, 700], Idle: refresh_rect == None
  ```
  Also run the crate's i18n check test, which fails until both `.ftl` files have the
  message.

- [ ] **Step 2: Run them to see them fail.**
  Run: `cargo test -p corundum-framework-gui shell::refresh`
  Expected: FAIL (`refresh_rect` stays `None`).

- [ ] **Step 3: Implement `button_ui`, hook it into `status_bar_ui`, and add the
  messages.**

- [ ] **Step 4: Run them to see them pass.**
  Run: `cargo test -p corundum-framework-gui`
  Expected: PASS, including the i18n check.

- [ ] **Step 5: Commit.**
  `git commit -m "feat(corundum-framework-gui): a refresh button on the status bar"`

---

### Task 3: `pull_to_refresh`

**Files:**
- Create: `crates/corundum-framework-gui/src/shell/pull.rs` (with its own tests)
- Modify: `crates/corundum-framework-gui/src/shell/mod.rs` (`mod pull; pub use pull::pull_to_refresh;`)
- Modify: `crates/corundum-framework-gui/src/lib.rs:87` (re-export `pull_to_refresh`)

**Interfaces:**
- Consumes: `refresh::state`, `refresh::request` (Task 1).
- Produces:
  `pub fn pull_to_refresh<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R`.
  It draws a `egui::ScrollArea::vertical().auto_shrink(false)` over `add_contents`. The
  doc comment says it refreshes the open screen when the area is pulled down at its top,
  and acts as a plain scroll area when the screen can't be refreshed.
- `const THRESHOLD: f32 = 64.0;`

The algorithm. The signature doesn't determine it, so here it is:

```text
state kept in ui temp data under the scroll area's id: Option<Pull { origin: Pos2, armed: bool }>
on primary press inside the area's rect, with scroll offset.y <= 0 and refresh::state == Idle:
    Pull { origin: press pos, armed: false }
while held: dy = pos.y - origin.y, dx = |pos.x - origin.x|
    if dy <= 0 (pulled back up, or scrolled): drop the pull
    armed = dy >= THRESHOLD && dy > dx
    draw at the area's top: offset min(dy, THRESHOLD*1.5)*0.5; an arrow-down icon,
    rotated 180° once armed
on release: if armed { refresh::request(ctx) }; drop the pull
while refresh::state == Running: draw an egui::Spinner at the area's top centre
```

- [ ] **Step 1: Write the failing tests** in `pull.rs`. Drive a `Shell` with a fixture
  whose `screen_ui` is `CentralPanel` → `pull_to_refresh` over 60 labels, so the content
  overflows. Mouse events: `PointerMoved` and `PointerButton`, moving 10 px per frame.

  ```rust
  #[test] fn pulling_past_the_threshold_refreshes_once()       // press at top, drag 100 px down, release → [Entries]
  #[test] fn a_short_pull_does_not_refresh()                    // drag 40 px → []
  #[test] fn a_drag_begun_scrolled_down_does_not_pull()         // scroll down first (MouseWheel), then press, drag up to the top and on 100 px → []
  #[test] fn a_sideways_drag_does_not_pull()                    // dx 150, dy 80 → []
  #[test] fn an_unrefreshable_screen_is_a_plain_scroll_area()   // Unsupported: drag 100 → refresh never called, no panic
  #[test] fn nothing_is_pulled_while_running()                  // Running: drag 100 → []
  ```

- [ ] **Step 2: Run them to see them fail.**
  Run: `cargo test -p corundum-framework-gui shell::pull`
  Expected: compile error (`pull_to_refresh` is not defined).

- [ ] **Step 3: Implement `pull_to_refresh` as above.**

- [ ] **Step 4: Run them to see them pass.**
  Run: `cargo test -p corundum-framework-gui`
  Expected: PASS.

- [ ] **Step 5: Commit.**
  `git commit -m "feat(corundum-framework-gui): pull a screen down to refresh it"`

---

### Task 4: The demo

**Files:**
- Modify: `demo/src/screens/mod.rs:130-150`. Replace the shared
  `egui::ScrollArea::vertical().auto_shrink(false).show(…)` with
  `corundum_framework::gui::pull_to_refresh(ui, |ui| …)`. Add `pub(crate) fn
  refresh_state(&self, screen: Screen) -> Refresh` and `pub(crate) fn refresh(&mut self,
  screen: Screen)` on `State`.
- Modify: `demo/src/lib.rs:170-210`. `impl ShellApp<Screen> for App` forwards the two
  methods to `self.state`.
- Modify: `demo/src/screens/platform.rs`. Make `State::refresh` `pub(crate)`, and remove
  the "Read again" button (`demo-platform-refresh`, line 113).
- Modify: `demo/src/screens/overview.rs`. Add a fake slow refresh and a line showing when
  it last refreshed.
- Modify: `demo/locales/{en,ja}/demo.ftl`. Remove `demo-platform-refresh`. Add
  `demo-overview-refreshed = Refreshed { $count } times` /
  `{ $count } 回再読み込みしました` and
  `demo-overview-refresh-hint = Pull down, press F5, or use the button on the status bar to refresh.` /
  `下に引っぱる、F5 を押す、ステータスバーのボタンを押すと再読み込みします。`
- Test: `demo/src/tests.rs`

**Interfaces:**
- Consumes: `Refresh`, `ShellApp::{refresh_state, refresh}`, `pull_to_refresh`
  (Tasks 1 and 3).
- Produces: `screens::State::{refresh_state, refresh}`.
  - Platform: always `Idle`, and `refresh` calls `platform::State::refresh()`.
  - Overview: `Running` while `overview_busy: Arc<AtomicBool>` is set, else `Idle`.
    `refresh` sets the flag and spawns a `std::thread` that sleeps 2 s, bumps
    `overview_count: Arc<AtomicU32>`, clears the flag, and calls
    `ctx.request_repaint()`. To get the `ctx`, store a clone of it in
    `State` on the first `ui` call. On wasm, where `std::thread::spawn` panics, the
    Overview refresh finishes at once instead (`cfg(target_arch = "wasm32")`).
  - Every other screen: `Unsupported`.

- [ ] **Step 1: Write the failing test** in `demo/src/tests.rs`:

  ```rust
  #[test] fn only_overview_and_platform_refresh()
  // for screen in Screen::ALL: state.refresh_state(screen) is Idle for Overview and
  // Platform and Unsupported for the rest
  #[test] fn overview_refresh_runs_then_finishes()
  // refresh(Overview) → Running; poll up to 5 s → Idle, count == 1
  ```

- [ ] **Step 2: Run it to see it fail.**
  Run: `cargo test -p corundum-framework-demo` (check the package name in
  `demo/Cargo.toml`)
  Expected: compile error.

- [ ] **Step 3: Implement as above.**

- [ ] **Step 4: Verify.**
  Run: `cargo test --workspace` → PASS. Then
  `cargo clippy --workspace --all-targets -- -D warnings` → no warnings.
  Run `cargo check -p <demo> --target wasm32-unknown-unknown` if the target is installed.
  Then run the demo (the `run` skill), open Overview, and check each of these once: F5,
  the status bar button (spinner for about 2 s), and a mouse pull in a window narrower
  than 600 px.

- [ ] **Step 5: Commit.**
  `git commit -m "feat(demo): refresh the Overview and Platform screens from the shell"`
