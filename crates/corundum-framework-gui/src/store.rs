//! What corundum keeps on the device between starts — for now the [look](crate::look) in
//! effect, the [rail's order](crate::RailOrder), whether the shell's side column is on and
//! whether the app is kept in the OS's app menu — and the file it is kept in.
//!
//! [`attach`] gives the egui context a [`Store`] and puts back what is saved there: the
//! window does it before the app's first frame
//! ([`WindowOptions::remember`](crate::window::WindowOptions::remember)), so the app opens in
//! its look and with its rail as the person left them. From then on [`look::apply`] and the
//! [`Shell`](crate::Shell)'s menu save each change.
//!
//! The file is TOML, by default [`FILE_NAME`] in the app's config directory. Named for
//! corundum, it is told apart from the app's own files there: the app may keep the same
//! settings elsewhere too (synced, per workgroup), and this is corundum's copy. What
//! corundum comes to keep beyond the UI goes into it as a further table. The person can read
//! and edit it by hand.
//!
//! ```toml
//! [look.theme]
//! preference = "Dark"
//! contrast = "High"
//!
//! [look.language]
//! language = { Explicit = "ja" }
//!
//! [rail]
//! order = ["demo-screen-theme", "demo-screen-overview", "demo-screen-fonts"]
//! hidden = ["demo-screen-fonts"]
//!
//! [side]
//! column = false
//! ```
//!
//! An app whose window registers it in the OS's app menu
//! ([`WindowOptions::launcher`](crate::window::WindowOptions::launcher)) also has
//! `launcher = false` there once the person has taken the entry out.
//!
//! [`look::apply`]: crate::look::apply

use std::io;
use std::path::{Path, PathBuf};

use corundum_framework_platform::dirs::AppDirs;
use serde::{Deserialize, Serialize};

use crate::RailOrder;
use crate::look::Look;

/// The file a [`Store`] keeps corundum's tables in, under the app's config directory.
pub const FILE_NAME: &str = "corundum.toml";

/// What the device keeps: each part `None` until it is first set.
#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Stored {
    /// The theme and the language in effect.
    pub look: Option<Look>,
    /// The order of the rail's items, and which of them it shows.
    pub rail: Option<RailOrder>,
    /// The shell's side pane.
    pub side: Option<SidePane>,
    /// Whether the app's entry is kept in the OS's app menu: `false` once the person has
    /// taken it out ([`LauncherRow`](crate::settings::LauncherRow)), so that the window does
    /// not write it back. `None`, as in a file written before this was kept, is `true`.
    pub launcher: Option<bool>,
    /// Whether the [first-run flow](crate::first_run::FirstRun) has been finished: `None`
    /// until it is, and again after [`FirstRun::reset`](crate::first_run::FirstRun::reset).
    pub first_run: Option<bool>,
    /// Whether the person has been asked about the app's background services: set once the
    /// question has been answered either way, so it is not asked again.
    pub services_asked: Option<bool>,
}

/// Whether the window registers the app's entry in the OS's app menu as it opens: unless
/// the person took it out.
// Read by the window, which is the `eframe` feature's.
#[cfg_attr(not(feature = "eframe"), allow(dead_code))]
pub(crate) fn wants_launcher(stored: &Stored) -> bool {
    stored.launcher != Some(false)
}

/// What the device keeps of the [`Shell`](crate::Shell)'s side pane.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SidePane {
    /// Whether the right column is on.
    pub column: bool,
}

impl Default for SidePane {
    fn default() -> SidePane {
        SidePane { column: true }
    }
}

