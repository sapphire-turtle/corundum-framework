//! The Overview: the shell itself — its layout at this width, the action, the foot slot.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use corundum_framework::gui::theme::{Swatch, Tone};
use corundum_framework::gui::{COMPACT_WIDTH, Refresh, egui, icons, tr};

use super::{Screen, State, heading, section};

/// corundum's logo: sapphire's hexagon holding asteria's sparkle. It strokes in white, as
/// the bundled icons do, so the theme tints it.
fn logo() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/corundum.svg")
}

/// The screen's made-up refresh, slow as a sync with a server: it runs for two seconds on
/// a thread of its own, then counts itself.
#[derive(Default)]
pub(crate) struct Refreshing {
    busy: Arc<AtomicBool>,
    count: Arc<AtomicU32>,
    /// Repainted when a refresh ends: kept from the screen's first frame.
    ctx: Option<egui::Context>,
}

impl Refreshing {
    pub(crate) fn state(&self) -> Refresh {
        if self.busy.load(Ordering::Acquire) {
            Refresh::Running
        } else {
            Refresh::Idle
        }
    }

    /// Start a refresh. In a browser, which has no threads, it ends at once.
    pub(crate) fn start(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            self.count.fetch_add(1, Ordering::AcqRel);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.busy.store(true, Ordering::Release);
            let (busy, count, ctx) = (self.busy.clone(), self.count.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                count.fetch_add(1, Ordering::AcqRel);
                busy.store(false, Ordering::Release);
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            });
        }
    }

    /// How many refreshes have ended.
    pub(crate) fn count(&self) -> u32 {
        self.count.load(Ordering::Acquire)
    }
}

pub(super) fn ui(ui: &mut egui::Ui, state: &mut State) {
    state.overview.ctx.get_or_insert_with(|| ui.ctx().clone());
    ui.add(icons::image_as(
        ui,
        logo(),
        48.0,
        Tone::Text,
        Swatch::Accent,
    ));
    ui.add_space(8.0);
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

    section(ui, "demo-overview-refresh");
    ui.label(tr!(ui, "demo-overview-refresh-hint"));
    ui.label(tr!(
        ui,
        "demo-overview-refreshed",
        count = state.overview.count()
    ));
    ui.add_space(8.0);
    if ui.button(tr!(ui, "demo-overview-open-settings")).clicked() {
        state.open = Some(Screen::Settings);
    }
}
