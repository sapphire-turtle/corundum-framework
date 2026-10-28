//! Drawing the explorer, and gathering what the person did to it: the trail and its
//! buttons, the list's headings, the rows or tiles (only those in view), the menus, the
//! drags, the rename field and the keys.

use super::{
    Dragged, ExplorerEvent, FileExplorer, MenuItem, Renaming, SortBy, Status, View, icon_for,
    names_for,
};
use crate::{COMPACT_WIDTH, icons, tr};

/// A list row's height, on a desktop.
const ROW: f32 = 24.0;
/// A list row's height on a phone's layout: room for a finger, and a second line.
const ROW_COMPACT: f32 = 48.0;
/// A grid tile's width, and its picture's room with the margins.
const TILE: f32 = 112.0;
/// A grid tile's height: the picture, and the name under it.
const TILE_HEIGHT: f32 = TILE + 22.0;
/// Between rows, tiles and columns.
const GAP: f32 = 4.0;
/// The list's size and time columns.
const SIZE_WIDTH: f32 = 80.0;
const TIME_WIDTH: f32 = 128.0;
/// The trail's and the buttons' icons.
const ICON: f32 = 16.0;

/// What the person did, gathered while drawing and acted on once the frame's drawing is
/// done, as the drawing borrows what the acts change.
enum Act {
    /// A click on the entry shown at a place, with Ctrl (⌘) and Shift as they were.
    Click {
        at: usize,
        command: bool,
        shift: bool,
    },
    Open(usize),
    /// A menu opened on the entry at a place: it is picked alone, unless it already was.
    MenuOn(usize),
    Menu {
        id: String,
        names: Vec<String>,
    },
    StartRename(String),
    /// The rename field let go of: by Enter, by Esc, or by a click elsewhere.
    EndRename(End),
    Move {
        names: Vec<String>,
        to: Vec<String>,
    },
    Navigate(Vec<String>),
    Up,
    Refresh,
    Sort(SortBy),
    View(View),
    LoadMore,
    /// A click on no entry: nothing picked.
    Background,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum End {
    Enter,
    Escape,
    Elsewhere,
}

/// The rows or tiles' geometry, for the keys and for revealing the cursor.
#[derive(Clone, Copy)]
struct Layout {
    /// Entries a step up or down moves by: 1 in a list, a row of tiles in a grid.
    columns: usize,
    grid: bool,
}

impl FileExplorer {
    /// Draw the explorer filling `ui`, and return what the person did this frame.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Vec<ExplorerEvent> {
        let compact = ui.available_width() < COMPACT_WIDTH;
        // The background under the entries holds the focus (`entries_ui`).
        let focus = self.focus_id();
        let focused = ui.ctx().memory(|m| m.has_focus(focus));
        if focused {
            ui.ctx().memory_mut(|m| {
                m.set_focus_lock_filter(
                    focus,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                        tab: false,
                    },
                )
            });
        }
        #[cfg(test)]
        self.drawn.borrow_mut().clear();

        let mut acts = Vec::new();
        self.toolbar_ui(ui, compact, &mut acts);
        ui.separator();
        let mut renaming = self.renaming.take();
        let layout = if self.entries.is_empty() {
            self.empty_ui(ui, &mut acts);
            None
        } else if compact || self.view == View::List {
            if !compact {
                self.header_ui(ui, &mut acts);
            }
            Some(self.entries_ui(ui, compact, false, focused, &mut renaming, &mut acts))
        } else {
            Some(self.entries_ui(ui, compact, true, focused, &mut renaming, &mut acts))
        };
        self.renaming = renaming;
        self.drag_preview_ui(ui);

