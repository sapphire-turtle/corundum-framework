//! [`AppearancePage`]: the theme and the language, a tab per scope they can be kept in.

use crate::i18n::{self, LanguageSettings};
use crate::theme::{self, Swatch, ThemeSettings};
use crate::tr;

use super::scope::{ScopeTab, scope_tabs};

/// One scope the page can show: its tab, and the theme and the language it shows there —
/// the scope's own values over the defaults, or (on the scope that wins, a device over its
/// workgroup) the values in effect.
#[derive(Clone, PartialEq, Debug)]
pub struct AppearanceScope<S> {
    /// The tab.
    pub tab: ScopeTab<S>,
    /// The theme the tab shows.
    pub theme: ThemeSettings,
    /// The language the tab shows.
    pub language: LanguageSettings,
}

/// What [`AppearancePage::ui`] reads each frame.
#[derive(Clone, Debug)]
pub struct AppearanceCtx<'a, S> {
    /// The scopes, in tab order. With one, there is no row of tabs; with none, the
    /// widgets are shown disabled.
    pub scopes: &'a [AppearanceScope<S>],
    /// Why nothing can be changed now, shown above the widgets, which are disabled.
    pub unavailable: Option<String>,
    /// Why the language cannot be changed now; the theme still can.
    pub language_unavailable: Option<String>,
    /// A change the page gave is still being stored: the widgets wait.
    pub busy: bool,
}

impl<'a, S> AppearanceCtx<'a, S> {
    /// Everything available, nothing being stored.
    pub fn new(scopes: &'a [AppearanceScope<S>]) -> AppearanceCtx<'a, S> {
        AppearanceCtx {
            scopes,
            unavailable: None,
            language_unavailable: None,
            busy: false,
        }
    }
}

/// What the person changed this frame, for the host to store in `scope`.
#[derive(Clone, PartialEq, Debug)]
pub struct AppearanceChange<S> {
    /// The scope whose tab was open.
    pub scope: S,
    /// The new theme, when it changed.
    pub theme: Option<ThemeSettings>,
    /// The new language, when it changed.
    pub language: Option<LanguageSettings>,
}

/// The Appearance page: a tab per scope, and the theme's settings and the language picker
/// over the open tab's values. A change is given as it is made — a click, a choice, or the
/// end of a drag — for the host to store; the page saves nothing.
///
/// The widgets install what they show, so a drag previews live. Once the host has stored a
/// change, it installs the look in effect, which on a scope that does not win (a
/// workgroup's, under the device's own) is not the one just chosen. Until the host calls
/// [`reloaded`](Self::reloaded), the page keeps showing the change rather than the values
/// the host gives, which are still the ones from before it; a host that stores at once
/// calls it right after.
#[derive(Clone, Debug)]
pub struct AppearancePage<S> {
    /// The open tab; `None` until there are scopes to choose from.
    tab: Option<S>,
    /// The tab whose values the widgets last showed.
    shown: Option<S>,
    /// What the theme's widgets edit.
    working: ThemeSettings,
    /// What the language picker edits.
    language: LanguageSettings,
    /// A drag is under way: keep `working` rather than the tab's values.
    editing: bool,
    /// A change was given and the host has not reloaded since.
    held: bool,
    /// Why the last change could not be stored.
    error: Option<String>,
}

impl<S> Default for AppearancePage<S> {
    fn default() -> AppearancePage<S> {
        AppearancePage {
            tab: None,
            shown: None,
            working: ThemeSettings::default(),
            language: LanguageSettings::default(),
            editing: false,
            held: false,
            error: None,
        }
    }
}

impl<S: Copy + PartialEq> AppearancePage<S> {
    /// A page that opens on the first enabled tab.
    pub fn new() -> AppearancePage<S> {
        AppearancePage::default()
    }

    /// A page that opens on `scope`'s tab.
    pub fn with_tab(mut self, scope: S) -> AppearancePage<S> {
        self.tab = Some(scope);
        self
    }

    /// The open tab.
    pub fn tab(&self) -> Option<S> {
        self.tab
    }

