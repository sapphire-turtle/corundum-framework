//! [`DetailPane`]: a right column beside the screens — what the main view's item holds (the
//! playlist playing, the journal open), or a new entry begun from the rail's action — over
//! the main view when there is no room for it, or in its place on a phone; and
//! [`toggle_ui`], the button turning its column on and off, which the
//! [`Shell`](crate::Shell)'s status bar holds.

use crate::{COMPACT_WIDTH, icons, tr};

/// The column's width at first; a drag changes it.
const COLUMN_WIDTH: f32 = 320.0;
/// The narrowest the column is dragged to.
const COLUMN_MIN_WIDTH: f32 = 240.0;
/// The narrowest the main view gets beside the column; with less room, the detail is shown
/// over the main view instead.
const MAIN_MIN_WIDTH: f32 = 360.0;
/// The card's widest, over the main view.
const CARD_MAX_WIDTH: f32 = 560.0;
/// The share of the screen the card takes at most, across and down.
const CARD_SHARE: f32 = 0.8;

/// The pane drawn in a frame, for [`toggle_ui`]: kept in the context, as the toggle is drawn
/// apart from it.
#[derive(Clone, Copy, Debug)]
struct Offer {
    /// The pane.
    pane: egui::Id,
    /// Whether its column is on.
    column: bool,
    /// The pass it was drawn in.
    pass: u64,
}

/// Where the context keeps the [`Offer`].
fn offer_id() -> egui::Id {
    egui::Id::unique("corundum_detail_offer")
}

/// Where the context keeps the pane whose toggle was clicked.
fn clicked_id() -> egui::Id {
    egui::Id::unique("corundum_detail_clicked")
}

/// The part of a pane [`DetailPane::show`] asks to be drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Part {
    /// The main view, as the root of what the pane leaves.
    Main,
    /// The detail, in the column, the card or the phone's screen.
    Detail,
}

/// How the detail was shown in the last frame.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shown {
    /// Not at all: the main view alone.
    Not,
    /// In the right column, beside the main view.
    Column,
    /// On a card over the main view.
    Card,
    /// In the main view's place, on a phone.
    Screen,
}

/// The main view and the detail beside it, laid out for the window. Hold one for the app
/// and draw it around every screen, inside the shell's `content`: the column and its toggle
/// then stay whatever screen is open, and a new entry begun from the rail's action shows
/// beside the screen the person is on. What the detail shows is the app's — the draft being
/// written, else the open screen's item, else a word that there is none. (A pane held by
/// one screen alone is drawn, and toggled, on that screen only.)
///
/// - **Wide, with the column on**: the detail is always in a right column beside the main
///   view, its width dragged at its edge. [`toggle_ui`] — at the right end of the shell's
///   status bar ([`Shell::ui_with_status`](crate::Shell::ui_with_status)) — turns the
///   column off and on.
/// - **Wide, with the column off** (or no room for it): the detail shows once
///   [`open`](Self::open)ed, on a card over the middle of the screen, which `×`, `Esc` or a
///   click outside it closes.
/// - **Narrower than [`COMPACT_WIDTH`]**: the detail shows once opened, in the main view's
///   place, under a way back to it.
///
/// The app opens it when the person picks an item — and only then, while the column is
/// showing it anyway, it does nothing.
#[derive(Clone, Debug)]
pub struct DetailPane {
    id: egui::Id,
    /// The person wants the right column.
    column: bool,
    /// The detail is shown over the main view, or in its place.
    open: bool,
    #[cfg(test)]
    shown: Shown,
}

impl DetailPane {
    /// A pane named by `id_salt` (unique among the app's panes), with the column on and
    /// nothing open.
    pub fn new(id_salt: impl egui::AsIdSalt) -> DetailPane {
        DetailPane {
            id: egui::Id::unique(("corundum_detail", id_salt)),
            column: true,
            open: false,
            #[cfg(test)]
            shown: Shown::Not,
        }
    }

    /// The pane, with the column `on` or off: as the app saved it.
    pub fn with_column(mut self, on: bool) -> DetailPane {
        self.column = on;
        self
    }

