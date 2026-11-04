//! When to raise and lower a phone's soft keyboard, from what egui asks frame by frame.
//!
//! egui asks for the keyboard (`PlatformOutput::ime`) on every frame a text field has the
//! focus, and egui-winit raises and lowers it the moment that changes. On a phone both are
//! wrong: the keyboard slides in and out over a few hundred milliseconds, and egui lets the
//! focus go for a frame or two at a time — a press on a toolbar button that gives the focus
//! back to its editor, or every `request_focus`, which egui-winit turns into a lower and a
//! raise to end the IME's composition. Each such blink would be the keyboard leaving and
//! coming back. So the keyboard goes up as soon as a field asks, and down only once no field
//! has asked for [`LOWER_AFTER`], with no finger on the screen.

use std::time::{Duration, Instant};

/// How long no field must ask for the keyboard before it goes down.
pub const LOWER_AFTER: Duration = Duration::from_millis(200);

/// What to do with the keyboard this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardCall {
    Raise,
    Lower,
}

/// This frame, as far as the keyboard is concerned.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// A text field has the focus and takes typing.
    pub wanted: bool,
    /// A tap ended this frame: on a field that has the keyboard, it brings back a keyboard
    /// the user put away (the back gesture) without the field losing focus.
    pub tapped: bool,
    /// A finger is on the screen: perhaps on a button that will give the focus back.
    pub pointer_down: bool,
    pub now: Instant,
}

/// Whether the keyboard is up, and when a field last asked for it.
#[derive(Debug, Default)]
pub struct SoftKeyboard {
    raised: bool,
    wanted_at: Option<Instant>,
}

impl SoftKeyboard {
    /// What to tell the platform this frame, if anything.
    pub fn update(&mut self, frame: Frame) -> Option<KeyboardCall> {
        if frame.wanted {
            self.wanted_at = Some(frame.now);
            if !self.raised || frame.tapped {
                self.raised = true;
                return Some(KeyboardCall::Raise);
            }
            return None;
        }
        let waited = self
            .wanted_at
            .is_none_or(|at| frame.now.duration_since(at) >= LOWER_AFTER);
        if self.raised && waited && !frame.pointer_down {
            self.raised = false;
            return Some(KeyboardCall::Lower);
        }
        None
    }

    /// Whether the keyboard is up and waiting to be lowered: the caller looks again within
    /// [`LOWER_AFTER`], since no event may come to wake it.
    pub fn lowering(&self, wanted: bool) -> bool {
        self.raised && !wanted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Clock(Instant);

    impl Clock {
        fn frame(&mut self, ms: u64, wanted: bool) -> Frame {
            self.0 += Duration::from_millis(ms);
            Frame {
                wanted,
                tapped: false,
                pointer_down: false,
                now: self.0,
            }
        }
    }

    #[test]
    fn a_field_raises_it_at_once_and_only_once() {
        let mut clock = Clock(Instant::now());
        let mut kb = SoftKeyboard::default();
        assert_eq!(kb.update(clock.frame(0, false)), None);
        assert_eq!(kb.update(clock.frame(16, true)), Some(KeyboardCall::Raise));
        assert_eq!(kb.update(clock.frame(16, true)), None);
        assert_eq!(kb.update(clock.frame(16, true)), None);
    }

    #[test]
    fn a_blink_in_the_focus_does_not_lower_it() {
        let mut clock = Clock(Instant::now());
        let mut kb = SoftKeyboard::default();
        kb.update(clock.frame(0, true));
        for _ in 0..3 {
            assert_eq!(kb.update(clock.frame(16, false)), None);
        }
        assert_eq!(kb.update(clock.frame(16, true)), None);
    }

    #[test]
    fn it_goes_down_once_no_field_has_asked_for_a_while() {
        let mut clock = Clock(Instant::now());
        let mut kb = SoftKeyboard::default();
        kb.update(clock.frame(0, true));
        assert_eq!(kb.update(clock.frame(100, false)), None);
        assert!(kb.lowering(false));
        assert_eq!(
            kb.update(clock.frame(100, false)),
            Some(KeyboardCall::Lower)
        );
        assert!(!kb.lowering(false));
        assert_eq!(kb.update(clock.frame(500, false)), None);
    }

    #[test]
    fn a_finger_on_the_screen_holds_it_up() {
        let mut clock = Clock(Instant::now());
        let mut kb = SoftKeyboard::default();
        kb.update(clock.frame(0, true));
        let mut held = clock.frame(1000, false);
        held.pointer_down = true;
        assert_eq!(kb.update(held), None);
        assert_eq!(kb.update(clock.frame(16, false)), Some(KeyboardCall::Lower));
    }

    #[test]
    fn a_tap_on_the_field_brings_back_a_keyboard_put_away() {
        let mut clock = Clock(Instant::now());
        let mut kb = SoftKeyboard::default();
        kb.update(clock.frame(0, true));
        let mut tap = clock.frame(16, true);
        tap.tapped = true;
        assert_eq!(kb.update(tap), Some(KeyboardCall::Raise));
    }
}
