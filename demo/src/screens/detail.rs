//! The Detail screen: albums in the main view, the picked one's tracks in the app's side
//! pane.

use corundum_framework::gui::egui;
use corundum_framework::gui::i18n::Message;
use corundum_framework::gui::tr;

/// The albums, by title, artist and tracks: names, not translated.
const ALBUMS: [(&str, &str, &[&str]); 4] = [
    ("Blue Hours", "Aoi", &["Dawn", "Tide", "Lantern", "Low Sun"]),
    ("Paper Moons", "Kite Club", &["Fold", "Crease", "Origami"]),
    (
        "Night Bus",
        "Sora",
        &["Last Stop", "Neon", "Rain on Glass", "Depot", "Home"],
    ),
    ("Garden", "Mika", &["Moss"]),
];

/// What the screen keeps: the album picked, and whether a pick asks the pane to open.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) picked: usize,
    pub(crate) opened: bool,
}

/// Draw the screen into what the pane leaves of `ui`, as its root; a pick is logged, and
/// asks the app's side pane to open.
pub(super) fn ui(ui: &mut egui::Ui, state: &mut State, log: &mut Message) {
    let mut chosen = None;
    egui::CentralPanel::default().show(ui, |ui| {
        super::heading(ui, "demo-screen-detail");
        ui.label(tr!(ui, "demo-detail-intro"));
        ui.add_space(8.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                for (i, (title, artist, _)) in ALBUMS.iter().enumerate() {
                    if ui
                        .selectable_label(state.picked == i, format!("{title} — {artist}"))
                        .clicked()
                    {
                        chosen = Some(i);
                    }
                }
            });
        });
    });
    if let Some(i) = chosen {
        state.picked = i;
        state.opened = true;
        *log = Message::new("demo-status-picked").arg("album", ALBUMS[i].0);
    }
}

/// The picked album's tracks, in the side pane.
pub(crate) fn side_ui(ui: &mut egui::Ui, picked: usize) {
    let (title, artist, tracks) = ALBUMS[picked];
    ui.heading(title);
    ui.label(artist);
    ui.weak(tr!(ui, "demo-detail-tracks", count = tracks.len()));
    ui.separator();
    for (n, track) in tracks.iter().enumerate() {
        ui.label(format!("{}. {track}", n + 1));
    }
}
