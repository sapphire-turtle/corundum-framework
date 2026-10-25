//! [`Shell`]: an app's window frame — its screens and actions on a rail, and at the rail's
//! foot the screens that belong there (Settings) and a slot of the app's own (a status dot).
//!
//! The screens and the actions are in the app's order, or the person's ([`RailOrder`], set
//! on the settings page [`RailPage`](crate::settings::RailPage)). When some are left off
//! the rail — the window has no room for them, or the person hid them — the footer's place
//! is taken by a "More" button: its menu holds the footer first, then those left off.
//!
//! Wide, the rail runs down the left (as Obsidian's does); narrower than
//! [`COMPACT_WIDTH`], it becomes a bar along the bottom, as on a phone. The width decides,
//! not the platform, so a desktop window made narrow shows the phone layout.

mod order;

pub use order::RailOrder;
pub(crate) use order::{RailEntry, move_to, published, request};

/// Below this width the layout is a phone's: the rail becomes a bottom bar, and screens
/// with a list and a page show one at a time.
pub const COMPACT_WIDTH: f32 = 600.0;

/// The rail's width, wide.
const RAIL_WIDTH: f32 = 48.0;

/// The space above and below the status bar's row.
const STATUS_MARGIN: f32 = 3.0;

/// The space at the rail's ends.
const RAIL_END: f32 = 4.0;

/// One of the app's screens on the rail.
pub struct Destination<D> {
    /// What the app matches on to draw it.
    pub id: D,
    /// A Lucide icon (see [`icons`](crate::icons)), stroked in white so it takes the text
    /// colour: the rail shows only this.
    pub icon: egui::ImageSource<'static>,
    /// The id of the screen's name in the app's translations, looked up each frame and shown
    /// when the icon is hovered.
    pub label: &'static str,
}

impl<D> Destination<D> {
    /// A screen named by the message `label`, drawn as `icon`.
    pub fn new(id: D, icon: egui::ImageSource<'static>, label: &'static str) -> Destination<D> {
        Destination { id, icon, label }
    }
}

/// A button of the app's on the rail that does something — "new-entry" — rather than
/// open a screen. [`Shell::take_action`] says which one was clicked.
pub struct Action {
    /// What [`Shell::take_action`] returns when it is clicked.
    pub id: &'static str,
    /// A Lucide icon, as for a [`Destination`].
    pub icon: egui::ImageSource<'static>,
    /// The id of what it does in the app's translations, looked up each frame and shown
    /// when the icon is hovered.
    pub label: &'static str,
}

impl Action {
    /// A button named by the message `label`, drawn as `icon`, reported as `id`.
    pub fn new(id: &'static str, icon: egui::ImageSource<'static>, label: &'static str) -> Action {
        Action { id, icon, label }
    }
}

/// A screen or an action on the rail, for [`Shell::from_items`]: the two mix in any order,
/// so the action used most — a new entry — can come first.
pub enum RailItem<D> {
    Screen(Destination<D>),
    Action(Action),
}

impl<D> From<Destination<D>> for RailItem<D> {
    fn from(d: Destination<D>) -> RailItem<D> {
        RailItem::Screen(d)
    }
}

impl<D> From<Action> for RailItem<D> {
    fn from(a: Action) -> RailItem<D> {
        RailItem::Action(a)
    }
}

/// The window frame. Hold one as the app's root; call [`ui`](Self::ui) every frame.
///
/// Labels are message ids: each frame the hover text is looked up in the language
/// installed (see [`i18n`](crate::i18n)), so a language switch shows on the next frame.
///
/// The shell keeps clear of the system's bars and notches (egui's
/// [safe area](egui::InputState::safe_area_insets), which iOS fills): the rail, the bar's
/// buttons and the screens stay inside it, and the bottom bar's fill runs on under the home
/// indicator, as a phone's tab bar does. Give it the window's whole root `ui`; the app adds
/// no margin of its own. If the app does keep `ui` clear of them itself, the shell sees
/// that and adds nothing, so the margin is never doubled. On Android,
/// `window::run_android` keeps the app's `ui` clear of the bars and the
/// keyboard already.
///
/// ```no_run
/// # use corundum_framework_gui::{Action, Destination, Shell, icons};
/// # fn example(ui: &mut egui::Ui) {
/// #[derive(Clone, Copy, PartialEq)]
/// enum Screen { Gallery, Albums, Settings }
///
/// let mut shell = Shell::from_items(vec![
///     Action::new("import", icons::plus(), "import").into(),
///     // The app's own Lucide SVGs: `egui::include_image!("../assets/icons/images.svg")`.
///     Destination::new(Screen::Gallery, icons::folder(), "gallery").into(),
///     Destination::new(Screen::Albums, icons::folder(), "albums").into(),
/// ])
/// .with_footer(vec![Destination::new(Screen::Settings, icons::settings(), "settings")]);
/// shell.ui(ui, |ui, screen| match screen {
///     Screen::Gallery => { egui::CentralPanel::default().show(ui, |ui| ui.label("…")); }
///     Screen::Albums => { egui::CentralPanel::default().show(ui, |ui| ui.label("…")); }
///     Screen::Settings => { egui::CentralPanel::default().show(ui, |ui| ui.label("…")); }
/// });
/// if shell.take_action() == Some("import") {
///     // …
/// }
/// # }
/// ```
pub struct Shell<D> {
    destinations: Vec<Destination<D>>,
    actions: Vec<Action>,
    footer: Vec<Destination<D>>,
    /// The screens and the actions in the app's order.
    defaults: Vec<Item>,
    /// The screens and the actions in the order the rail shows them, and whether each is
    /// shown there or left to the menu.
    slots: Vec<Slot>,
    /// The slots on the rail or the bar this frame; the rest are in the menu.
    on_rail: Vec<usize>,
    /// Whether the footer is in the menu this frame, the More button in its place.
    footer_in_menu: bool,
    /// The action clicked and not yet taken.
    clicked: Option<&'static str>,
    /// Where each action and footer destination was last drawn, for the tests to click.
    #[cfg(test)]
    action_rects: Vec<egui::Rect>,
    #[cfg(test)]
    footer_rects: Vec<egui::Rect>,
    /// The rects of the slots on the rail or the bar, in order, for the tests.
    #[cfg(test)]
    cell_rects: Vec<egui::Rect>,
    /// The More button, and its menu's rows, for the tests.
    #[cfg(test)]
    more_rect: egui::Rect,
    #[cfg(test)]
    menu_rects: Vec<egui::Rect>,
    open: Option<D>,
    /// How big the foot was last frame: the rail or the bar keeps room for it.
    foot_size: egui::Vec2,
    /// Whether the SVG loader, the theme and the language have been put in place.
    loaders: bool,
}

/// A screen or an action, by its index in the shell's.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Item {
    Screen(usize),
    Action(usize),
}

