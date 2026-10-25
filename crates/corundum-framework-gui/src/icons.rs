//! The Lucide icons (ISC license, `assets/icons/LICENSE-Lucide`) shared by the apps.
//!
//! Embedded with [`egui::include_image!`] and decoded by `egui_extras`'s SVG loader,
//! which [`install_loaders`] puts in place. The SVGs stroke in white, so
//! [`egui::Image::tint`] colours them, in the installed [`theme`](crate::theme)'s colours:
//!
//! - [`image`]: the text colour, the default.
//! - [`image_as`]: a [`Tone`] of a [`Swatch`] — the accent, a warning, weak text.
//! - [`image_in`] and [`paint`]: a colour of the caller's.
//! - [`button`]: the colour of the button's state, as its text would be — stronger when
//!   hovered, the accent when selected, faded when disabled.
//!
//! An app's own icons take the colours the same way, as long as they stroke (or fill) in
//! white: hand their [`egui::ImageSource`] to any of the above.

use crate::theme::{Swatch, Tone};

macro_rules! icon {
    ($name:literal) => {
        egui::include_image!(concat!("../assets/icons/", $name, ".svg"))
    };
}

/// Put the image loaders in place, SVG included, unless they already are. The shell calls
/// it; an app drawing these icons without one calls it once at startup.
pub fn install_loaders(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
}

/// `src` as a `size`-point square in the text colour.
pub fn image(ui: &egui::Ui, src: egui::ImageSource<'static>, size: f32) -> egui::Image<'static> {
    image_in(src, size, ui.visuals().text_color())
}

/// `src` as a `size`-point square in the current theme's `swatch` at `tone`.
pub fn image_as(
    ui: &egui::Ui,
    src: egui::ImageSource<'static>,
    size: f32,
    tone: Tone,
    swatch: Swatch,
) -> egui::Image<'static> {
    image_in(
        src,
        size,
        crate::theme::current(ui.ctx()).color(tone, swatch),
    )
}

/// `src` as a `size`-point square in `colour`.
pub fn image_in(
    src: egui::ImageSource<'static>,
    size: f32,
    colour: egui::Color32,
) -> egui::Image<'static> {
    square(src, size).tint(colour)
}

/// `src` in `colour`, filling `rect`: for a widget that paints its own icon.
pub fn paint(
    ui: &egui::Ui,
    rect: egui::Rect,
    src: egui::ImageSource<'static>,
    colour: egui::Color32,
) {
    egui::Image::new(src).tint(colour).paint_at(ui, rect);
}

/// A button showing `src` as a `size`-point square in the colour of its state.
pub fn button(src: egui::ImageSource<'static>, size: f32) -> egui::Button<'static> {
    // Left white, the image takes the button's text colour.
    egui::Button::image(square(src, size)).image_tint_follows_text_color(true)
}

fn square(src: egui::ImageSource<'static>, size: f32) -> egui::Image<'static> {
    egui::Image::new(src).fit_to_exact_size(egui::vec2(size, size))
}

/// Settings: where an app's preferences are.
pub fn settings() -> egui::ImageSource<'static> {
    icon!("settings")
}

/// Back, to the screen or list before.
pub fn arrow_left() -> egui::ImageSource<'static> {
    icon!("arrow-left")
}

/// Close or dismiss.
pub fn x() -> egui::ImageSource<'static> {
    icon!("x")
}

/// Add or create.
pub fn plus() -> egui::ImageSource<'static> {
    icon!("plus")
}

/// Delete.
pub fn trash_2() -> egui::ImageSource<'static> {
    icon!("trash-2")
}

/// Edit.
pub fn pencil_line() -> egui::ImageSource<'static> {
    icon!("pencil-line")
}

/// Refresh or reload.
pub fn refresh_cw() -> egui::ImageSource<'static> {
    icon!("refresh-cw")
}

/// Information, About.
pub fn info() -> egui::ImageSource<'static> {
    icon!("info")
}

/// The detail pane at the right, shown or hidden.
pub fn panel_right() -> egui::ImageSource<'static> {
    icon!("panel-right")
}

/// More actions, in a menu.
pub fn ellipsis() -> egui::ImageSource<'static> {
    icon!("ellipsis")
}

