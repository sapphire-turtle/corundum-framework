//! The demo's screens, one per piece of the framework, and what they keep between frames.

use corundum_framework::gui::i18n::{self, LanguageSettings, Message};
use corundum_framework::gui::settings::Settings;
use corundum_framework::gui::theme::{ThemeSettings, Tint};
use corundum_framework::gui::{Side, egui};

mod detail;
mod fonts;
pub(crate) mod icons;
mod language;
pub(crate) mod media;
mod overview;
mod platform;
pub(crate) mod settings;
mod theme_screen;

/// The screens, as the shell opens them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    /// The shell: its layout, the action and the foot slot.
    Overview,
    /// The theme: its settings, a TintPicker, the palette, widgets in it.
    Theme,
    /// The system's CJK fallback font, in samples.
    Fonts,
    /// Every bundled icon, in the theme's colours and a button's states.
    Icons,
    /// The language picker, switched in place, and the demo's own messages.
    Language,
    /// The side pane: albums, and the picked one's tracks.
    Detail,
    /// The clipboard, the insets and the app's directories.
    Platform,
    /// A made-up player under the OS's media keys and in its "now playing".
    Media,
    /// An app's Settings screen, in the footer: corundum's, with the demo's pages in it.
    Settings,
}

impl Screen {
    /// Every screen, in the rail's order, Settings last.
    pub const ALL: [Screen; 9] = [
        Screen::Overview,
        Screen::Theme,
        Screen::Fonts,
        Screen::Icons,
        Screen::Language,
        Screen::Detail,
        Screen::Platform,
        Screen::Media,
        Screen::Settings,
    ];
}

/// What the screens keep between frames.
pub(crate) struct State {
    /// The rail's action's clicks.
    pub(crate) clicks: u32,
    /// The foot slot's dot.
    pub(crate) online: bool,
    /// The last thing that happened, for the status bar: a log's last line.
    pub(crate) log: Message,
    /// A screen a screen asked to open, opened after the shell has drawn.
    pub(crate) open: Option<Screen>,
    /// The CJK font the window installed: `None` when the demo did not start in a window.
    cjk: Option<Option<String>>,
    /// The TintPicker on its own's value.
    accent: Option<Tint>,
    /// The sample widgets' values.
    checked: bool,
    selected: bool,
    text: String,
    icon_style: icons::Style,
    detail: detail::State,
    platform: platform::State,
    pub(crate) media: media::State,
}

impl State {
    pub(crate) fn new(cjk: Option<Option<String>>) -> State {
        State {
            clicks: 0,
            online: true,
            log: Message::new("demo-status-ready"),
            open: None,
            cjk,
            accent: None,
            checked: true,
            selected: true,
            text: String::new(),
            icon_style: icons::Style::Text,
            detail: detail::State::default(),
            platform: platform::State::read(),
            media: media::State::new(),
        }
    }
}

/// Draw `screen` into what the shell and the side pane left of `ui`, as its root.
pub(crate) fn ui(
    ui: &mut egui::Ui,
    screen: Screen,
    state: &mut State,
    settings: &mut Settings,
    theme: &mut ThemeSettings,
    language: &mut LanguageSettings,
    side: &mut Side,
) {
    // These two lay out their own panels.
    match screen {
        Screen::Settings => return settings.ui(ui),
        Screen::Detail => return detail::ui(ui, &mut state.detail, &mut state.log, side),
        _ => {}
    }
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| match screen {
                Screen::Overview => overview::ui(ui, state),
                Screen::Theme => theme_screen::ui(ui, state, theme),
                Screen::Fonts => fonts::ui(ui, state.cjk.as_ref()),
                Screen::Icons => icons::ui(ui, &mut state.icon_style),
                Screen::Language => language::ui(ui, state, language),
                Screen::Platform => platform::ui(ui, &mut state.platform),
                Screen::Media => media::ui(ui, &mut state.media),
                Screen::Settings | Screen::Detail => unreachable!("drawn above"),
            });
    });
}

/// The app's side pane: the note being written, else the open screen's item (the Detail
/// screen's album), else a word that there is none. A note saved or discarded closes the
/// pane, where it was open on the card or in the screen's place.
pub(crate) fn side_ui(
    ui: &mut egui::Ui,
    screen: Screen,
    state: &mut State,
    draft: &mut Option<String>,
    side: &mut Side,
) {
    if let Some(text) = draft {
        ui.heading(i18n::tr(ui.ctx(), "demo-note-new"));
        ui.add(egui::TextEdit::multiline(text).desired_width(f32::INFINITY));
        let (save, discard) = ui
            .horizontal(|ui| {
                let save = ui
                    .add_enabled(
                        !text.trim().is_empty(),
                        egui::Button::new(i18n::tr(ui.ctx(), "demo-note-save")),
                    )
                    .clicked();
                let discard = ui.button(i18n::tr(ui.ctx(), "demo-note-discard")).clicked();
                (save, discard)
            })
            .inner;
        if save {
            state.log = Message::new("demo-status-saved").arg("note", text.trim().to_owned());
        }
        if save || discard {
            *draft = None;
            side.close();
        }
        return;
    }
    match screen {
        Screen::Detail => detail::side_ui(ui, &state.detail),
        _ => {
            ui.weak(i18n::tr(ui.ctx(), "demo-pane-empty"));
        }
    }
}

/// A screen's heading: its name, as the rail has it.
fn heading(ui: &mut egui::Ui, id: &str) {
    ui.heading(i18n::tr(ui.ctx(), id));
    ui.add_space(8.0);
}

/// A section's title within a screen.
fn section(ui: &mut egui::Ui, id: &str) {
    ui.add_space(12.0);
    ui.strong(i18n::tr(ui.ctx(), id));
    ui.add_space(4.0);
}
