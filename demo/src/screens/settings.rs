//! The Settings screen, as an app puts its pages into corundum's: one of its own, the
//! Appearance page over two scopes (a server's settings and this device's own, as a client
//! of a server keeps them), and About.

use std::cell::RefCell;
use std::rc::Rc;

use corundum_framework::gui::egui;
use corundum_framework::gui::i18n::{self, LanguageSettings};
use corundum_framework::gui::settings::{
    AppearanceCtx, AppearancePage, AppearanceScope, ScopeTab, Settings, SettingsGroup,
    SettingsSection,
};
use corundum_framework::gui::theme::{self, ThemeSettings};
use corundum_framework::gui::tr;

/// Where the demo keeps a look: as a client of a server would.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scope {
    /// The server's, for every device.
    Server,
    /// This device's own, over the server's.
    Device,
}

/// The theme and the language as the demo stores them: the server's, and this device's own
/// where it has them. A real app saves these; the demo keeps them while it runs.
#[derive(Default)]
pub(crate) struct Looks {
    server_theme: ThemeSettings,
    server_language: LanguageSettings,
    device_theme: Option<ThemeSettings>,
    device_language: Option<LanguageSettings>,
    /// The look in effect changed: the Theme and Language screens take it.
    pub(crate) changed: bool,
}

impl Looks {
    /// The theme and the language in effect: the device's own over the server's.
    pub(crate) fn effective(&self) -> (ThemeSettings, LanguageSettings) {
        (
            self.device_theme
                .clone()
                .unwrap_or_else(|| self.server_theme.clone()),
            self.device_language
                .clone()
                .unwrap_or_else(|| self.server_language.clone()),
        )
    }

    /// Whether this device sets anything itself.
    fn own(&self) -> bool {
        self.device_theme.is_some() || self.device_language.is_some()
    }
}

/// The demo's Settings: its own page, General (Appearance), and About.
pub(crate) fn settings(looks: Rc<RefCell<Looks>>) -> Settings {
    Settings::new()
        .with_group(
            SettingsGroup::new(vec![Box::new(Sample::default())]).titled("demo-settings-demo"),
        )
        .with_group(
            SettingsGroup::new(vec![Box::new(Appearance {
                looks,
                page: AppearancePage::new().with_tab(Scope::Server),
            })])
            .titled("demo-settings-general"),
        )
        .with_group(SettingsGroup::new(vec![Box::new(About)]))
}

/// A page of the app's own: a name, applied with a button, the entry marked until then.
#[derive(Default)]
struct Sample {
    saved: String,
    draft: String,
}

impl SettingsSection for Sample {
    fn label(&self, ctx: &egui::Context) -> String {
        tr!(ctx, "demo-settings-sample")
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading(tr!(ui, "demo-settings-sample"));
        ui.separator();
        ui.label(tr!(ui, "demo-settings-sample-intro"));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(tr!(ui, "demo-settings-sample-name"));
            ui.text_edit_singleline(&mut self.draft);
        });
        ui.horizontal(|ui| {
            let dirty = self.dirty();
            if ui
                .add_enabled(dirty, egui::Button::new(tr!(ui, "demo-settings-revert")))
                .clicked()
            {
                self.draft.clone_from(&self.saved);
            }
            if ui
                .add_enabled(dirty, egui::Button::new(tr!(ui, "demo-settings-apply")))
                .clicked()
            {
                self.saved.clone_from(&self.draft);
            }
        });
        ui.weak(tr!(
            ui,
            "demo-settings-sample-saved",
            name = self.saved.as_str()
        ));
    }

    fn dirty(&self) -> bool {
        self.draft != self.saved
    }
}

/// The Appearance page over the server's look and this device's own.
struct Appearance {
    looks: Rc<RefCell<Looks>>,
    page: AppearancePage<Scope>,
}

impl SettingsSection for Appearance {
    fn label(&self, ctx: &egui::Context) -> String {
        tr!(ctx, "corundum-settings-appearance")
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let (scopes, own) = {
            let looks = self.looks.borrow();
            let (theme, language) = looks.effective();
            let scopes = [
                AppearanceScope {
                    tab: ScopeTab::new(Scope::Server, tr!(ui, "demo-settings-scope-server")),
                    theme: looks.server_theme.clone(),
                    language: looks.server_language.clone(),
                },
                // The device's tab shows what is in effect, which its own values would change.
                AppearanceScope {
                    tab: ScopeTab::new(Scope::Device, tr!(ui, "demo-settings-scope-device")),
                    theme,
                    language,
                },
            ];
            (scopes, looks.own())
        };
        let mut use_server = false;
        let change = self
            .page
            .ui(ui, &AppearanceCtx::new(&scopes), |ui, scope| match scope {
                Scope::Server if own => {
                    ui.weak(tr!(ui, "demo-settings-device-wins"));
                }
                Scope::Server => {}
                Scope::Device if own => {
                    ui.weak(tr!(ui, "demo-settings-own"));
                    use_server = ui.button(tr!(ui, "demo-settings-use-server")).clicked();
                }
                Scope::Device => {
                    ui.weak(tr!(ui, "demo-settings-from-server"));
                }
            });

        let mut looks = self.looks.borrow_mut();
        let mut stored = false;
        if use_server {
            looks.device_theme = None;
            looks.device_language = None;
            stored = true;
        }
        if let Some(change) = change {
            match change.scope {
                Scope::Server => {
                    if let Some(theme) = change.theme {
                        looks.server_theme = theme;
                    }
                    if let Some(language) = change.language {
                        looks.server_language = language;
                    }
                }
                Scope::Device => {
                    if let Some(theme) = change.theme {
                        looks.device_theme = Some(theme);
                    }
                    if let Some(language) = change.language {
                        looks.device_language = Some(language);
                    }
                }
            }
            stored = true;
        }
        if stored {
            // Stored at once: install the look in effect, which on the server's tab is the
            // device's own where it has one, and show the stored values.
            let (theme, language) = looks.effective();
            theme::install(ui.ctx(), &theme);
            i18n::install(ui.ctx(), &language);
            looks.changed = true;
            self.page.reloaded();
        }
    }
}

/// The app's name and version.
struct About;

impl SettingsSection for About {
    fn label(&self, ctx: &egui::Context) -> String {
        tr!(ctx, "demo-settings-about")
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("corundum-framework demo");
        ui.label(tr!(
            ui,
            "demo-settings-version",
            version = env!("CARGO_PKG_VERSION")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_devices_own_look_wins() {
        let mut looks = Looks::default();
        assert_eq!(looks.effective().0, ThemeSettings::default());
        looks.server_theme.contrast = theme::Contrast::High;
        assert_eq!(looks.effective().0.contrast, theme::Contrast::High);
        looks.device_theme = Some(ThemeSettings {
            contrast: theme::Contrast::Low,
            ..Default::default()
        });
        assert_eq!(looks.effective().0.contrast, theme::Contrast::Low);
        assert!(looks.own());
    }
}
