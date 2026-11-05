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

#[cfg(feature = "image")]
#[test]
fn image_is_the_image_crate() {
    let _: fn(
        corundum_framework::image::Source,
        u32,
        &corundum_framework::image::Limits,
    ) -> Result<corundum_framework::image::Image, corundum_framework::image::Error> =
        corundum_framework::image::thumbnail;
}

#[cfg(feature = "audio")]
#[test]
fn audio_is_the_audio_crate() {
    use corundum_framework::audio::decode::{DecodeError, Info, Source};
    use std::path::PathBuf;

    type Probe = fn(Box<dyn Source>, Option<&str>) -> Result<Info, DecodeError>;
    let _: Probe = corundum_framework::audio::decode::probe;
    let _ = corundum_framework::audio::Command::<PathBuf>::Toggle;
    let _: corundum_framework::audio::FileOpener = corundum_framework::audio::FileOpener::new();
}

#[cfg(feature = "audio-cpal")]
#[test]
fn audio_cpal_brings_the_device() {
    fn is_outputs<O: corundum_framework::audio::OutputFactory>() {}
    is_outputs::<corundum_framework::audio::CpalOutputs>();
}

#[cfg(feature = "audio-preview")]
#[test]
fn audio_preview_is_the_gui_crate_s() {
    let _: fn() -> corundum_framework::gui::preview::AudioPreview =
        corundum_framework::gui::preview::AudioPreview::new;
    fn is_opener<
        O: corundum_framework::audio::Opener<corundum_framework::gui::preview::Content>,
    >() {
    }
    is_opener::<corundum_framework::gui::preview::ContentOpener>();
}

#[cfg(feature = "gui")]
#[test]
fn file_preview_needs_no_preview_feature() {
    let _: fn() -> corundum_framework::gui::preview::FilePreview =
        corundum_framework::gui::preview::FilePreview::new;
    let _ = corundum_framework::gui::preview::Content::bytes("k", "a.bin", Vec::<u8>::new());
}

#[cfg(feature = "code-editor")]
#[test]
fn code_editor_is_alumina_s() {
    let _ = corundum_framework::gui::code::CodeEditor::new;
    let _ = corundum_framework::gui::code::Indent::default();
}

#[cfg(feature = "text-preview")]
#[test]
fn text_preview_is_the_gui_crate_s() {
    let _: fn() -> corundum_framework::gui::preview::TextPreview =
        corundum_framework::gui::preview::TextPreview::new;
}

#[cfg(feature = "markdown-editor")]
#[test]
fn markdown_editor_is_alumina_s() {
    let _ = corundum_framework::gui::markdown::MarkdownEditor::new;
    let _ = corundum_framework::gui::markdown::toolbar;
}

#[cfg(feature = "markdown-preview")]
#[test]
fn markdown_preview_is_the_gui_crate_s() {
    let _: fn() -> corundum_framework::gui::preview::MarkdownPreview =
        corundum_framework::gui::preview::MarkdownPreview::new;
}

#[cfg(feature = "datetime")]
#[test]
fn datetime_is_alumina_s() {
    let _ = corundum_framework::gui::datetime::config;
    let _: Option<corundum_framework::gui::datetime::DatePickerButton> = None;
}
