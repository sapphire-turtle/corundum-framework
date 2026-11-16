# Refreshing the open screen from the shell

Asked for on 2026-11-16.

## Why

Every downstream app has screens whose data can go stale: a folder listing, a synced
journal, a server's status. Today each screen draws its own refresh button, if it has one
(the explorer's `Act::Refresh`, the demo's platform screen). Since every app needs one,
the shell should offer it, so refreshing looks and works the same everywhere and uses
each platform's usual gesture:

- phone and tablet: pull the main view down;
- desktop: a button on the status bar, and F5 (Cmd+R / Ctrl+R too).

## Goals

- One way for an app to say "this screen can be refreshed" and "it is refreshing now",
  and one callback to do it.
- The shell turns the button, the keys and the pull gesture into that one callback, for
  the screen that is open.
- It works for a quick local re-read and for a slow sync that takes seconds: the shell
  shows that a refresh is running and does not start a second one.
- Existing apps keep compiling and behave as before until they opt in.

## Non-goals

- Refreshing the whole app or other screens: only the open screen.
- Showing failures. The app reports a failed refresh its own way (status bar, toast).
- Running the refresh. The shell has no async runtime and won't get one; the app starts
  its work (thread, task) and reports its state.
- Moving the explorer's own refresh button onto this. The explorer is a widget inside a
  screen, not a `ShellApp`. That can come later.
- A refresh button on the compact bottom bar. Narrow layouts get the pull gesture and the
  keys. We may add a button later if the gesture turns out to be too hard to discover.

## Anti-goals

- A pull gesture that fires while the person is scrolling, or on screens that use
  vertical drags themselves (maps, canvases, nested scrolls).
- Breaking `ShellApp` implementors.

## Approaches

- **A. Methods on `ShellApp` (picked).** `refresh_state(screen)` and `refresh(screen)`,
  both with default implementations. They read like `foot_ui` and `action`, and they don't
  break anything. The shell polls the state each frame, before it draws the status bar.
  The catch: an app has to implement both, and keep them in step.
- **B. A per-frame handle passed to `screen_ui`.** It would replace `Side` with a wider
  `Frame` that the screen tells about refreshing as it draws. That is more immediate-mode,
  but it changes `screen_ui`'s signature for every app. The status bar is also drawn
  before the screen, so its button would lag a frame.
- **C. The shell owns refresh tasks (futures, channels).** Dropped: the shell has no
  runtime, and this is far heavier than the need.

## Design

### API (`corundum_framework_gui::shell`, re-exported at the crate root)

```rust
/// Whether the open screen can be refreshed, and whether it is now.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Refresh {
    /// The screen has nothing to refresh: no button, keys and pulls do nothing.
    #[default]
    Unsupported,
    /// It can be refreshed.
    Idle,
    /// A refresh is running: the button spins, and nothing starts another.
    Running,
}

pub trait ShellApp<D> {
    // … existing methods …

    /// Whether `screen` can be refreshed, and whether it is now. Asked every frame.
    fn refresh_state(&self, screen: D) -> Refresh { Refresh::Unsupported }

    /// Refresh `screen`: the person clicked the button, pressed the key, or pulled the
    /// view. Called only while `refresh_state` says `Idle`. A quick re-read can finish
    /// here; slow work starts here and makes `refresh_state` say `Running` until it is
    /// done.
    fn refresh(&mut self, screen: D) {}
}
```

The shell calls `refresh` at most once per frame, and only when `refresh_state` for the
open screen is `Idle`. That one check covers double clicks, key repeat, and a pull that
lands while a sync is still running.

`Closures` (the deprecated closure adapter) keeps the defaults.

### The status bar button (wide layout)

- It goes in `status_bar_ui`'s right-to-left row, just left of the foot (after
  `toggle_ui` and `foot_ui`), so it sits with the shell's controls rather than the app's
  status text.
- `Unsupported`: no button. `Idle`: an `icons::refresh_cw()` button with the hover text
  `corundum-shell-refresh`. `Running`: an `egui::Spinner` of the same size, not clickable.
- New message `corundum-shell-refresh` in `locales/{en,ja}/corundum.ftl` ("Refresh" /
  "再読み込み", the same wording as the explorer).

### Keys

- F5, and `Modifiers::COMMAND` + R (Cmd+R on macOS, Ctrl+R elsewhere), read with
  `ctx.input_mut(|i| i.consume_key(..))` once per frame in `frame_ui`. They work in both
  the wide and the compact layout.
- Only when the state is `Idle`. They are not consumed otherwise, so a screen that binds
  the key itself still gets it.
- Not while a text field has focus (`ctx.wants_keyboard_input()`), so Ctrl+R in an
  editor isn't taken.
- On wasm (`target_arch = "wasm32"`), the shell does not bind them: there F5 and Ctrl+R
  reload the page, and people expect that.

### Pull to refresh (the screen's helper)

Screens own their scroll areas (`screen_ui` draws its own `CentralPanel`), so the shell
can't tell where the top is. A screen opts in by drawing its main scroll through a
helper:

```rust
/// A vertical scroll area that refreshes the open screen when pulled down at its top.
pub fn pull_to_refresh<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R
```

(It lives in a new module, `shell::pull`, re-exported. A builder variant that takes a
configured `egui::ScrollArea` is the builder's call if it is needed.)

- Before `screen_ui`, the shell writes the open screen's `Refresh` into `ctx` temp data.
  The helper reads it from there. `Unsupported` makes the helper a plain scroll area.
- The helper starts a pull when a press that began inside it drags down while the offset
  is at 0. Touch and mouse both count. A desktop window can be narrow, and egui's scroll
  areas already drag with the mouse.
- It shows the pull as an indicator at the top that follows the finger, damped: an arrow
  that turns to "release" past the threshold (about 64 px), then a spinner while
  `Running`. Letting go past the threshold writes a request into `ctx` temp data;
  letting go short of it, or scrolling up, cancels.
- After `screen_ui`, the shell takes the request and, if the state is still `Idle`,
  calls `app.refresh(screen)`.
- Screens that don't use the helper never react to pulls. That is how maps and canvases
  stay out of it.

### Flow per frame

1. `state = app.refresh_state(open)`; store it for the helper.
2. Wide: the status bar draws the button from `state`; a click marks a request.
3. Keys (not on wasm): mark a request.
4. `screen_ui` runs; the helper may mark a request.
5. If anything marked a request and `state == Idle`: `app.refresh(open)` once.

### Demo

Following the rule that the demo turns on every feature it can:

- The platform screen moves its refresh button onto this. Its re-read is instant, so it
  returns `Idle` and refreshes inside `refresh`.
- One screen fakes a slow refresh: a thread sleeps about 2 s and sets an
  `Arc<AtomicBool>`, which shows `Running`. Its content goes in `pull_to_refresh`. The
  files screen is the natural place; the builder may pick another.

### Tests

The shell already has egui_kittest-style tests that click rects (`cell_rects` and the
like). Following that pattern:

- the button is hidden for `Unsupported`, clickable for `Idle`, a spinner for `Running`;
- a click, F5 and Cmd/Ctrl+R each call `refresh` exactly once; nothing is called while
  `Running`; keys are ignored while a text field has focus;
- the helper: a drag past the threshold at offset 0 requests once; a short drag, or a
  drag once scrolled, does not.

## Left to the builder

- The exact threshold, damping and indicator drawing.
- Whether the helper needs a variant that takes an `egui::ScrollArea`.
- Which demo screen gets the slow refresh.
- Checking that eframe's web runner doesn't swallow F5/Ctrl+R, so the browser reload
  still works on wasm.
