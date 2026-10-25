//! The look in effect — the theme and the language — and where the device keeps it.
//!
//! [`apply`] installs a [`Look`] in the egui context and, when the context has a
//! [`LookStore`], saves it there. The window installs the saved look before the app's first
//! frame ([`WindowOptions::remember_look`](crate::window::WindowOptions::remember_look)),
//! so the app opens in it rather than in the defaults.
//!
//! The look saved is the one in effect, after the app has merged whatever it keeps the
//! settings in: a workgroup's and a device's, a server's, one per workspace. corundum does
//! not know those layers and does not need to: an app that has them gives [`apply`] the
//! result each time it changes, and the store is a copy of it for the next start. An app
//! with no settings of its own beyond the look (the demo, say) gives it what the person
//! chose, and the store is where that is kept.

use std::io;
use std::path::{Path, PathBuf};

use corundum_framework_platform::dirs::AppDirs;
use serde::{Deserialize, Serialize};

use crate::i18n::{self, LanguageSettings};
use crate::theme::{self, ThemeSettings};

/// The file a [`LookStore`] keeps the look in, under the app's config directory.
pub const FILE_NAME: &str = "look.json";

/// The theme and the language in effect.
#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub theme: ThemeSettings,
    pub language: LanguageSettings,
}

/// Where the device keeps the [`Look`]: a JSON file, by default [`FILE_NAME`] in the app's
/// config directory.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LookStore {
    path: PathBuf,
}

impl LookStore {
    /// The store in `dirs`' config directory.
    pub fn new(dirs: &AppDirs) -> LookStore {
        LookStore::at(dirs.config().join(FILE_NAME))
    }

    /// The store in `app_name`'s config directory, or `None` where the platform has none
    /// (an Android process without a context, say).
    pub fn for_app(app_name: &str) -> Option<LookStore> {
        AppDirs::new(app_name).map(|dirs| LookStore::new(&dirs))
    }

    /// The store in the file at `path`.
    pub fn at(path: impl Into<PathBuf>) -> LookStore {
        LookStore { path: path.into() }
    }

    /// The file the look is kept in.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The look saved, or `None` when nothing is saved yet or the file cannot be read. A
    /// file that is not a look is logged and taken as none: the app opens in the defaults.
    pub fn load(&self) -> Option<Look> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return None,
            Err(err) => {
                tracing::warn!(
                    "could not read the look from {}: {err}",
                    self.path.display()
                );
                return None;
            }
        };
        serde_json::from_str(&text)
            .inspect_err(|err| {
                tracing::warn!("the look in {} is not readable: {err}", self.path.display());
            })
            .ok()
    }

    /// Save `look`, creating the directory if needed. The file is replaced whole, so a
    /// crash midway leaves the look saved before.
    pub fn save(&self, look: &Look) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(look).map_err(io::Error::other)?;
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.path)
    }
}

/// What the context holds: the look last applied and the store it is saved to.
#[derive(Clone, Default)]
struct State {
    current: Option<Look>,
    store: Option<LookStore>,
}

fn state_id() -> egui::Id {
    egui::Id::unique("corundum_look")
}

fn state(ctx: &egui::Context) -> State {
    ctx.data(|d| d.get_temp::<State>(state_id()))
        .unwrap_or_default()
}

fn set_state(ctx: &egui::Context, state: State) {
    ctx.data_mut(|d| d.insert_temp(state_id(), state));
}

/// Keep the look in `store` from now on, and install the one saved there, if any. `true`
/// when a saved look was installed. The window calls this before the app's first frame
/// ([`WindowOptions::remember_look`](crate::window::WindowOptions::remember_look)).
pub fn attach(ctx: &egui::Context, store: LookStore) -> bool {
    let saved = store.load();
    let mut st = state(ctx);
    st.store = Some(store);
    if let Some(look) = &saved {
        install(ctx, look);
        st.current = Some(look.clone());
    }
    set_state(ctx, st);
    saved.is_some()
}