        let mut changed = !acts.is_empty();
        if focused && let Some(layout) = layout {
            changed |= self.keys(ui, layout);
        }
        for act in acts {
            self.act(ui.ctx(), act);
        }
        if changed {
            ui.ctx().request_repaint();
        }
        std::mem::take(&mut self.events)
    }

    /// The id the explorer takes the keyboard's focus with.
    fn focus_id(&self) -> egui::Id {
        self.id.with("focus")
    }

    fn act(&mut self, ctx: &egui::Context, act: Act) {
        let focus_id = self.focus_id();
        let focus = |ctx: &egui::Context| ctx.memory_mut(|m| m.request_focus(focus_id));
        match act {
            Act::Click { at, command, shift } if at < self.order.len() => {
                let shown = self.shown_names();
                self.selection.click(&shown, at, command, shift);
                focus(ctx);
            }
            Act::Open(at) if at < self.order.len() => {
                focus(ctx);
                self.open(at);
            }
            Act::MenuOn(at) if at < self.order.len() => {
                let name = self.shown(at).name.clone();
                if !self.selection.contains(&name) {
                    self.selection.only(name);
                }
            }
            Act::Click { .. } | Act::Open(_) | Act::MenuOn(_) => {}
            Act::Menu { id, names } => self.events.push(ExplorerEvent::Menu { id, names }),
            Act::StartRename(name) => self.start_rename(name),
            Act::EndRename(end) => {
                match end {
                    End::Escape => self.renaming = None,
                    End::Enter => self.finish_rename(true),
                    End::Elsewhere => self.finish_rename(false),
                }
                if self.renaming.is_none() && end != End::Elsewhere {
                    focus(ctx);
                }
            }
            Act::Move { names, to } => self.events.push(ExplorerEvent::Move { names, to }),
            Act::Navigate(location) => self.navigate(location),
            Act::Up => self.up(),
            Act::Refresh => self.refresh(),
            Act::Sort(by) => {
                let mut sort = self.sort;
                if sort.by == by {
                    sort.ascending = !sort.ascending;
                } else {
                    sort.by = by;
                    sort.ascending = true;
                }
                self.set_sort(sort);
            }
            Act::View(view) => self.view = view,
            Act::LoadMore => {
                self.status = Status::Loading;
                self.events.push(ExplorerEvent::LoadMore);
            }
            Act::Background => {
                self.selection.none();
                focus(ctx);
            }
        }
    }

    /// The way up, Refresh, the trail of folders, and the view's buttons.
    fn toolbar_ui(&self, ui: &mut egui::Ui, compact: bool, acts: &mut Vec<Act>) {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !compact {
                    for (view, icon, label) in [
                        (View::Grid, icons::layout_grid(), "corundum-explorer-grid"),
                        (View::List, icons::list(), "corundum-explorer-list"),
                    ] {
                        let button = icons::button(icon, ICON).selected(self.view == view);
                        if ui.add(button).on_hover_text(tr!(ui, label)).clicked() {
                            acts.push(Act::View(view));
                        }
                    }
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let up = ui
                        .add_enabled(
                            !self.location.is_empty(),
                            icons::button(icons::arrow_up(), ICON),
                        )
                        .on_hover_text(tr!(ui, "corundum-explorer-up"));
                    if up.clicked() {
                        acts.push(Act::Up);
                    }
                    if let Some((_, above)) = self.location.split_last() {
                        self.drop_target(ui, &up, above.to_vec(), None, acts);
                    }
                    let refresh = ui
                        .add(icons::button(icons::refresh_cw(), ICON))
                        .on_hover_text(tr!(ui, "corundum-explorer-refresh"));
                    if refresh.clicked() {
                        acts.push(Act::Refresh);
                    }
                    self.trail_ui(ui, acts);
                });
            });
        });
    }

    /// The root and the folders down to the one shown; any but the last goes back up to
    /// it, and takes a drop.
    fn trail_ui(&self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        egui::ScrollArea::horizontal()
            .id_salt(self.id.with("trail"))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .stick_to_right(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let names = std::iter::once(&self.root).chain(&self.location);
                    let last = self.location.len();
                    for (depth, name) in names.enumerate() {
                        if depth > 0 {
                            ui.add(icons::image_as(
                                ui,
                                icons::chevron_right(),
                                ICON,
                                crate::theme::Tone::Muted,
                                crate::theme::Swatch::Neutral,
                            ));
                        }
                        let text = if depth == last {
                            egui::RichText::new(name).strong()
                        } else {
                            egui::RichText::new(name)
                        };
                        let crumb = ui.add(egui::Button::new(text).frame(false));
                        if depth < last {
                            let location = self.location[..depth].to_vec();
                            if crumb.clicked() {
                                acts.push(Act::Navigate(location.clone()));
                            }
                            self.drop_target(ui, &crumb, location, None, acts);
                        }
                    }
                });
            });
    }

    /// Let `response` take entries dropped on it, to move them into `to`, outlining it
    /// while they are held over it — unless they are of another explorer or folder, or
    /// carry `folder`, the folder it is (which cannot go into itself).
    fn drop_target(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        to: Vec<String>,
        folder: Option<&str>,
        acts: &mut Vec<Act>,
    ) {
        if !self.movable {
            return;
        }
        let ours = |d: &Dragged| {
            d.explorer == self.id
                && d.from == self.location
                && folder.is_none_or(|f| !d.names.iter().any(|n| n == f))
        };
        if response
            .dnd_hover_payload::<Dragged>()
            .is_some_and(|d| ours(&d))
        {
            ui.painter().rect_stroke(
                response.rect,
                4.0,
                egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(dragged) = response.dnd_release_payload::<Dragged>()
            && ours(&dragged)
        {
            acts.push(Act::Move {
                names: dragged.names.clone(),
                to,
            });
        }
    }

    /// The list's headings: a click sorts by the column, again the other way round.
    fn header_ui(&self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::hover());
        let [_, name, size, time] = columns(rect);
        for (by, cell, label, right) in [
            (SortBy::Name, name, "corundum-explorer-name", false),
            (SortBy::Size, size, "corundum-explorer-size", true),
            (SortBy::Modified, time, "corundum-explorer-modified", false),
        ] {
            let response =
                ui.interact(cell, self.id.with(("heading", label)), egui::Sense::click());
            if response.clicked() {
                acts.push(Act::Sort(by));
            }
            let colour = if response.hovered() {
                ui.visuals().strong_text_color()
            } else {
                ui.visuals().weak_text_color()
            };
            let galley = galley(ui, tr!(ui, label), cell.width() - ICON, colour);
            let x = if right {
                cell.right() - galley.size().x
            } else {
                cell.left()
            };
            let text_pos = egui::pos2(x, cell.center().y - galley.size().y / 2.0);
            let text_width = galley.size().x;
            ui.painter().galley(text_pos, galley, colour);
            if self.sort.by == by {
                let icon = if self.sort.ascending {
                    icons::chevron_up()
                } else {
                    icons::chevron_down()
                };
                let x = if right {
                    x - ICON
                } else {
                    x + text_width + 2.0
                };
                let at = egui::Rect::from_min_size(
                    egui::pos2(x, cell.center().y - ICON / 2.0),
                    egui::vec2(ICON, ICON),
                );
                icons::paint(ui, at, icon, colour);
            }
        }
    }

    /// What shows with no entries: that the folder is loading, could not be listed (with
    /// Retry), or is empty.
    fn empty_ui(&self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| match &self.status {
            Status::Loading => {
                ui.add(egui::Spinner::new());
                ui.weak(tr!(ui, "corundum-explorer-loading"));
            }
            Status::Failed(error) => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    tr!(ui, "corundum-explorer-failed", error = error.as_str()),
                );
                if ui.button(tr!(ui, "corundum-explorer-retry")).clicked() {
                    acts.push(Act::Refresh);
                }
            }
            Status::Ready => {
                ui.weak(tr!(ui, "corundum-explorer-empty"));
            }
        });
    }

    /// The entries as rows (or tiles, `grid`), drawing those in view alone, then a last
    /// row asking for more while the app has more.
    fn entries_ui(
        &mut self,
        ui: &mut egui::Ui,
        compact: bool,
        grid: bool,
        focused: bool,
        renaming: &mut Option<Renaming>,
        acts: &mut Vec<Act>,
    ) -> Layout {
        let count = self.order.len();
        let footer = self.more || self.status != Status::Ready;
        let reveal = std::mem::take(&mut self.reveal);
        let mut layout = Layout { columns: 1, grid };
        egui::ScrollArea::vertical()
            .id_salt(self.id.with("entries"))
            .auto_shrink(false)
            .show_viewport(ui, |ui, viewport| {
                let width = ui.available_width();
                let (columns, cell, pitch) = if grid {
                    let columns = (((width + GAP) / (TILE + GAP)).floor() as usize).max(1);
                    (columns, egui::vec2(TILE, TILE_HEIGHT), TILE_HEIGHT + GAP)
                } else {
                    let height = if compact { ROW_COMPACT } else { ROW };
                    (1, egui::vec2(width, height), height + GAP)
                };
                layout.columns = columns;
                let rows = count.div_ceil(columns) + usize::from(footer);
                ui.set_height((pitch * rows as f32 - GAP).max(0.0));
                let top = ui.max_rect().min;
                let at = |n: usize| {
                    let (row, column) = (n / columns, n % columns);
                    egui::Rect::from_min_size(
                        top + egui::vec2(column as f32 * (TILE + GAP), row as f32 * pitch),
                        cell,
                    )
                };

                // Under the entries, so they take the clicks they are on; it holds the
                // keyboard's focus for them, as egui lets go of a focus no widget holds.
                let background = ui.interact(
                    ui.max_rect().union(viewport.translate(top.to_vec2())),
                    self.focus_id(),
                    egui::Sense::click(),
                );
                if background.clicked() {
                    acts.push(Act::Background);
                }

                let first = (viewport.min.y / pitch).floor().max(0.0) as usize;
                let last = ((viewport.max.y / pitch).ceil() as usize + 1).min(rows);
                let cursor = self.selection.cursor_at(&self.shown_names());
                for row in first..last {
                    for n in row * columns..((row + 1) * columns).min(count) {
                        let rect = at(n);
                        let response = self.entry_response(ui, rect, n, compact, acts);
                        let is_cursor = focused && cursor == Some(n);
                        if grid {
                            self.tile_ui(ui, rect, n, &response, is_cursor, renaming, acts);
                        } else {
                            self.row_ui(ui, rect, n, compact, &response, is_cursor, renaming, acts);
                        }
                    }
                }
                if footer && last == rows {
                    let rect = egui::Rect::from_min_size(
                        top + egui::vec2(0.0, (rows - 1) as f32 * pitch),
                        egui::vec2(width, ROW),
                    );
                    if self.status == Status::Loading {
                        ui.put(rect, egui::Spinner::new());
                    } else if self.more {
                        acts.push(Act::LoadMore);
                    }
                }
                if reveal && let Some(n) = cursor {
                    ui.scroll_to_rect(at(n), None);
                }
            });
        layout
    }

    /// The response of the entry shown at `n`, drawn in `rect`: a click picks it (a tap
    /// opens it, on a phone), a double click opens it, a right click or a long press opens
    /// its menu; dragged, it carries what is picked, and a folder takes a drop.
    fn entry_response(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        n: usize,
        compact: bool,
        acts: &mut Vec<Act>,
    ) -> egui::Response {
        let entry = self.shown(n);
        // A phone's drag scrolls the list.
        let drags = self.movable && !compact;
        let sense = if drags {
            egui::Sense::click_and_drag()
        } else {
            egui::Sense::click()
        };
        let response = ui.interact(rect, self.id.with(("entry", &entry.name)), sense);
        #[cfg(test)]
        self.drawn.borrow_mut().push((entry.name.clone(), rect));
        if response.clicked() {
            if compact {
                acts.push(Act::Open(n));
            } else {
                let modifiers = ui.input(|i| i.modifiers);
                acts.push(Act::Click {
                    at: n,
                    command: modifiers.command,
                    shift: modifiers.shift,
                });
            }
        }
        if response.double_clicked() && !compact {
            acts.push(Act::Open(n));
        }
        if response.secondary_clicked() {
            acts.push(Act::MenuOn(n));
        }
        if drags {
            if response.drag_started() {
                let names = names_for(&self.selection.names, &self.shown_names(), &entry.name);
                if !self.selection.contains(&entry.name) {
                    acts.push(Act::Click {
                        at: n,
                        command: false,
                        shift: false,
                    });
                }
                response.dnd_set_drag_payload(Dragged {
                    explorer: self.id,
                    from: self.location.clone(),
                    names,
                });
            }
            if entry.is_folder() {
                let mut to = self.location.clone();
                to.push(entry.name.clone());
                self.drop_target(ui, &response, to, Some(&entry.name), acts);
            }
        }
        response.context_menu(|ui| self.menu_ui(ui, n, acts));
        response
    }

    /// The menu on the entry shown at `n` (and the others picked with it): Open and
    /// Rename for one, then the app's items.
    fn menu_ui(&self, ui: &mut egui::Ui, n: usize, acts: &mut Vec<Act>) {
        let name = &self.shown(n).name;
        let names = names_for(&self.selection.names, &self.shown_names(), name);
        if names.len() == 1 {
            if ui.button(tr!(ui, "corundum-explorer-open")).clicked() {
                acts.push(Act::Open(n));
            }
            if self.renamable
                && ui
                    .add(egui::Button::image_and_text(
                        icons::image(ui, icons::pencil_line(), ICON),
                        tr!(ui, "corundum-explorer-rename"),
                    ))
                    .clicked()
            {
                acts.push(Act::StartRename(name.clone()));
            }
        }
        let Some(menu) = &self.menu else {
            return;
        };
        let entries: Vec<_> = self
            .entries
            .iter()
            .filter(|e| names.contains(&e.name))
            .collect();
        let items = menu(ui.ctx(), &entries);
        if items.is_empty() {
            return;
        }
        if names.len() == 1 {
            ui.separator();
        }
        for MenuItem { id, label, icon } in items {
            let clicked = match icon {
                Some(icon) => ui
                    .add(egui::Button::image_and_text(
                        icons::image(ui, icon, ICON),
                        label,
                    ))
                    .clicked(),
                None => ui.button(label).clicked(),
            };
            if clicked {
                acts.push(Act::Menu {
                    id,
                    names: names.clone(),
                });
            }
        }
    }

    /// The entry shown at `n` as a list row: its icon, its name, and its size and time in
    /// columns — or on a phone, under its name.
    #[allow(clippy::too_many_arguments)]
    fn row_ui(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        n: usize,
        compact: bool,
        response: &egui::Response,
        is_cursor: bool,
        renaming: &mut Option<Renaming>,
        acts: &mut Vec<Act>,
    ) {
        let entry = self.shown(n);
        let colour = self.paint_backdrop(ui, rect, &entry.name, response, is_cursor);
        let [icon, name, size, time] = columns(rect);
        let icon_size = if compact { 28.0 } else { ICON };
        let icon_cell = icon;
        let icon =
            egui::Rect::from_center_size(icon_cell.center(), egui::vec2(icon_size, icon_size));
        let src = entry.icon.clone().unwrap_or_else(|| icon_for(entry));
        icons::paint(ui, icon, src, colour);

        let size_text = entry
            .size
            .filter(|_| !entry.is_folder())
            .map(super::format_size);
        let time_text = entry.modified.map(|t| (self.time)(t));
        // On a phone the size and the time go under the name, across the row.
        let (name_cell, detail) = if compact {
            let x = name.left()..=rect.right() - 4.0;
            let name = egui::Rect::from_x_y_ranges(x.clone(), rect.top() + 4.0..=rect.center().y);
            let detail = egui::Rect::from_x_y_ranges(x, rect.center().y..=rect.bottom() - 4.0);
            (name, Some(detail))
        } else {
            (name, None)
        };
        if !self.rename_ui(ui, name_cell, &entry.name, renaming, acts) {
            paint_text(ui, name_cell, &entry.name, colour, egui::Align::Min);
        }
        let weak = ui.visuals().weak_text_color();
        match detail {
            Some(detail) => {
                let text: Vec<String> = [size_text, time_text].into_iter().flatten().collect();
                paint_text(ui, detail, &text.join(" · "), weak, egui::Align::Min);
            }
            None => {
                if let Some(text) = size_text {
                    paint_text(ui, size, &text, weak, egui::Align::Max);
                }
                if let Some(text) = time_text {
                    paint_text(ui, time, &text, weak, egui::Align::Min);
                }
            }
        }
    }

    /// The entry shown at `n` as a grid tile: its thumbnail (else its icon), its name
    /// under it.
    #[allow(clippy::too_many_arguments)]
    fn tile_ui(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        n: usize,
        response: &egui::Response,
        is_cursor: bool,
        renaming: &mut Option<Renaming>,
        acts: &mut Vec<Act>,
    ) {
        let entry = self.shown(n);
        let colour = self.paint_backdrop(ui, rect, &entry.name, response, is_cursor);
        let picture = egui::Rect::from_min_size(rect.min, egui::vec2(TILE, TILE)).shrink(8.0);
        let drawn = entry
            .thumbnail
            .clone()
            .is_some_and(|src| paint_thumbnail(ui, picture, src));
        if !drawn {
            let src = entry.icon.clone().unwrap_or_else(|| icon_for(entry));
            let icon = egui::Rect::from_center_size(picture.center(), egui::vec2(48.0, 48.0));
            icons::paint(ui, icon, src, colour);
        }
        let label = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 4.0, rect.top() + TILE - 4.0),
            rect.right_bottom() - egui::vec2(4.0, 2.0),
        );
        if !self.rename_ui(ui, label, &entry.name, renaming, acts) {
            paint_text(ui, label, &entry.name, colour, egui::Align::Center);
        }
    }

    /// An entry's background — picked, hovered, under the cursor — and the colour its
    /// text and icon are drawn in over it.
    fn paint_backdrop(
        &self,
        ui: &egui::Ui,
        rect: egui::Rect,
        name: &str,
        response: &egui::Response,
        is_cursor: bool,
    ) -> egui::Color32 {
        let visuals = ui.visuals();
        let picked = self.selection.contains(name);
        if picked {
            ui.painter()
                .rect_filled(rect, 4.0, visuals.selection.bg_fill);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
        }
        if is_cursor {
            ui.painter().rect_stroke(
                rect,
                4.0,
                egui::Stroke::new(1.0, visuals.selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        }
        if picked {
            visuals.strong_text_color()
        } else {
            visuals.text_color()
        }
    }

    /// The rename field in `cell`, if the entry `name` is being renamed: `true` when it is
    /// drawn. Opened, it is focused with the name's stem picked, so typing replaces the
    /// name and keeps its extension.
    fn rename_ui(
        &self,
        ui: &mut egui::Ui,
        cell: egui::Rect,
        name: &str,
        renaming: &mut Option<Renaming>,
        acts: &mut Vec<Act>,
    ) -> bool {
        let Some(r) = renaming.as_mut().filter(|r| r.name == name) else {
            return false;
        };
        let id = self.id.with("rename");
        let allowed = r.text.trim() == r.name || self.name_allowed(r.text.trim());
        let mut edit = egui::TextEdit::singleline(&mut r.text)
            .id(id)
            .desired_width(cell.width());
        if !allowed {
            edit = edit.text_color(ui.visuals().error_fg_color);
        }
        let cell = egui::Rect::from_center_size(
            cell.center(),
            egui::vec2(cell.width(), cell.height().max(ROW)),
        );
        let response = ui.put(cell, edit);
        if !r.started {
            r.started = true;
            response.request_focus();
            let stem = match r.name.rsplit_once('.') {
                Some((stem, _)) if !stem.is_empty() => stem.chars().count(),
                _ => r.name.chars().count(),
            };
            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(0),
                    egui::text::CCursor::new(stem),
                )));
            state.store(ui.ctx(), id);
        } else if response.lost_focus() {
            let end = ui.input(|i| {
                if i.key_pressed(egui::Key::Escape) {
                    End::Escape
                } else if i.key_pressed(egui::Key::Enter) {
                    End::Enter
                } else {
                    End::Elsewhere
                }
            });
            acts.push(Act::EndRename(end));
        }
        true
    }

    /// What is dragged, by the pointer: its name, or how many.
    fn drag_preview_ui(&self, ui: &egui::Ui) {
        let Some(dragged) = egui::DragAndDrop::payload::<Dragged>(ui.ctx()) else {
            return;
        };
        if dragged.explorer != self.id {
            return;
        }
        let Some(pointer) = ui.ctx().pointer_interact_pos() else {
            return;
        };
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let text = match dragged.names.as_slice() {
            [one] => one.clone(),
            many => tr!(ui, "corundum-explorer-items", count = many.len()),
        };
        egui::Area::new(self.id.with("dragged"))
            .order(egui::Order::Tooltip)
            .fixed_pos(pointer + egui::vec2(12.0, 12.0))
            .interactable(false)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(text));
            });
    }

    /// The keys, while the explorer is focused and nothing is being renamed: `true` when
    /// one did something.
    fn keys(&mut self, ui: &egui::Ui, layout: Layout) -> bool {
        if self.renaming.is_some() {
            return false;
        }
        let presses: Vec<_> = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
                .collect()
        });
        let columns = layout.columns as isize;
        let mut did = false;
        for (key, modifiers) in presses {
            use egui::Key;
            let shown = self.shown_names();
            let shift = modifiers.shift;
            let mut moved = true;
            match key {
                Key::ArrowUp if modifiers.alt => {
                    self.up();
                    moved = false;
                }
                Key::ArrowDown => self.selection.step(&shown, columns, shift),
                Key::ArrowUp => self.selection.step(&shown, -columns, shift),
                Key::ArrowRight if layout.grid => self.selection.step(&shown, 1, shift),
                Key::ArrowLeft if layout.grid => self.selection.step(&shown, -1, shift),
                Key::PageDown => self.selection.step(&shown, 10 * columns, shift),
                Key::PageUp => self.selection.step(&shown, -10 * columns, shift),
                Key::Home => self.selection.move_to(&shown, 0, shift),
                Key::End => self
                    .selection
                    .move_to(&shown, shown.len().saturating_sub(1), shift),
                Key::A if modifiers.command => {
                    self.selection.all(&shown);
                    moved = false;
                }
                Key::Escape => {
                    self.selection.none();
                    moved = false;
                }
                Key::Enter => {
                    if let Some(at) = self.selection.cursor_at(&shown) {
                        self.open(at);
                    }
                    moved = false;
                }
                Key::Backspace => {
                    self.up();
                    moved = false;
                }
                Key::F2 => {
                    if let Some(name) = self.selection.cursor.clone() {
                        self.start_rename(name);
                    }
                    moved = false;
                }
                _ => continue,
            }
            self.reveal |= moved;
            did = true;
        }
        did
    }
}

