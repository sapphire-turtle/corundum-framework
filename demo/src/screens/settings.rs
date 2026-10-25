//! The Settings screen, as an app puts its pages into corundum's: one of its own, the
//! Appearance page, and About.
//!
//! The demo keeps no settings of its own beyond the look, so the look corundum keeps on the
//! device (`look`) is where the Appearance page stores it: a change is applied, saved, and
//! there again at the next start.

use corundum_framework::gui::egui;
use corundum_framework::gui::look::{self, Look};
use corundum_framework::gui::settings::{
    AppearanceCtx, AppearancePage, AppearanceScope, ScopeTab, Settings, SettingsGroup,
    SettingsSection,
};
use corundum_framework::gui::tr;

/// The one place the demo keeps the look: this device.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Device;

/// The demo's Settings: its own page, General (Appearance), and About.
pub(crate) fn settings() -> Settings {
    Settings::new()
        .with_group(
            SettingsGroup::new(vec![Box::new(Sample::default())]).titled("demo-settings-demo"),
        )
        .with_group(
            SettingsGroup::new(vec![Box::new(Appearance {
                page: AppearancePage::new(),
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

/// The Appearance page over the look the device keeps.
struct Appearance {
    page: AppearancePage<Device>,
}

impl SettingsSection for Appearance {
    fn label(&self, ctx: &egui::Context) -> String {
        tr!(ctx, "corundum-settings-appearance")
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let current = look::current(ui.ctx()).unwrap_or_default();
        let scopes = [AppearanceScope {
            tab: ScopeTab::new(Device, tr!(ui, "demo-settings-scope-device")),
            theme: current.theme.clone(),
            language: current.language.clone(),
        }];
        let change = self
            .page
            .ui(ui, &AppearanceCtx::new(&scopes), |ui, Device| {
                file_line(ui);
            });
        if let Some(change) = change {
            let look = Look {
                theme: change.theme.unwrap_or(current.theme),
                language: change.language.unwrap_or(current.language),
            };
            // Saved at once: show what is stored.
            look::apply(ui.ctx(), &look);
            self.page.reloaded();
        }
    }
}

/// Where the look is saved, or that it is kept only while the demo runs.
fn file_line(ui: &mut egui::Ui) {
    match look::store(ui.ctx()) {
        Some(store) => ui.weak(tr!(
            ui,
            "demo-settings-look-file",
            path = store.path().display().to_string()
        )),
        None => ui.weak(tr!(ui, "demo-settings-look-memory")),
    };
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