/// A collapsed row, or a way further in.
pub fn chevron_right() -> egui::ImageSource<'static> {
    icon!("chevron-right")
}

/// An expanded row, or a menu to open.
pub fn chevron_down() -> egui::ImageSource<'static> {
    icon!("chevron-down")
}

/// Done, or a success.
pub fn circle_check() -> egui::ImageSource<'static> {
    icon!("circle-check")
}

/// Failed, or cancelled.
pub fn circle_x() -> egui::ImageSource<'static> {
    icon!("circle-x")
}

/// A folder.
pub fn folder() -> egui::ImageSource<'static> {
    icon!("folder")
}

/// A text file or document.
pub fn file_text() -> egui::ImageSource<'static> {
    icon!("file-text")
}

/// An arrow turning back: reset to the default.
pub fn rotate_ccw() -> egui::ImageSource<'static> {
    icon!("rotate-ccw")
}

/// A die: pick at random.
pub fn dice_5() -> egui::ImageSource<'static> {
    icon!("dice-5")
}

/// A picture or photo. Lucide's `image`, named apart from [`image`] the drawing helper.
pub fn picture() -> egui::ImageSource<'static> {
    icon!("image")
}

/// Pictures, a gallery.
pub fn images() -> egui::ImageSource<'static> {
    icon!("images")
}

/// Music, or a track.
pub fn music() -> egui::ImageSource<'static> {
    icon!("music")
}

/// A playlist.
pub fn list_music() -> egui::ImageSource<'static> {
    icon!("list-music")
}

/// A phone, the device.
pub fn smartphone() -> egui::ImageSource<'static> {
    icon!("smartphone")
}

/// The display: how the app looks on this screen and device.
pub fn monitor_smartphone() -> egui::ImageSource<'static> {
    icon!("monitor-smartphone")
}

/// Play.
pub fn play() -> egui::ImageSource<'static> {
    icon!("play")
}

/// Pause.
pub fn pause() -> egui::ImageSource<'static> {
    icon!("pause")
}

/// Back to the previous track.
pub fn skip_back() -> egui::ImageSource<'static> {
    icon!("skip-back")
}

/// On to the next track.
pub fn skip_forward() -> egui::ImageSource<'static> {
    icon!("skip-forward")
}

/// Sound, or the volume.
pub fn volume_2() -> egui::ImageSource<'static> {
    icon!("volume-2")
}

/// Muted.
pub fn volume_x() -> egui::ImageSource<'static> {
    icon!("volume-x")
}

/// A rating, or a favourite.
pub fn star() -> egui::ImageSource<'static> {
    icon!("star")
}

/// A tag.
pub fn tag() -> egui::ImageSource<'static> {
    icon!("tag")
}

/// Search.
pub fn search() -> egui::ImageSource<'static> {
    icon!("search")
}

/// Copy to the clipboard.
pub fn clipboard_copy() -> egui::ImageSource<'static> {
    icon!("clipboard-copy")
}

/// Upload, or send to the server.
pub fn upload() -> egui::ImageSource<'static> {
    icon!("upload")
}

/// Download, or fetch from the server.
pub fn download() -> egui::ImageSource<'static> {
    icon!("download")
}

/// Offline: not synced.
pub fn cloud_off() -> egui::ImageSource<'static> {
    icon!("cloud-off")
}

#[cfg(test)]
mod tests {
    use super::*;

    type Icon = fn() -> egui::ImageSource<'static>;

    const ALL: [(&str, Icon); 36] = [
        ("settings", settings),
        ("arrow_left", arrow_left),
        ("x", x),
        ("plus", plus),
        ("trash_2", trash_2),
        ("pencil_line", pencil_line),
        ("refresh_cw", refresh_cw),
        ("info", info),
        ("ellipsis", ellipsis),
        ("chevron_right", chevron_right),
        ("chevron_down", chevron_down),
        ("circle_check", circle_check),
        ("circle_x", circle_x),
        ("folder", folder),
        ("file_text", file_text),
        ("rotate_ccw", rotate_ccw),
        ("dice_5", dice_5),
        ("picture", picture),
        ("images", images),
        ("music", music),
        ("list_music", list_music),
        ("smartphone", smartphone),
        ("monitor_smartphone", monitor_smartphone),
        ("play", play),
        ("pause", pause),
        ("skip_back", skip_back),
        ("skip_forward", skip_forward),
        ("volume_2", volume_2),
        ("volume_x", volume_x),
        ("star", star),
        ("tag", tag),
        ("search", search),
        ("clipboard_copy", clipboard_copy),
        ("upload", upload),
        ("download", download),
        ("cloud_off", cloud_off),
    ];

