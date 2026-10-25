//! The settings screen, and the pieces its pages are made of.
//!
//! [`Settings`] is the screen: the pages an app and the frameworks under it put in, by
//! group, in a list beside the open page — or, narrower than
//! [`COMPACT_WIDTH`](crate::COMPACT_WIDTH), the list, then one page under a way back to
//! it. A host whose pages share state, or come and go (pages made from a schema, say),
//! gives the list itself each frame to a [`SettingsLayout`] instead.
//!
//! [`scope_tabs`] is the row of tabs a page shows when a setting can be kept in more than
//! one place — on this device or for the whole workgroup, on the client or on the server.
//! [`AppearancePage`] is the theme and the language over those tabs. [`RailPage`] is the
//! order of the shell's rail, and which items it shows.
//!
//! Nothing here stores a setting or checks one: the pages show the host's values and give
//! back what the person changed, and the host keeps them as it keeps the rest of its
//! settings. [`RailPage`] alone hands its change to the shell at once.

mod appearance;
mod layout;
mod rail;
mod scope;

pub use appearance::{AppearanceChange, AppearanceCtx, AppearancePage, AppearanceScope};
pub use layout::{ListItem, SettingsLayout};
pub use rail::RailPage;
pub use scope::{ScopeTab, scope_tabs};

/// A page of [`Settings`].
pub trait SettingsSection {
    /// The page's name in the list, in the language installed in `ctx`. It is asked every
    /// frame, so a language switch reaches the list at once.
    fn label(&self, ctx: &egui::Context) -> String;

    /// Render the page, its own heading first.
    fn ui(&mut self, ui: &mut egui::Ui);

    /// Whether the page holds changes not saved yet; its entry is marked.
    fn dirty(&self) -> bool {
        false
    }
}

/// How a group's heading is named.
enum Heading {
    /// No heading: the group is set apart by a gap.
    None,
    /// A message id, looked up in the installed language.
    Message(&'static str),
    /// Text as it is: the app's name, say.
    Text(String),
}

/// Pages under one heading in [`Settings`]: the app's own, a framework's, About.
pub struct SettingsGroup {
    heading: Heading,
    sections: Vec<Box<dyn SettingsSection>>,
}

impl SettingsGroup {
    /// `sections` with no heading, set apart from the group before by a gap: About, say.
    pub fn new(sections: Vec<Box<dyn SettingsSection>>) -> SettingsGroup {
        SettingsGroup {
            heading: Heading::None,
            sections,
        }
    }

    /// Head the group with the message `id`, in the installed language.
    pub fn titled(mut self, id: &'static str) -> SettingsGroup {
        self.heading = Heading::Message(id);
        self
    }

    /// Head the group with `name` as it is: the app's name, say.
    pub fn named(mut self, name: impl Into<String>) -> SettingsGroup {
        self.heading = Heading::Text(name.into());
        self
    }
}

/// The settings screen: groups of pages, in a list beside the open page. Hold one; call
/// [`ui`](Self::ui) every frame it is shown, as the root of what the shell leaves.
///
/// ```no_run
/// # use corundum_framework_gui::settings::{Settings, SettingsGroup, SettingsSection};
/// # fn example(general: Box<dyn SettingsSection>, about: Box<dyn SettingsSection>, ui: &mut egui::Ui) {
/// let mut settings = Settings::new()
///     .with_group(SettingsGroup::new(vec![general]).named("Journal"))
///     .with_group(SettingsGroup::new(vec![about]));
/// settings.ui(ui);
/// # }
/// ```
#[derive(Default)]
pub struct Settings {
    groups: Vec<SettingsGroup>,
    /// The open page, by group and by section.
    layout: SettingsLayout<(usize, usize)>,
}

impl Settings {
    /// Settings with no pages yet.
    pub fn new() -> Settings {
        Settings::default()
    }

    /// Add `group`, after the groups already there.
    pub fn with_group(mut self, group: SettingsGroup) -> Settings {
        self.groups.push(group);
        self
    }

