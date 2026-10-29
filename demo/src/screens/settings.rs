//! The Settings screen, as an app puts its pages into corundum's: one of its own, the
//! Appearance page, and About.
//!
//! The demo keeps no settings of its own beyond the look, so the look corundum keeps on the
//! device (`look`, in `store`'s file) is where the Appearance page stores it: a change is
//! applied, saved, and there again at the next start.

use corundum_framework::gui::egui;
use corundum_framework::gui::look::{self, Look};
use corundum_framework::gui::settings::{
    AppearanceCtx, AppearancePage, AppearanceScope, RailPage, ScopeTab, Settings, SettingsGroup,
    SettingsSection,
};
use corundum_framework::gui::store;
use corundum_framework::gui::tr;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use corundum_framework::gui::update::{AfterUpdate, UpdateTarget, UpdatesPanel, semver};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use corundum_framework::update::ReleaseSource;

/// The one place the demo keeps the look: this device.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Device;

/// The demo's Settings: its own page, General (Appearance, Navigation), and About.
pub(crate) fn settings() -> Settings {
    Settings::new()
        .with_group(
            SettingsGroup::new(vec![Box::new(Sample::default())]).titled("demo-settings-demo"),
        )
        .with_group(
            SettingsGroup::new(vec![
                Box::new(Appearance {
                    page: AppearancePage::new(),
                }),
                Box::new(RailPage::new()),
            ])
            .titled("demo-settings-general"),
        )
        .with_group(SettingsGroup::new(about_pages()))
}

/// About, and with the `velopack` feature the spike's page beside it.
fn about_pages() -> Vec<Box<dyn SettingsSection>> {
    #[allow(unused_mut)]
    let mut pages: Vec<Box<dyn SettingsSection>> = vec![Box::new(About::new())];
    #[cfg(feature = "velopack")]
    pages.push(Box::new(crate::velopack::Page::new()));
    pages
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
    match store::store(ui.ctx()) {
        Some(store) => ui.weak(tr!(
            ui,
            "demo-settings-look-file",
            path = store.path().display().to_string()
        )),
        None => ui.weak(tr!(ui, "demo-settings-look-memory")),
    };
}

/// The app's name and version, its entry in the app menu, and the update row for the demo
/// itself.
///
/// The update row is not on Android or iOS, where the app is never updated this way (and the
/// panel's HTTP client needs a C toolchain the phones' builds do not have).
struct About {
    launcher: corundum_framework::gui::settings::LauncherRow,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    updates: UpdatesPanel,
}

impl About {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    fn new() -> About {
        About {
            launcher: corundum_framework::gui::settings::LauncherRow::new(crate::LAUNCHER),
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn new() -> About {
        // The demo is not released as a binary, so the source names no asset: the row
        // shows the newest release and where to download it.
        let target = UpdateTarget {
            label: "corundum-framework demo".to_owned(),
            current: semver::Version::parse(env!("CARGO_PKG_VERSION")).ok(),
            source: ReleaseSource::github("sapphire-turtle", "corundum-framework").tag_prefix("v"),
            // An empty path is harmless: the source has no asset, so nothing is ever replaced.
            exe: std::env::current_exe().unwrap_or_default(),
            after: AfterUpdate::Relaunch,
        };
        About {
            launcher: corundum_framework::gui::settings::LauncherRow::new(crate::LAUNCHER),
            updates: UpdatesPanel::new(vec![target.into()], "corundum-framework-demo"),
        }
    }
}

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
        self.launcher.ui(ui);
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            ui.add_space(8.0);
            self.updates.ui(ui);
        }
    }
}
