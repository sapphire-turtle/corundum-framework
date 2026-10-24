//! The Language: the picker, switched in place, and the demo's own messages in it.

use corundum_framework::gui::egui;
use corundum_framework::gui::i18n::{self, LanguageSettings};
use corundum_framework::gui::tr;

use super::{State, heading, section};

pub(super) fn ui(ui: &mut egui::Ui, state: &State, settings: &mut LanguageSettings) {
    heading(ui, "demo-screen-language");

    section(ui, "demo-language-picker");
    i18n::settings_ui(ui, settings);
    ui.add_space(8.0);
    let ctx = ui.ctx().clone();
    let current = i18n::name_of(&ctx, &i18n::current(&ctx));
    ui.label(tr!(ui, "demo-language-current", language = current));
    let available = i18n::available(&ctx)
        .iter()
        .map(|l| i18n::name_of(&ctx, l))
        .collect::<Vec<_>>()
        .join(", ");
    ui.label(tr!(ui, "demo-language-available", languages = available));
    ui.label(tr!(
        ui,
        "demo-language-system",
        languages = state.platform.languages.join(", ")
    ));

    ui.add_space(12.0);
    ui.label(tr!(ui, "demo-language-greeting"));
    ui.label(tr!(ui, "demo-language-clicks", count = state.clicks));
}
