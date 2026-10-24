//! The Icons: every bundled Lucide icon, in one of the theme's colours or a button's states.

use corundum_framework::gui::egui;
use corundum_framework::gui::icons;
use corundum_framework::gui::theme::{Swatch, Tone};
use corundum_framework::gui::tr;

use super::heading;

type Icon = fn() -> egui::ImageSource<'static>;

/// Every icon corundum-framework-gui bundles, by its Lucide name.
pub(crate) const ALL: [(&str, Icon); 36] = [
    ("settings", icons::settings),
    ("arrow-left", icons::arrow_left),
    ("x", icons::x),
    ("plus", icons::plus),
    ("trash-2", icons::trash_2),
    ("pencil-line", icons::pencil_line),
    ("refresh-cw", icons::refresh_cw),
    ("info", icons::info),
    ("ellipsis", icons::ellipsis),
    ("chevron-right", icons::chevron_right),
    ("chevron-down", icons::chevron_down),
    ("circle-check", icons::circle_check),
    ("circle-x", icons::circle_x),
    ("folder", icons::folder),
    ("file-text", icons::file_text),
    ("rotate-ccw", icons::rotate_ccw),
    ("dice-5", icons::dice_5),
    ("image", icons::picture),
    ("images", icons::images),
    ("music", icons::music),
    ("list-music", icons::list_music),
    ("smartphone", icons::smartphone),
    ("monitor-smartphone", icons::monitor_smartphone),
    ("play", icons::play),
    ("pause", icons::pause),
    ("skip-back", icons::skip_back),
    ("skip-forward", icons::skip_forward),
    ("volume-2", icons::volume_2),
    ("volume-x", icons::volume_x),
    ("star", icons::star),
    ("tag", icons::tag),
    ("search", icons::search),
    ("clipboard-copy", icons::clipboard_copy),
    ("upload", icons::upload),
    ("download", icons::download),
    ("cloud-off", icons::cloud_off),
];

/// How the grid draws the icons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Style {
    Text,
    Muted,
    Accent,
    Red,
    Green,
    Button,
    Selected,
    Disabled,
}

impl Style {
    pub(crate) const ALL: [Style; 8] = [
        Style::Text,
        Style::Muted,
        Style::Accent,
        Style::Red,
        Style::Green,
        Style::Button,
        Style::Selected,
        Style::Disabled,
    ];

    fn label(self) -> &'static str {
        match self {
            Style::Text => "demo-icons-text",
            Style::Muted => "demo-icons-muted",
            Style::Accent => "demo-icons-accent",
            Style::Red => "demo-icons-red",
            Style::Green => "demo-icons-green",
            Style::Button => "demo-icons-button",
            Style::Selected => "demo-icons-selected",
            Style::Disabled => "demo-icons-disabled",
        }
    }
}

const SIZE: f32 = 24.0;

pub(super) fn ui(ui: &mut egui::Ui, style: &mut Style) {
    heading(ui, "demo-screen-icons");
    ui.label(tr!(ui, "demo-icons-intro", count = ALL.len()));
    ui.horizontal_wrapped(|ui| {
        for s in Style::ALL {
            let label = tr!(ui, s.label());
            ui.radio_value(style, s, label);
        }
    });
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        for (name, icon) in ALL {
            ui.allocate_ui(egui::vec2(96.0, 64.0), |ui| {
                ui.set_width(96.0);
                ui.vertical_centered(|ui| {
                    draw(ui, *style, icon());
                    ui.small(name);
                });
            });
        }
    });
}

fn draw(ui: &mut egui::Ui, style: Style, src: egui::ImageSource<'static>) {
    let as_ = |ui: &egui::Ui, tone, swatch| icons::image_as(ui, src.clone(), SIZE, tone, swatch);
    let button = || icons::button(src.clone(), SIZE).min_size(egui::vec2(36.0, 36.0));
    match style {
        Style::Text => {
            ui.add(icons::image(ui, src.clone(), SIZE));
        }
        Style::Muted => {
            ui.add(as_(ui, Tone::Muted, Swatch::Neutral));
        }
        Style::Accent => {
            ui.add(as_(ui, Tone::Text, Swatch::Accent));
        }
        Style::Red => {
            ui.add(as_(ui, Tone::Text, Swatch::Red));
        }
        Style::Green => {
            ui.add(as_(ui, Tone::Text, Swatch::Green));
        }
        Style::Button => {
            ui.add(button());
        }
        Style::Selected => {
            ui.add(button().selected(true));
        }
        Style::Disabled => {
            ui.add_enabled(false, button());
        }
    }
}
