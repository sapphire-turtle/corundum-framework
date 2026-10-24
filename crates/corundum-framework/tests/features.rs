//! The facade's features, and what each one brings.

#[test]
fn every_module_feature_has_a_matching_optional_dependency() {
    let manifest = include_str!("../Cargo.toml");
    for feature in ["platform", "gui"] {
        let line = format!("\n{feature} = [\"dep:corundum-framework-{feature}\"");
        assert!(
            manifest.contains(&line),
            "the feature {feature:?} does not turn on corundum-framework-{feature}"
        );
    }
}

#[test]
fn the_default_leaves_egui_out() {
    let manifest = include_str!("../Cargo.toml");
    // By line: a Windows checkout ends them with CRLF.
    assert!(
        manifest
            .lines()
            .any(|line| line.trim_end() == "default = [\"platform\"]")
    );
}

#[cfg(feature = "platform")]
#[test]
fn platform_is_the_platform_crate() {
    let _: fn(&str) -> Option<corundum_framework::platform::dirs::AppDirs> =
        corundum_framework::platform::dirs::AppDirs::new;
    let _: fn() -> Vec<String> = corundum_framework::platform::locale::system_languages;
}

#[cfg(feature = "gui")]
#[test]
fn gui_brings_platform_and_egui() {
    let _: fn() -> Vec<String> = corundum_framework::platform::locale::system_languages;
    let ctx = corundum_framework::egui::Context::default();
    corundum_framework::gui::icons::install_loaders(&ctx);
}

// The phone's window is eframe's too: an Android app names its `eframe::App` through the
// facade as a desktop app does (#22).
#[cfg(feature = "android")]
#[test]
fn android_brings_eframe() {
    fn is_app<A: corundum_framework::eframe::App>() {}
    struct Nothing;
    impl corundum_framework::eframe::App for Nothing {
        fn ui(
            &mut self,
            _: &mut corundum_framework::egui::Ui,
            _: &mut corundum_framework::eframe::Frame,
        ) {
        }
    }
    is_app::<Nothing>();
}
