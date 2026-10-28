//! QR codes: text drawn as one, to hand a long string (an invite ticket, a token) from a
//! screen to a phone's camera.
//!
//! [`QrCode`] is the widget: the text's code, dark on light whatever the theme (a scanner
//! needs the contrast), with the four-module quiet zone the standard asks for around it.
//! It is drawn from a texture of one pixel per module, scaled without smoothing, and made
//! again only when the text changes. [`image`] is the same picture for an app that draws it
//! elsewhere. Reading a code is the platform's: `platform::scan_qr`, with the `qr-scan`
//! feature.

use egui::{Color32, ColorImage, TextureHandle, TextureOptions, Vec2};

/// The modules of light left around the code, as the standard asks.
const QUIET_ZONE: usize = 4;

/// The side a code takes when the app gives none, in points.
const DEFAULT_SIDE: f32 = 200.0;

/// `text` as a QR code, one pixel per module, with the quiet zone: dark modules black,
/// light ones white. `None` when the text is too long for any QR code (about 2.3 KB at
/// the error correction used, the standard's medium).
pub fn image(text: &str) -> Option<ColorImage> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let width = code.width();
    let side = width + 2 * QUIET_ZONE;
    let mut pixels = vec![Color32::WHITE; side * side];
    for (at, colour) in code.to_colors().into_iter().enumerate() {
        if colour == qrcode::Color::Dark {
            let (x, y) = (at % width + QUIET_ZONE, at / width + QUIET_ZONE);
            pixels[y * side + x] = Color32::BLACK;
        }
    }
    Some(ColorImage::new([side, side], pixels))
}

/// A QR code of some text, as a widget.
///
/// ```no_run
/// # use corundum_framework_gui::egui;
/// use corundum_framework_gui::qr::QrCode;
///
/// # fn ui(ui: &mut egui::Ui, ticket: &str) {
/// ui.add(QrCode::new(ticket).side(240.0));
/// # }
/// ```
///
/// Text too long for a QR code shows a line saying so instead.
#[derive(Clone, Debug)]
pub struct QrCode<'a> {
    text: &'a str,
    side: f32,
}

impl<'a> QrCode<'a> {
    /// `text`'s QR code, 200 points a side.
    pub fn new(text: &'a str) -> QrCode<'a> {
        QrCode {
            text,
            side: DEFAULT_SIDE,
        }
    }

    /// Draw the code `side` points a side, quiet zone included.
    pub fn side(mut self, side: f32) -> QrCode<'a> {
        self.side = side;
        self
    }
}

/// The texture of the code a widget last drew, and its text: made again when it changes.
#[derive(Clone)]
struct Drawn {
    text: String,
    texture: Option<TextureHandle>,
}

impl egui::Widget for QrCode<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        // Kept by the widget's place in the layout, so a code whose text changes as the
        // person types replaces its texture rather than adding one per keystroke.
        let id = ui.next_auto_id().with("qr-code");
        let ctx = ui.ctx().clone();
        let drawn = ctx.data_mut(|data| data.get_temp::<Drawn>(id));
        let texture = match drawn {
            Some(drawn) if drawn.text == self.text => drawn.texture,
            _ => {
                let texture = image(self.text)
                    .map(|image| ctx.load_texture("qr-code", image, TextureOptions::NEAREST));
                let drawn = Drawn {
                    text: self.text.to_owned(),
                    texture: texture.clone(),
                };
                ctx.data_mut(|data| data.insert_temp(id, drawn));
                texture
            }
        };
        match texture {
            Some(texture) => ui.add(
                egui::Image::new(&texture)
                    .fit_to_exact_size(Vec2::splat(self.side))
                    .alt_text(self.text),
            ),
            None => ui.weak(crate::i18n::tr(ui.ctx(), "corundum-qr-too-long")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_has_its_quiet_zone_and_dark_finders() {
        let image = image("corundum").unwrap();
        let [width, height] = image.size;
        assert_eq!(width, height);
        // Version 1 is 21 modules; with the quiet zone on both sides, 29.
        assert_eq!(width, 21 + 2 * QUIET_ZONE);
        let at = |x: usize, y: usize| image.pixels[y * width + x];
        assert_eq!(at(0, 0), Color32::WHITE);
        assert_eq!(at(QUIET_ZONE - 1, QUIET_ZONE), Color32::WHITE);
        // The top-left finder pattern's corner.
        assert_eq!(at(QUIET_ZONE, QUIET_ZONE), Color32::BLACK);
    }

    #[test]
    fn longer_text_takes_a_bigger_code() {
        let short = image("a").unwrap().size[0];
        let long = image(&"a".repeat(200)).unwrap().size[0];
        assert!(long > short);
    }

    #[test]
    fn text_too_long_for_any_code_has_none() {
        assert!(image(&"x".repeat(8000)).is_none());
    }
}
