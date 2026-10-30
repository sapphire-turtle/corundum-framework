//! A demo of corundum-framework's shared pieces — the shell, the theme, the fonts, the icons,
//! the translations and the platform calls — to try them on each OS apart from the apps.
//!
//! [`Demo`] is the app; [`run`] opens it in a desktop window (and on iOS, where
//! `corundum_demo_main` calls it), and `android_main` on an Android phone.

use std::sync::OnceLock;

use corundum_framework::gui::i18n::{self, LanguageSettings, Resources};
use corundum_framework::gui::look::{self, Look};
use corundum_framework::gui::settings::Settings;
use corundum_framework::gui::store::Store;
use corundum_framework::gui::theme::{self, ThemeSettings};
use corundum_framework::gui::{Action, Destination, Shell, ShellApp, Side, egui, icons};

mod screens;
#[cfg(feature = "velopack")]
pub mod velopack;

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

/// The demo's entry in the desktop's app menu: registered at start, and shown on the
/// Settings About page, where it can be removed and added back.
pub(crate) const LAUNCHER: corundum_framework::platform::launcher::LauncherSpec =
    corundum_framework::platform::launcher::LauncherSpec {
        id: "corundum-demo",
        name: "Corundum Demo",
        description: "Every piece corundum-framework shares",
        category: "Development",
        icon_svg: include_bytes!("../assets/app-icon.svg"),
        bundle_prefix: "net.fireturtle.corundum",
    };

/// The CJK font the window installed at start, for the Fonts screen: `Some(None)` when the
/// system has none, unset when the app did not start through [`run`].
static CJK_FAMILY: OnceLock<Option<String>> = OnceLock::new();

/// The demo: the shell, and the app it draws.
pub struct Demo {
    shell: Shell<Screen>,
    app: App,
    /// The window's rects as last logged, to log them again only when they change.
    logged: Option<[egui::Rect; 3]>,
}

/// What the shell draws: the screens, the side pane beside them all, the foot and the
/// status, and the look the app keeps on the device. The shell's rail and side column keep
/// their state there too, by themselves.
struct App {
    /// The note the rail's action began, written in the side pane.
    draft: Option<String>,
    settings: Settings,
    /// The theme and the language the Theme and Language screens edit.
    theme: ThemeSettings,
    language: LanguageSettings,
    /// The look as last applied: what the screens' edits and the Settings page's changes
    /// are told apart from.
    applied: Look,
    state: screens::State,
    /// The status bar's icon for a downloaded update, in a package.
    #[cfg(feature = "velopack")]
    badge: Option<corundum_framework::gui::update::ReadyBadge>,
}

impl Demo {
    /// The app in `ctx`, in the look the window installed from the device, or else the
    /// default theme and the system's language.
    pub fn new(ctx: &egui::Context) -> Demo {
        i18n::register(ctx, DEMO);
        let applied = look::current(ctx).unwrap_or_default();
        // Saved too, when the device had none: the file is there from the first start.
        look::apply(ctx, &applied);
        // The action first, as an app's "new entry" would be.
        let shell = Shell::from_items(vec![
            Action::new("new-note", icons::plus(), "demo-action-new").into(),
            Destination::new(Screen::Overview, icons::info(), "demo-screen-overview").into(),
            Destination::new(Screen::Theme, icons::star(), "demo-screen-theme").into(),
            Destination::new(Screen::Fonts, icons::file_text(), "demo-screen-fonts").into(),
            Destination::new(Screen::Icons, icons::images(), "demo-screen-icons").into(),
            Destination::new(Screen::Language, icons::tag(), "demo-screen-language").into(),
            Destination::new(Screen::Detail, icons::list_music(), "demo-screen-detail").into(),
            Destination::new(Screen::Files, icons::folder(), "demo-screen-files").into(),
            Destination::new(
                Screen::Platform,
                icons::monitor_smartphone(),
                "demo-screen-platform",
            )
            .into(),
            Destination::new(Screen::Media, icons::music(), "demo-screen-media").into(),
        ])
        .with_footer(vec![Destination::new(
            Screen::Settings,
            icons::settings(),
            "demo-screen-settings",
        )]);
        Demo {
            shell,
            app: App {
                draft: None,
                settings: screens::settings::settings(),
                theme: applied.theme.clone(),
                language: applied.language.clone(),
                applied,
                state: screens::State::new(CJK_FAMILY.get().cloned()),
                #[cfg(feature = "velopack")]
                badge: velopack::updater().map(|updater| {
                    // The badge appears when the background download ends: draw it then.
                    let ctx = ctx.clone();
                    updater.on_change(move || ctx.request_repaint());
                    corundum_framework::gui::update::ReadyBadge::new(updater)
                }),
            },
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
        // The player follows the OS's media keys from every screen.
        let on_screen = self.shell.current() == Some(Screen::Media);
        self.app.state.media.update(ui.ctx(), on_screen);
        self.shell.show(ui, &mut self.app);
        if let Some(screen) = self.app.state.open.take() {
            self.shell.open(screen);
        }
        self.app.follow_look(ui.ctx());
    }
}

impl ShellApp<Screen> for App {
    fn screen_ui(&mut self, ui: &mut egui::Ui, screen: Screen, side: &mut Side) {
        let App {
            settings,
            theme,
            language,
            state,
            ..
        } = self;
        screens::ui(ui, screen, state, settings, theme, language, side);
    }