/// A row's icon, name, size and time columns, left to right; the name takes what the
/// others leave.
pub(super) fn columns(rect: egui::Rect) -> [egui::Rect; 4] {
    // A phone's taller row has a bigger icon.
    let icon_width = if rect.height() > ROW {
        rect.height()
    } else {
        ICON + 12.0
    };
    let icon = egui::Rect::from_min_size(rect.min, egui::vec2(icon_width, rect.height()));
    let time = egui::Rect::from_min_max(
        egui::pos2(rect.right() - TIME_WIDTH, rect.top()),
        rect.right_bottom(),
    );
    let size = egui::Rect::from_min_max(
        egui::pos2(time.left() - SIZE_WIDTH - 2.0 * GAP, rect.top()),
        egui::pos2(time.left() - 2.0 * GAP, rect.bottom()),
    );
    let name = egui::Rect::from_min_max(
        egui::pos2(icon.right(), rect.top()),
        egui::pos2((size.left() - 2.0 * GAP).max(icon.right()), rect.bottom()),
    );
    [icon, name, size, time]
}

/// `text` laid out on one line no wider than `width`, cut short with `…` if longer.
fn galley(
    ui: &egui::Ui,
    text: String,
    width: f32,
    colour: egui::Color32,
) -> std::sync::Arc<egui::Galley> {
    egui::WidgetText::from(egui::RichText::new(text).color(colour)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        width.max(0.0),
        egui::TextStyle::Body,
    )
}

/// `text` on one line in `cell`, centred down it and aligned across it by `align`.
fn paint_text(
    ui: &egui::Ui,
    cell: egui::Rect,
    text: &str,
    colour: egui::Color32,
    align: egui::Align,
) {
    let galley = galley(ui, text.to_owned(), cell.width(), colour);
    let x = match align {
        egui::Align::Min => cell.left(),
        egui::Align::Center => cell.center().x - galley.size().x / 2.0,
        egui::Align::Max => cell.right() - galley.size().x,
    };
    let pos = egui::pos2(x, cell.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, colour);
}

/// `src` fitted into `rect`, keeping its shape: `false` while it loads or if it cannot.
fn paint_thumbnail(ui: &egui::Ui, rect: egui::Rect, src: egui::ImageSource<'static>) -> bool {
    let image = egui::Image::new(src)
        .max_size(rect.size())
        .maintain_aspect_ratio(true);
    let Ok(poll) = image.load_for_size(ui.ctx(), rect.size()) else {
        return false;
    };
    let Some(size) = poll.size() else {
        return false;
    };
    let fitted = image.calc_size(rect.size(), Some(size));
    image.paint_at(ui, egui::Rect::from_center_size(rect.center(), fitted));
    true
}