    /// Add `group`, after the groups already there: a framework's pages, put in by the
    /// framework once the app has added its own.
    pub fn push_group(&mut self, group: SettingsGroup) {
        self.groups.push(group);
    }

    /// Open the `section`th page of the `group`th group.
    pub fn open(&mut self, group: usize, section: usize) {
        self.layout.open(Some((group, section)));
    }

    /// The open page, by group and by section.
    pub fn current(&self) -> Option<(usize, usize)> {
        self.layout.current().copied()
    }

    /// Render into `ui`, as the root of what the shell leaves of it.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let items = self.items(ui.ctx());
        let Settings { groups, layout } = self;
        layout.ui(ui, &items, |ui, (g, s)| {
            if let Some(section) = groups.get_mut(g).and_then(|g| g.sections.get_mut(s)) {
                section.ui(ui);
            }
        });
    }

    /// The list, in the language installed in `ctx`.
    fn items(&self, ctx: &egui::Context) -> Vec<ListItem<(usize, usize)>> {
        let mut out = Vec::new();
        for (g, group) in self.groups.iter().enumerate() {
            match &group.heading {
                Heading::None if g > 0 => out.push(ListItem::Gap),
                Heading::None => {}
                Heading::Message(id) => out.push(ListItem::Heading(crate::i18n::tr(ctx, id))),
                Heading::Text(text) => out.push(ListItem::Heading(text.clone())),
            }
            for (s, section) in group.sections.iter().enumerate() {
                out.push(ListItem::Entry {
                    page: (g, s),
                    label: section.label(ctx),
                    dirty: section.dirty(),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    struct Named(&'static str, bool);

    impl SettingsSection for Named {
        fn label(&self, _ctx: &egui::Context) -> String {
            self.0.to_owned()
        }

        fn ui(&mut self, ui: &mut egui::Ui) {
            ui.label(self.0);
        }

        fn dirty(&self) -> bool {
            self.1
        }
    }

    fn settings() -> Settings {
        Settings::new()
            .with_group(
                SettingsGroup::new(vec![
                    Box::new(Named("Look", false)),
                    Box::new(Named("Sync", true)),
                ])
                .named("Example"),
            )
            .with_group(
                SettingsGroup::new(vec![Box::new(Named("Appearance", false))])
                    .titled("corundum-settings"),
            )
            .with_group(SettingsGroup::new(vec![Box::new(Named("About", false))]))
    }

    fn labels(settings: &Settings) -> Vec<String> {
        let ctx = egui::Context::default();
        crate::i18n::install(
            &ctx,
            &crate::i18n::LanguageSettings {
                language: crate::i18n::Language::Explicit("en".into()),
            },
        );
        settings
            .items(&ctx)
            .into_iter()
            .map(|item| match item {
                ListItem::Heading(h) => format!("# {h}"),
                ListItem::Entry { label, dirty, .. } => layout::marked(&label, dirty),
                ListItem::Gap => "-".into(),
            })
            .collect()
    }

    #[test]
    fn groups_are_listed_in_order() {
        let mut settings = settings();
        settings
            .push_group(SettingsGroup::new(vec![Box::new(Named("Later", false))]).named("Late"));
        let labels = labels(&settings);
        assert_eq!(
            labels,
            [
                "# Example",
                "Look",
                "Sync •",
                "# Settings",
                "Appearance",
                "-",
                "About",
                "# Late",
                "Later",
            ]
        );
    }

    #[test]
    fn wide_it_opens_the_first_page() {
        let mut settings = settings();
        render([900.0, 600.0], |ui| settings.ui(ui));
        assert_eq!(settings.current(), Some((0, 0)));
    }

    #[test]
    fn compact_it_shows_the_list_then_the_page_opened() {
        let mut settings = settings();
        render([400.0, 700.0], |ui| settings.ui(ui));
        assert_eq!(settings.current(), None);
        settings.open(2, 0);
        render([400.0, 700.0], |ui| settings.ui(ui));
        assert_eq!(settings.current(), Some((2, 0)));
    }
}
