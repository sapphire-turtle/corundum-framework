//! An iPhone keyboard's text, turned into egui's events.
//!
//! winit 0.30's view on iOS takes text through `UIKeyInput` alone, and UIKit keeps the
//! keyboards that compose (Japanese, Chinese, Korean) away from such a view (#38). The view
//! the iOS window gives winit's (`ios::ime`) takes `UITextInput` too, and hands each call
//! here: text the keyboard is composing is UIKit's "marked text", which egui hears as an IME
//! preedit; text that settles, by the keyboard unmarking it or inserting over it, as an IME
//! commit. Typing outside a composition arrives as text and keys, in the order it was typed.
//!
//! The document UIKit sees is the marked text alone: the field's settled text lives in egui,
//! and a composition needs nothing before it. Offsets are in UTF-16 code units, as
//! `NSString` counts them; a span that does not fall on characters is taken as the nearest
//! one that does.

/// The marked text, and the events not yet handed to egui.
#[derive(Debug, Default)]
pub struct MarkedText {
    /// The text being composed; empty when nothing is.
    text: String,
    /// The selection inside it, as `start..end` in UTF-16 units.
    selection: (usize, usize),
    events: Vec<egui::Event>,
}

impl MarkedText {
    /// `setMarkedText:selectedRange:`: the keyboard is composing `text`, with `selection`
    /// (location, length) inside it. An empty `text` ends the composition with nothing.
    pub fn set(&mut self, text: &str, selection: (usize, usize)) {
        if text.is_empty() {
            if self.is_marked() {
                // An empty preedit is how egui hears a composition ended with nothing.
                self.events.push(preedit(""));
            }
            self.clear();
            return;
        }
        self.text = text.to_owned();
        let len = self.len();
        let start = selection.0.min(len);
        self.selection = (start, (start + selection.1).min(len));
        self.events.push(preedit(text));
    }

    /// `unmarkText`: the composition settles as it is.
    pub fn unmark(&mut self) {
        if self.is_marked() {
            self.events
                .push(egui::Event::Ime(egui::ImeEvent::Commit(self.text.clone())));
        }
        self.clear();
    }

    /// `insertText:`: `text` settles in place of the marked text, if any. A line break
    /// settles the composition first and is the Enter key.
    pub fn insert(&mut self, text: &str) {
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                self.unmark();
                self.events.push(key(egui::Key::Enter, true));
                self.events.push(key(egui::Key::Enter, false));
            }
            if line.is_empty() {
                continue;
            }
            if self.is_marked() {
                self.events
                    .push(egui::Event::Ime(egui::ImeEvent::Commit(line.to_owned())));
                self.clear();
            } else {
                self.events.push(egui::Event::Text(line.to_owned()));
            }
        }
    }

    /// `replaceRange:withText:`: the document, which is the marked text, with `start..end`
    /// replaced by `text`, settles.
    pub fn replace(&mut self, start: usize, end: usize, text: &str) {
        let (start, end) = (start.min(end), start.max(end));
        let mut settled = self.slice(0, start);
        settled.push_str(text);
        settled.push_str(&self.slice(end, self.len()));
        if self.is_marked() {
            self.events
                .push(egui::Event::Ime(egui::ImeEvent::Commit(settled)));
        } else if !settled.is_empty() {
            self.events.push(egui::Event::Text(settled));
        }
        self.clear();
    }

    /// `deleteBackward`: the Backspace key. UIKit edits the marked text itself while it
    /// composes, so this comes outside a composition.
    pub fn delete_backward(&mut self) {
        self.events.push(key(egui::Key::Backspace, true));
        self.events.push(key(egui::Key::Backspace, false));
    }

    /// `setSelectedTextRange:`: the caret moved inside the marked text.
    pub fn select(&mut self, start: usize, end: usize) {
        let len = self.len();
        self.selection = (start.min(end).min(len), start.max(end).min(len));
    }

    /// Focus moved to another field: forget the composition without settling it there.
    pub fn reset(&mut self) {
        self.clear();
    }

    /// The events since the last call, in the order they came.
    pub fn take_events(&mut self) -> Vec<egui::Event> {
        std::mem::take(&mut self.events)
    }

    /// Whether the keyboard is composing.
    pub fn is_marked(&self) -> bool {
        !self.text.is_empty()
    }

    /// The document's length: the marked text's, in UTF-16 units.
    pub fn len(&self) -> usize {
        self.text.encode_utf16().count()
    }

    /// The selection inside the marked text, as `start..end` in UTF-16 units.
    pub fn selection(&self) -> (usize, usize) {
        self.selection
    }

    /// The document's text from `start` to `end`, in UTF-16 units.
    pub fn slice(&self, start: usize, end: usize) -> String {
        let units: Vec<u16> = self.text.encode_utf16().collect();
        let end = end.min(units.len());
        let start = start.min(end);
        String::from_utf16_lossy(&units[start..end])
    }

    fn clear(&mut self) {
        self.text.clear();
        self.selection = (0, 0);
    }
}