/// Where the device keeps the [`Stored`]: a TOML file, by default [`FILE_NAME`] in the app's
/// config directory.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    /// The store in `dirs`' config directory.
    pub fn new(dirs: &AppDirs) -> Store {
        Store::at(dirs.config().join(FILE_NAME))
    }

    /// The store in `app_name`'s config directory, or `None` where the platform has none
    /// (an Android process without a context, say).
    pub fn for_app(app_name: &str) -> Option<Store> {
        AppDirs::new(app_name).map(|dirs| Store::new(&dirs))
    }

    /// The store in the file at `path`.
    pub fn at(path: impl Into<PathBuf>) -> Store {
        Store { path: path.into() }
    }

    /// The file the tables are kept in.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What is saved, or `None` when nothing is saved yet or the file cannot be read. A file
    /// that is not a [`Stored`] is logged and taken as none: the app opens in the defaults.
    pub fn load(&self) -> Option<Stored> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return None,
            Err(err) => {
                tracing::warn!(
                    "could not read corundum's file {}: {err}",
                    self.path.display()
                );
                return None;
            }
        };
        toml::from_str(&text)
            .inspect_err(|err| {
                tracing::warn!(
                    "corundum's file {} is not readable: {err}",
                    self.path.display()
                );
            })
            .ok()
    }

    /// Save `state`, creating the directory if needed. The file is replaced whole, so a crash
    /// midway leaves what was saved before.
    pub fn save(&self, state: &Stored) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(state).map_err(io::Error::other)?;
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.path)
    }
}

/// What the context holds: what was last set, and the store it is saved to.
#[derive(Clone, Default)]
struct Held {
    state: Stored,
    store: Option<Store>,
}

fn held_id() -> egui::Id {
    egui::Id::unique("corundum_ui_store")
}

fn held(ctx: &egui::Context) -> Held {
    ctx.data(|d| d.get_temp::<Held>(held_id()))
        .unwrap_or_default()
}

fn set_held(ctx: &egui::Context, held: Held) {
    ctx.data_mut(|d| d.insert_temp(held_id(), held));
}

/// Keep corundum's tables in `store` from now on, and put back what is saved there: the look is
/// installed, and a [`Shell`](crate::Shell) takes the rail's order on its first frame. `true`
/// when something was saved. The window calls this before the app's first frame
/// ([`WindowOptions::remember`](crate::window::WindowOptions::remember)).
pub fn attach(ctx: &egui::Context, store: Store) -> bool {
    let saved = store.load();
    let found = saved.is_some();
    let state = saved.unwrap_or_default();
    if let Some(look) = &state.look {
        crate::look::install(ctx, look);
    }
    set_held(
        ctx,
        Held {
            state,
            store: Some(store),
        },
    );
    found
}

/// What was last set (or put back from the store).
pub fn state(ctx: &egui::Context) -> Stored {
    held(ctx).state
}

/// The store the context saves to, if one is attached.
pub fn store(ctx: &egui::Context) -> Option<Store> {
    held(ctx).store
}

