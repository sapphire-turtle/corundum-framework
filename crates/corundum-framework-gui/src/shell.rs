//! [`Shell`]: an app's window frame — its screens on a rail, its actions under them, and
//! at the rail's foot the screens that belong there (Settings) and a slot of the app's own
//! (a status dot).
//!
//! Wide, the rail runs down the left (as Obsidian's does); narrower than
//! [`COMPACT_WIDTH`], it becomes a bar along the bottom, as on a phone. The width decides,
//! not the platform, so a desktop window made narrow shows the phone layout.

/// Below this width the layout is a phone's: the rail becomes a bottom bar, and screens
/// with a list and a page show one at a time.
pub const COMPACT_WIDTH: f32 = 600.0;

/// The rail's width, wide.
const RAIL_WIDTH: f32 = 48.0;

/// One of the app's screens on the rail.
pub struct Destination<D> {
    /// What the app matches on to draw it.
    pub id: D,
    /// A Lucide icon (see [`icons`](crate::icons)), stroked in white so it takes the text
    /// colour: the rail shows only this.
    pub icon: egui::ImageSource<'static>,
    /// The id of the screen's name in the app's translations, looked up each frame and shown
    /// when the icon is hovered.
    pub label: &'static str,
}

impl<D> Destination<D> {
    /// A screen named by the message `label`, drawn as `icon`.
    pub fn new(id: D, icon: egui::ImageSource<'static>, label: &'static str) -> Destination<D> {
        Destination { id, icon, label }
    }
}

/// A button of the app's on the rail that does something — "new-entry" — rather than
/// open a screen. [`Shell::take_action`] says which one was clicked.
pub struct Action {
    /// What [`Shell::take_action`] returns when it is clicked.
    pub id: &'static str,
    /// A Lucide icon, as for a [`Destination`].
    pub icon: egui::ImageSource<'static>,
    /// The id of what it does in the app's translations, looked up each frame and shown
    /// when the icon is hovered.
    pub label: &'static str,
}

impl Action {
    /// A button named by the message `label`, drawn as `icon`, reported as `id`.
    pub fn new(id: &'static str, icon: egui::ImageSource<'static>, label: &'static str) -> Action {
        Action { id, icon, label }
    }
}

/// The window frame. Hold one as the app's root; call [`ui`](Self::ui) every frame.
///
/// Labels are message ids: each frame the hover text is looked up in the language
/// installed (see [`i18n`](crate::i18n)), so a language switch shows on the next frame.
///
/// The shell keeps clear of the system's bars and notches (egui's
/// [safe area](egui::InputState::safe_area_insets), which iOS fills): the rail, the bar's
/// buttons and the screens stay inside it, and the bottom bar's fill runs on under the home
/// indicator, as a phone's tab bar does. Give it the window's whole root `ui`; the app adds
/// no margin of its own. If the app does keep `ui` clear of them itself, the shell sees
/// that and adds nothing, so the margin is never doubled. On Android,
/// `window::run_android` keeps the app's `ui` clear of the bars and the
/// keyboard already.
///
/// ```no_run
/// # use corundum_framework_gui::{Destination, Shell, icons};
/// # fn example(ui: &mut egui::Ui) {
/// #[derive(Clone, Copy, PartialEq)]
/// enum Screen { Gallery, Albums, Settings }
///
/// let mut shell = Shell::new(vec![
///     // The app's own Lucide SVGs: `egui::include_image!("../assets/icons/images.svg")`.
///     Destination::new(Screen::Gallery, icons::folder(), "gallery"),
///     Destination::new(Screen::Albums, icons::folder(), "albums"),
/// ])
/// .with_footer(vec![Destination::new(Screen::Settings, icons::settings(), "settings")]);
/// shell.ui(ui, |ui, screen| match screen {
///     Screen::Gallery => { egui::CentralPanel::default().show(ui, |ui| ui.label("…")); }
///     Screen::Albums => { egui::CentralPanel::default().show(ui, |ui| ui.label("…")); }
///     Screen::Settings => { egui::CentralPanel::default().show(ui, |ui| ui.label("…")); }
/// });
/// # }
/// ```
pub struct Shell<D> {
    destinations: Vec<Destination<D>>,
    actions: Vec<Action>,
    footer: Vec<Destination<D>>,
    /// The action clicked and not yet taken.
    clicked: Option<&'static str>,
    /// Where each action and footer destination was last drawn, for the tests to click.
    #[cfg(test)]
    action_rects: Vec<egui::Rect>,
    #[cfg(test)]
    footer_rects: Vec<egui::Rect>,
    /// The destinations' and the actions' rects on the bar, for the tests.
    #[cfg(test)]
    cell_rects: Vec<egui::Rect>,
    open: Option<D>,
    /// How wide the footer and the foot were on the bar last frame: their end of the bar
    /// is kept this wide, and the cells share the rest.
    foot_width: f32,
    /// Whether the SVG loader, the theme and the language have been put in place.
    loaders: bool,
}

