//! The Fonts: which CJK font the system gave, and text in each script at a few sizes.

use corundum_framework::gui::{egui, tr};

use super::{heading, section};

/// One line per script the fallback must cover, egui's own fonts' Latin first.
const SAMPLES: [&str; 5] = [
    "The quick brown fox jumps over the lazy dog. 0123456789",
    "日本語：いろはにほへと　漢字とかなの混じった文",
    "简体中文：我能吞下玻璃而不伤身体。",
    "繁體中文：我能吞下玻璃而不傷身體。",
    "한국어: 나는 유리를 먹을 수 있어요. 그래도 아프지 않아요.",
];

/// `cjk`: the family the window installed — `Some(None)` when the system has none,
/// `None` when the demo was not started in a window.
pub(super) fn ui(ui: &mut egui::Ui, cjk: Option<&Option<String>>) {
    heading(ui, "demo-screen-fonts");
    match cjk {
        Some(Some(family)) => {
            ui.label(tr!(ui, "demo-fonts-fallback", family = family.as_str()));
        }
        _ => {
            ui.label(tr!(ui, "demo-fonts-none"));
        }
    }

    section(ui, "demo-fonts-proportional");
    for size in [12.0, 16.0, 24.0] {
        for text in SAMPLES {
            ui.label(egui::RichText::new(text).size(size));
        }
        ui.add_space(4.0);
    }

    section(ui, "demo-fonts-monospace");
    for text in SAMPLES {
        ui.label(egui::RichText::new(text).monospace());
    }
}
