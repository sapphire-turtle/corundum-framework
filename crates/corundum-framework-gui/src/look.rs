//! The look in effect — the theme and the language — kept on the device.
//!
//! [`apply`] installs a [`Look`] in the egui context and, when the context has a
//! [`Store`](crate::store::Store), saves it there. The window installs the saved look
//! before the app's first frame
//! ([`WindowOptions::remember`](crate::window::WindowOptions::remember)), so the app opens in
//! it rather than in the defaults.
//!
//! The look saved is the one in effect, after the app has merged whatever it keeps the
//! settings in: a workgroup's and a device's, a server's, one per workspace. corundum does
//! not know those layers and does not need to: an app that has them gives [`apply`] the
//! result each time it changes, and the store is a copy of it for the next start. An app
//! with no settings of its own beyond the look (the demo, say) gives it what the person
//! chose, and the store is where that is kept.

use serde::{Deserialize, Serialize};

use crate::i18n::{self, LanguageSettings};
use crate::theme::{self, ThemeSettings};

/// The theme and the language in effect.
#[derive(Clone, Default, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub theme: ThemeSettings,
    pub language: LanguageSettings,
}

/// Install `look` and, when it is not the look last applied, save it to the context's
/// store. A failure to save is logged: the look still shows.
pub fn apply(ctx: &egui::Context, look: &Look) {
    install(ctx, look);
    crate::store::update(ctx, |state| state.look = Some(look.clone()));
}

/// The look last applied (or installed from the store), or `None` before any.
pub fn current(ctx: &egui::Context) -> Option<Look> {
    crate::store::state(ctx).look
}

pub(crate) fn install(ctx: &egui::Context, look: &Look) {
    theme::install(ctx, &look.theme);
    i18n::install(ctx, &look.language);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{self, Store};

    fn look() -> Look {
        Look {
            theme: ThemeSettings {
                contrast: theme::Contrast::High,
                ..Default::default()
            },
            language: LanguageSettings {
                language: i18n::Language::Explicit("ja".into()),
            },
        }
    }

    #[test]
    fn apply_installs_and_saves() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join(store::FILE_NAME));
        let ctx = egui::Context::default();
        store::attach(&ctx, store.clone());
        apply(&ctx, &look());
        assert_eq!(current(&ctx), Some(look()));
        assert_eq!(i18n::current(&ctx).to_string(), "ja");
        assert_eq!(store.load().and_then(|s| s.look), Some(look()));
    }

    #[test]
    fn without_a_store_apply_only_installs() {
        let ctx = egui::Context::default();
        apply(&ctx, &look());
        assert_eq!(current(&ctx), Some(look()));
        assert_eq!(i18n::current(&ctx).to_string(), "ja");
    }
}