fn preedit(text: &str) -> egui::Event {
    egui::Event::Ime(egui::ImeEvent::Preedit {
        text: text.to_owned(),
        active_range_chars: None,
    })
}

fn key(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> egui::Event {
        egui::Event::Text(s.into())
    }

    fn commit(s: &str) -> egui::Event {
        egui::Event::Ime(egui::ImeEvent::Commit(s.into()))
    }

    #[test]
    fn typing_outside_a_composition_is_text() {
        let mut marked = MarkedText::default();
        marked.insert("s");
        marked.insert("a");
        marked.delete_backward();
        assert_eq!(
            marked.take_events(),
            vec![
                text("s"),
                text("a"),
                key(egui::Key::Backspace, true),
                key(egui::Key::Backspace, false),
            ]
        );
        assert!(marked.take_events().is_empty());
    }

    #[test]
    fn a_composition_unmarked_is_a_preedit_then_a_commit() {
        let mut marked = MarkedText::default();
        marked.set("に", (1, 0));
        marked.set("にほん", (3, 0));
        marked.set("日本", (2, 0));
        assert!(marked.is_marked());
        marked.unmark();
        assert!(!marked.is_marked());
        assert_eq!(
            marked.take_events(),
            vec![
                preedit("に"),
                preedit("にほん"),
                preedit("日本"),
                commit("日本")
            ]
        );
    }

    #[test]
    fn text_inserted_over_a_composition_is_its_commit() {
        let mut marked = MarkedText::default();
        marked.set("か", (1, 0));
        marked.insert("下");
        marked.insert("a");
        assert_eq!(
            marked.take_events(),
            vec![preedit("か"), commit("下"), text("a")]
        );
    }

    #[test]
    fn a_composition_emptied_ends_with_an_empty_preedit() {
        let mut marked = MarkedText::default();
        marked.set("か", (1, 0));
        marked.set("", (0, 0));
        // Nothing marked: unmarking settles nothing.
        marked.unmark();
        assert_eq!(marked.take_events(), vec![preedit("か"), preedit("")]);
    }

    #[test]
    fn a_line_break_settles_the_composition_and_is_enter() {
        let mut marked = MarkedText::default();
        marked.set("にほん", (3, 0));
        marked.insert("\n");
        marked.insert("a\nb");
        assert_eq!(
            marked.take_events(),
            vec![
                preedit("にほん"),
                commit("にほん"),
                key(egui::Key::Enter, true),
                key(egui::Key::Enter, false),
                text("a"),
                key(egui::Key::Enter, true),
                key(egui::Key::Enter, false),
                text("b"),
            ]
        );
    }

    #[test]
    fn a_replaced_range_settles_the_whole_document() {
        let mut marked = MarkedText::default();
        marked.set("にほんご", (4, 0));
        // "ご" (UTF-16 offset 3..4) becomes "語".
        marked.replace(3, 4, "語");
        assert!(!marked.is_marked());
        // Outside a composition the document is empty: the replacement is plain text.
        marked.replace(0, 0, "x");
        assert_eq!(
            marked.take_events(),
            vec![preedit("にほんご"), commit("にほん語"), text("x")]
        );
    }

    #[test]
    fn reset_forgets_the_composition_silently() {
        let mut marked = MarkedText::default();
        marked.set("か", (1, 0));
        marked.take_events();
        marked.reset();
        marked.unmark();
        assert!(marked.take_events().is_empty());
    }

    #[test]
    fn offsets_count_utf16_units_and_are_clamped() {
        let mut marked = MarkedText::default();
        marked.set("a😀b", (10, 5));
        assert_eq!(marked.len(), 4);
        assert_eq!(marked.selection(), (4, 4));
        assert_eq!(marked.slice(1, 3), "😀");
        assert_eq!(marked.slice(3, 99), "b");
        assert_eq!(marked.slice(5, 2), "");
        marked.select(3, 1);
        assert_eq!(marked.selection(), (1, 3));
    }
}