impl<D: Copy + PartialEq> Shell<D> {
    /// A frame with `destinations` on the rail, the first one open.
    pub fn new(destinations: Vec<Destination<D>>) -> Shell<D> {
        let open = destinations.first().map(|d| d.id);
        Shell {
            destinations,
            actions: Vec::new(),
            footer: Vec::new(),
            clicked: None,
            #[cfg(test)]
            action_rects: Vec::new(),
            #[cfg(test)]
            footer_rects: Vec::new(),
            #[cfg(test)]
            cell_rects: Vec::new(),
            open,
            foot_width: 0.0,
            loaders: false,
        }
    }

    /// Put `actions` on the rail, under the screens (on the bottom bar, after them).
    pub fn with_actions(mut self, actions: Vec<Action>) -> Shell<D> {
        self.actions = actions;
        self
    }

    /// Put `footer` at the rail's foot (at the bottom bar's end): where an app puts
    /// Settings. With no other destinations, the first of these is open.
    pub fn with_footer(mut self, footer: Vec<Destination<D>>) -> Shell<D> {
        self.footer = footer;
        if self.open.is_none() {
            self.open = self.footer.first().map(|d| d.id);
        }
        self
    }

    /// The [`Action`] clicked in the last [`ui`](Self::ui), once: call it after `ui`
    /// every frame and do what it names.
    pub fn take_action(&mut self) -> Option<&'static str> {
        self.clicked.take()
    }

    /// The destination that is open; `None` only when there are none.
    pub fn current(&self) -> Option<D> {
        self.open
    }

    /// Open the destination `id`.
    pub fn open(&mut self, id: D) {
        self.open = Some(id);
    }

    /// Render into the window's root `ui`. `content` draws the open destination into what
    /// the frame leaves of `ui`, as it would at the root: its own panels, then a
    /// `CentralPanel`. The first frame installs the default [`theme`](crate::theme) and
    /// [`language`](crate::i18n) if the app has not installed them.
    pub fn ui(&mut self, ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui, D)) {
        self.ui_with_foot(ui, |_| None, content);
    }

    /// As [`ui`](Self::ui), with `foot` drawn beside the footer — a status dot, say. A
    /// destination `foot` returns is opened, and drawn this frame.
    pub fn ui_with_foot(
        &mut self,
        ui: &mut egui::Ui,
        foot: impl FnOnce(&mut egui::Ui) -> Option<D>,
        content: impl FnOnce(&mut egui::Ui, D),
    ) {
        if !std::mem::replace(&mut self.loaders, true) {
            crate::icons::install_loaders(ui.ctx());
            // The app may have installed its saved theme already; only fill the gap.
            if !crate::theme::is_installed(ui.ctx()) {
                crate::theme::install(ui.ctx(), &crate::theme::ThemeSettings::default());
            }
            // Likewise the language.
            if !crate::i18n::is_installed(ui.ctx()) {
                crate::i18n::install(ui.ctx(), &crate::i18n::LanguageSettings::default());
            }
        }
        let safe = unsafe_margin(ui);
        let full = ui.max_rect();
        let compact = full.width() - safe.sum().x < COMPACT_WIDTH;
        // Narrow, the bar's fill runs on under the home indicator, its buttons above it;
        // wide, the rail stops above it too.
        let inner = if compact {
            full - egui::epaint::MarginF32 {
                bottom: 0.0,
                ..safe
            }
        } else {
            full - safe
        };
        // The strips under the status bar and the notch take the panels' colour.
        if safe != egui::epaint::MarginF32::ZERO {
            ui.painter().rect_filled(full, 0.0, ui.visuals().panel_fill);
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            let opened = if compact {
                let mut frame = egui::Frame::side_top_panel(ui.style());
                frame.inner_margin.bottom = frame
                    .inner_margin
                    .bottom
                    .saturating_add(safe.bottom.round().min(127.0) as i8);
                egui::Panel::bottom("shell_bar")
                    .frame(frame)
                    .show(ui, |ui| self.bar_ui(ui, foot))
                    .inner
            } else {
                egui::Panel::left("shell_rail")
                    .resizable(false)
                    .exact_size(RAIL_WIDTH)
                    .show(ui, |ui| self.rail_ui(ui, foot))
                    .inner
            };
            if let Some(id) = opened {
                self.open = Some(id);
            }
            if let Some(id) = self.open {
                content(ui, id);
            }
        });
    }

    fn rail_ui(
        &mut self,
        ui: &mut egui::Ui,
        foot: impl FnOnce(&mut egui::Ui) -> Option<D>,
    ) -> Option<D> {
        ui.vertical_centered(|ui| {
            ui.add_space(4.0);
            for i in 0..self.destinations.len() {
                self.destination_ui(ui, i, BUTTON);
            }
            if !self.actions.is_empty() {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                for i in 0..self.actions.len() {
                    self.action_ui(ui, i, BUTTON);
                }
            }
        });
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            ui.add_space(4.0);
            for i in 0..self.footer.len() {
                self.footer_ui(ui, i, BUTTON);
            }
            foot(ui)
        })
        .inner
    }

    /// The phone layout: one cell per screen, then one per action, the footer and the
    /// foot last.
    ///
    /// The footer and the foot keep the width they need at the bar's end; the screens and
    /// the actions share what is left, their buttons narrowing below [`BUTTON`] when the
    /// cells do. So the foot never leaves the screen, however many screens there are.
    ///
    /// Every screen gets a cell for now. When an app has more than a bar can hold, the ones
    /// that do not fit move to a separate list of all of them (with a choice of which stay
    /// on the bar) — not into Settings (#275).
    fn bar_ui(
        &mut self,
        ui: &mut egui::Ui,
        foot: impl FnOnce(&mut egui::Ui) -> Option<D>,
    ) -> Option<D> {
        let rect = ui.available_rect_before_wrap();
        let gap = ui.spacing().item_spacing.x;
        let end = rect.right() - self.foot_width.min(rect.width());
        let screens = self.destinations.len();
        let cells = screens + self.actions.len();
        if cells > 0 {
            let row = egui::Rect::from_min_max(rect.min, egui::pos2(end - gap, rect.bottom()));
            ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
                ui.columns(cells, |columns| {
                    for (i, column) in columns.iter_mut().enumerate() {
                        column.vertical_centered(|ui| {
                            let side = ui.available_width().min(BUTTON);
                            let response = if i < screens {
                                self.destination_ui(ui, i, side)
                            } else {
                                self.action_ui(ui, i - screens, side)
                            };
                            #[cfg(test)]
                            {
                                self.cell_rects.resize(cells, egui::Rect::NOTHING);
                                self.cell_rects[i] = response.rect;
                            }
                            #[cfg(not(test))]
                            let _ = response;
                        });
                    }
                });
            });
        }
        let at_end = egui::Rect::from_min_max(egui::pos2(end, rect.top()), rect.max);
        let end_row = ui
            .scope_builder(egui::UiBuilder::new().max_rect(at_end), |ui| {
                ui.horizontal(|ui| {
                    for i in 0..self.footer.len() {
                        self.footer_ui(ui, i, BUTTON);
                    }
                    foot(ui)
                })
            })
            .inner;
        let opened = end_row.inner;
        // The end's width is known only once drawn: lay the frame out again with it.
        let width = end_row.response.rect.width();
        if (width - self.foot_width).abs() > 0.5 {
            self.foot_width = width;
            ui.ctx()
                .request_discard("the shell's bar foot changed width");
        }
        opened
    }

    fn destination_ui(&mut self, ui: &mut egui::Ui, i: usize, side: f32) -> egui::Response {
        let d = &self.destinations[i];
        let open = self.open == Some(d.id);
        let response = icon_button(ui, open, d.icon.clone(), side)
            .on_hover_text(crate::i18n::tr(ui.ctx(), d.label));
        if response.clicked() {
            self.open = Some(d.id);
        }
        response
    }

    /// What the hover text of destination `i` says now.
    #[cfg(test)]
    fn label_text(&self, ctx: &egui::Context, i: usize) -> String {
        crate::i18n::tr(ctx, self.destinations[i].label)
    }

    fn footer_ui(&mut self, ui: &mut egui::Ui, i: usize, side: f32) {
        let d = &self.footer[i];
        let open = self.open == Some(d.id);
        let response = icon_button(ui, open, d.icon.clone(), side)
            .on_hover_text(crate::i18n::tr(ui.ctx(), d.label));
        #[cfg(test)]
        {
            self.footer_rects
                .resize(self.footer.len(), egui::Rect::NOTHING);
            self.footer_rects[i] = response.rect;
        }
        if response.clicked() {
            self.open = Some(self.footer[i].id);
        }
    }

    fn action_ui(&mut self, ui: &mut egui::Ui, i: usize, side: f32) -> egui::Response {
        let a = &self.actions[i];
        let response = icon_button(ui, false, a.icon.clone(), side)
            .on_hover_text(crate::i18n::tr(ui.ctx(), a.label));
        #[cfg(test)]
        {
            self.action_rects
                .resize(self.actions.len(), egui::Rect::NOTHING);
            self.action_rects[i] = response.rect;
        }
        if response.clicked() {
            self.clicked = Some(a.id);
        }
        response
    }
}

