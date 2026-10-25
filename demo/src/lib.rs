//! A demo of corundum-framework's shared pieces — the shell, the theme, the fonts, the icons,
//! the translations and the platform calls — to try them on each OS apart from the apps.
//!
//! [`Demo`] is the app; [`run`] opens it in a desktop window (and on iOS, where
//! `corundum_demo_main` calls it), and `android_main` on an Android phone.

use std::sync::OnceLock;

use corundum_framework::gui::i18n::{self, LanguageSettings, Resources};
use corundum_framework::gui::look::{self, Look};
use corundum_framework::gui::settings::Settings;
use corundum_framework::gui::store::UiStore;
use corundum_framework::gui::theme::{self, ThemeSettings};
use corundum_framework::gui::{Action, Destination, Shell, egui, icons};

mod screens;

pub use screens::Screen;

/// The demo's own translations, after corundum-framework-gui's.
pub const DEMO: Resources = Resources {
    name: "corundum-framework-demo",
    files: &[
        ("en", include_str!("../locales/en/demo.ftl")),
        ("ja", include_str!("../locales/ja/demo.ftl")),
    ],
};

/// The name the app's directories are kept under.
pub const APP_NAME: &str = "corundum-framework-demo";

/// The CJK font the window installed at start, for the Fonts screen: `Some(None)` when the
/// system has none, unset when the app did not start through [`run`].
static CJK_FAMILY: OnceLock<Option<String>> = OnceLock::new();

/// The app: the shell around the screens, and the look it keeps on the device. The shell's
/// rail keeps its order there too, by itself.
pub struct Demo {
    shell: Shell<Screen>,
    settings: Settings,
    /// The theme and the language the Theme and Language screens edit.
    theme: ThemeSettings,
    language: LanguageSettings,
    /// The look as last applied: what the screens' edits and the Settings page's changes
    /// are told apart from.
    applied: Look,
    state: screens::State,
    /// The window's rects as last logged, to log them again only when they change.
    logged: Option<[egui::Rect; 3]>,
}

impl Demo {
    /// The app in `ctx`, in the look the window installed from the device, or else the
    /// default theme and the system's language.
    pub fn new(ctx: &egui::Context) -> Demo {
        i18n::register(ctx, DEMO);
        let applied = look::current(ctx).unwrap_or_default();
        // Saved too, when the device had none: the file is there from the first start.
        look::apply(ctx, &applied);
        let shell = Shell::new(vec![
            Destination::new(Screen::Overview, icons::info(), "demo-screen-overview"),
            Destination::new(Screen::Theme, icons::star(), "demo-screen-theme"),
            Destination::new(Screen::Fonts, icons::file_text(), "demo-screen-fonts"),
            Destination::new(Screen::Icons, icons::images(), "demo-screen-icons"),
            Destination::new(Screen::Language, icons::tag(), "demo-screen-language"),
            Destination::new(Screen::Detail, icons::list_music(), "demo-screen-detail"),
            Destination::new(
                Screen::Platform,
                icons::monitor_smartphone(),
                "demo-screen-platform",
            ),
        ])
        .with_actions(vec![Action::new(
            "count",
            icons::plus(),
            "demo-action-count",
        )])
        .with_footer(vec![Destination::new(
            Screen::Settings,
            icons::settings(),
            "demo-screen-settings",
        )]);
        Demo {
            shell,
            settings: screens::settings::settings(),
            theme: applied.theme.clone(),
            language: applied.language.clone(),
            applied,
            state: screens::State::new(CJK_FAMILY.get().cloned()),
            logged: None,
        }
    }

    /// The screen that is open.
    pub fn current(&self) -> Option<Screen> {
        self.shell.current()
    }

    /// Open `screen`.
    pub fn open(&mut self, screen: Screen) {
        self.shell.open(screen);
    }