/// An item in its place on the rail, and whether the rail shows it.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Slot {
    item: Item,
    shown: bool,
}

/// A row of the More menu.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Row {
    Footer(usize),
    Slot(usize),
}

impl<D: Copy + PartialEq> Shell<D> {
    /// A frame with `destinations` on the rail, the first one open.
    pub fn new(destinations: Vec<Destination<D>>) -> Shell<D> {
        Shell::from_items(destinations.into_iter().map(RailItem::from).collect())
    }

    /// A frame with `items` — screens and actions — on the rail in this order, the first
    /// screen open.
    pub fn from_items(items: Vec<RailItem<D>>) -> Shell<D> {
        let mut shell = Shell {
            destinations: Vec::new(),
            actions: Vec::new(),
            footer: Vec::new(),
            defaults: Vec::new(),
            slots: Vec::new(),
            on_rail: Vec::new(),
            footer_in_menu: false,
            clicked: None,
            #[cfg(test)]
            action_rects: Vec::new(),
            #[cfg(test)]
            footer_rects: Vec::new(),
            #[cfg(test)]
            cell_rects: Vec::new(),
            #[cfg(test)]
            more_rect: egui::Rect::NOTHING,
            #[cfg(test)]
            menu_rects: Vec::new(),
            open: None,
            foot_size: egui::Vec2::ZERO,
            loaders: false,
        };
        for item in items {
            shell.push(item);
        }
        shell.open = shell.destinations.first().map(|d| d.id);
        shell.slots = shell.default_slots();
        shell
    }

    fn push(&mut self, item: RailItem<D>) {
        match item {
            RailItem::Screen(d) => {
                self.defaults.push(Item::Screen(self.destinations.len()));
                self.destinations.push(d);
            }
            RailItem::Action(a) => {
                self.defaults.push(Item::Action(self.actions.len()));
                self.actions.push(a);
            }
        }
    }

    /// Put `actions` on the rail after the items already there. To put one first, give the
    /// items to [`from_items`](Self::from_items) instead.
    pub fn with_actions(mut self, actions: Vec<Action>) -> Shell<D> {
        let order = self.rail_order();
        for action in actions {
            self.push(action.into());
        }
        self.set_rail_order(&order);
        self
    }

    /// Put `footer` at the rail's foot (at the bottom bar's end): where an app puts
    /// Settings. With no other destinations, the first of these is open. When some items
    /// are left off the rail, the footer moves into the More menu, which takes its place.
    pub fn with_footer(mut self, footer: Vec<Destination<D>>) -> Shell<D> {
        self.footer = footer;
        if self.open.is_none() {
            self.open = self.footer.first().map(|d| d.id);
        }
        self
    }

    /// Lay the rail out in `order`, as [`set_rail_order`](Self::set_rail_order).
    pub fn with_rail_order(mut self, order: &RailOrder) -> Shell<D> {
        self.set_rail_order(order);
        self
    }

    /// The order the rail's items are in, and which of them it leaves to the menu.
    pub fn rail_order(&self) -> RailOrder {
        RailOrder {
            order: self.slots.iter().map(|s| self.key(s.item).into()).collect(),
            hidden: self
                .slots
                .iter()
                .filter(|s| !s.shown)
                .map(|s| self.key(s.item).into())
                .collect(),
        }
    }

    /// Lay the rail out in `order`. With the `serde` feature and a
    /// [store](crate::store) attached, the order saved there replaces this on the first
    /// frame.
    pub fn set_rail_order(&mut self, order: &RailOrder) {
        let place = |key: &str| {
            order
                .order
                .iter()
                .position(|k| k == key)
                .unwrap_or(usize::MAX)
        };
        let mut slots = self.default_slots();
        // Stable: items of one name, and those the order does not name, keep the app's order.
        slots.sort_by_key(|s| place(self.key(s.item)));
        for slot in &mut slots {
            slot.shown = !order.hidden.iter().any(|k| k == self.key(slot.item));
        }
        self.slots = slots;
    }

    fn default_slots(&self) -> Vec<Slot> {
        self.defaults
            .iter()
            .map(|&item| Slot { item, shown: true })
            .collect()
    }