    /// Whether the person wants the right column. Save it with the app's settings to keep
    /// it across runs.
    pub fn column(&self) -> bool {
        self.column
    }

    /// Turn the right column on or off.
    pub fn set_column(&mut self, on: bool) {
        self.column = on;
    }

    /// Show the detail: on the card, or on a phone in the main view's place. While the
    /// column shows it, there is nothing more to show.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Back to the main view alone.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Whether the detail is shown on the card or in the main view's place.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Render into `ui`, as the root of what the shell leaves of it. `main` draws the main
    /// view into what the column leaves, as it would at the root — its own panels, then a
    /// `CentralPanel` — so a screen draws the same inside the pane as without it. `detail`
    /// draws the detail into the column, the card or the phone's screen (no panels of its
    /// own).
    ///
    /// A [`Shell`](crate::Shell) built [`with_side_pane`](crate::Shell::with_side_pane)
    /// holds a pane and draws it itself, around every screen.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        main: impl FnOnce(&mut egui::Ui),
        detail: impl FnOnce(&mut egui::Ui),
    ) {
        let (mut main, mut detail) = (Some(main), Some(detail));
        self.show(ui, |ui, part| match part {
            Part::Main => main.take().map_or((), |m| m(ui)),
            Part::Detail => detail.take().map_or((), |d| d(ui)),
        });
    }

    /// As [`ui`](Self::ui), with one `draw` for both parts, called once for each part shown:
    /// what lets the shell hand both to an app's `&mut self` in turn.
    pub(crate) fn show(&mut self, ui: &mut egui::Ui, mut draw: impl FnMut(&mut egui::Ui, Part)) {
        if ui.ctx().content_rect().width() < COMPACT_WIDTH {
            self.screen_ui(ui, &mut draw);
            return;
        }
        let room = ui.available_width();
        let fits = room >= MAIN_MIN_WIDTH + COLUMN_WIDTH;
        if fits {
            self.offer_toggle(ui.ctx());
        }
        let column = self.column && fits;
        if column {
            // The column shows it already: nothing is left open to pop up when it goes.
            self.open = false;
            egui::Panel::right(self.id.with("column"))
                .resizable(true)
                .default_size(COLUMN_WIDTH)
                .min_size(COLUMN_MIN_WIDTH)
                .max_size(room - MAIN_MIN_WIDTH)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(self.id.with("column_scroll"))
                        .auto_shrink(false)
                        .show(ui, |ui| draw(ui, Part::Detail));
                });
            self.set_shown(true, false);
        }
        draw(ui, Part::Main);
        if !column {
            self.set_shown(false, self.open);
            if self.open {
                self.card_ui(ui.ctx(), &mut draw);
            }
        }
    }

    /// On a phone: the main view, or the detail in its place under a way back.
    fn screen_ui(&mut self, ui: &mut egui::Ui, draw: &mut impl FnMut(&mut egui::Ui, Part)) {
        #[cfg(test)]
        {
            self.shown = if self.open { Shown::Screen } else { Shown::Not };
        }
        if !self.open {
            draw(ui, Part::Main);
            return;
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let back = icons::image(ui, icons::arrow_left(), 14.0);
            if ui
                .add(egui::Button::image_and_text(
                    back,
                    tr!(ui, "corundum-detail-back"),
                ))
                .clicked()
            {
                self.open = false;
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt(self.id.with("screen_scroll"))
                .auto_shrink(false)
                .show(ui, |ui| draw(ui, Part::Detail));
        });
    }

    /// Take a click of the toggle on this pane, then offer the toggle for this frame.
    fn offer_toggle(&mut self, ctx: &egui::Context) {
        let clicked = ctx.data_mut(|d| {
            let ours = d.get_temp::<egui::Id>(clicked_id()) == Some(self.id);
            if ours {
                d.remove::<egui::Id>(clicked_id());
            }
            ours
        });
        if clicked {
            self.column = !self.column;
        }
        let offer = Offer {
            pane: self.id,
            column: self.column,
            pass: ctx.cumulative_pass_nr(),
        };
        ctx.data_mut(|d| d.insert_temp(offer_id(), offer));
    }

    /// The card over the middle of the screen, with a way to close it.
    fn card_ui(&mut self, ctx: &egui::Context, draw: &mut impl FnMut(&mut egui::Ui, Part)) {
        let screen = ctx.content_rect();
        let mut close = false;
        let response = egui::Modal::new(self.id.with("card")).show(ctx, |ui| {
            ui.set_width((screen.width() * CARD_SHARE).min(CARD_MAX_WIDTH));
            // A row as tall as the button: a layout of its own would take the whole height.
            egui::Sides::new().show(
                ui,
                |_| {},
                |ui| {
                    close = ui
                        .add(icons::button(icons::x(), 14.0))
                        .on_hover_text(tr!(ui, "corundum-detail-close"))
                        .clicked();
                },
            );
            egui::ScrollArea::vertical()
                .id_salt(self.id.with("card_scroll"))
                .max_height(screen.height() * CARD_SHARE)
                .show(ui, |ui| draw(ui, Part::Detail));
        });
        if close || response.should_close() {
            self.open = false;
        }
    }

    #[cfg_attr(not(test), allow(unused_variables))]
    fn set_shown(&mut self, column: bool, card: bool) {
        #[cfg(test)]
        {
            self.shown = match (column, card) {
                (true, _) => Shown::Column,
                (false, true) => Shown::Card,
                (false, false) => Shown::Not,
            };
        }
    }
}