/// How far the system's bars and notches (egui's safe area insets) reach into `ui`, side by
/// side: none where `ui` already keeps clear of them.
fn unsafe_margin(ui: &egui::Ui) -> egui::epaint::MarginF32 {
    let safe = ui.ctx().content_rect();
    let rect = ui.max_rect();
    egui::epaint::MarginF32 {
        left: (safe.left() - rect.left()).max(0.0),
        right: (rect.right() - safe.right()).max(0.0),
        top: (safe.top() - rect.top()).max(0.0),
        bottom: (rect.bottom() - safe.bottom()).max(0.0),
    }
}

/// A rail or bar button's side, where there is room for it.
const BUTTON: f32 = 36.0;

/// A `side`-point square button with `icon` in it ([`BUTTON`], or less on a crowded bar).
fn icon_button(
    ui: &mut egui::Ui,
    open: bool,
    icon: egui::ImageSource<'static>,
    side: f32,
) -> egui::Response {
    // A narrow button takes a smaller icon first, then less padding beside it, so it stays
    // `side` wide.
    let pad = ui.spacing().button_padding.x;
    let icon_size = (side - 2.0 * pad).clamp(12.0, 20.0);
    ui.scope(|ui| {
        let padding = &mut ui.spacing_mut().button_padding.x;
        *padding = padding.min(((side - icon_size) / 2.0).max(0.0));
        // The open destination's icon takes the accent, as egui's selected text does.
        ui.add(
            crate::icons::button(icon, icon_size)
                .selected(open)
                .frame_when_inactive(open)
                .min_size(egui::vec2(side, side)),
        )
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Screen {
        Entries,
        Summary,
        Prefs,
    }

    const SIZES: [[f32; 2]; 2] = [[900.0, 600.0], [400.0, 700.0]];

    fn shell() -> Shell<Screen> {
        Shell::new(vec![
            Destination::new(Screen::Entries, crate::icons::settings(), "entries"),
            Destination::new(Screen::Summary, crate::icons::arrow_left(), "summary"),
        ])
    }

    fn prefs() -> Vec<Destination<Screen>> {
        vec![Destination::new(
            Screen::Prefs,
            crate::icons::settings(),
            "preferences",
        )]
    }

    fn screen(ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| ui.label("screen"));
    }

    /// Lay `shell` out over two frames of a `size` screen, then click where `at` says.
    fn click(shell: &mut Shell<Screen>, size: [f32; 2], at: impl Fn(&Shell<Screen>) -> egui::Pos2) {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0], size[1]));
        let frame = |events: Vec<egui::Event>, shell: &mut Shell<Screen>| {
            let input = egui::RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| shell.ui(ui, |ui, _| screen(ui)));
            output.textures_delta.clear();
        };
        frame(vec![], shell);
        frame(vec![], shell);
        let pos = at(shell);
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(pos)], shell);
        frame(vec![button(true)], shell);
        frame(vec![button(false)], shell);
    }

    #[test]
    fn it_opens_on_the_first_screen_and_draws_it() {
        let mut shell = shell();
        assert_eq!(shell.current(), Some(Screen::Entries));
        for size in SIZES {
            let mut drawn = Vec::new();
            render(size, |ui| {
                shell.ui(ui, |ui, s| {
                    drawn.push(s);
                    screen(ui);
                })
            });
            assert_eq!(drawn, [Screen::Entries, Screen::Entries]);
        }
    }

    #[test]
    fn actions_are_drawn_in_both_layouts_and_report_nothing_unclicked() {
        let mut shell =
            shell().with_actions(vec![Action::new("new", crate::icons::plus(), "new-entry")]);
        for size in SIZES {
            shell.action_rects.clear();
            render(size, |ui| shell.ui(ui, |ui, _| screen(ui)));
            assert!(shell.action_rects[0].is_positive(), "{size:?}");
            assert_eq!(shell.take_action(), None);
        }
        assert_eq!(shell.current(), Some(Screen::Entries));
    }

    #[test]
    fn a_clicked_action_is_taken_once_and_opens_nothing() {
        let mut shell =
            shell().with_actions(vec![Action::new("new", crate::icons::plus(), "new-entry")]);
        click(&mut shell, SIZES[0], |s| s.action_rects[0].center());
        assert_eq!(shell.take_action(), Some("new"));
        assert_eq!(shell.take_action(), None);
        assert_eq!(shell.current(), Some(Screen::Entries));
    }

    #[test]
    fn footer_and_foot_are_in_both_layouts() {
        let mut shell = shell().with_footer(prefs());
        for size in SIZES {
            shell.footer_rects.clear();
            let mut foot = egui::Rect::NOTHING;
            render(size, |ui| {
                shell.ui_with_foot(
                    ui,
                    |ui| {
                        foot = ui
                            .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click())
                            .0;
                        None
                    },
                    |ui, _| screen(ui),
                )
            });
            assert!(shell.footer_rects[0].is_positive(), "{size:?}");
            assert!(foot.is_positive(), "{size:?}");
        }
    }

    #[test]
    fn clicking_a_footer_destination_opens_it() {
        for size in SIZES {
            let mut shell = shell().with_footer(prefs());
            click(&mut shell, size, |s| s.footer_rects[0].center());
            assert_eq!(shell.current(), Some(Screen::Prefs), "{size:?}");
        }
        let mut shell = shell().with_footer(prefs());
        shell.open(Screen::Prefs);
        let mut drawn = Vec::new();
        render([900.0, 600.0], |ui| {
            shell.ui(ui, |ui, s| {
                drawn.push(s);
                screen(ui);
            })
        });
        assert_eq!(drawn, [Screen::Prefs, Screen::Prefs]);
    }

    #[test]
    fn a_destination_the_foot_returns_is_opened() {
        let mut shell = shell();
        let mut drawn = Vec::new();
        render([900.0, 600.0], |ui| {
            shell.ui_with_foot(
                ui,
                |_| Some(Screen::Summary),
                |ui, s| {
                    drawn.push(s);
                    screen(ui);
                },
            )
        });
        assert_eq!(shell.current(), Some(Screen::Summary));
        assert_eq!(drawn, [Screen::Summary, Screen::Summary]);
    }

    #[test]
    fn the_shell_installs_the_default_theme() {
        let ctx = egui::Context::default();
        let mut shell = shell();
        crate::test_support::render_in(&ctx, [900.0, 600.0], |ui| shell.ui(ui, |ui, _| screen(ui)));
        assert!(crate::theme::is_installed(&ctx));
    }

    #[test]
    fn the_shell_keeps_an_installed_theme() {
        use crate::theme::{Contrast, ThemeSettings};
        let ctx = egui::Context::default();
        let settings = ThemeSettings {
            preference: egui::ThemePreference::Dark,
            contrast: Contrast::High,
            ..Default::default()
        };
        crate::theme::install(&ctx, &settings);
        let mut shell = shell();
        crate::test_support::render_in(&ctx, [900.0, 600.0], |ui| shell.ui(ui, |ui, _| screen(ui)));
        let high = crate::theme::Theme::new(
            true,
            Contrast::High,
            crate::theme::Tint::Swatch(crate::theme::Swatch::Blue),
        );
        assert_eq!(*crate::theme::current(&ctx), high);
    }

    const T: crate::i18n::Resources = crate::i18n::Resources {
        name: "shell-test",
        files: &[
            ("en", "t-entries = Entries\n"),
            ("ja", "t-entries = 項目\n"),
        ],
    };

    #[test]
    fn the_shell_installs_the_default_language() {
        let ctx = egui::Context::default();
        let mut shell = shell();
        crate::test_support::render_in(&ctx, [900.0, 600.0], |ui| shell.ui(ui, |ui, _| screen(ui)));
        assert!(crate::i18n::is_installed(&ctx));
    }

    #[test]
    fn the_shell_keeps_an_installed_language() {
        use crate::i18n::{Language, LanguageSettings};
        let ctx = egui::Context::default();
        let ja = LanguageSettings {
            language: Language::Explicit("ja".into()),
        };
        crate::i18n::install(&ctx, &ja);
        let mut shell = shell();
        crate::test_support::render_in(&ctx, [900.0, 600.0], |ui| shell.ui(ui, |ui, _| screen(ui)));
        assert_eq!(crate::i18n::current(&ctx).to_string(), "ja");
    }

    #[test]
    fn hover_text_follows_the_language() {
        use crate::i18n::{Language, LanguageSettings};
        let ctx = egui::Context::default();
        crate::i18n::register(&ctx, T);
        let shell = Shell::new(vec![Destination::new(
            Screen::Entries,
            crate::icons::settings(),
            "t-entries",
        )]);
        let set = |tag: &str| {
            crate::i18n::install(
                &ctx,
                &LanguageSettings {
                    language: Language::Explicit(tag.into()),
                },
            )
        };
        set("en");
        assert_eq!(shell.label_text(&ctx, 0), "Entries");
        set("ja");
        assert_eq!(shell.label_text(&ctx, 0), "項目");
    }

    /// Lay `shell` out over two frames of a `size` screen whose system bars take `safe`,
    /// with `ui` clear of them by `own` already; returns the rect the open screen got.
    fn render_safe(
        shell: &mut Shell<Screen>,
        size: [f32; 2],
        safe: egui::epaint::MarginF32,
        own: egui::epaint::MarginF32,
    ) -> egui::Rect {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0], size[1]));
        let mut drawn = egui::Rect::NOTHING;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(rect),
                safe_area_insets: Some(egui::SafeAreaInsets(safe)),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let inner = ui.max_rect() - own;
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    shell.ui(ui, |ui, _| {
                        drawn = ui.available_rect_before_wrap();
                        screen(ui);
                    })
                });
            });
            output.textures_delta.clear();
        }
        drawn
    }

    fn margin(left: f32, top: f32, right: f32, bottom: f32) -> egui::epaint::MarginF32 {
        egui::epaint::MarginF32 {
            left,
            right,
            top,
            bottom,
        }
    }

    #[test]
    fn narrow_it_keeps_clear_of_the_status_bar_and_the_home_indicator() {
        // An iPhone upright: the status bar and the dynamic island, the home indicator.
        let mut shell = shell().with_footer(prefs());
        let drawn = render_safe(
            &mut shell,
            [400.0, 800.0],
            margin(0.0, 62.0, 0.0, 34.0),
            egui::epaint::MarginF32::ZERO,
        );
        assert_eq!(drawn.top(), 62.0);
        let button = shell.footer_rects[0];
        assert!(button.bottom() <= 800.0 - 34.0, "{button:?}");
        // The bar sits right above the indicator: its fill runs on under it.
        assert!(button.bottom() > 800.0 - 34.0 - 12.0, "{button:?}");
        assert!(drawn.bottom() <= button.top(), "{drawn:?} {button:?}");
    }

    #[test]
    fn wide_it_keeps_clear_of_the_notch_and_the_home_indicator() {
        // An iPhone on its side: the notch on the left, the home indicator below.
        let mut shell = shell().with_footer(prefs());
        let drawn = render_safe(
            &mut shell,
            [900.0, 400.0],
            margin(59.0, 0.0, 59.0, 21.0),
            egui::epaint::MarginF32::ZERO,
        );
        let button = shell.footer_rects[0];
        assert!(button.left() >= 59.0, "{button:?}");
        assert!(button.bottom() <= 400.0 - 21.0, "{button:?}");
        assert!(drawn.right() <= 900.0 - 59.0, "{drawn:?}");
        assert!(drawn.bottom() <= 400.0 - 21.0, "{drawn:?}");
    }

    #[test]
    fn a_margin_the_app_already_keeps_is_not_doubled() {
        let safe = margin(0.0, 62.0, 0.0, 34.0);
        let mut shell = shell().with_footer(prefs());
        let drawn = render_safe(&mut shell, [400.0, 800.0], safe, safe);
        assert_eq!(drawn.top(), 62.0);
        let button = shell.footer_rects[0];
        assert!(button.bottom() > 800.0 - 34.0 - 12.0, "{button:?}");
    }

    #[test]
    fn with_only_a_footer_it_opens_on_the_footer() {
        let shell = Shell::new(vec![]).with_footer(prefs());
        assert_eq!(shell.current(), Some(Screen::Prefs));
    }

    /// A crowded bar on a `width`-point phone: eight screens, an action, the footer and a
    /// 12-point foot. Returns the cells', the footer's and the foot's rects.
    fn crowded(width: f32) -> (Vec<egui::Rect>, egui::Rect, egui::Rect) {
        let screens = (0..8)
            .map(|_| Destination::new(Screen::Entries, crate::icons::settings(), "entries"))
            .collect();
        let mut shell = Shell::new(screens)
            .with_actions(vec![Action::new("new", crate::icons::plus(), "new-entry")])
            .with_footer(prefs());
        let mut foot = egui::Rect::NOTHING;
        render([width, 700.0], |ui| {
            shell.ui_with_foot(
                ui,
                |ui| {
                    foot = ui
                        .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click())
                        .0;
                    None
                },
                |ui, _| screen(ui),
            )
        });
        (shell.cell_rects, shell.footer_rects[0], foot)
    }

    #[test]
    fn a_crowded_bar_keeps_the_footer_and_the_foot_on_the_screen() {
        for width in [320.0, 360.0, 411.0] {
            let (cells, footer, foot) = crowded(width);
            assert_eq!(cells.len(), 9, "{width}");
            for rect in cells.iter().chain([&footer, &foot]) {
                assert!(rect.is_positive(), "{width}: {rect:?}");
                assert!(
                    rect.left() >= 0.0 && rect.right() <= width,
                    "{width}: {rect:?}"
                );
            }
            // In order, and none on top of the next.
            for pair in cells.windows(2) {
                assert!(pair[0].right() <= pair[1].left(), "{width}: {pair:?}");
            }
            assert!(cells[8].right() <= footer.left(), "{width}");
            assert!(footer.right() <= foot.left(), "{width}");
            // The footer keeps its full size; the cells give way.
            assert_eq!(footer.width(), BUTTON, "{width}");
        }
    }

    #[test]
    fn a_roomy_bar_keeps_full_size_buttons() {
        let mut shell = shell().with_footer(prefs());
        render(SIZES[1], |ui| shell.ui(ui, |ui, _| screen(ui)));
        assert_eq!(shell.cell_rects.len(), 2);
        for rect in &shell.cell_rects {
            assert_eq!(rect.size(), egui::vec2(BUTTON, BUTTON), "{rect:?}");
        }
    }

    #[test]
    fn with_nothing_there_is_nothing_to_draw() {
        let mut shell: Shell<Screen> = Shell::new(vec![]);
        assert_eq!(shell.current(), None);
        for size in SIZES {
            render(size, |ui| shell.ui(ui, |_, s| panic!("{s:?} drawn")));
        }
    }
}
