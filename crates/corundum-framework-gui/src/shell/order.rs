//! [`RailOrder`], and how the [`Shell`](super::Shell) and the rail's settings page
//! ([`RailPage`](crate::settings::RailPage)) reach each other: through the egui context, as
//! the page is drawn inside a screen the shell knows nothing of. Each frame the shell lists
//! its items there ([`publish`]); the page draws that list, and leaves the order the person
//! set there ([`request`]) for the shell to take on its next frame.

/// The order of the rail's screens and actions, and which of them it leaves to the More
/// menu: what the person set on the rail's settings page, kept by the app or, with the
/// `serde` feature, on the device by [`store`](crate::store).
///
/// A screen is named by its label's message id, an action by its id. An item the order does
/// not name — one the app added since — comes after those it names; a name no item has is
/// passed over. The default, empty, is the app's order with every item shown.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct RailOrder {
    /// The items, first to last.
    pub order: Vec<String>,
    /// The items left to the More menu.
    pub hidden: Vec<String>,
}

/// One of the rail's items, as the shell lists it for the page.
#[derive(Clone, Debug)]
pub(crate) struct RailEntry {
    /// Its name in a [`RailOrder`].
    pub key: &'static str,
    pub icon: egui::ImageSource<'static>,
    /// Its name's message id.
    pub label: &'static str,
    pub shown: bool,
}

fn published_id() -> egui::Id {
    egui::Id::unique("corundum_rail_entries")
}

fn request_id() -> egui::Id {
    egui::Id::unique("corundum_rail_request")
}

/// List the rail's items, in their order, for the page.
pub(crate) fn publish(ctx: &egui::Context, entries: Vec<RailEntry>) {
    ctx.data_mut(|d| d.insert_temp(published_id(), entries));
}

/// The rail's items as the shell last listed them; `None` when no shell has drawn.
pub(crate) fn published(ctx: &egui::Context) -> Option<Vec<RailEntry>> {
    ctx.data(|d| d.get_temp(published_id()))
}

/// Ask the shell to lay its rail out in `order`, on its next frame.
pub(crate) fn request(ctx: &egui::Context, order: RailOrder) {
    ctx.data_mut(|d| d.insert_temp(request_id(), order));
    ctx.request_repaint();
}

/// The order asked for since the shell's last frame, once.
pub(crate) fn take_request(ctx: &egui::Context) -> Option<RailOrder> {
    ctx.data_mut(|d| d.remove_temp(request_id()))
}

/// Move the element at `from` to before (or, `after`, after) the one at `to`. `true` when
/// that changed the order.
pub(crate) fn move_to<T>(items: &mut Vec<T>, from: usize, to: usize, after: bool) -> bool {
    let mut at = to + usize::from(after);
    if from < at {
        at -= 1;
    }
    if at == from {
        return false;
    }
    let item = items.remove(from);
    items.insert(at, item);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_to_puts_it_before_or_after() {
        let mut v = vec!['a', 'b', 'c', 'd'];
        assert!(move_to(&mut v, 0, 2, true));
        assert_eq!(v, ['b', 'c', 'a', 'd']);
        assert!(move_to(&mut v, 3, 0, false));
        assert_eq!(v, ['d', 'b', 'c', 'a']);
        assert!(!move_to(&mut v, 1, 1, false), "onto itself");
        assert!(!move_to(&mut v, 1, 0, true), "already after it");
        assert!(!move_to(&mut v, 1, 2, false), "already before it");
    }

    #[test]
    fn a_request_is_taken_once() {
        let ctx = egui::Context::default();
        assert_eq!(take_request(&ctx), None);
        let order = RailOrder {
            order: vec!["a".into()],
            hidden: vec![],
        };
        request(&ctx, order.clone());
        assert_eq!(take_request(&ctx), Some(order));
        assert_eq!(take_request(&ctx), None);
    }
}
