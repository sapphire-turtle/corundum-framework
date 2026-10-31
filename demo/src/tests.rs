use corundum_framework::gui::i18n::{self, check};

use super::*;

/// Run `app` over two frames of a `size` screen (the first lays out, the second draws).
fn render(ctx: &egui::Context, size: [f32; 2], app: &mut Demo) {
    for _ in 0..2 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0], size[1]),
            )),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.ui(ui));
        // Unapplied texture deltas panic on drop; there is no renderer here.
        output.textures_delta.clear();
    }
}

/// A store in a directory of its own, `name`d for the test, that says the first run was
/// finished: a window over it draws the shell, not the flow.
fn finished_store(name: &str) -> (Store, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("corundum-demo-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::at(dir.join(corundum_framework::gui::store::FILE_NAME));
    store
        .save(&corundum_framework::gui::store::Stored {
            first_run: Some(true),
            ..Default::default()
        })
        .expect("the test's store");
    (store, dir)
}

/// A context on a device that has been through the first run.
fn finished_context(name: &str) -> (egui::Context, std::path::PathBuf) {
    let (store, dir) = finished_store(name);
    let ctx = egui::Context::default();
    corundum_framework::gui::store::attach(&ctx, store);
    (ctx, dir)
}

#[test]
fn every_screen_draws_wide_and_narrow() {
    for size in [[1000.0, 700.0], [390.0, 844.0]] {
        let (ctx, dir) = finished_context("every-screen");
        let mut app = Demo::new(&ctx);
        for screen in Screen::ALL {
            app.open(screen);
            render(&ctx, size, &mut app);
            assert_eq!(app.current(), Some(screen), "at {size:?}");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_note_stays_open_beside_every_screen() {
    for size in [[1000.0, 700.0], [390.0, 844.0]] {
        let (ctx, dir) = finished_context("note");
        let mut app = Demo::new(&ctx);
        app.app.draft = Some("groceries".into());
        for screen in Screen::ALL {
            app.open(screen);
            render(&ctx, size, &mut app);
            assert_eq!(
                app.app.draft.as_deref(),
                Some("groceries"),
                "{screen:?} at {size:?}"
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn the_screens_draw_in_japanese() {
    let (ctx, dir) = finished_context("japanese");
    let mut app = Demo::new(&ctx);
    i18n::install(
        &ctx,
        &LanguageSettings {
            language: i18n::Language::Explicit("ja".into()),
        },
    );
    for screen in Screen::ALL {
        app.open(screen);
        render(&ctx, [1000.0, 700.0], &mut app);
    }
    assert_eq!(i18n::tr(&ctx, "demo-screen-icons"), "アイコン");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_look_changed_on_a_screen_is_there_at_the_next_start() {
    let (store, dir) = finished_store("look");

    let ctx = egui::Context::default();
    corundum_framework::gui::store::attach(&ctx, store.clone());
    let mut app = Demo::new(&ctx);
    assert_eq!(
        store.load().and_then(|s| s.look),
        Some(Look::default()),
        "saved from the first start"
    );
    app.app.theme.contrast = theme::Contrast::High;
    app.open(Screen::Theme);
    render(&ctx, [1000.0, 700.0], &mut app);
    assert_eq!(
        store.load().and_then(|s| s.look).map(|l| l.theme.contrast),
        Some(theme::Contrast::High)
    );

    // The next start: a new context, the same file.
    let ctx = egui::Context::default();
    assert!(corundum_framework::gui::store::attach(&ctx, store));
    let app = Demo::new(&ctx);
    assert_eq!(app.app.theme.contrast, theme::Contrast::High);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_translations_parse_and_match() {
    assert_eq!(check::parse_errors(&DEMO), Vec::<String>::new());
    assert_eq!(
        check::key_difference(&DEMO, "en", "ja"),
        Vec::<String>::new()
    );
}

// Reads the sources from the checkout, which a phone's test binary does not have.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn every_id_the_code_uses_exists() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        check::unknown_ids(&DEMO, "en", &src, "demo-", &[]),
        Vec::<String>::new()
    );
}

// Reads corundum-framework-gui's icon files from the checkout, as the test above does.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn the_icon_screen_lists_every_bundled_icon() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../crates/corundum-framework-gui/assets/icons");
    let mut bundled: Vec<String> = std::fs::read_dir(&dir)
        .expect("the gui crate's icons")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            Some(name.strip_suffix(".svg")?.to_owned())
        })
        .collect();
    bundled.sort();
    let mut listed: Vec<String> = screens::icons::ALL
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    listed.sort();
    assert_eq!(listed, bundled);
}

// The first run is the desktops' (`gui-service`).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn a_new_device_sees_the_first_run_before_the_screens() {
    let ctx = egui::Context::default();
    let mut app = Demo::new(&ctx);
    render(&ctx, [1000.0, 700.0], &mut app);
    assert!(
        app.first_run.is_some(),
        "the flow, on a device that has not seen it"
    );

    // Finished on a device, as the store says on a later start: the screens.
    let (ctx, dir) = finished_context("first-run");
    let mut app = Demo::new(&ctx);
    render(&ctx, [1000.0, 700.0], &mut app);
    assert!(app.first_run.is_none());

    // Settings › About's button: the flow again.
    corundum_framework::gui::first_run::FirstRun::reset(&ctx);
    render(&ctx, [1000.0, 700.0], &mut app);
    assert!(app.first_run.is_some(), "the flow again after a reset");
    let _ = std::fs::remove_dir_all(dir);
}

// The command line and the first run are the desktops' (`main`, `gui-service`).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn the_daemon_subcommand_is_recognised() {
    use std::ffi::OsString;

    let line = |args: &[&str]| command_line(args.iter().map(OsString::from));
    assert_eq!(line(&["daemon"]), CommandLine::Daemon);
    assert_eq!(line(&[]), CommandLine::Window);
    assert_eq!(line(&["--version"]), CommandLine::Done);
    // Only an exact first argument: the window keeps the files it was opened with, a file
    // called `daemon` among them.
    assert_eq!(line(&["notes.txt", "daemon"]), CommandLine::Window);
    assert_eq!(line(&["daemons"]), CommandLine::Window);
    assert_eq!(line(&["./daemon"]), CommandLine::Window);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[test]
fn the_first_run_has_a_welcome_and_the_service_page() {
    let ctx = egui::Context::default();
    i18n::register(&ctx, DEMO);
    let pages = first_run::pages(&ctx);
    let titles: Vec<String> = pages.iter().map(|page| page.title(&ctx)).collect();
    assert_eq!(
        titles,
        [
            i18n::tr(&ctx, "demo-first-run-welcome"),
            i18n::tr(&ctx, "demo-first-run-service-question"),
        ]
    );
}