/// Install `look` and, when it is not the look last applied, save it to the context's
/// store. A failure to save is logged: the look still shows.
pub fn apply(ctx: &egui::Context, look: &Look) {
    install(ctx, look);
    let mut st = state(ctx);
    if st.current.as_ref() == Some(look) {
        return;
    }
    st.current = Some(look.clone());
    if let Some(store) = &st.store
        && let Err(err) = store.save(look)
    {
        tracing::warn!(
            "could not save the look to {}: {err}",
            store.path().display()
        );
    }
    set_state(ctx, st);
}

/// The look last applied (or installed from the store), or `None` before any.
pub fn current(ctx: &egui::Context) -> Option<Look> {
    state(ctx).current
}

/// The store the context saves the look to, if one is attached.
pub fn store(ctx: &egui::Context) -> Option<LookStore> {
    state(ctx).store
}

fn install(ctx: &egui::Context, look: &Look) {
    theme::install(ctx, &look.theme);
    i18n::install(ctx, &look.language);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;
    use crate::theme::Contrast;

    fn look() -> Look {
        Look {
            theme: ThemeSettings {
                contrast: Contrast::High,
                ..Default::default()
            },
            language: LanguageSettings {
                language: Language::Explicit("ja".into()),
            },
        }
    }

    fn store(dir: &tempfile::TempDir) -> LookStore {
        LookStore::at(dir.path().join("app").join(FILE_NAME))
    }

    #[test]
    fn a_saved_look_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        assert_eq!(store.load(), None, "nothing saved yet");
        store.save(&look()).unwrap();
        assert_eq!(store.load(), Some(look()));
        assert!(!dir.path().join("app/look.json.tmp").exists());
    }

    #[test]
    fn a_broken_file_is_no_look() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(store.path(), "{ not json").unwrap();
        assert_eq!(store.load(), None);
    }

    #[test]
    fn a_partial_file_takes_the_defaults_for_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(&dir);
        std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
        std::fs::write(store.path(), "{}").unwrap();
        assert_eq!(store.load(), Some(Look::default()));
    }

    #[test]
    fn attach_installs_the_saved_look() {
        let dir = tempfile::tempdir().unwrap();
        store(&dir).save(&look()).unwrap();
        let ctx = egui::Context::default();
        assert!(attach(&ctx, store(&dir)));
        assert_eq!(current(&ctx), Some(look()));
        assert!(theme::is_installed(&ctx));
        assert_eq!(i18n::current(&ctx).to_string(), "ja");
    }

    #[test]
    fn attach_without_a_saved_look_installs_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        assert!(!attach(&ctx, store(&dir)));
        assert_eq!(current(&ctx), None);
        assert!(!theme::is_installed(&ctx));
        assert_eq!(super::store(&ctx), Some(store(&dir)));
    }

    #[test]
    fn apply_installs_and_saves() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        attach(&ctx, store(&dir));
        apply(&ctx, &look());
        assert_eq!(current(&ctx), Some(look()));
        assert_eq!(i18n::current(&ctx).to_string(), "ja");
        assert_eq!(store(&dir).load(), Some(look()));
    }

    #[test]
    fn an_unchanged_look_is_not_saved_again() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        attach(&ctx, store(&dir));
        apply(&ctx, &look());
        std::fs::remove_file(store(&dir).path()).unwrap();
        apply(&ctx, &look());
        assert!(!store(&dir).path().exists());
    }

    #[test]
    fn without_a_store_apply_only_installs() {
        let ctx = egui::Context::default();
        apply(&ctx, &look());
        assert_eq!(current(&ctx), Some(look()));
        assert_eq!(i18n::current(&ctx).to_string(), "ja");
    }

    #[test]
    fn an_unwritable_store_still_installs() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the directory should be.
        std::fs::write(dir.path().join("app"), "").unwrap();
        let ctx = egui::Context::default();
        attach(&ctx, store(&dir));
        apply(&ctx, &look());
        assert_eq!(current(&ctx), Some(look()));
    }
}
