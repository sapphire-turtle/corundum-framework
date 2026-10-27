//! [`RailPage`]: the order of the shell's rail, and which items it shows.

use crate::shell::{RailEntry, RailOrder, move_to, published, request};
use crate::tr;

/// The settings page where the person drags the [`Shell`](crate::Shell)'s screens and
/// actions into their order and ticks those the rail shows; the rest are in its More menu,
/// with those the window has no room for. Put it into the app's
/// [`Settings`](super::Settings), in its general group, say.
///
/// Unlike the other pages, it takes effect at once: it reaches the shell through the egui
/// context, and the shell keeps the order — on the device too, with the `serde` feature and
/// a [store](crate::store). An app that keeps the order itself reads it back with
/// [`Shell::rail_order`](crate::Shell::rail_order).
#[derive(Default)]
pub struct RailPage {
    /// The rows' rects, for the tests.
    #[cfg(test)]
    rows: Vec<egui::Rect>,
}

/// What a row carries when dragged: its place.
struct RowDrag(usize);

impl RailPage {
    pub fn new() -> RailPage {
        RailPage::default()
    }
}

impl super::SettingsSection for RailPage {
    fn label(&self, ctx: &egui::Context) -> String {
        tr!(ctx, "corundum-settings-rail")
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr!(ui, "corundum-settings-rail"));
        ui.separator();
        ui.label(tr!(ui, "corundum-settings-rail-intro"));
        ui.add_space(8.0);
        let Some(mut entries) = published(ui.ctx()) else {
            return;
        };
        if let Some(changed) = self.rows_ui(ui, &entries) {
            match changed {
                Change::Moved(from, to, after) => {
                    move_to(&mut entries, from, to, after);
                }
                Change::Toggled(i) => entries[i].shown = !entries[i].shown,
            }
            request(ui.ctx(), order_of(&entries));
        }
        ui.add_space(8.0);
        if ui.button(tr!(ui, "corundum-settings-rail-reset")).clicked() {
            request(ui.ctx(), RailOrder::default());
        }
    }
}

/// What the person did to the list.
enum Change {
    /// The row at the first place was dropped before (or, `true`, after) the second's.
    Moved(usize, usize, bool),
    /// The row's box was ticked or cleared.
    Toggled(usize),
}

impl RailPage {
    fn rows_ui(&mut self, ui: &mut egui::Ui, entries: &[RailEntry]) -> Option<Change> {
        let ctx = ui.ctx().clone();
        let mut change = None;
        #[cfg(test)]
        self.rows.clear();
        for (n, entry) in entries.iter().enumerate() {
            let row = ui.horizontal(|ui| {
                ui.dnd_drag_source(
                    egui::Id::unique(("corundum_rail_row", n)),
                    RowDrag(n),
                    |ui| {
                        ui.add(crate::icons::image_as(
                            ui,
                            crate::icons::grip_vertical(),
                            16.0,
                            crate::theme::Tone::Muted,
                            crate::theme::Swatch::Neutral,
                        ));
                    },
                )
                .response
                .on_hover_text(tr!(&ctx, "corundum-settings-rail-drag"));
                let mut shown = entry.shown;
                let image = crate::icons::image(ui, entry.icon.clone(), 16.0);
                if ui
                    .checkbox(&mut shown, (image, tr!(&ctx, entry.label)))
                    .on_hover_text(tr!(&ctx, "corundum-settings-rail-show"))
                    .changed()
                {
                    change = Some(Change::Toggled(n));
                }
            });
            let row = row.response;
            #[cfg(test)]
            self.rows.push(row.rect);
            // Dropped on the row's upper half, the dragged one goes before it; else after.
            let below = ctx
                .pointer_interact_pos()
                .is_some_and(|p| p.y > row.rect.center().y);
            if row.dnd_hover_payload::<RowDrag>().is_some() {
                let y = if below {
                    row.rect.bottom()
                } else {
                    row.rect.top()
                };
                ui.painter().hline(
                    row.rect.x_range(),
                    y,
                    egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                );
            }
            if let Some(drag) = row.dnd_release_payload::<RowDrag>() {
                change = Some(Change::Moved(drag.0, n, below));
            }
        }
        change
    }
}

/// The order `entries` stand in.
fn order_of(entries: &[RailEntry]) -> RailOrder {
    RailOrder {
        order: entries.iter().map(|e| e.key.into()).collect(),
        hidden: entries
            .iter()
            .filter(|e| !e.shown)
            .map(|e| e.key.into())
            .collect(),
    }
}

// The shell drawn through its deprecated closures: the page is all these look at.
#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::settings::SettingsSection;
    use crate::{Action, Destination, Shell};

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Screen {
        A,
        B,
    }

    fn shell() -> Shell<Screen> {
        Shell::from_items(vec![
            Action::new("new", crate::icons::plus(), "new-entry").into(),
            Destination::new(Screen::A, crate::icons::info(), "a").into(),
            Destination::new(Screen::B, crate::icons::star(), "b").into(),
        ])
    }

    /// Frames of the shell with the page as its screen, in `ctx`; a click at `at` if given.
    fn frames(
        ctx: &egui::Context,
        shell: &mut Shell<Screen>,
        page: &mut RailPage,
        at: Option<egui::Pos2>,
    ) {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0));
        let mut frame = |events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                shell.ui(ui, |ui, _| {
                    egui::CentralPanel::default().show(ui, |ui| page.ui(ui));
                })
            });
            output.textures_delta.clear();
        };
        frame(vec![]);
        frame(vec![]);
        if let Some(pos) = at {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(vec![egui::Event::PointerMoved(pos)]);
            frame(vec![button(true)]);
            frame(vec![button(false)]);
            frame(vec![]);
        }
    }

    #[test]
    fn the_page_lists_the_rail_in_its_order() {
        let ctx = egui::Context::default();
        let (mut shell, mut page) = (shell(), RailPage::new());
        frames(&ctx, &mut shell, &mut page, None);
        assert_eq!(page.rows.len(), 3);
        for pair in page.rows.windows(2) {
            assert!(pair[0].bottom() <= pair[1].top(), "{pair:?}");
        }
    }

    #[test]
    fn clearing_a_box_hides_the_item() {
        let ctx = egui::Context::default();
        let (mut shell, mut page) = (shell(), RailPage::new());
        frames(&ctx, &mut shell, &mut page, None);
        // The box sits after the handle.
        let row = page.rows[0];
        frames(
            &ctx,
            &mut shell,
            &mut page,
            Some(row.left_center() + egui::vec2(30.0, 0.0)),
        );
        assert_eq!(shell.rail_order().hidden, ["new"]);
    }

    #[test]
    fn the_order_of_entries_is_their_keys() {
        let ctx = egui::Context::default();
        let (mut shell, mut page) = (shell(), RailPage::new());
        frames(&ctx, &mut shell, &mut page, None);
        let mut entries = published(&ctx).unwrap();
        move_to(&mut entries, 0, 2, true);
        entries[1].shown = false;
        let order = order_of(&entries);
        assert_eq!(order.order, ["a", "b", "new"]);
        assert_eq!(order.hidden, ["b"]);
    }

    #[test]
    fn without_a_shell_the_page_lists_nothing() {
        let mut page = RailPage::new();
        crate::test_support::render([600.0, 400.0], |ui| page.ui(ui));
        assert!(page.rows.is_empty());
    }
}