    /// Draw the app into the window's root `ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.log_rects(ui);
        let Demo {
            shell,
            settings,
            theme,
            language,
            state,
            ..
        } = self;
        // The foot and the status are drawn apart from the screen: a flip of the dot shows on
        // the Overview, and a line the screen logs on the status bar, next frame.
        let mut online = state.online;
        let log = state.log.clone();
        shell.ui_with_status(
            ui,
            |ui| foot_ui(ui, &mut online),
            |ui| {
                ui.weak(log.text(ui.ctx()));
            },
            |ui, screen| screens::ui(ui, screen, state, settings, theme, language),
        );
        if log != state.log {
            ui.ctx().request_repaint();
        }
        if online != state.online {
            state.online = online;
            ui.ctx().request_repaint();
        }
        if shell.take_action() == Some("count") {
            state.clicks += 1;
            state.log = i18n::Message::new("demo-status-counted").arg("count", state.clicks);
            ui.ctx().request_repaint();
        }
        if let Some(screen) = state.open.take() {
            shell.open(screen);
        }
        self.follow_look(ui.ctx());
    }

    /// Save what the Theme and Language screens changed — a drag's once it ends — or take
    /// what the Settings page applied.
    fn follow_look(&mut self, ctx: &egui::Context) {
        let edited = Look {
            theme: self.theme.clone(),
            language: self.language.clone(),
        };
        if edited != self.applied {
            if !ctx.input(|i| i.pointer.any_down()) {
                look::apply(ctx, &edited);
                self.applied = edited;
            }
            return;
        }
        if let Some(current) = look::current(ctx)
            && current != self.applied
        {
            self.theme = current.theme.clone();
            self.language = current.language.clone();
            self.applied = current;
        }
    }
}

impl Demo {
    /// Log the screen, the area egui keeps clear of the system's bars, and the root `ui`,
    /// when they change: what the insets did, in the log a CI run keeps.
    fn log_rects(&mut self, ui: &egui::Ui) {
        let ctx = ui.ctx();
        let rects = [ctx.viewport_rect(), ctx.content_rect(), ui.max_rect()];
        if self.logged != Some(rects) {
            self.logged = Some(rects);
            let safe = ctx.input(|i| i.safe_area_insets()).0;
            tracing::info!(
                viewport = ?rects[0],
                content = ?rects[1],
                root = ?rects[2],
                ?safe,
                "corundum demo: window"
            );
        }
    }
}

/// The foot slot: a dot saying online or offline, which a click flips. Opens the Overview,
/// where the state is written out.
fn foot_ui(ui: &mut egui::Ui, online: &mut bool) -> Option<Screen> {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let swatch = if *online {
        theme::Swatch::Green
    } else {
        theme::Swatch::Neutral
    };
    let colour = theme::current(ui.ctx()).color(theme::Tone::Text, swatch);
    ui.painter().circle_filled(rect.center(), 5.0, colour);
    let hover = if *online {
        "demo-foot-online"
    } else {
        "demo-foot-offline"
    };
    let response = response.on_hover_text(i18n::tr(ui.ctx(), hover));
    if response.clicked() {
        *online = !*online;
        return Some(Screen::Overview);
    }
    None
}

impl corundum_framework::eframe::App for Demo {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut corundum_framework::eframe::Frame) {
        Demo::ui(self, ui);
    }
}

/// The window's options: its title and size, the fonts, recording the CJK font found, and
/// the look and the rail's order, kept in the app's config directory.
fn window_options() -> corundum_framework::gui::window::WindowOptions {
    let options = corundum_framework::gui::window::WindowOptions::new("corundum-framework demo")
        .inner_size([960.0, 680.0])
        .min_inner_size([320.0, 400.0])
        .fonts(|ctx| {
            let family = corundum_framework::gui::fonts::install_system_cjk_fallback(ctx);
            let _ = CJK_FAMILY.set(family);
        });
    match UiStore::for_app(APP_NAME) {
        Some(store) => {
            tracing::info!(path = %store.path().display(), "corundum demo: ui file");
            options.remember(store)
        }
        None => {
            tracing::warn!(
                "corundum demo: no config directory; the look and the rail are not saved"
            );
            options
        }
    }
}

/// Open the demo in a window and run it until it closes. On iOS this runs on the main
/// thread and does not return while the app runs.
#[cfg(not(target_os = "android"))]
pub fn run() -> corundum_framework::eframe::Result<()> {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,corundum_demo=debug"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    tracing::info!("corundum demo: start");
    corundum_framework::gui::window::run(window_options(), |cc| Box::new(Demo::new(&cc.egui_ctx)))
}

/// The iOS app's entry: `main.m` in ios/ hands the main thread to this.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn corundum_demo_main() {
    if let Err(err) = run() {
        tracing::error!("corundum demo: run failed: {err}");
    }
}

/// The Android app's entry, called by android-activity on GameActivity's thread.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: corundum_framework::gui::window::AndroidApp) {
    corundum_framework::gui::logcat::install("corundum-demo", "corundum_demo=debug");
    tracing::info!("corundum demo: start");
    if let Err(err) = corundum_framework::gui::window::run_android(app, window_options(), |cc| {
        Box::new(Demo::new(&cc.egui_ctx))
    }) {
        tracing::error!("corundum demo: run failed: {err}");
    }
}

#[cfg(test)]
mod tests;