/// Change what is kept with `change` and, when that changed it, save it to the context's store. A
/// failure to save is logged: the change still holds while the app runs.
pub(crate) fn update(ctx: &egui::Context, change: impl FnOnce(&mut Stored)) {
    let mut held = held(ctx);
    let before = held.state.clone();
    change(&mut held.state);
    if held.state == before {
        return;
    }
    if let Some(store) = &held.store
        && let Err(err) = store.save(&held.state)
    {
        tracing::warn!(
            "could not save corundum's file {}: {err}",
            store.path().display()
        );
    }
    set_held(ctx, held);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, LanguageSettings};
    use crate::theme::{Contrast, Swatch, ThemeSettings, Tint};

    fn look() -> Look {
        Look {
            theme: ThemeSettings {
                preference: egui::ThemePreference::Dark,
                contrast: Contrast::High,
                accent: Some(Tint::Swatch(Swatch::Green)),
            },
            language: LanguageSettings {
                language: Language::Explicit("ja".into()),
            },
        }
    }

    fn rail() -> RailOrder {
        RailOrder {
            order: vec!["b".into(), "a".into()],
            hidden: vec!["a".into()],
        }
    }

    fn store(dir: &tempfile::TempDir) -> Store {
        Store::at(dir.path().join("app").join(FILE_NAME))
    }

    #[test]
    fn what_is_saved_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        assert_eq!(store.load(), None, "nothing saved yet");
        let state = Stored {
            look: Some(look()),
            rail: Some(rail()),
            side: None,
            launcher: Some(false),
            ..Default::default()
        };
        store.save(&state).unwrap();
        assert_eq!(store.load(), Some(state));
        assert!(!dir.path().join("app/corundum.toml.tmp").exists());
    }

    #[test]
    fn every_accent_and_language_is_written_as_toml() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        let accents = [
            None,
            Some(Tint::Swatch(Swatch::Red)),
            Some(Tint::Custom {
                chroma: 0.125,
                hue: 270.0,
            }),
        ];
        let languages = [Language::System, Language::Explicit("en".into())];
        for accent in accents {
            for language in languages.clone() {
                let state = Stored {
                    look: Some(Look {
                        theme: ThemeSettings {
                            accent,
                            ..Default::default()
                        },
                        language: LanguageSettings { language },
                    }),
                    rail: None,
                    side: None,
                    launcher: None,
                    ..Default::default()
                };
                store.save(&state).unwrap();
                assert_eq!(store.load(), Some(state));
            }
        }
    }

    #[test]
    fn a_hand_written_file_reads() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(
            store.path(),
            "[look.theme]\ncontrast = \"High\"\n\n[look.language]\nlanguage = { Explicit = \"ja\" }\n\n[rail]\norder = [\"b\", \"a\"]\n",
        )
        .unwrap();
        let state = store.load().unwrap();
        let look = state.look.unwrap();
        assert_eq!(look.theme.contrast, Contrast::High);
        assert_eq!(look.language.language, Language::Explicit("ja".into()));
        assert_eq!(state.rail.unwrap().order, ["b", "a"]);
    }

    #[test]
    fn a_file_without_the_launcher_wants_it() {
        // A file written before the launcher was kept has no key for it.
        let stored: Stored = toml::from_str("").unwrap();
        assert!(wants_launcher(&stored));
        let stored: Stored = toml::from_str("launcher = true").unwrap();
        assert!(wants_launcher(&stored));
        let stored: Stored = toml::from_str("launcher = false").unwrap();
        assert!(!wants_launcher(&stored));
    }

    #[test]
    fn a_broken_file_is_nothing_saved() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(store.path(), "[look\n").unwrap();
        assert_eq!(store.load(), None);
    }

    #[test]
    fn an_empty_file_is_an_empty_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(store.path(), "").unwrap();
        assert_eq!(store.load(), Some(Stored::default()));
    }

    #[test]
    fn attach_installs_the_saved_look() {
        let dir = tempfile::tempdir().unwrap();
        let state = Stored {
            look: Some(look()),
            rail: Some(rail()),
            side: None,
            launcher: None,
            first_run: None,
            services_asked: None,
        };
        store(&dir).save(&state).unwrap();
        let ctx = egui::Context::default();
        assert!(attach(&ctx, store(&dir)));
        assert_eq!(super::state(&ctx), state);
        assert!(crate::theme::is_installed(&ctx));
        assert_eq!(crate::i18n::current(&ctx).to_string(), "ja");
    }

    #[test]
    fn attach_without_anything_saved_installs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        assert!(!attach(&ctx, store(&dir)));
        assert_eq!(super::state(&ctx), Stored::default());
        assert!(!crate::theme::is_installed(&ctx));
        assert_eq!(super::store(&ctx), Some(store(&dir)));
    }

    #[test]
    fn an_update_saves_what_changed_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        attach(&ctx, store(&dir));
        update(&ctx, |s| s.look = Some(look()));
        update(&ctx, |s| s.rail = Some(rail()));
        let saved = store(&dir).load().unwrap();
        assert_eq!(saved.look, Some(look()));
        assert_eq!(saved.rail, Some(rail()));
    }

    #[test]
    fn an_update_that_changes_nothing_is_not_saved() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        attach(&ctx, store(&dir));
        update(&ctx, |s| s.rail = Some(rail()));
        std::fs::remove_file(store(&dir).path()).unwrap();
        update(&ctx, |s| s.rail = Some(rail()));
        assert!(!store(&dir).path().exists());
    }

    #[test]
    fn an_unwritable_store_still_holds_the_change() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the directory should be.
        std::fs::write(dir.path().join("app"), "").unwrap();
        let ctx = egui::Context::default();
        attach(&ctx, store(&dir));
        update(&ctx, |s| s.rail = Some(rail()));
        assert_eq!(super::state(&ctx).rail, Some(rail()));
    }
}