/// The button turning the column of the detail pane on screen on and off: the
/// [`Shell`](crate::Shell)'s status bar draws it at its right end, and an app without one
/// draws it where it likes. Nothing is drawn when no pane was drawn this frame or the last,
/// or when the window has no room for a column.
pub fn toggle_ui(ui: &mut egui::Ui) -> Option<egui::Response> {
    let offer = ui.ctx().data(|d| d.get_temp::<Offer>(offer_id()))?;
    // This frame's, or the last's when the toggle is drawn before the pane.
    if offer.pass + 1 < ui.ctx().cumulative_pass_nr() {
        return None;
    }
    let hover = if offer.column {
        "corundum-detail-hide"
    } else {
        "corundum-detail-show"
    };
    let response = ui
        .add(icons::button(icons::panel_right(), 14.0).selected(offer.column))
        .on_hover_text(tr!(ui, hover));
    if response.clicked() {
        // The pane turns on its next frame.
        ui.ctx()
            .data_mut(|d| d.insert_temp(clicked_id(), offer.pane));
        ui.ctx().request_repaint();
    }
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{render, render_in};

    const WIDE: [f32; 2] = [1000.0, 700.0];
    const NARROW_DESKTOP: [f32; 2] = [640.0, 700.0];
    const PHONE: [f32; 2] = [390.0, 844.0];

    /// Render `pane` at `size`: which of the main view and the detail were drawn.
    fn drawn(pane: &mut DetailPane, size: [f32; 2]) -> (bool, bool) {
        let (mut main, mut detail) = (false, false);
        render(size, |ui| {
            pane.ui(ui, |_| main = true, |_| detail = true);
        });
        (main, detail)
    }

    #[test]
    fn wide_the_column_shows_the_detail_beside_the_main_view() {
        let mut pane = DetailPane::new("t");
        assert_eq!(drawn(&mut pane, WIDE), (true, true));
        assert_eq!(pane.shown, Shown::Column);
    }

    #[test]
    fn a_main_view_with_its_own_panels_keeps_clear_of_the_column() {
        let ctx = egui::Context::default();
        let mut pane = DetailPane::new("t");
        let (mut list, mut page, mut column) = (
            egui::Rect::NOTHING,
            egui::Rect::NOTHING,
            egui::Rect::NOTHING,
        );
        render_in(&ctx, WIDE, |ui| {
            pane.ui(
                ui,
                |ui| {
                    list = egui::Panel::left("t_list")
                        .show(ui, |ui| ui.label("list"))
                        .response
                        .rect;
                    page = egui::CentralPanel::default()
                        .show(ui, |ui| ui.label("page"))
                        .response
                        .rect;
                },
                |ui| column = ui.max_rect(),
            )
        });
        assert!(list.right() <= page.left(), "{list:?} {page:?}");
        assert!(page.right() <= column.left(), "{page:?} {column:?}");
    }

    #[test]
    fn opening_while_the_column_shows_leaves_nothing_to_pop_up() {
        let mut pane = DetailPane::new("t");
        pane.open();
        drawn(&mut pane, WIDE);
        assert!(!pane.is_open());
        pane.set_column(false);
        assert_eq!(drawn(&mut pane, WIDE), (true, false));
        assert_eq!(pane.shown, Shown::Not);
    }

    #[test]
    fn without_the_column_an_opened_detail_is_a_card() {
        let mut pane = DetailPane::new("t").with_column(false);
        assert_eq!(drawn(&mut pane, WIDE), (true, false));
        pane.open();
        assert_eq!(drawn(&mut pane, WIDE), (true, true));
        assert_eq!(pane.shown, Shown::Card);
        pane.close();
        assert_eq!(drawn(&mut pane, WIDE), (true, false));
    }

    #[test]
    fn the_card_is_as_tall_as_its_detail() {
        let ctx = egui::Context::default();
        let mut pane = DetailPane::new("t").with_column(false);
        pane.open();
        render_in(&ctx, WIDE, |ui| {
            pane.ui(
                ui,
                |_| {},
                |ui| {
                    ui.label("one line");
                },
            )
        });
        let card = ctx
            .memory(|m| m.area_rect(pane.id.with("card")))
            .expect("the card is drawn");
        assert!(card.height() < WIDE[1] / 4.0, "{card:?}");
        assert!(card.width() <= CARD_MAX_WIDTH + 32.0, "{card:?}");
    }

    #[test]
    fn with_no_room_for_the_column_the_detail_is_a_card() {
        let mut pane = DetailPane::new("t");
        assert_eq!(drawn(&mut pane, NARROW_DESKTOP), (true, false));
        pane.open();
        assert_eq!(drawn(&mut pane, NARROW_DESKTOP), (true, true));
        assert_eq!(pane.shown, Shown::Card);
        assert!(pane.column(), "the column stays wanted, for a wider window");
    }

    #[test]
    fn on_a_phone_the_detail_takes_the_main_views_place() {
        let mut pane = DetailPane::new("t");
        assert_eq!(drawn(&mut pane, PHONE), (true, false));
        pane.open();
        assert_eq!(drawn(&mut pane, PHONE), (false, true));
        assert_eq!(pane.shown, Shown::Screen);
    }

    /// Frames of `pane`, a `toggle_ui` drawn after it at the top left, at `size`; then a
    /// click on the toggle. Whether the toggle was drawn.
    fn click_toggle(pane: &mut DetailPane, size: [f32; 2]) -> bool {
        let ctx = egui::Context::default();
        let frame = |events: Vec<egui::Event>, pane: &mut DetailPane| {
            let mut drawn = None;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size[0], size[1]),
                )),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                pane.ui(ui, |_| {}, |_| {});
                let corner =
                    egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(40.0, 40.0));
                let response = ui
                    .scope_builder(egui::UiBuilder::new().max_rect(corner), toggle_ui)
                    .inner;
                drawn = response.map(|r| r.rect);
            });
            output.textures_delta.clear();
            drawn
        };
        frame(vec![], pane);
        let Some(rect) = frame(vec![], pane) else {
            return false;
        };
        let at = rect.center();
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(at), press(true)], pane);
        frame(vec![press(false)], pane);
        // The pane takes the click on its next frame.
        frame(vec![], pane);
        true
    }

    #[test]
    fn the_toggle_turns_the_column_off_and_on() {
        let mut pane = DetailPane::new("t");
        assert!(click_toggle(&mut pane, WIDE));
        assert!(!pane.column());
        assert!(click_toggle(&mut pane, WIDE));
        assert!(pane.column());
    }

    #[test]
    fn without_room_for_the_column_there_is_no_toggle() {
        for size in [NARROW_DESKTOP, PHONE] {
            let mut pane = DetailPane::new("t");
            assert!(!click_toggle(&mut pane, size), "{size:?}");
            assert!(pane.column());
        }
    }
}