    /// The name `item` goes by in a [`RailOrder`].
    fn key(&self, item: Item) -> &'static str {
        match item {
            Item::Screen(i) => self.destinations[i].label,
            Item::Action(i) => self.actions[i].id,
        }
    }

    fn icon_and_label(&self, item: Item) -> (egui::ImageSource<'static>, &'static str) {
        match item {
            Item::Screen(i) => (
                self.destinations[i].icon.clone(),
                self.destinations[i].label,
            ),
            Item::Action(i) => (self.actions[i].icon.clone(), self.actions[i].label),
        }
    }

    /// Take the order the [rail's page](crate::settings::RailPage) set, and keep it in the
    /// store; then list the items for the page.
    fn sync_order(&mut self, ctx: &egui::Context) {
        if let Some(order) = order::take_request(ctx) {
            self.set_rail_order(&order);
            #[cfg(feature = "serde")]
            crate::store::update(ctx, |state| state.rail = Some(self.rail_order()));
        }
        let entries = self
            .slots
            .iter()
            .map(|s| {
                let (icon, label) = self.icon_and_label(s.item);
                RailEntry {
                    key: self.key(s.item),
                    icon,
                    label,
                    shown: s.shown,
                }
            })
            .collect();
        order::publish(ctx, entries);
    }

    /// The [`Action`] clicked in the last [`ui`](Self::ui), once: call it after `ui`
    /// every frame and do what it names.
    pub fn take_action(&mut self) -> Option<&'static str> {
        self.clicked.take()
    }

    /// The destination that is open; `None` only when there are none.
    pub fn current(&self) -> Option<D> {
        self.open
    }

    /// Open the destination `id`.
    pub fn open(&mut self, id: D) {
        self.open = Some(id);
    }

    /// Render into the window's root `ui`. `content` draws the open destination into what
    /// the frame leaves of `ui`, as it would at the root: its own panels, then a
    /// `CentralPanel`. The first frame installs the default [`theme`](crate::theme) and
    /// [`language`](crate::i18n) if the app has not installed them, and takes the rail's
    /// order from the [store](crate::store) if one is attached.
    pub fn ui(&mut self, ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui, D)) {
        self.ui_with_foot(ui, |_| None, content);
    }

    /// As [`ui`](Self::ui), with `foot` drawn beside the footer — a status dot, say. A
    /// destination `foot` returns is opened, and drawn this frame.
    pub fn ui_with_foot(
        &mut self,
        ui: &mut egui::Ui,
        foot: impl FnOnce(&mut egui::Ui) -> Option<D>,
        content: impl FnOnce(&mut egui::Ui, D),
    ) {
        self.frame_ui(ui, foot, None::<fn(&mut egui::Ui)>, content);
    }

    /// As [`ui_with_foot`](Self::ui_with_foot), with a status bar along the window's
    /// bottom when wide. `status` fills it from the left — a log's last line, the services'
    /// state, the track playing; the bar's right end holds `foot` (there rather than at the
    /// rail's foot), then the [detail pane](crate::detail)'s toggle when the open screen has
    /// one. Narrow, as on a phone, there is no status bar: `foot` stays on the bottom bar,
    /// and `status` is not called.
    pub fn ui_with_status(
        &mut self,
        ui: &mut egui::Ui,
        foot: impl FnOnce(&mut egui::Ui) -> Option<D>,
        status: impl FnOnce(&mut egui::Ui),
        content: impl FnOnce(&mut egui::Ui, D),
    ) {
        self.frame_ui(ui, foot, Some(status), content);
    }

    fn frame_ui(
        &mut self,
        ui: &mut egui::Ui,
        foot: impl FnOnce(&mut egui::Ui) -> Option<D>,
        status: Option<impl FnOnce(&mut egui::Ui)>,
        content: impl FnOnce(&mut egui::Ui, D),
    ) {
        if !std::mem::replace(&mut self.loaders, true) {
            crate::icons::install_loaders(ui.ctx());
            // The app may have installed its saved theme already; only fill the gap.
            if !crate::theme::is_installed(ui.ctx()) {
                crate::theme::install(ui.ctx(), &crate::theme::ThemeSettings::default());
            }
            // Likewise the language.
            if !crate::i18n::is_installed(ui.ctx()) {
                crate::i18n::install(ui.ctx(), &crate::i18n::LanguageSettings::default());
            }
            #[cfg(feature = "serde")]
            if let Some(order) = crate::store::state(ui.ctx()).rail {
                self.set_rail_order(&order);
            }
        }
        self.sync_order(ui.ctx());
        let safe = unsafe_margin(ui);
        let full = ui.max_rect();
        let compact = full.width() - safe.sum().x < COMPACT_WIDTH;
        // Narrow, the bar's fill runs on under the home indicator, its buttons above it;
        // wide, the rail stops above it too.
        let inner = if compact {
            full - egui::epaint::MarginF32 {
                bottom: 0.0,
                ..safe
            }
        } else {
            full - safe
        };
        // The strips under the status bar and the notch take the panels' colour.
        if safe != egui::epaint::MarginF32::ZERO {
            ui.painter().rect_filled(full, 0.0, ui.visuals().panel_fill);
        }
        if !compact && let Some(status) = status {
            // The bar under the rail and the screen, which keep what is above it.
            let height = ui.spacing().interact_size.y + 2.0 * STATUS_MARGIN;
            let above = inner.with_max_y(inner.bottom() - height);
            let strip = inner.with_min_y(above.bottom());
            ui.scope_builder(egui::UiBuilder::new().max_rect(above), |ui| {
                let opened = egui::Panel::left("shell_rail")
                    .resizable(false)
                    .exact_size(RAIL_WIDTH)
                    .show(ui, |ui| self.rail_ui(ui, |_| None))
                    .inner;
                if let Some(id) = opened {
                    self.open = Some(id);
                }
                if let Some(id) = self.open {
                    content(ui, id);
                }
            });
            // Drawn after the screen, so the toggle knows of the detail pane drawn there.
            if let Some(id) = status_bar_ui(ui, strip, foot, status) {
                self.open = Some(id);
                ui.ctx().request_repaint();
            }
            return;
        }
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            let opened = if compact {
                let mut frame = egui::Frame::side_top_panel(ui.style());
                frame.inner_margin.bottom = frame
                    .inner_margin
                    .bottom
                    .saturating_add(safe.bottom.round().min(127.0) as i8);
                egui::Panel::bottom("shell_bar")
                    .frame(frame)
                    .show(ui, |ui| self.bar_ui(ui, foot))
                    .inner
            } else {
                egui::Panel::left("shell_rail")
                    .resizable(false)
                    .exact_size(RAIL_WIDTH)
                    .show(ui, |ui| self.rail_ui(ui, foot))
                    .inner
            };
            if let Some(id) = opened {
                self.open = Some(id);
            }
            if let Some(id) = self.open {
                content(ui, id);
            }
        });
    }

    /// How many of the `shown` items the rail or bar holds in `room` (its length less the
    /// foot's), at `step` per button, and whether the More button takes the footer's place.
    /// When every shown item fits beside the footer and none is hidden, there is no menu;
    /// else the More button stands where the footer was and the items share the rest.
    fn fit(&self, shown: usize, room: f32, step: f32) -> (usize, bool) {
        let hidden = shown < self.slots.len();
        let footer = self.footer.len() as f32 * step;
        if !hidden && shown as f32 * step + footer <= room {
            return (shown, false);
        }
        let held = ((room - step) / step).floor().max(0.0) as usize;
        (held.min(shown), true)
    }

    /// Keep `size` as the foot's, laying the frame out again when it changed.
    fn foot_measured(&mut self, ctx: &egui::Context, size: egui::Vec2) {
        if (size - self.foot_size).length() > 0.5 {
            self.foot_size = size;
            ctx.request_discard("the shell's foot changed size");
        }
    }

    /// The wide layout: the footer (or the More button) and the foot from the bottom, then
    /// the screens and the actions from the top, as many as fit above them.
    fn rail_ui(
        &mut self,
        ui: &mut egui::Ui,
        foot: impl FnOnce(&mut egui::Ui) -> Option<D>,
    ) -> Option<D> {
        let full = ui.available_rect_before_wrap();
        let step = BUTTON + ui.spacing().item_spacing.y;
        let shown = self.shown();
        let room = full.height() - 2.0 * RAIL_END - self.foot_size.y;
        let (held, more) = self.fit(shown.len(), room, step);
        self.lay_out(&shown[..held], more);
        let bottom = ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(full)
                .layout(egui::Layout::bottom_up(egui::Align::Center)),
            |ui| {
                ui.add_space(RAIL_END);
                self.end_ui(ui, more, false);
                let foot = ui.scope(foot);
                (foot.inner, foot.response.rect.size())
            },
        );
        let (opened, foot_size) = bottom.inner;
        self.foot_measured(ui.ctx(), foot_size);
        let top = full.with_max_y(bottom.response.rect.top());
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(top)
                .layout(egui::Layout::top_down(egui::Align::Center)),
            |ui| {
                ui.add_space(RAIL_END);
                for &s in &shown[..held] {
                    self.slot_ui(ui, s, BUTTON);
                }
            },
        );
        opened
    }

    /// The phone layout: a cell per screen or action, as many as fit, then the footer (or
    /// the More button) and the foot at the bar's end.
    ///
    /// The end keeps the width it needs; the items share what is left, a full-size button
    /// each, and those that do not fit are in the More menu. So the foot never leaves the
    /// screen, however many items there are.
    fn bar_ui(
        &mut self,
        ui: &mut egui::Ui,
        foot: impl FnOnce(&mut egui::Ui) -> Option<D>,
    ) -> Option<D> {
        let rect = ui.available_rect_before_wrap();
        let gap = ui.spacing().item_spacing.x;
        let step = BUTTON + gap;
        let shown = self.shown();
        let room = rect.width() - self.foot_size.x - gap;
        let (held, more) = self.fit(shown.len(), room, step);
        self.lay_out(&shown[..held], more);
        // The end first, from the right: the cells get what it leaves.
        let end = ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                let foot = ui.scope(foot);
                self.end_ui(ui, more, true);
                (foot.inner, foot.response.rect.size())
            },
        );
        let (opened, foot_size) = end.inner;
        self.foot_measured(ui.ctx(), foot_size);
        let cells = held;
        if cells > 0 {
            let row = rect.with_max_x(end.response.rect.left() - gap);
            ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
                ui.columns(cells, |columns| {
                    for (i, column) in columns.iter_mut().enumerate() {
                        column.vertical_centered(|ui| {
                            let side = ui.available_width().min(BUTTON);
                            self.slot_ui(ui, shown[i], side);
                        });
                    }
                });
            });
        }
        opened
    }

    /// Set out this frame's rail: the slots on it, and whether the footer is in the menu.
    fn lay_out(&mut self, on_rail: &[usize], more: bool) {
        self.on_rail = on_rail.to_vec();
        self.footer_in_menu = more;
        #[cfg(test)]
        {
            self.cell_rects.clear();
            self.footer_rects.clear();
            self.more_rect = egui::Rect::NOTHING;
        }
    }

    /// The footer, or the More button in its place; `backwards` for a layout running right
    /// to left, so the footer still reads in its order.
    fn end_ui(&mut self, ui: &mut egui::Ui, more: bool, backwards: bool) {
        if more {
            self.more_ui(ui);
            return;
        }
        let mut order: Vec<usize> = (0..self.footer.len()).collect();
        if backwards {
            order.reverse();
        }
        for i in order {
            self.footer_ui(ui, i);
        }
    }

    /// The slots the person shows on the rail, in order: the first of them that fit are on it.
    fn shown(&self) -> Vec<usize> {
        (0..self.slots.len())
            .filter(|&s| self.slots[s].shown)
            .collect()
    }

    /// Slot `s`'s button on the rail or the bar.
    fn slot_ui(&mut self, ui: &mut egui::Ui, s: usize, side: f32) {
        let response = match self.slots[s].item {
            Item::Screen(i) => self.destination_ui(ui, i, side),
            Item::Action(i) => self.action_ui(ui, i, side),
        };
        #[cfg(test)]
        self.cell_rects.push(response.rect);
        #[cfg(not(test))]
        let _ = response;
    }

    /// The More menu's rows: the footer first, then the items left off the rail, in order.
    fn menu_rows(&self) -> Vec<Row> {
        let footer = (0..self.footer.len()).map(Row::Footer);
        let off = (0..self.slots.len())
            .filter(|s| !self.on_rail.contains(s))
            .map(Row::Slot);
        footer.chain(off).collect()
    }

    /// The More button, in the footer's place, and its menu.
    fn more_ui(&mut self, ui: &mut egui::Ui) {
        let rows = self.menu_rows();
        // Marked, as a screen's button is, when the screen open is in the menu.
        let open_inside = rows
            .iter()
            .any(|&row| self.row_opens(row) == self.open && self.open.is_some());
        let response = icon_button(ui, open_inside, crate::icons::ellipsis(), BUTTON)
            .on_hover_text(crate::i18n::tr(ui.ctx(), "corundum-shell-more"));
        #[cfg(test)]
        {
            self.more_rect = response.rect;
        }
        egui::Popup::menu(&response).show(|ui| self.menu_ui(ui, &rows));
    }

    /// The screen `row` opens, if it is one.
    fn row_opens(&self, row: Row) -> Option<D> {
        match row {
            Row::Footer(i) => Some(self.footer[i].id),
            Row::Slot(s) => match self.slots[s].item {
                Item::Screen(i) => Some(self.destinations[i].id),
                Item::Action(_) => None,
            },
        }
    }

    fn menu_ui(&mut self, ui: &mut egui::Ui, rows: &[Row]) {
        #[cfg(test)]
        self.menu_rects.clear();
        for (n, &row) in rows.iter().enumerate() {
            if n > 0 && matches!(rows[n - 1], Row::Footer(_)) && matches!(row, Row::Slot(_)) {
                ui.separator();
            }
            let (icon, label) = match row {
                Row::Footer(i) => (self.footer[i].icon.clone(), self.footer[i].label),
                Row::Slot(s) => self.icon_and_label(self.slots[s].item),
            };
            let open = self.open.is_some() && self.row_opens(row) == self.open;
            let image = crate::icons::image(ui, icon, 16.0);
            let response = ui.add(
                egui::Button::image_and_text(image, crate::i18n::tr(ui.ctx(), label))
                    .selected(open),
            );
            #[cfg(test)]
            self.menu_rects.push(response.rect);
            if response.clicked() {
                match row {
                    Row::Slot(s) => match self.slots[s].item {
                        Item::Screen(i) => self.open = Some(self.destinations[i].id),
                        Item::Action(i) => self.clicked = Some(self.actions[i].id),
                    },
                    Row::Footer(i) => self.open = Some(self.footer[i].id),
                }
            }
        }
    }

    fn destination_ui(&mut self, ui: &mut egui::Ui, i: usize, side: f32) -> egui::Response {
        let d = &self.destinations[i];
        let open = self.open == Some(d.id);
        let response = icon_button(ui, open, d.icon.clone(), side)
            .on_hover_text(crate::i18n::tr(ui.ctx(), d.label));
        if response.clicked() {
            self.open = Some(d.id);
        }
        response
    }

    /// What the hover text of destination `i` says now.
    #[cfg(test)]
    fn label_text(&self, ctx: &egui::Context, i: usize) -> String {
        crate::i18n::tr(ctx, self.destinations[i].label)
    }

    fn footer_ui(&mut self, ui: &mut egui::Ui, i: usize) {
        let d = &self.footer[i];
        let open = self.open == Some(d.id);
        let response = icon_button(ui, open, d.icon.clone(), BUTTON)
            .on_hover_text(crate::i18n::tr(ui.ctx(), d.label));
        #[cfg(test)]
        {
            self.footer_rects
                .resize(self.footer.len(), egui::Rect::NOTHING);
            self.footer_rects[i] = response.rect;
        }
        if response.clicked() {
            self.open = Some(self.footer[i].id);
        }
    }

    fn action_ui(&mut self, ui: &mut egui::Ui, i: usize, side: f32) -> egui::Response {
        let a = &self.actions[i];
        let response = icon_button(ui, false, a.icon.clone(), side)
            .on_hover_text(crate::i18n::tr(ui.ctx(), a.label));
        #[cfg(test)]
        {
            self.action_rects
                .resize(self.actions.len(), egui::Rect::NOTHING);
            self.action_rects[i] = response.rect;
        }
        if response.clicked() {
            self.clicked = Some(a.id);
        }
        response
    }
}

