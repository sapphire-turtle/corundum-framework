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

#[test]
fn every_screen_draws_wide_and_narrow() {
    for size in [[1000.0, 700.0], [390.0, 844.0]] {
        let ctx = egui::Context::default();
        let mut app = Demo::new(&ctx);
        for screen in Screen::ALL {
            app.open(screen);
            render(&ctx, size, &mut app);
            assert_eq!(app.current(), Some(screen), "at {size:?}");
        }
    }
}

#[test]
fn the_screens_draw_in_japanese() {
    let ctx = egui::Context::default();
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
}

#[test]
fn a_look_changed_on_a_screen_is_there_at_the_next_start() {
    let dir = std::env::temp_dir().join(format!("corundum-demo-look-{}", std::process::id()));
    let store = Store::at(dir.join(corundum_framework::gui::store::FILE_NAME));
    let _ = std::fs::remove_dir_all(&dir);

    let ctx = egui::Context::default();
    corundum_framework::gui::store::attach(&ctx, store.clone());
    let mut app = Demo::new(&ctx);
    assert_eq!(
        store.load().and_then(|s| s.look),
        Some(Look::default()),
        "saved from the first start"
    );
    app.theme.contrast = theme::Contrast::High;
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
    assert_eq!(app.theme.contrast, theme::Contrast::High);
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