    fn side_ui(&mut self, ui: &mut egui::Ui, screen: Screen, side: &mut Side) {
        screens::side_ui(ui, screen, &mut self.state, &mut self.draft, side);
    }

    fn foot_ui(&mut self, ui: &mut egui::Ui) -> Option<Screen> {
        foot_ui(ui, &mut self.state.online)
    }

    fn status_ui(&mut self, ui: &mut egui::Ui) {
        #[cfg(feature = "velopack")]
        if let Some(badge) = &mut self.badge {
            badge.ui(ui);
        }
        ui.weak(self.state.log.text(ui.ctx()));
    }

    fn action(&mut self, id: &'static str, side: &mut Side) {
        if id == "new-note" {
            self.state.clicks += 1;
            self.draft.get_or_insert_with(String::new);
            side.open();
            self.state.log = i18n::Message::new("demo-status-writing");
        }
    }
}

impl App {
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
        .launcher(LAUNCHER)
        .fonts(|ctx| {
            let family = corundum_framework::gui::fonts::install_system_cjk_fallback(ctx);
            let _ = CJK_FAMILY.set(family);
        });
    match Store::for_app(APP_NAME) {
        Some(store) => {
            tracing::info!(path = %store.path().display(), "corundum demo: corundum file");
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

/// Log to standard error, `RUST_LOG` or the demo's default filter. Called again, it does
/// nothing: `main` calls it before `package::start`, whose hooks only log, and [`run`]
/// for the other starts (iOS, a build without `velopack`). Velopack's own log is kept to its
/// errors: it notes every start, which would print before each command's output.
#[cfg(not(target_os = "android"))]
pub fn init_logging() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,corundum_demo=debug,velopack=error"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

/// Move the demo's directory from Roaming to Local on Windows, where an older demo kept it
/// (corundum-framework#57), before anything reads it. Called again, it does nothing: as
/// with [`init_logging`], `main` calls it before `package::start`, which reads the data
/// directory, and [`run`] for the other starts.
///
/// A move that fails may have stopped halfway, so the demo does not run on what is in
/// Local: it says why on standard error and exits with 1, and the next start goes on with
/// the move. A start for one of Velopack's hooks (`--veloapp-install`, ...) is the
/// exception: it has 30 seconds and no window, its exit code goes to the installer, and
/// all it writes is a line of the hooks' log (`velopack::log`), so the failure is only
/// logged there.
#[cfg(not(target_os = "android"))]
pub fn migrate_dirs() {
    use corundum_framework::platform::dirs::AppDirs;

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Err(err) = AppDirs::migrate_from_roaming(APP_NAME) else {
            return;
        };
        let hook = std::env::args_os()
            .nth(1)
            .is_some_and(|arg| arg.as_encoded_bytes().starts_with(b"--veloapp-"));
        if hook {
            tracing::warn!("corundum demo: could not move the directory from Roaming: {err}");
            return;
        }
        tracing::error!("corundum demo: could not move the directory from Roaming: {err}");
        eprintln!("corundum demo: could not move the directory from Roaming: {err}");
        std::process::exit(1);
    });
}

/// Open the demo in a window and run it until it closes. On iOS this runs on the main
/// thread and does not return while the app runs.
#[cfg(not(target_os = "android"))]
pub fn run() -> corundum_framework::eframe::Result<()> {
    init_logging();
    migrate_dirs();
    tracing::info!("corundum demo: start");
    corundum_framework::gui::window::run(window_options(), |cc| {
        let mut demo = Demo::new(&cc.egui_ctx);
        // The media session belongs to the window, which exists from here.
        demo.app.state.media.connect(cc);
        Box::new(demo)
    })
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