    #[test]
    fn every_icon_decodes() {
        let ctx = egui::Context::default();
        install_loaders(&ctx);
        for (name, src) in ALL {
            let egui::ImageSource::Bytes { uri, bytes } = src() else {
                panic!("{name} is not embedded");
            };
            ctx.include_bytes(uri.clone(), bytes);
            let poll = ctx.try_load_image(
                &uri,
                egui::SizeHint::Size {
                    width: 20,
                    height: 20,
                    maintain_aspect_ratio: true,
                },
            );
            match poll {
                Ok(egui::load::ImagePoll::Ready { .. }) => {}
                Ok(egui::load::ImagePoll::Pending { .. }) => panic!("{name}: still pending"),
                Err(e) => panic!("{name}: {e}"),
            }
        }
    }

    /// The tint of each image `ui` draws, in order, over two frames of a dark or light
    /// theme.
    fn drawn_tints(dark: bool, mut ui: impl FnMut(&mut egui::Ui)) -> Vec<egui::Color32> {
        let ctx = egui::Context::default();
        install_loaders(&ctx);
        crate::theme::install(
            &ctx,
            &crate::theme::ThemeSettings {
                preference: if dark {
                    egui::ThemePreference::Dark
                } else {
                    egui::ThemePreference::Light
                },
                ..Default::default()
            },
        );
        let mut tints = Vec::new();
        for _ in 0..2 {
            let mut output = ctx.run_ui(egui::RawInput::default(), &mut ui);
            output.textures_delta.clear();
            tints = output
                .shapes
                .iter()
                .filter_map(|clipped| match &clipped.shape {
                    // An image is a rectangle filled with its texture, in its tint.
                    egui::Shape::Rect(rect) if rect.brush.is_some() => Some(rect.fill),
                    _ => None,
                })
                .collect();
        }
        tints
    }

    #[test]
    fn images_follow_the_theme() {
        let draw = |ui: &mut egui::Ui| {
            ui.add(image(ui, settings(), 20.0));
            ui.add(image_as(ui, settings(), 20.0, Tone::Text, Swatch::Accent));
        };
        let [dark, light] = [true, false].map(|dark| {
            let theme = crate::theme::Theme::new(
                dark,
                crate::theme::Contrast::default(),
                crate::theme::Tint::Swatch(Swatch::Blue),
            );
            let tints = drawn_tints(dark, draw);
            assert_eq!(
                tints,
                [theme.text(Swatch::Neutral), theme.text(Swatch::Accent)],
                "dark {dark}"
            );
            tints
        });
        assert_ne!(dark, light);
    }

    #[test]
    fn a_button_takes_the_colour_of_its_state() {
        for dark in [true, false] {
            let tints = drawn_tints(dark, |ui| {
                ui.add(button(settings(), 20.0));
                ui.add(button(settings(), 20.0).selected(true));
                ui.add_enabled(false, button(settings(), 20.0));
            });
            let [normal, selected, disabled] = tints[..] else {
                panic!("dark {dark}: {tints:?}");
            };
            let (text, accent) = selected_visuals(dark);
            assert_eq!(normal, text, "dark {dark}");
            assert_eq!(selected, accent, "dark {dark}");
            assert_ne!(normal, selected, "dark {dark}");
            assert_ne!(disabled, normal, "dark {dark}");
        }
    }

    /// The text colour and the selected text colour (the accent) of the default theme.
    fn selected_visuals(dark: bool) -> (egui::Color32, egui::Color32) {
        let v = crate::theme::Theme::new(
            dark,
            crate::theme::Contrast::default(),
            crate::theme::Tint::Swatch(Swatch::Blue),
        )
        .visuals();
        (v.widgets.inactive.fg_stroke.color, v.selection.stroke.color)
    }
}
