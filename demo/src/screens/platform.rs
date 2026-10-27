//! The Platform: the OS, the clipboard, the insets, the system's dialogs and the app's
//! directories — what corundum-framework-platform reads, and the window's view of the
//! screen.

use std::path::Path;

use corundum_framework::gui::platform::{self, FilePicker, IMAGE_EXTENSIONS};
use corundum_framework::gui::{egui, tr};
use corundum_framework::platform::dirs::AppDirs;

use super::{heading, section};

/// What the screen read from the platform: once at start, and again on Read again.
pub(crate) struct State {
    dirs: Option<AppDirs>,
    pub(crate) languages: Vec<String>,
    /// The system clipboard as last read: `None` before the first read.
    clipboard: Option<Option<String>>,
    pasted: String,
    typed: String,
    /// The files or the folder last picked.
    picked: Vec<String>,
}

impl State {
    pub(crate) fn read() -> State {
        State {
            dirs: AppDirs::new(crate::APP_NAME),
            languages: corundum_framework::platform::locale::system_languages(),
            clipboard: None,
            pasted: String::new(),
            typed: String::new(),
            picked: Vec::new(),
        }
    }

    fn refresh(&mut self) {
        self.dirs = AppDirs::new(crate::APP_NAME);
        self.languages = corundum_framework::platform::locale::system_languages();
    }
}

pub(super) fn ui(ui: &mut egui::Ui, state: &mut State) {
    heading(ui, "demo-screen-platform");

    section(ui, "demo-platform-system");
    ui.label(tr!(
        ui,
        "demo-platform-os",
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH
    ));

    section(ui, "demo-platform-clipboard");
    clipboard_ui(ui, state);

    section(ui, "demo-platform-insets");
    insets_ui(ui, state);

    section(ui, "demo-platform-dialogs");
    dialogs_ui(ui, state);

    section(ui, "demo-platform-dirs");
    let none = tr!(ui, "demo-platform-none");
    let shown = |p: Option<&Path>| p.map_or_else(|| none.clone(), |p| p.display().to_string());
    let dirs = state.dirs.as_ref();
    ui.label(tr!(
        ui,
        "demo-platform-config",
        path = shown(dirs.map(AppDirs::config))
    ));
    ui.label(tr!(
        ui,
        "demo-platform-data",
        path = shown(dirs.map(AppDirs::data))
    ));
    ui.label(tr!(
        ui,
        "demo-platform-cache",
        path = shown(dirs.map(AppDirs::cache))
    ));
    ui.label(tr!(
        ui,
        "demo-language-system",
        languages = state.languages.join(", ")
    ));
    if ui.button(tr!(ui, "demo-platform-refresh")).clicked() {
        state.refresh();
    }
}

fn clipboard_ui(ui: &mut egui::Ui, state: &mut State) {
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr!(ui, "demo-platform-copy")).clicked() {
            ui.ctx().copy_text(tr!(ui, "demo-platform-sample"));
        }
        if ui.button(tr!(ui, "demo-platform-read")).clicked() {
            state.clipboard = Some(corundum_framework::gui::platform::clipboard_text());
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.pasted)
                .hint_text(tr!(ui, "demo-platform-paste-here")),
        );
        // Where egui cannot paste (Android), the app offers a button reading the clipboard.
        if corundum_framework::gui::platform::needs_paste_button()
            && ui.button(tr!(ui, "demo-platform-paste")).clicked()
            && let Some(text) = corundum_framework::gui::platform::clipboard_text()
        {
            state.pasted = text;
        }
    });
    match &state.clipboard {
        Some(Some(text)) => {
            ui.label(tr!(
                ui,
                "demo-platform-clipboard-text",
                text = text.as_str()
            ));
        }
        Some(None) => {
            ui.label(tr!(ui, "demo-platform-clipboard-none"));
        }
        None => {}
    }
}

fn dialogs_ui(ui: &mut egui::Ui, state: &mut State) {
    if !platform::file_picker_available() {
        ui.weak(tr!(ui, "demo-platform-no-dialogs"));
        return;
    }
    let shown = |paths: Vec<std::path::PathBuf>| -> Vec<String> {
        paths.iter().map(|p| p.display().to_string()).collect()
    };
    ui.horizontal_wrapped(|ui| {
        if ui.button(tr!(ui, "demo-platform-pick-images")).clicked() {
            let images = FilePicker::new()
                .title(tr!(ui, "demo-platform-pick-images"))
                .filter(tr!(ui, "demo-platform-images"), IMAGE_EXTENSIONS)
                .pick_files();
            state.picked = shown(images);
        }
        if ui.button(tr!(ui, "demo-platform-pick-file")).clicked() {
            state.picked = shown(FilePicker::new().pick_file().into_iter().collect());
        }
        if ui.button(tr!(ui, "demo-platform-pick-folder")).clicked() {
            let folder = platform::pick_folder(&tr!(ui, "demo-platform-pick-folder"));
            state.picked = shown(folder.into_iter().collect());
        }
    });
    for path in &state.picked {
        ui.horizontal(|ui| {
            ui.label(path);
            if platform::file_manager_available()
                && ui.small_button(tr!(ui, "demo-platform-show")).clicked()
            {
                let path = Path::new(path);
                platform::open_in_file_manager(if path.is_dir() {
                    path
                } else {
                    path.parent().unwrap_or(path)
                });
            }
        });
    }
}

fn insets_ui(ui: &mut egui::Ui, state: &mut State) {
    let ctx = ui.ctx().clone();
    let safe = ctx.input(|i| i.safe_area_insets()).0;
    let safe = format!(
        "L {:.0} · T {:.0} · R {:.0} · B {:.0}",
        safe.left, safe.top, safe.right, safe.bottom
    );
    ui.label(tr!(ui, "demo-platform-safe-area", insets = safe));
    let window = corundum_framework::gui::platform::window_insets().map_or_else(
        || tr!(ui, "demo-platform-none"),
        |[l, t, r, b]| format!("L {l} · T {t} · R {r} · B {b}"),
    );
    ui.label(tr!(ui, "demo-platform-window-insets", insets = window));
    ui.label(tr!(
        ui,
        "demo-platform-viewport",
        rect = rect(ctx.viewport_rect())
    ));
    ui.label(tr!(
        ui,
        "demo-platform-content",
        rect = rect(ctx.content_rect())
    ));
    ui.label(tr!(
        ui,
        "demo-platform-ppp",
        ppp = format!("{:.2}", ctx.pixels_per_point())
    ));
    ui.add(
        egui::TextEdit::singleline(&mut state.typed).hint_text(tr!(ui, "demo-platform-keyboard")),
    );
}

/// `rect` as its size and where it starts, in points.
fn rect(rect: egui::Rect) -> String {
    format!(
        "{:.0} × {:.0} at ({:.0}, {:.0})",
        rect.width(),
        rect.height(),
        rect.min.x,
        rect.min.y
    )
}
