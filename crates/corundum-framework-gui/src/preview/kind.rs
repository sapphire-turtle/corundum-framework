//! What a file is, by its name.

use crate::explorer::TEXT_EXTENSIONS;
use crate::platform::{AUDIO_EXTENSIONS, IMAGE_EXTENSIONS, VIDEO_EXTENSIONS};

/// What a file is, as far as its name says: which preview suits it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Image,
    Audio,
    Video,
    Text,
    Other,
}

/// The kind of the file called `name`, by its extension: from the platform's image, audio
/// and video extensions and the explorer's text ones. `svg` is an image.
pub fn kind_of(name: &str) -> Kind {
    let Some((_, extension)) = name.rsplit_once('.') else {
        return Kind::Other;
    };
    let extension = extension.to_ascii_lowercase();
    let is = |list: &[&str]| list.contains(&extension.as_str());
    if is(IMAGE_EXTENSIONS) || extension == "svg" {
        Kind::Image
    } else if is(AUDIO_EXTENSIONS) {
        Kind::Audio
    } else if is(VIDEO_EXTENSIONS) {
        Kind::Video
    } else if is(TEXT_EXTENSIONS) {
        Kind::Text
    } else {
        Kind::Other
    }
}
