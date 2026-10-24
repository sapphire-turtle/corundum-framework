//! The Overview: the shell itself — its layout at this width, the action, the foot slot.

use corundum_framework::gui::{COMPACT_WIDTH, egui, tr};

use super::{Screen, State, heading, section};

pub(super) fn ui(ui: &mut egui::Ui, state: &mut State) {
    heading(ui, "demo-screen-overview");
    ui.label(tr!(ui, "demo-overview-intro", width = COMPACT_WIDTH as f64));

    section(ui, "demo-overview-layout");
    let width = ui.ctx().content_rect().width().round() as f64;
    if width < COMPACT_WIDTH as f64 {
        ui.label(tr!(ui, "demo-overview-bar", width = width));
    } else {
        ui.label(tr!(ui, "demo-overview-rail", width = width));
    }
    ui.label(tr!(ui, "demo-overview-clicks", count = state.clicks));
    ui.label(if state.online {
        tr!(ui, "demo-overview-online")
    } else {
        tr!(ui, "demo-overview-offline")
    });
    ui.add_space(8.0);
    if ui.button(tr!(ui, "demo-overview-open-settings")).clicked() {
        state.open = Some(Screen::Settings);
    }
}