    /// Render; the change the person made, if any. `notes` draws under the tabs, for the
    /// open scope: where its values come from, a way to drop its own.
    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        cx: &AppearanceCtx<S>,
        notes: impl FnOnce(&mut egui::Ui, S),
    ) -> Option<AppearanceChange<S>> {
        ui.horizontal(|ui| {
            ui.heading(tr!(ui, "corundum-settings-appearance"));
            if cx.busy {
                ui.spinner();
            }
        });
        ui.separator();

        if cx.scopes.len() > 1 {
            let tabs: Vec<ScopeTab<S>> = cx.scopes.iter().map(|s| s.tab.clone()).collect();
            let mut open = self.tab.unwrap_or(tabs[0].scope);
            scope_tabs(ui, &mut open, &tabs, None);
            self.tab = Some(open);
        } else if let Some(only) = cx.scopes.first() {
            self.tab = Some(only.tab.scope);
        }
        let scope = self
            .tab
            .and_then(|tab| cx.scopes.iter().find(|s| s.tab.scope == tab))
            .filter(|s| s.tab.enabled);
        let (Some(scope), None) = (scope, cx.unavailable.as_ref()) else {
            if let Some(why) = &cx.unavailable {
                ui.weak(why);
            }
            // Shown as it is, not editable.
            let mut theme = self.working.clone();
            let mut language = self.language.clone();
            ui.add_enabled_ui(false, |ui| {
                theme::settings_ui(ui, &mut theme);
                language_row(ui, &mut language);
            });
            return None;
        };

        if self.shown != Some(scope.tab.scope) {
            // Another tab: what was edited or held was the old one's.
            self.shown = Some(scope.tab.scope);
            self.editing = false;
            self.held = false;
        }
        notes(ui, scope.tab.scope);
        error_line(ui, &mut self.error);

        let mut change = AppearanceChange {
            scope: scope.tab.scope,
            theme: None,
            language: None,
        };
        if !self.editing && !self.held {
            self.working = scope.theme.clone();
        }
        let before = self.working.clone();
        let done = ui
            .add_enabled_ui(!cx.busy, |ui| theme::settings_ui(ui, &mut self.working))
            .inner;
        if done {
            self.editing = false;
            if self.working != scope.theme {
                self.held = true;
                change.theme = Some(self.working.clone());
            }
        } else if self.working != before {
            self.editing = true;
        } else if !ui.input(|i| i.pointer.any_down()) {
            self.editing = false;
        }

        ui.add_space(8.0);
        if !self.held {
            self.language = scope.language.clone();
        }
        if let Some(why) = &cx.language_unavailable {
            ui.weak(why);
        }
        let chosen = ui
            .add_enabled_ui(!cx.busy && cx.language_unavailable.is_none(), |ui| {
                language_row(ui, &mut self.language)
            })
            .inner;
        if chosen && self.language != scope.language {
            self.held = true;
            change.language = Some(self.language.clone());
        }

        (change.theme.is_some() || change.language.is_some()).then_some(change)
    }

    /// The host has its values again, a change stored or not: show them.
    pub fn reloaded(&mut self) {
        self.held = false;
    }

    /// The last change could not be stored, for `why`: say so, and show the host's values
    /// again.
    pub fn failed(&mut self, why: impl Into<String>) {
        self.editing = false;
        self.held = false;
        self.error = Some(why.into());
    }
}

/// The language picker's row, aligned as the theme's rows; `true` on a choice.
fn language_row(ui: &mut egui::Ui, language: &mut LanguageSettings) -> bool {
    egui::Grid::new("corundum_language_row")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr!(ui, "corundum-language"));
            let chosen = i18n::settings_ui(ui, language);
            ui.end_row();
            chosen
        })
        .inner
}

