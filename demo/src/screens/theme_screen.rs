//! The Theme: its settings, a TintPicker on its own, the palette, and widgets in it.

use corundum_framework::gui::egui;
use corundum_framework::gui::theme::{self, Swatch, ThemeSettings, Tint, TintPicker, Tone};
use corundum_framework::gui::tr;

use super::{State, heading, section};

pub(super) fn ui(ui: &mut egui::Ui, state: &mut State, settings: &mut ThemeSettings) {
    heading(ui, "demo-screen-theme");

    section(ui, "demo-theme-settings");
    theme::settings_ui(ui, settings);

    section(ui, "demo-theme-picker");
    TintPicker::new(&mut state.accent, Tint::Swatch(Swatch::Green))
        .tone(Tone::Raised)
        .preview(true)
        .show(ui);

    section(ui, "demo-theme-palette");
    palette(ui);

    section(ui, "demo-theme-widgets");
    ui.horizontal_wrapped(|ui| {
        let _ = ui.button(tr!(ui, "demo-theme-button"));
        ui.add_enabled(false, egui::Button::new(tr!(ui, "demo-theme-disabled")));
        ui.checkbox(&mut state.checked, tr!(ui, "demo-theme-checkbox"));
        ui.toggle_value(&mut state.selected, tr!(ui, "demo-theme-selectable"));
        ui.hyperlink_to(
            tr!(ui, "demo-theme-link"),
            "https://github.com/sapphire-turtle/corundum-framework",
        );
    });
    ui.add(egui::TextEdit::singleline(&mut state.text).hint_text(tr!(ui, "demo-theme-hint")));
}

/// Every cell of the theme's grid: a row per tone, a column per swatch.
fn palette(ui: &mut egui::Ui) {
    let theme = theme::current(ui.ctx());
    egui::Grid::new("demo_palette")
        .spacing([4.0, 4.0])
        .show(ui, |ui| {
            ui.label("");
            for swatch in Swatch::ALL {
                ui.small(format!("{swatch:?}"));
            }
            ui.end_row();
            for tone in Tone::ALL {
                ui.small(format!("{tone:?}"));
                for swatch in Swatch::ALL {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 18.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 3.0, theme.color(tone, swatch));
                }
                ui.end_row();
            }
        });
}
