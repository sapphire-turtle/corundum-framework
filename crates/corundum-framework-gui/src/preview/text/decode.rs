//! A text file's first bytes made into text, with no egui in it: the worker of `TextPreview`
//! calls [`decode_text`].

use encoding_rs::{Decoder, DecoderResult, Encoding, UTF_8, UTF_16BE, UTF_16LE};

/// The most bytes of a file that are shown.
pub(crate) const TEXT_LIMIT: usize = 256 * 1024;

/// A file's text, as far as it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Text {
    pub text: String,
    /// Whether the file goes on past what was decoded.
    pub cut: bool,
    /// The encoding_rs name of what the bytes were read as, like "UTF-8" or "Shift_JIS".
    pub encoding: &'static str,
}

/// What a file that is not text is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotText {
    Binary,
}

/// `bytes` are the file's first bytes, at most `TEXT_LIMIT + 1` of them: more than
/// `TEXT_LIMIT` means the file was cut, and only the first `TEXT_LIMIT` are decoded.
///
/// A BOM decides the encoding; without one a NUL byte means a binary file, valid UTF-8 means
/// UTF-8, and anything else is guessed by chardetng. When cut, a character cut in two at the
/// end is dropped rather than shown as U+FFFD.
pub(crate) fn decode_text(bytes: &[u8]) -> Result<Text, NotText> {
    let cut = bytes.len() > TEXT_LIMIT;
    let bytes = &bytes[..bytes.len().min(TEXT_LIMIT)];

    let (encoding, body) = if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        (UTF_8, rest)
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        (UTF_16LE, rest)
    } else if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        (UTF_16BE, rest)
    } else if bytes.contains(&0) {
        return Err(NotText::Binary);
    } else if utf8_prefix_ok(bytes, cut) {
        (UTF_8, bytes)
    } else {
        let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
        detector.feed(bytes, !cut);
        (detector.guess(None, chardetng::Utf8Detection::Allow), bytes)
    };

    Ok(Text {
        text: decode(encoding, body, cut),
        cut,
        encoding: encoding.name(),
    })
}

/// Whether `bytes` are UTF-8, but for an incomplete character at the end when `cut`.
fn utf8_prefix_ok(bytes: &[u8], cut: bool) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(e) => cut && e.error_len().is_none(),
    }
}

/// Decode `bytes`. With `cut`, the decoder is not told the input has ended, so it holds back
/// a trailing partial character instead of emitting U+FFFD; a malformed sequence elsewhere
/// is replaced.
fn decode(encoding: &'static Encoding, bytes: &[u8], cut: bool) -> String {
    let mut decoder: Decoder = encoding.new_decoder_without_bom_handling();
    let mut out = String::new();
    out.reserve(
        decoder
            .max_utf8_buffer_length_without_replacement(bytes.len())
            .unwrap_or(bytes.len() * 3),
    );
    let mut rest = bytes;
    loop {
        let (result, read) = decoder.decode_to_string_without_replacement(rest, &mut out, !cut);
        rest = &rest[read..];
        match result {
            DecoderResult::InputEmpty => return out,
            DecoderResult::OutputFull => out.reserve(rest.len().max(16) * 3),
            DecoderResult::Malformed(..) => out.push('\u{FFFD}'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(bytes: &[u8]) -> Text {
        decode_text(bytes).expect("text")
    }

    #[test]
    fn utf8_without_a_bom() {
        let t = ok("メモ\nnote".as_bytes());
        assert_eq!(t.text, "メモ\nnote");
        assert_eq!(t.encoding, "UTF-8");
        assert!(!t.cut);
    }

    #[test]
    fn utf8_bom_is_dropped() {
        let t = ok(b"\xEF\xBB\xBFa");
        assert_eq!(t.text, "a");
        assert_eq!(t.encoding, "UTF-8");
    }

    #[test]
    fn utf16_with_a_bom_is_text_not_binary() {
        let t = ok(b"\xFF\xFEa\0b\0");
        assert_eq!((t.text.as_str(), t.encoding), ("ab", "UTF-16LE"));
        let t = ok(b"\xFE\xFF\0a");
        assert_eq!((t.text.as_str(), t.encoding), ("a", "UTF-16BE"));
    }

    #[test]
    fn a_shift_jis_cue_sheet() {
        let src = "TITLE \"夜の歌\"\r\nPERFORMER \"山田\"\r\n";
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(src);
        let t = ok(&bytes);
        assert!(t.text.contains("夜の歌"), "{:?}", t.text);
        assert_eq!(t.encoding, "Shift_JIS");
    }

    #[test]
    fn euc_jp_is_read() {
        let src = "吾輩は猫である。名前はまだ無い。\nどこで生れたかとんと見当がつかぬ。\n何でも薄暗いじめじめした所でニャーニャー泣いていた事だけは記憶している。\n";
        let (bytes, _, _) = encoding_rs::EUC_JP.encode(src);
        let t = ok(&bytes);
        assert_eq!(t.text, src);
        assert_eq!(t.encoding, "EUC-JP");
    }

    #[test]
    fn a_nul_without_a_bom_is_binary() {
        assert_eq!(
            decode_text(&[0x89, b'P', b'N', b'G', 0, 0]),
            Err(NotText::Binary)
        );
    }

    #[test]
    fn an_empty_file_is_empty_text() {
        let t = ok(&[]);
        assert_eq!(t.text, "");
        assert!(!t.cut);
    }

    #[test]
    fn ascii_is_itself() {
        let t = ok(b"hello, world\n");
        assert_eq!(t.text, "hello, world\n");
        assert_eq!(t.encoding, "UTF-8");
    }

    #[test]
    fn over_the_limit_is_cut() {
        let t = ok(&vec![b'a'; TEXT_LIMIT + 1]);
        assert_eq!(t.text.len(), TEXT_LIMIT);
        assert!(t.cut);
    }

    #[test]
    fn a_cut_inside_a_character_drops_the_partial_character() {
        let bytes = "あ".repeat(TEXT_LIMIT / 3 + 2).into_bytes();
        let t = ok(&bytes[..TEXT_LIMIT + 1]);
        assert!(!t.text.contains('\u{FFFD}'));
        assert_eq!(t.text.len() % 3, 0);
        assert!(t.cut);
    }

    #[test]
    fn a_shift_jis_cut_inside_a_character_drops_it() {
        let src = "あ".repeat(TEXT_LIMIT / 2 + 4);
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(&src);
        // TEXT_LIMIT is even, so the kept bytes end on a character; shift the cut by one.
        let t = ok(&bytes[..TEXT_LIMIT + 1]);
        assert!(!t.text.contains('\u{FFFD}'));
        assert!(t.cut);
        let mut odd = vec![b'x'];
        odd.extend_from_slice(&bytes[..TEXT_LIMIT + 1]);
        let t = ok(&odd);
        assert!(!t.text.contains('\u{FFFD}'));
        assert!(t.cut);
    }
}