/// The red line saying why a change failed, with a way to dismiss it.
fn error_line(ui: &mut egui::Ui, error: &mut Option<String>) {
    let Some(text) = error.clone() else {
        return;
    };
    // Wrapped: a phone is narrower than most messages.
    ui.horizontal_wrapped(|ui| {
        if ui.small_button("×").clicked() {
            *error = None;
        }
        let colour = theme::current(ui.ctx()).text(Swatch::DANGER);
        ui.add(egui::Label::new(egui::RichText::new(text).color(colour)).wrap());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;
    use crate::test_support::{render, render_in};
    use crate::theme::Contrast;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Scope {
        Client,
        Server,
    }

    fn scope(scope: Scope, contrast: Contrast) -> AppearanceScope<Scope> {
        AppearanceScope {
            tab: ScopeTab::new(scope, format!("{scope:?}")),
            theme: ThemeSettings {
                contrast,
                ..Default::default()
            },
            language: LanguageSettings::default(),
        }
    }

    fn scopes() -> Vec<AppearanceScope<Scope>> {
        vec![
            scope(Scope::Client, Contrast::High),
            scope(Scope::Server, Contrast::Low),
        ]
    }

    #[test]
    fn it_opens_on_the_first_tab_and_shows_its_values() {
        let scopes = scopes();
        let mut page = AppearancePage::new();
        let mut noted = None;
        render([600.0, 600.0], |ui| {
            let change = page.ui(ui, &AppearanceCtx::new(&scopes), |_, s| noted = Some(s));
            assert_eq!(change, None);
        });
        assert_eq!(page.tab(), Some(Scope::Client));
        assert_eq!(noted, Some(Scope::Client));
        assert_eq!(page.working.contrast, Contrast::High);
    }

    #[test]
    fn with_tab_opens_on_that_tab() {
        let scopes = scopes();
        let mut page = AppearancePage::new().with_tab(Scope::Server);
        render([600.0, 600.0], |ui| {
            page.ui(ui, &AppearanceCtx::new(&scopes), |_, _| {});
        });
        assert_eq!(page.working.contrast, Contrast::Low);
    }

    #[test]
    fn a_disabled_tab_yields() {
        let mut scopes = scopes();
        scopes[1].tab.enabled = false;
        let mut page = AppearancePage::new().with_tab(Scope::Server);
        render([600.0, 600.0], |ui| {
            page.ui(ui, &AppearanceCtx::new(&scopes), |_, _| {});
        });
        assert_eq!(page.tab(), Some(Scope::Client));
    }

    #[test]
    fn unavailable_shows_why_and_gives_nothing() {
        let scopes = scopes();
        let mut page = AppearancePage::new();
        let cx = AppearanceCtx {
            unavailable: Some("the server is down".into()),
            ..AppearanceCtx::new(&scopes)
        };
        let mut noted = false;
        render([600.0, 600.0], |ui| {
            assert_eq!(page.ui(ui, &cx, |_, _| noted = true), None);
        });
        assert!(!noted, "no notes without values");
        let none: Vec<AppearanceScope<Scope>> = Vec::new();
        render([600.0, 600.0], |ui| {
            assert_eq!(page.ui(ui, &AppearanceCtx::new(&none), |_, _| {}), None);
        });
    }

    #[test]
    fn a_held_change_stays_until_reloaded() {
        let scopes = scopes();
        let mut page = AppearancePage::new();
        render([600.0, 600.0], |ui| {
            page.ui(ui, &AppearanceCtx::new(&scopes), |_, _| {});
        });
        // A change given, the host not yet reloaded: its values are the old ones.
        page.working.contrast = Contrast::Medium;
        page.held = true;
        render([600.0, 600.0], |ui| {
            page.ui(ui, &AppearanceCtx::new(&scopes), |_, _| {});
        });
        assert_eq!(page.working.contrast, Contrast::Medium);
        page.reloaded();
        render([600.0, 600.0], |ui| {
            page.ui(ui, &AppearanceCtx::new(&scopes), |_, _| {});
        });
        assert_eq!(page.working.contrast, Contrast::High);
    }

    #[test]
    fn a_failure_shows_the_hosts_values_again() {
        let scopes = scopes();
        let mut page = AppearancePage::new();
        page.working.contrast = Contrast::Medium;
        page.held = true;
        page.failed("rejected");
        render([600.0, 600.0], |ui| {
            page.ui(ui, &AppearanceCtx::new(&scopes), |_, _| {});
        });
        assert_eq!(page.working.contrast, Contrast::High);
        assert_eq!(page.error.as_deref(), Some("rejected"));
    }

    #[test]
    fn switching_tabs_drops_a_held_change() {
        let scopes = scopes();
        let mut page = AppearancePage::new();
        render([600.0, 600.0], |ui| {
            page.ui(ui, &AppearanceCtx::new(&scopes), |_, _| {});
        });
        page.held = true;
        page.tab = Some(Scope::Server);
        render([600.0, 600.0], |ui| {
            page.ui(ui, &AppearanceCtx::new(&scopes), |_, _| {});
        });
        assert_eq!(page.working.contrast, Contrast::Low);
    }

    #[test]
    fn the_page_follows_the_language() {
        let ctx = egui::Context::default();
        crate::i18n::install(
            &ctx,
            &LanguageSettings {
                language: Language::Explicit("ja".into()),
            },
        );
        let scopes = scopes();
        let mut page = AppearancePage::new();
        render_in(&ctx, [600.0, 600.0], |ui| {
            page.ui(ui, &AppearanceCtx::new(&scopes), |_, _| {});
        });
        assert_eq!(
            crate::i18n::tr(&ctx, "corundum-settings-appearance"),
            "外観"
        );
    }
}