/// The status bar along the bottom of `strip`: `status` from the left, `foot` and the
/// detail pane's toggle at the right end. The destination `foot` returns, if any.
fn status_bar_ui<D>(
    ui: &mut egui::Ui,
    strip: egui::Rect,
    foot: impl FnOnce(&mut egui::Ui) -> Option<D>,
    status: impl FnOnce(&mut egui::Ui),
) -> Option<D> {
    let visuals = ui.visuals();
    ui.painter().rect_filled(strip, 0.0, visuals.panel_fill);
    ui.painter().hline(
        strip.x_range(),
        strip.top(),
        visuals.widgets.noninteractive.bg_stroke,
    );
    let row = strip.shrink2(egui::vec2(8.0, STATUS_MARGIN));
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(row)
            .layout(egui::Layout::right_to_left(egui::Align::Center)),
        |ui| {
            ui.set_clip_rect(row);
            crate::detail::toggle_ui(ui);
            let opened = foot(ui);
            // What is left, from the left: the app's.
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), status);
            opened
        },
    )
    .inner
}

/// How far the system's bars and notches (egui's safe area insets) reach into `ui`, side by
/// side: none where `ui` already keeps clear of them.
fn unsafe_margin(ui: &egui::Ui) -> egui::epaint::MarginF32 {
    let safe = ui.ctx().content_rect();
    let rect = ui.max_rect();
    egui::epaint::MarginF32 {
        left: (safe.left() - rect.left()).max(0.0),
        right: (rect.right() - safe.right()).max(0.0),
        top: (safe.top() - rect.top()).max(0.0),
        bottom: (rect.bottom() - safe.bottom()).max(0.0),
    }
}

