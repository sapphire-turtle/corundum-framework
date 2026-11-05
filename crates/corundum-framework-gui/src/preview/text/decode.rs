//! A text file's first bytes made into text, with no egui in it: the worker of `TextPreview`
//! calls [`decode_text`].

/// The most bytes of a file that are shown.
pub(crate) const TEXT_LIMIT: usize = 256 * 1024;

/// A file's text, as far as it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Text {
    pub text: String,
    /// Whether the file goes on past what was decoded.
    pub cut: bool,
    /// What the bytes were read as: "UTF-8", "UTF-16LE" or "UTF-16BE".
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
/// Only Unicode is read. A UTF-8 or UTF-16 BOM decides the encoding; without one a NUL byte
/// means a binary file, and anything else is UTF-8, where bytes that are not UTF-8 (a file in
/// a legacy encoding) show as U+FFFD. When cut, a character cut in two at the end is dropped
/// rather than shown as U+FFFD.
pub(crate) fn decode_text(bytes: &[u8]) -> Result<Text, NotText> {
    let cut = bytes.len() > TEXT_LIMIT;
    let bytes = &bytes[..bytes.len().min(TEXT_LIMIT)];

    let (text, encoding) = if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        (utf8(rest, cut), "UTF-8")
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        (utf16(rest, cut, u16::from_le_bytes), "UTF-16LE")
    } else if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        (utf16(rest, cut, u16::from_be_bytes), "UTF-16BE")
    } else if bytes.contains(&0) {
        return Err(NotText::Binary);
    } else {
        (utf8(bytes, cut), "UTF-8")
    };

    Ok(Text {
        text,
        cut,
        encoding,
    })
}

/// `bytes` as UTF-8, invalid bytes replaced; with `cut`, an incomplete character at the end
/// is dropped.
fn utf8(bytes: &[u8], cut: bool) -> String {
    let bytes = match std::str::from_utf8(bytes) {
        Err(e) if cut && e.error_len().is_none() => &bytes[..e.valid_up_to()],
        _ => bytes,
    };
    String::from_utf8_lossy(bytes).into_owned()
}

/// `bytes` as UTF-16; with `cut`, a trailing odd byte and an unpaired leading surrogate at
/// the end are dropped.
fn utf16(bytes: &[u8], cut: bool, unit: fn([u8; 2]) -> u16) -> String {
    let mut units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| unit(*pair))
        .collect();
    if cut && units.last().is_some_and(|u| (0xD800..0xDC00).contains(u)) {
        units.pop();
    }
    char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
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
    fn utf16_beyond_the_basic_plane() {
        let mut bytes = vec![0xFF, 0xFE];
        for u in "a😀".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(ok(&bytes).text, "a😀");
    }

    #[test]
    fn a_utf16_cut_inside_a_code_unit_or_pair_drops_the_partial() {
        let units: Vec<u16> = "a".repeat(TEXT_LIMIT / 2).encode_utf16().collect();
        let mut bytes = vec![0xFF, 0xFE];
        for u in &units {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        // 2 BOM bytes + units: cut after an odd number of body bytes.
        let t = ok(&bytes[..TEXT_LIMIT + 1]);
        assert!(t.cut);
        assert!(!t.text.contains('\u{FFFD}'));
        // Cut between the halves of a surrogate pair.
        let mut bytes = vec![0xFF, 0xFE];
        for u in "a".repeat(TEXT_LIMIT / 2 - 2).encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        for u in "😀".encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(bytes.len(), TEXT_LIMIT + 2);
        let t = ok(&bytes[..TEXT_LIMIT + 1]);
        assert!(t.cut);
        assert!(
            !t.text.contains('\u{FFFD}'),
            "{:?}",
            &t.text[t.text.len() - 4..]
        );
    }

    #[test]
    fn invalid_utf8_is_shown_lossily() {
        assert_eq!(ok(b"ok \xff ok").text, "ok \u{FFFD} ok");
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
}
