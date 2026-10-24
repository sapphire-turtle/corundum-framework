//! The Lucide icons (ISC license, `assets/icons/LICENSE-Lucide`) shared by the apps.
//!
//! Embedded with [`egui::include_image!`] and decoded by `egui_extras`'s SVG loader,
//! which [`install_loaders`] puts in place. The SVGs stroke in white, so
//! [`egui::Image::tint`] colours them — the text colour, through [`image`]. An app's own
//! Lucide icons (stroked in white the same way) are drawn with [`image`] too.

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
    egui::Image::new(src)
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(ui.visuals().text_color())
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
}
