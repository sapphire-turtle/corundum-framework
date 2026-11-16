//! Refreshing the open screen: what the app says of it ([`Refresh`]), and how the
//! shell turns the keys, the status bar's button and a pull into one call of
//! [`ShellApp::refresh`](super::ShellApp::refresh).

/// Whether the open screen can be refreshed, and whether it is now: what
/// [`ShellApp::refresh_state`](super::ShellApp::refresh_state) answers, every frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Refresh {
    /// The screen has nothing to refresh: there is no button, and the keys and a pull do
    /// nothing.
    #[default]
    Unsupported,
    /// It can be refreshed.
    Idle,
    /// A refresh is running: the button spins, and nothing starts another.
    Running,
}

/// Where the frame's state is kept, for the status bar's button and a pull.
fn state_id() -> egui::Id {
    egui::Id::unique("corundum_shell_refresh_state")
}

/// Where a request for a refresh is kept until the shell takes it.
fn request_id() -> egui::Id {
    egui::Id::unique("corundum_shell_refresh_request")
}

/// Keep `state` as the open screen's for this frame, and drop a request left over.
pub(crate) fn begin(ctx: &egui::Context, state: Refresh) {
    ctx.data_mut(|d| {
        d.insert_temp(state_id(), state);
        d.remove::<bool>(request_id());
    });
}

/// The open screen's state, as [`begin`] kept it: `Unsupported` outside a shell.
pub(crate) fn state(ctx: &egui::Context) -> Refresh {
    ctx.data(|d| d.get_temp(state_id())).unwrap_or_default()
}

/// Ask for the open screen to be refreshed, once the shell has drawn it.
pub(crate) fn request(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(request_id(), true));
}

/// Whether a refresh was asked for; it is taken.
pub(crate) fn take(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(request_id()))
        .unwrap_or(false)
}

/// Ask for a refresh on a press of F5 or of Cmd+R (Ctrl+R off macOS), taking the key,
/// while the screen can be refreshed and no text field has the keyboard. A key held down
/// asks once. In a browser the keys are left to it: they reload the page.
pub(crate) fn keys(ctx: &egui::Context, state: Refresh) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if state != Refresh::Idle || ctx.egui_wants_keyboard_input() {
            return;
        }
        let pressed = ctx.input_mut(|i| {
            let before = i.events.len();
            i.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Key { key: egui::Key::F5, pressed: true, repeat: false, .. }
                ) && !matches!(
                    e,
                    egui::Event::Key { key: egui::Key::R, pressed: true, repeat: false, modifiers, .. }
                        if modifiers.command
                )
            });
            i.events.len() != before
        });
        if pressed {
            request(ctx);
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (ctx, state);
}