/// A rail or bar button's side, where there is room for it.
const BUTTON: f32 = 36.0;

/// A button with `icon` in it, `side` points wide ([`BUTTON`], or less on a crowded bar) and
/// [`BUTTON`] tall, so a narrowed one still lines up with the rest and is as easy to tap.
fn icon_button(
    ui: &mut egui::Ui,
    open: bool,
    icon: egui::ImageSource<'static>,
    side: f32,
) -> egui::Response {
    // A narrow button takes a smaller icon first, then less padding beside it, so it stays
    // `side` wide.
    let pad = ui.spacing().button_padding.x;
    let icon_size = (side - 2.0 * pad).clamp(12.0, 20.0);
    ui.scope(|ui| {
        let padding = &mut ui.spacing_mut().button_padding.x;
        *padding = padding.min(((side - icon_size) / 2.0).max(0.0));
        // The open destination's icon takes the accent, as egui's selected text does.
        ui.add(
            crate::icons::button(icon, icon_size)
                .selected(open)
                .frame_when_inactive(open)
                .min_size(egui::vec2(side, BUTTON)),
        )
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Screen {
        Entries,
        Summary,
        Prefs,
    }

    const SIZES: [[f32; 2]; 2] = [[900.0, 600.0], [400.0, 700.0]];

    fn shell() -> Shell<Screen> {
        Shell::new(vec![
            Destination::new(Screen::Entries, crate::icons::settings(), "entries"),
            Destination::new(Screen::Summary, crate::icons::arrow_left(), "summary"),
        ])
    }

    fn prefs() -> Vec<Destination<Screen>> {
        vec![Destination::new(
            Screen::Prefs,
            crate::icons::settings(),
            "preferences",
        )]
    }

    fn screen(ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| ui.label("screen"));
    }

    /// Lay `shell` out over two frames of a `size` screen, then click where `at` says.
    fn click(shell: &mut Shell<Screen>, size: [f32; 2], at: impl Fn(&Shell<Screen>) -> egui::Pos2) {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0], size[1]));
        let frame = |events: Vec<egui::Event>, shell: &mut Shell<Screen>| {
            let input = egui::RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| shell.ui(ui, |ui, _| screen(ui)));
            output.textures_delta.clear();
        };
        frame(vec![], shell);
        frame(vec![], shell);
        let pos = at(shell);
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(pos)], shell);
        frame(vec![button(true)], shell);
        frame(vec![button(false)], shell);
    }

    #[test]
    fn it_opens_on_the_first_screen_and_draws_it() {
        let mut shell = shell();
        assert_eq!(shell.current(), Some(Screen::Entries));
        for size in SIZES {
            let mut drawn = Vec::new();
            render(size, |ui| {
                shell.ui(ui, |ui, s| {
                    drawn.push(s);
                    screen(ui);
                })
            });
            assert_eq!(drawn, [Screen::Entries, Screen::Entries]);
        }
    }

    #[test]
    fn actions_are_drawn_in_both_layouts_and_report_nothing_unclicked() {
        let mut shell =
            shell().with_actions(vec![Action::new("new", crate::icons::plus(), "new-entry")]);
        for size in SIZES {
            shell.action_rects.clear();
            render(size, |ui| shell.ui(ui, |ui, _| screen(ui)));
            assert!(shell.action_rects[0].is_positive(), "{size:?}");
            assert_eq!(shell.take_action(), None);
        }
        assert_eq!(shell.current(), Some(Screen::Entries));
    }

    #[test]
    fn a_clicked_action_is_taken_once_and_opens_nothing() {
        let mut shell =
            shell().with_actions(vec![Action::new("new", crate::icons::plus(), "new-entry")]);
        click(&mut shell, SIZES[0], |s| s.action_rects[0].center());
        assert_eq!(shell.take_action(), Some("new"));
        assert_eq!(shell.take_action(), None);
        assert_eq!(shell.current(), Some(Screen::Entries));
    }

    #[test]
    fn footer_and_foot_are_in_both_layouts() {
        let mut shell = shell().with_footer(prefs());
        for size in SIZES {
            shell.footer_rects.clear();
            let mut foot = egui::Rect::NOTHING;
            render(size, |ui| {
                shell.ui_with_foot(
                    ui,
                    |ui| {
                        foot = ui
                            .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click())
                            .0;
                        None
                    },
                    |ui, _| screen(ui),
                )
            });
            assert!(shell.footer_rects[0].is_positive(), "{size:?}");
            assert!(foot.is_positive(), "{size:?}");
        }
    }

    #[test]
    fn clicking_a_footer_destination_opens_it() {
        for size in SIZES {
            let mut shell = shell().with_footer(prefs());
            click(&mut shell, size, |s| s.footer_rects[0].center());
            assert_eq!(shell.current(), Some(Screen::Prefs), "{size:?}");
        }
        let mut shell = shell().with_footer(prefs());
        shell.open(Screen::Prefs);
        let mut drawn = Vec::new();
        render([900.0, 600.0], |ui| {
            shell.ui(ui, |ui, s| {
                drawn.push(s);
                screen(ui);
            })
        });
        assert_eq!(drawn, [Screen::Prefs, Screen::Prefs]);
    }

    #[test]
    fn a_destination_the_foot_returns_is_opened() {
        let mut shell = shell();
        let mut drawn = Vec::new();
        render([900.0, 600.0], |ui| {
            shell.ui_with_foot(
                ui,
                |_| Some(Screen::Summary),
                |ui, s| {
                    drawn.push(s);
                    screen(ui);
                },
            )
        });
        assert_eq!(shell.current(), Some(Screen::Summary));
        assert_eq!(drawn, [Screen::Summary, Screen::Summary]);
    }

    #[test]
    fn the_shell_installs_the_default_theme() {
        let ctx = egui::Context::default();
        let mut shell = shell();
        crate::test_support::render_in(&ctx, [900.0, 600.0], |ui| shell.ui(ui, |ui, _| screen(ui)));
        assert!(crate::theme::is_installed(&ctx));
    }

    #[test]
    fn the_shell_keeps_an_installed_theme() {
        use crate::theme::{Contrast, ThemeSettings};
        let ctx = egui::Context::default();
        let settings = ThemeSettings {
            preference: egui::ThemePreference::Dark,
            contrast: Contrast::High,
            ..Default::default()
        };
        crate::theme::install(&ctx, &settings);
        let mut shell = shell();
        crate::test_support::render_in(&ctx, [900.0, 600.0], |ui| shell.ui(ui, |ui, _| screen(ui)));
        let high = crate::theme::Theme::new(
            true,
            Contrast::High,
            crate::theme::Tint::Swatch(crate::theme::Swatch::Blue),
        );
        assert_eq!(*crate::theme::current(&ctx), high);
    }

    const T: crate::i18n::Resources = crate::i18n::Resources {
        name: "shell-test",
        files: &[
            ("en", "t-entries = Entries\n"),
            ("ja", "t-entries = 項目\n"),
        ],
    };

    #[test]
    fn the_shell_installs_the_default_language() {
        let ctx = egui::Context::default();
        let mut shell = shell();
        crate::test_support::render_in(&ctx, [900.0, 600.0], |ui| shell.ui(ui, |ui, _| screen(ui)));
        assert!(crate::i18n::is_installed(&ctx));
    }

    #[test]
    fn the_shell_keeps_an_installed_language() {
        use crate::i18n::{Language, LanguageSettings};
        let ctx = egui::Context::default();
        let ja = LanguageSettings {
            language: Language::Explicit("ja".into()),
        };
        crate::i18n::install(&ctx, &ja);
        let mut shell = shell();
        crate::test_support::render_in(&ctx, [900.0, 600.0], |ui| shell.ui(ui, |ui, _| screen(ui)));
        assert_eq!(crate::i18n::current(&ctx).to_string(), "ja");
    }

    #[test]
    fn hover_text_follows_the_language() {
        use crate::i18n::{Language, LanguageSettings};
        let ctx = egui::Context::default();
        crate::i18n::register(&ctx, T);
        let shell = Shell::new(vec![Destination::new(
            Screen::Entries,
            crate::icons::settings(),
            "t-entries",
        )]);
        let set = |tag: &str| {
            crate::i18n::install(
                &ctx,
                &LanguageSettings {
                    language: Language::Explicit(tag.into()),
                },
            )
        };
        set("en");
        assert_eq!(shell.label_text(&ctx, 0), "Entries");
        set("ja");
        assert_eq!(shell.label_text(&ctx, 0), "項目");
    }

    /// Lay `shell` out over two frames of a `size` screen whose system bars take `safe`,
    /// with `ui` clear of them by `own` already; returns the rect the open screen got.
    fn render_safe(
        shell: &mut Shell<Screen>,
        size: [f32; 2],
        safe: egui::epaint::MarginF32,
        own: egui::epaint::MarginF32,
    ) -> egui::Rect {
        let ctx = egui::Context::default();
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size[0], size[1]));
        let mut drawn = egui::Rect::NOTHING;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(rect),
                safe_area_insets: Some(egui::SafeAreaInsets(safe)),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let inner = ui.max_rect() - own;
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    shell.ui(ui, |ui, _| {
                        drawn = ui.available_rect_before_wrap();
                        screen(ui);
                    })
                });
            });
            output.textures_delta.clear();
        }
        drawn
    }

    fn margin(left: f32, top: f32, right: f32, bottom: f32) -> egui::epaint::MarginF32 {
        egui::epaint::MarginF32 {
            left,
            right,
            top,
            bottom,
        }
    }

    #[test]
    fn narrow_it_keeps_clear_of_the_status_bar_and_the_home_indicator() {
        // An iPhone upright: the status bar and the dynamic island, the home indicator.
        let mut shell = shell().with_footer(prefs());
        let drawn = render_safe(
            &mut shell,
            [400.0, 800.0],
            margin(0.0, 62.0, 0.0, 34.0),
            egui::epaint::MarginF32::ZERO,
        );
        assert_eq!(drawn.top(), 62.0);
        let button = shell.footer_rects[0];
        assert!(button.bottom() <= 800.0 - 34.0, "{button:?}");
        // The bar sits right above the indicator: its fill runs on under it.
        assert!(button.bottom() > 800.0 - 34.0 - 12.0, "{button:?}");
        assert!(drawn.bottom() <= button.top(), "{drawn:?} {button:?}");
    }

    #[test]
    fn wide_it_keeps_clear_of_the_notch_and_the_home_indicator() {
        // An iPhone on its side: the notch on the left, the home indicator below.
        let mut shell = shell().with_footer(prefs());
        let drawn = render_safe(
            &mut shell,
            [900.0, 400.0],
            margin(59.0, 0.0, 59.0, 21.0),
            egui::epaint::MarginF32::ZERO,
        );
        let button = shell.footer_rects[0];
        assert!(button.left() >= 59.0, "{button:?}");
        assert!(button.bottom() <= 400.0 - 21.0, "{button:?}");
        assert!(drawn.right() <= 900.0 - 59.0, "{drawn:?}");
        assert!(drawn.bottom() <= 400.0 - 21.0, "{drawn:?}");
    }

    #[test]
    fn a_margin_the_app_already_keeps_is_not_doubled() {
        let safe = margin(0.0, 62.0, 0.0, 34.0);
        let mut shell = shell().with_footer(prefs());
        let drawn = render_safe(&mut shell, [400.0, 800.0], safe, safe);
        assert_eq!(drawn.top(), 62.0);
        let button = shell.footer_rects[0];
        assert!(button.bottom() > 800.0 - 34.0 - 12.0, "{button:?}");
    }

    #[test]
    fn with_only_a_footer_it_opens_on_the_footer() {
        let shell = Shell::new(vec![]).with_footer(prefs());
        assert_eq!(shell.current(), Some(Screen::Prefs));
    }

    /// The action first, then eight screens of their own names, with the footer.
    fn crowded_shell() -> Shell<Screen> {
        const NAMES: [&str; 8] = ["s0", "s1", "s2", "s3", "s4", "s5", "s6", "s7"];
        let mut items: Vec<RailItem<Screen>> =
            vec![Action::new("new", crate::icons::plus(), "new-entry").into()];
        items.extend(
            NAMES.iter().map(|&name| {
                Destination::new(Screen::Entries, crate::icons::settings(), name).into()
            }),
        );
        Shell::from_items(items).with_footer(prefs())
    }

    /// `shell` on a `size` screen, with a 12-point foot, and a status bar when `status`.
    /// Returns the foot's rect.
    fn crowded(shell: &mut Shell<Screen>, size: [f32; 2], status: bool) -> egui::Rect {
        let mut foot = egui::Rect::NOTHING;
        render(size, |ui| {
            let dot = |ui: &mut egui::Ui| {
                foot = ui
                    .allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::click())
                    .0;
                None
            };
            if status {
                shell.ui_with_status(ui, dot, |ui| drop(ui.label("saved")), |ui, _| screen(ui));
            } else {
                shell.ui_with_foot(ui, dot, |ui, _| screen(ui));
            }
        });
        foot
    }

    #[test]
    fn items_and_actions_mix_in_the_apps_order() {
        let mut shell = crowded_shell();
        assert_eq!(
            shell.current(),
            Some(Screen::Entries),
            "the first screen, not the action"
        );
        crowded(&mut shell, [914.0, 900.0], true);
        assert_eq!(shell.cell_rects.len(), 9);
        assert_eq!(
            shell.cell_rects[0], shell.action_rects[0],
            "the action on top"
        );
        assert!(!shell.footer_in_menu);
        assert!(shell.footer_rects[0].is_positive());
        assert_eq!(shell.more_rect, egui::Rect::NOTHING, "no menu when all fit");
    }

    #[test]
    fn a_crowded_bar_puts_the_more_button_where_the_footer_was() {
        for width in [320.0, 360.0, 411.0] {
            let mut shell = crowded_shell();
            let foot = crowded(&mut shell, [width, 700.0], false);
            let (cells, more) = (&shell.cell_rects, shell.more_rect);
            assert!(
                !cells.is_empty() && cells.len() < 9,
                "{width}: {}",
                cells.len()
            );
            assert!(shell.footer_in_menu, "{width}");
            assert!(
                shell.footer_rects.is_empty(),
                "{width}: the footer is in the menu"
            );
            for rect in cells.iter().chain([&more, &foot]) {
                assert!(rect.is_positive(), "{width}: {rect:?}");
                assert!(
                    rect.left() >= 0.0 && rect.right() <= width,
                    "{width}: {rect:?}"
                );
            }
            // In order, and none on top of the next.
            let row: Vec<egui::Rect> = cells.iter().copied().chain([more, foot]).collect();
            for pair in row.windows(2) {
                assert!(pair[0].right() <= pair[1].left(), "{width}: {pair:?}");
            }
            // Every button keeps its full size, and they line up.
            for rect in cells.iter().chain([&more]) {
                assert_eq!(rect.size(), egui::vec2(BUTTON, BUTTON), "{width}: {rect:?}");
                assert_eq!(rect.center().y, more.center().y, "{width}: {rect:?}");
            }
        }
    }

    #[test]
    fn a_low_rail_keeps_its_items_clear_of_the_more_button() {
        // A phone on its side (#29), a short desktop window, one shorter still.
        for (height, status) in [(411.0, true), (411.0, false), (300.0, true), (160.0, false)] {
            let mut shell = crowded_shell();
            let foot = crowded(&mut shell, [914.0, height], status);
            let (cells, more) = (&shell.cell_rects, shell.more_rect);
            let case = format!("{height} {status}");
            assert!(cells.len() < 9, "{case}: {}", cells.len());
            assert!(
                shell.footer_rects.is_empty(),
                "{case}: the footer is in the menu"
            );
            assert!(more.is_positive(), "{case}");
            let column: Vec<egui::Rect> = cells.iter().copied().chain([more]).collect();
            for pair in column.windows(2) {
                assert!(pair[0].bottom() <= pair[1].top(), "{case}: {pair:?}");
            }
            if !status {
                // The foot sits above the More button, at the rail's foot.
                let last = cells.last().copied().unwrap_or(egui::Rect::NOTHING);
                assert!(last.bottom() <= foot.top(), "{case}: {last:?} {foot:?}");
                assert!(foot.bottom() <= more.top(), "{case}: {foot:?} {more:?}");
            }
        }
    }

    #[test]
    fn at_every_height_the_footer_or_the_more_button_holds_the_end() {
        let mut both = (false, false);
        for height in (200..=900).step_by(10) {
            let height = height as f32;
            let mut shell = crowded_shell();
            crowded(&mut shell, [914.0, height], true);
            let cells = &shell.cell_rects;
            let end = if shell.footer_in_menu {
                both.1 = true;
                assert!(cells.len() < 9, "{height}");
                assert!(shell.footer_rects.is_empty(), "{height}");
                shell.more_rect
            } else {
                both.0 = true;
                assert_eq!(cells.len(), 9, "{height}: all fit, or the footer goes");
                assert_eq!(shell.more_rect, egui::Rect::NOTHING, "{height}");
                shell.footer_rects[0]
            };
            assert!(end.is_positive(), "{height}");
            for pair in cells
                .iter()
                .copied()
                .chain([end])
                .collect::<Vec<_>>()
                .windows(2)
            {
                assert!(pair[0].bottom() <= pair[1].top(), "{height}: {pair:?}");
            }
        }
        assert_eq!(
            both,
            (true, true),
            "the sweep crosses from one to the other"
        );
    }

    fn keys(shell: &Shell<Screen>) -> Vec<String> {
        shell.rail_order().order
    }

    #[test]
    fn the_rail_order_round_trips_and_skips_names_it_does_not_know() {
        let mut shell = crowded_shell();
        assert_eq!(keys(&shell)[..3], ["new", "s0", "s1"]);
        let order = RailOrder {
            order: vec!["s3".into(), "gone".into(), "new".into(), "s1".into()],
            hidden: vec!["s1".into(), "gone".into()],
        };
        shell.set_rail_order(&order);
        // The named first, the action among them, then the rest in the app's order.
        assert_eq!(
            keys(&shell),
            ["s3", "new", "s1", "s0", "s2", "s4", "s5", "s6", "s7"]
        );
        assert_eq!(shell.rail_order().hidden, ["s1"]);
        let again = shell.rail_order();
        shell.set_rail_order(&again);
        assert_eq!(shell.rail_order(), again);
        shell.set_rail_order(&RailOrder::default());
        assert_eq!(
            keys(&shell)[..3],
            ["new", "s0", "s1"],
            "the default is the app's"
        );
    }

    #[test]
    fn with_actions_still_puts_them_last() {
        let shell =
            shell().with_actions(vec![Action::new("new", crate::icons::plus(), "new-entry")]);
        assert_eq!(keys(&shell), ["entries", "summary", "new"]);
    }

    #[test]
    fn a_hidden_item_sends_the_footer_into_the_menu() {
        let mut shell = shell().with_footer(prefs()).with_rail_order(&RailOrder {
            order: vec![],
            hidden: vec!["entries".into()],
        });
        for size in SIZES {
            render(size, |ui| shell.ui(ui, |ui, _| screen(ui)));
            assert_eq!(shell.on_rail, [1], "{size:?}");
            assert!(shell.footer_in_menu, "{size:?}");
            assert_eq!(
                shell.menu_rows(),
                [Row::Footer(0), Row::Slot(0)],
                "{size:?}: the footer first"
            );
        }
    }

    /// Run `shell` on a 900 × 600 screen in `ctx`; clicking at `at` when given.
    fn frames(ctx: &egui::Context, shell: &mut Shell<Screen>, at: Option<egui::Pos2>) {
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0));
        let frame = |events: Vec<egui::Event>, shell: &mut Shell<Screen>| {
            let input = egui::RawInput {
                screen_rect: Some(rect),
                events,
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| shell.ui(ui, |ui, _| screen(ui)));
            output.textures_delta.clear();
        };
        let Some(pos) = at else {
            frame(vec![], shell);
            frame(vec![], shell);
            return;
        };
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(pos)], shell);
        frame(vec![button(true)], shell);
        frame(vec![button(false)], shell);
        frame(vec![], shell);
    }

    #[test]
    fn the_menu_opens_what_is_off_the_rail() {
        let mut shell = shell().with_footer(prefs()).with_rail_order(&RailOrder {
            order: vec![],
            hidden: vec!["summary".into()],
        });
        let ctx = egui::Context::default();
        frames(&ctx, &mut shell, None);
        let at = shell.more_rect.center();
        frames(&ctx, &mut shell, Some(at));
        assert_eq!(
            shell.menu_rects.len(),
            2,
            "the footer and the hidden screen"
        );
        let at = shell.menu_rects[1].center();
        frames(&ctx, &mut shell, Some(at));
        assert_eq!(shell.current(), Some(Screen::Summary));
        let at = shell.more_rect.center();
        frames(&ctx, &mut shell, Some(at));
        let at = shell.menu_rects[0].center();
        frames(&ctx, &mut shell, Some(at));
        assert_eq!(shell.current(), Some(Screen::Prefs));
    }

    #[test]
    fn the_shell_lists_its_items_and_takes_the_order_asked_for() {
        let ctx = egui::Context::default();
        let mut shell = crowded_shell();
        frames(&ctx, &mut shell, None);
        let listed = published(&ctx).unwrap();
        assert_eq!(listed.len(), 9);
        assert_eq!(listed[0].key, "new");
        request(
            &ctx,
            RailOrder {
                order: vec!["s7".into()],
                hidden: vec!["new".into()],
            },
        );
        frames(&ctx, &mut shell, None);
        assert_eq!(keys(&shell)[0], "s7");
        assert_eq!(shell.rail_order().hidden, ["new"]);
        assert_eq!(published(&ctx).unwrap()[0].key, "s7");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn the_rail_order_is_taken_from_the_store_and_saved_there() {
        use crate::store::{self, Store, Stored};
        let dir = tempfile::tempdir().unwrap();
        let file = Store::at(dir.path().join(store::FILE_NAME));
        let saved = RailOrder {
            order: vec!["summary".into(), "entries".into()],
            hidden: vec![],
        };
        file.save(&Stored {
            look: None,
            rail: Some(saved.clone()),
        })
        .unwrap();
        let ctx = egui::Context::default();
        store::attach(&ctx, file.clone());
        let mut shell = shell();
        frames(&ctx, &mut shell, None);
        assert_eq!(shell.rail_order(), saved);

        request(
            &ctx,
            RailOrder {
                order: vec!["entries".into(), "summary".into()],
                hidden: vec![],
            },
        );
        frames(&ctx, &mut shell, None);
        assert_eq!(
            file.load().and_then(|s| s.rail).map(|r| r.order),
            Some(vec!["entries".to_owned(), "summary".to_owned()])
        );
    }

    #[test]
    fn a_roomy_bar_keeps_full_size_buttons() {
        let mut shell = shell().with_footer(prefs());
        render(SIZES[1], |ui| shell.ui(ui, |ui, _| screen(ui)));
        assert_eq!(shell.cell_rects.len(), 2);
        for rect in &shell.cell_rects {
            assert_eq!(rect.size(), egui::vec2(BUTTON, BUTTON), "{rect:?}");
        }
    }

    /// Render `shell` with a status bar at `size`: where the foot was drawn, and whether
    /// the status was.
    fn with_status(shell: &mut Shell<Screen>, size: [f32; 2]) -> (egui::Rect, bool) {
        let (mut foot, mut status) = (egui::Rect::NOTHING, false);
        render(size, |ui| {
            shell.ui_with_status(
                ui,
                |ui| {
                    foot = ui.label("dot").rect;
                    None
                },
                |ui| {
                    status = true;
                    ui.label("saved");
                },
                |ui, _| screen(ui),
            )
        });
        (foot, status)
    }

    #[test]
    fn wide_the_status_bar_holds_the_status_and_the_foot() {
        let mut shell = shell().with_footer(prefs());
        let (foot, status) = with_status(&mut shell, SIZES[0]);
        assert!(status);
        // On the bar along the bottom, at its right end: not at the rail's foot.
        assert!(foot.bottom() > SIZES[0][1] - 30.0, "{foot:?}");
        assert!(foot.left() > SIZES[0][0] / 2.0, "{foot:?}");
        // The rail stops above the bar.
        assert!(
            shell.footer_rects[0].bottom() < foot.top(),
            "{:?}",
            shell.footer_rects
        );
    }

    #[test]
    fn narrow_there_is_no_status_bar_and_the_foot_stays_on_the_bar() {
        let mut shell = shell().with_footer(prefs());
        let (foot, status) = with_status(&mut shell, SIZES[1]);
        assert!(!status);
        assert!(foot.is_positive());
        let bar_top = shell.footer_rects[0].top();
        assert!(foot.top() >= bar_top - 10.0, "{foot:?} beside {bar_top}");
    }

    #[test]
    fn with_nothing_there_is_nothing_to_draw() {
        let mut shell: Shell<Screen> = Shell::new(vec![]);
        assert_eq!(shell.current(), None);
        for size in SIZES {
            render(size, |ui| shell.ui(ui, |_, s| panic!("{s:?} drawn")));
        }
    }
}
