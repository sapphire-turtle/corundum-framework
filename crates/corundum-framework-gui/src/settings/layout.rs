//! [`SettingsLayout`]: the list of pages beside the open one, or, narrow, one at a time.

use crate::COMPACT_WIDTH;
use crate::tr;

/// The list's width, wide.
const LIST_WIDTH: f32 = 170.0;

/// One line of the list.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ListItem<K> {
    /// A group's heading, in small weak text.
    Heading(String),
    /// A page: `page` names it to [`SettingsLayout::ui`]'s page closure. A `dirty` page
    /// holds changes not saved yet, and is marked.
    Entry { page: K, label: String, dirty: bool },
    /// Space setting the lines after it apart: About, say.
    Gap,
}

impl<K> ListItem<K> {
    /// A page's entry, not dirty.
    pub fn entry(page: K, label: impl Into<String>) -> ListItem<K> {
        ListItem::Entry {
            page,
            label: label.into(),
            dirty: false,
        }
    }
}

/// The settings screen's layout: the list beside the open page, or — narrower than
/// [`COMPACT_WIDTH`] — the list, or the open page under a way back to it. It keeps only
/// which page is open; the host gives the list every frame, so pages may come and go.
#[derive(Clone, Debug)]
pub struct SettingsLayout<K> {
    /// `None` shows the list alone when compact; when wide, the first page opens.
    open: Option<K>,
}

impl<K> Default for SettingsLayout<K> {
    fn default() -> SettingsLayout<K> {
        SettingsLayout { open: None }
    }
}

impl<K: Clone + PartialEq> SettingsLayout<K> {
    /// A layout with no page chosen yet.
    pub fn new() -> SettingsLayout<K> {
        SettingsLayout::default()
    }

    /// The open page.
    pub fn current(&self) -> Option<&K> {
        self.open.as_ref()
    }

    /// Open `page`; `None` goes back to the list when compact.
    pub fn open(&mut self, page: Option<K>) {
        self.open = page;
    }

    /// Render into `ui`, as the root of what the host leaves it: the list of `items`, and
    /// `page` drawing the open one in a vertical scroll area. A page gone from `items` is
    /// closed.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        items: &[ListItem<K>],
        page: impl FnOnce(&mut egui::Ui, K),
    ) {
        let listed = |k: &K| {
            items
                .iter()
                .any(|i| matches!(i, ListItem::Entry { page, .. } if page == k))
        };
        if self.open.as_ref().is_some_and(|k| !listed(k)) {
            self.open = None;
        }
        if ui.available_width() < COMPACT_WIDTH {
            egui::CentralPanel::default().show(ui, |ui| match self.open.clone() {
                None => {
                    egui::ScrollArea::vertical()
                        .id_salt("corundum_settings_list")
                        .show(ui, |ui| self.list_ui(ui, items));
                }
                Some(open) => {
                    let back = crate::icons::image(ui, crate::icons::arrow_left(), 14.0);
                    if ui
                        .add(egui::Button::image_and_text(
                            back,
                            tr!(ui, "corundum-settings"),
                        ))
                        .clicked()
                    {
                        self.open = None;
                    }
                    ui.separator();
                    page_ui(ui, open, page);
                }
            });
        } else {
            if self.open.is_none() {
                self.open = items.iter().find_map(|i| match i {
                    ListItem::Entry { page, .. } => Some(page.clone()),
                    _ => None,
                });
            }
            egui::Panel::left("corundum_settings_list")
                .resizable(false)
                .exact_size(LIST_WIDTH)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("corundum_settings_list")
                        .show(ui, |ui| self.list_ui(ui, items));
                });
            let open = self.open.clone();
            egui::CentralPanel::default().show(ui, |ui| {
                if let Some(open) = open {
                    page_ui(ui, open, page);
                }
            });
        }
    }

    fn list_ui(&mut self, ui: &mut egui::Ui, items: &[ListItem<K>]) {
        // Full-width rows, their labels at the left.
        ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
            for item in items {
                match item {
                    ListItem::Heading(text) => {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(text).small().weak());
                    }
                    ListItem::Entry { page, label, dirty } => {
                        let open = self.open.as_ref() == Some(page);
                        if ui.selectable_label(open, marked(label, *dirty)).clicked() {
                            self.open = Some(page.clone());
                        }
                    }
                    ListItem::Gap => {
                        ui.add_space(12.0);
                    }
                }
            }
        });
    }
}

/// The open page, scrolled.
fn page_ui<K>(ui: &mut egui::Ui, open: K, page: impl FnOnce(&mut egui::Ui, K)) {
    egui::ScrollArea::vertical()
        .id_salt("corundum_settings_page")
        .auto_shrink(false)
        .show(ui, |ui| page(ui, open));
}

/// `label`, marked when `dirty`: a page's entry, a scope's tab.
pub(super) fn marked(label: &str, dirty: bool) -> String {
    if dirty {
        format!("{label} •")
    } else {
        label.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    fn items() -> Vec<ListItem<u8>> {
        vec![
            ListItem::Heading("App".into()),
            ListItem::entry(1, "One"),
            ListItem::Entry {
                page: 2,
                label: "Two".into(),
                dirty: true,
            },
            ListItem::Gap,
            ListItem::entry(3, "About"),
        ]
    }

    #[test]
    fn wide_it_opens_the_first_page_beside_the_list() {
        let mut layout = SettingsLayout::new();
        let mut drawn = None;
        render([900.0, 600.0], |ui| {
            layout.ui(ui, &items(), |_, k| drawn = Some(k))
        });
        assert_eq!(layout.current(), Some(&1));
        assert_eq!(drawn, Some(1));
    }

    #[test]
    fn compact_it_shows_the_list_then_one_page() {
        let mut layout = SettingsLayout::new();
        let mut drawn = None;
        render([400.0, 700.0], |ui| {
            layout.ui(ui, &items(), |_, k| drawn = Some(k))
        });
        assert_eq!(layout.current(), None, "the list alone, no page chosen");
        assert_eq!(drawn, None);
        layout.open(Some(3));
        render([400.0, 700.0], |ui| {
            layout.ui(ui, &items(), |_, k| drawn = Some(k))
        });
        assert_eq!(drawn, Some(3));
    }

    #[test]
    fn a_page_gone_from_the_list_is_closed() {
        let mut layout = SettingsLayout::new();
        layout.open(Some(9));
        render([400.0, 700.0], |ui| layout.ui(ui, &items(), |_, _| {}));
        assert_eq!(layout.current(), None);
        layout.open(Some(9));
        render([900.0, 600.0], |ui| layout.ui(ui, &items(), |_, _| {}));
        assert_eq!(layout.current(), Some(&1), "wide, the first page instead");
    }

    #[test]
    fn a_dirty_entry_is_marked() {
        assert_eq!(marked("Two", true), "Two •");
        assert_eq!(marked("One", false), "One");
    }
}