/// The status bar's button for the open screen: nothing when it cannot be refreshed, a
/// spinner while it is refreshing, else a button that asks for a refresh. What was drawn.
pub(crate) fn button_ui(ui: &mut egui::Ui) -> Option<egui::Response> {
    const ICON: f32 = 14.0;
    match state(ui.ctx()) {
        Refresh::Unsupported => None,
        Refresh::Idle => {
            let response = ui
                .add(crate::icons::button(crate::icons::refresh_cw(), ICON))
                .on_hover_text(crate::tr!(ui, "corundum-shell-refresh"));
            if response.clicked() {
                request(ui.ctx());
            }
            Some(response)
        }
        Refresh::Running => {
            let side = ui.spacing().interact_size.y;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            ui.put(rect, egui::Spinner::new().size(ICON));
            Some(response)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::shell::{Destination, Shell, ShellApp, Side};

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    pub(crate) enum Screen {
        Entries,
        Summary,
    }

    pub(crate) const WIDE: [f32; 2] = [900.0, 600.0];
    pub(crate) const NARROW: [f32; 2] = [400.0, 700.0];

    /// An app whose screens refresh as `states` says, and which notes each refresh.
    #[derive(Default)]
    pub(crate) struct App {
        pub(crate) states: HashMap<Screen, Refresh>,
        pub(crate) refreshed: Vec<Screen>,
        /// Draw a text field on the screen, and keep it focused.
        pub(crate) focus_text: bool,
        text: String,
        /// Draw the screen through this instead of a label.
        pub(crate) screen: Option<fn(&mut egui::Ui)>,
    }

    impl App {
        pub(crate) fn with(states: &[(Screen, Refresh)]) -> App {
            App {
                states: states.iter().copied().collect(),
                ..App::default()
            }
        }
    }

    impl ShellApp<Screen> for App {
        fn screen_ui(&mut self, ui: &mut egui::Ui, _: Screen, _: &mut Side) {
            egui::CentralPanel::default().show(ui, |ui| {
                if let Some(screen) = self.screen {
                    screen(ui);
                } else if self.focus_text {
                    ui.text_edit_singleline(&mut self.text).request_focus();
                } else {
                    ui.label("screen");
                }
            });
        }

        fn refresh_state(&self, screen: Screen) -> Refresh {
            self.states.get(&screen).copied().unwrap_or_default()
        }

        fn refresh(&mut self, screen: Screen) {
            self.refreshed.push(screen);
        }
    }

    /// A shell over `App`, drawn frame by frame on a screen of one size.
    pub(crate) struct Run {
        ctx: egui::Context,
        pub(crate) shell: Shell<Screen>,
        pub(crate) app: App,
        size: [f32; 2],
    }

    impl Run {
        /// Two frames of `app` on a `size` screen, so the layout has settled.
        pub(crate) fn new(app: App, size: [f32; 2]) -> Run {
            let shell = Shell::new(vec![
                Destination::new(Screen::Entries, crate::icons::settings(), "entries"),
                Destination::new(Screen::Summary, crate::icons::arrow_left(), "summary"),
            ]);
            let mut run = Run {
                ctx: egui::Context::default(),
                shell,
                app,
                size,
            };
            run.frame(vec![]);
            run.frame(vec![]);
            run
        }

        pub(crate) fn frame(&mut self, events: Vec<egui::Event>) {
            let rect =
                egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(self.size[0], self.size[1]));
            let input = egui::RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            };
            let (shell, app) = (&mut self.shell, &mut self.app);
            let mut output = self.ctx.run_ui(input, |ui| shell.show(ui, app));
            output.textures_delta.clear();
        }

        /// A press of `key` with `modifiers`, then a frame without it.
        pub(crate) fn press(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
            self.frame(vec![key_event(key, modifiers, false)]);
            self.frame(vec![]);
        }

        /// A click at `pos`.
        pub(crate) fn click(&mut self, pos: egui::Pos2) {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(vec![egui::Event::PointerMoved(pos)]);
            self.frame(vec![button(true)]);
            self.frame(vec![button(false)]);
            self.frame(vec![]);
        }
    }

    pub(crate) fn key_event(
        key: egui::Key,
        modifiers: egui::Modifiers,
        repeat: bool,
    ) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        }
    }

    use egui::{Key, Modifiers};

    #[test]
    fn by_default_nothing_is_refreshable() {
        let mut run = Run::new(App::default(), WIDE);
        run.press(Key::F5, Modifiers::NONE);
        assert_eq!(run.app.refreshed, []);
    }

    #[test]
    fn f5_refreshes_the_open_screen_once() {
        let mut run = Run::new(App::with(&[(Screen::Entries, Refresh::Idle)]), WIDE);
        run.press(Key::F5, Modifiers::NONE);
        assert_eq!(run.app.refreshed, [Screen::Entries]);
    }

    #[test]
    fn command_r_refreshes_too() {
        let mut run = Run::new(App::with(&[(Screen::Entries, Refresh::Idle)]), WIDE);
        run.press(Key::R, Modifiers::COMMAND);
        assert_eq!(run.app.refreshed, [Screen::Entries]);
    }

    #[test]
    fn r_alone_does_not_refresh() {
        let mut run = Run::new(App::with(&[(Screen::Entries, Refresh::Idle)]), WIDE);
        run.press(Key::R, Modifiers::NONE);
        assert_eq!(run.app.refreshed, []);
    }

    #[test]
    fn nothing_refreshes_while_running() {
        let mut run = Run::new(App::with(&[(Screen::Entries, Refresh::Running)]), WIDE);
        run.press(Key::F5, Modifiers::NONE);
        assert_eq!(run.app.refreshed, []);
    }

    #[test]
    fn a_held_key_refreshes_once() {
        let mut run = Run::new(App::with(&[(Screen::Entries, Refresh::Idle)]), WIDE);
        run.frame(vec![key_event(Key::F5, Modifiers::NONE, false)]);
        for _ in 0..5 {
            run.frame(vec![key_event(Key::F5, Modifiers::NONE, true)]);
        }
        assert_eq!(run.app.refreshed, [Screen::Entries]);
    }

    #[test]
    fn keys_are_left_to_a_focused_text_field() {
        let mut app = App::with(&[(Screen::Entries, Refresh::Idle)]);
        app.focus_text = true;
        let mut run = Run::new(app, WIDE);
        run.press(Key::R, Modifiers::COMMAND);
        run.press(Key::F5, Modifiers::NONE);
        assert_eq!(run.app.refreshed, []);
    }

    #[test]
    fn keys_work_narrow_too() {
        let mut run = Run::new(App::with(&[(Screen::Entries, Refresh::Idle)]), NARROW);
        run.press(Key::F5, Modifiers::NONE);
        assert_eq!(run.app.refreshed, [Screen::Entries]);
    }
}

#[cfg(test)]
mod button_tests {
    use super::Refresh;
    use super::tests::{App, NARROW, Run, Screen, WIDE};

    #[test]
    fn the_button_is_absent_when_unsupported() {
        let run = Run::new(App::default(), WIDE);
        assert_eq!(run.shell.refresh_rect, None);
    }

    #[test]
    fn clicking_the_button_refreshes_once() {
        let mut run = Run::new(App::with(&[(Screen::Entries, Refresh::Idle)]), WIDE);
        let at = run.shell.refresh_rect.expect("a button").center();
        // On the status bar, along the bottom.
        assert!(at.y > WIDE[1] - 30.0, "{at:?}");
        run.click(at);
        assert_eq!(run.app.refreshed, [Screen::Entries]);
    }

    #[test]
    fn a_running_screen_shows_a_spinner_not_a_button() {
        let mut run = Run::new(App::with(&[(Screen::Entries, Refresh::Running)]), WIDE);
        let at = run.shell.refresh_rect.expect("a spinner").center();
        run.click(at);
        assert_eq!(run.app.refreshed, []);
    }

    #[test]
    fn the_button_follows_the_open_screen() {
        let app = App::with(&[
            (Screen::Entries, Refresh::Running),
            (Screen::Summary, Refresh::Idle),
        ]);
        let mut run = Run::new(app, WIDE);
        run.shell.open(Screen::Summary);
        run.frame(vec![]);
        run.frame(vec![]);
        let at = run.shell.refresh_rect.expect("a button").center();
        run.click(at);
        assert_eq!(run.app.refreshed, [Screen::Summary]);
    }

    #[test]
    fn narrow_there_is_no_button() {
        let run = Run::new(App::with(&[(Screen::Entries, Refresh::Idle)]), NARROW);
        assert_eq!(run.shell.refresh_rect, None);
    }
}
