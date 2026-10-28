//! Which entries are picked, and how a click or a key changes that. Entries are known by
//! name, so a sort or a new page keeps what is picked; each change is given the names
//! shown, in order, to find places by.

use std::collections::BTreeSet;

#[derive(Default, Debug)]
pub(super) struct Selection {
    pub(super) names: BTreeSet<String>,
    /// Where a Shift range starts: the entry last clicked or moved to without Shift.
    anchor: Option<String>,
    /// The entry the keys move from, outlined while the explorer is focused.
    pub(super) cursor: Option<String>,
}

impl Selection {
    pub(super) fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// The cursor's place among `shown`.
    pub(super) fn cursor_at(&self, shown: &[String]) -> Option<usize> {
        place(shown, self.cursor.as_deref())
    }

    /// A click on the entry at `at`: it alone; with `command`, it added or taken away;
    /// with `shift`, the range from the anchor to it (added to what is picked, with
    /// `command` too).
    pub(super) fn click(&mut self, shown: &[String], at: usize, command: bool, shift: bool) {
        let name = shown[at].clone();
        if shift {
            let from = place(shown, self.anchor.as_deref()).unwrap_or(at);
            if !command {
                self.names.clear();
            }
            self.names
                .extend(shown[from.min(at)..=from.max(at)].iter().cloned());
            if self.anchor.is_none() {
                self.anchor = Some(name.clone());
            }
        } else if command {
            if !self.names.remove(&name) {
                self.names.insert(name.clone());
            }
            self.anchor = Some(name.clone());
        } else {
            self.only(name.clone());
        }
        self.cursor = Some(name);
    }

    /// Pick the entry `name` alone, as a right click on one not picked does.
    pub(super) fn only(&mut self, name: String) {
        self.names = BTreeSet::from([name.clone()]);
        self.anchor = Some(name.clone());
        self.cursor = Some(name);
    }

    /// Move the cursor by `by` places (clamped to the ends): from nowhere, to the first
    /// entry (or the last, moving back). The entry it lands on is picked alone, or with
    /// `shift` the range from the anchor to it.
    pub(super) fn step(&mut self, shown: &[String], by: isize, shift: bool) {
        if shown.is_empty() {
            return;
        }
        let at = match self.cursor_at(shown) {
            Some(at) => at.saturating_add_signed(by).min(shown.len() - 1),
            None if by < 0 => shown.len() - 1,
            None => 0,
        };
        self.move_to(shown, at, shift);
    }

    /// Move the cursor to `at`, as [`step`](Self::step) does.
    pub(super) fn move_to(&mut self, shown: &[String], at: usize, shift: bool) {
        let Some(name) = shown.get(at).cloned() else {
            return;
        };
        if shift {
            let from = place(shown, self.anchor.as_deref()).unwrap_or(at);
            self.names = shown[from.min(at)..=from.max(at)].iter().cloned().collect();
            if self.anchor.is_none() {
                self.anchor = Some(name.clone());
            }
            self.cursor = Some(name);
        } else {
            self.only(name);
        }
    }

    /// Pick every entry shown.
    pub(super) fn all(&mut self, shown: &[String]) {
        self.names = shown.iter().cloned().collect();
    }

    /// Pick nothing; the cursor stays.
    pub(super) fn none(&mut self) {
        self.names.clear();
        self.anchor = None;
    }

    pub(super) fn clear(&mut self) {
        *self = Selection::default();
    }

    /// Let go of the names no longer shown.
    pub(super) fn retain(&mut self, shown: &[String]) {
        let shown: BTreeSet<&String> = shown.iter().collect();
        self.names.retain(|n| shown.contains(n));
        for name in [&mut self.anchor, &mut self.cursor] {
            if name.as_ref().is_some_and(|n| !shown.contains(n)) {
                *name = None;
            }
        }
    }
}

fn place(shown: &[String], name: Option<&str>) -> Option<usize> {
    let name = name?;
    shown.iter().position(|n| n == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown() -> Vec<String> {
        ["a", "b", "c", "d", "e"].map(String::from).to_vec()
    }

    fn picked(s: &Selection) -> Vec<&str> {
        s.names.iter().map(String::as_str).collect()
    }

    #[test]
    fn a_click_picks_one_and_command_toggles() {
        let (shown, mut s) = (shown(), Selection::default());
        s.click(&shown, 1, false, false);
        assert_eq!(picked(&s), ["b"]);
        s.click(&shown, 3, true, false);
        assert_eq!(picked(&s), ["b", "d"]);
        s.click(&shown, 1, true, false);
        assert_eq!(picked(&s), ["d"]);
        s.click(&shown, 0, false, false);
        assert_eq!(picked(&s), ["a"]);
    }

    #[test]
    fn shift_picks_the_range_from_the_anchor() {
        let (shown, mut s) = (shown(), Selection::default());
        s.click(&shown, 1, false, false);
        s.click(&shown, 3, false, true);
        assert_eq!(picked(&s), ["b", "c", "d"]);
        // The anchor stays: back past it, the range turns round.
        s.click(&shown, 0, false, true);
        assert_eq!(picked(&s), ["a", "b"]);
        // With command, the range is added.
        s.click(&shown, 4, true, false);
        s.click(&shown, 3, true, true);
        assert_eq!(picked(&s), ["a", "b", "d", "e"]);
    }

    #[test]
    fn the_keys_move_and_extend() {
        let (shown, mut s) = (shown(), Selection::default());
        s.step(&shown, 1, false);
        assert_eq!(picked(&s), ["a"]);
        s.step(&shown, 2, false);
        assert_eq!(picked(&s), ["c"]);
        s.step(&shown, 1, true);
        s.step(&shown, 1, true);
        assert_eq!(picked(&s), ["c", "d", "e"]);
        s.step(&shown, 10, false);
        assert_eq!(picked(&s), ["e"]);
        s.step(&shown, -10, false);
        assert_eq!(picked(&s), ["a"]);
        s.move_to(&shown, 2, true);
        assert_eq!(picked(&s), ["a", "b", "c"]);
    }

    #[test]
    fn from_nowhere_back_goes_to_the_last() {
        let (shown, mut s) = (shown(), Selection::default());
        s.step(&shown, -1, false);
        assert_eq!(picked(&s), ["e"]);
        s.step(&[], 1, false);
        assert_eq!(picked(&s), ["e"]);
    }

    #[test]
    fn names_gone_are_let_go() {
        let (shown, mut s) = (shown(), Selection::default());
        s.all(&shown);
        s.cursor = Some("e".into());
        s.retain(&shown[..2]);
        assert_eq!(picked(&s), ["a", "b"]);
        assert_eq!(s.cursor, None);
        s.none();
        assert!(s.names.is_empty());
    }
}
