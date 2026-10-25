//! The keyboards that compose (Japanese, Chinese, Korean) on the window's view.
//!
//! winit 0.30's view takes text through `UIKeyInput` alone, and UIKit keeps the keyboards
//! that compose away from such a view: they type nothing (#38). [`install`] makes winit's
//! view a [`TEXT_VIEW`], a subclass made at run time that adopts `UITextInput` too. Every
//! text call to it goes to a [`MarkedText`], whose events egui takes before each frame
//! ([`take_events`]): plain typing and the composition alike, so egui hears them in the
//! order they were typed. Remove this once winit's view adopts `UITextInput`
//! (rust-windowing/winit#4592).

use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::sync::OnceLock;

use objc2::rc::{Allocated, Retained, Weak};
use objc2::runtime::{AnyClass, AnyObject, AnyProtocol, ClassBuilder, NSObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2::{class, define_class, msg_send, sel};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSArray, NSRange, NSString};
use objc2_ui_kit::{UITextPosition, UITextRange, UIView};

use crate::window::marked_text::MarkedText;

/// The name of the view class made at run time: winit's, with `UITextInput`.
const TEXT_VIEW: &CStr = c"CorundumTextInputView";

define_class!(
    // SAFETY: UITextPosition has no subclassing requirements, and the class does not
    // implement Drop.
    #[unsafe(super(UITextPosition, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CorundumTextPosition"]
    #[ivars = Cell<usize>]
    struct TextPosition;
);

define_class!(
    // SAFETY: UITextRange's subclasses override `start`, `end` and `isEmpty`, as here, and
    // the class does not implement Drop.
    #[unsafe(super(UITextRange, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "CorundumTextRange"]
    #[ivars = Cell<(usize, usize)>]
    struct TextRange;

    impl TextRange {
        #[unsafe(method_id(start))]
        fn start(&self) -> Retained<UITextPosition> {
            TextPosition::new(self.mtm(), self.ivars().get().0).into_super()
        }

        #[unsafe(method_id(end))]
        fn end(&self) -> Retained<UITextPosition> {
            TextPosition::new(self.mtm(), self.ivars().get().1).into_super()
        }

        #[unsafe(method(isEmpty))]
        fn is_empty(&self) -> bool {
            let (start, end) = self.ivars().get();
            start == end
        }
    }
);

impl TextPosition {
    fn new(mtm: MainThreadMarker, offset: usize) -> Retained<TextPosition> {
        let this = TextPosition::alloc(mtm).set_ivars(Cell::new(offset));
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    }
}

impl TextRange {
    fn new(mtm: MainThreadMarker, start: usize, end: usize) -> Retained<TextRange> {
        let this = TextRange::alloc(mtm).set_ivars(Cell::new((start.min(end), start.max(end))));
        // SAFETY: NSObject's `init`.
        unsafe { msg_send![super(this), init] }
    }
}

/// The keyboard's state, on the main thread.
#[derive(Default)]
struct State {
    marked: MarkedText,
    /// Woken when events arrive, for a frame to take them.
    ctx: Option<egui::Context>,
    /// Where the text cursor is, in the view's points: the keyboard shows its candidates
    /// there.
    caret: CGRect,
    /// UIKit's object told when the text changes from outside the keyboard.
    input_delegate: Option<Weak<AnyObject>>,
    tokenizer: Option<Retained<AnyObject>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Run `f` on the state, or return `T::default()` when it is busy: these are called from
/// UIKit, which must not be unwound into.
fn with<T: Default>(f: impl FnOnce(&mut State) -> T) -> T {
    STATE.with(|state| match state.try_borrow_mut() {
        Ok(mut state) => f(&mut state),
        Err(_) => T::default(),
    })
}

/// Change the marked text with `f`, and wake egui to take the events.
fn edit(f: impl FnOnce(&mut MarkedText)) {
    let ctx = with(|state| {
        f(&mut state.marked);
        state.ctx.clone()
    });
    if let Some(ctx) = ctx {
        ctx.request_repaint();
    }
}

/// Make `view`, winit's, take the keyboards that compose. Its text goes to egui through
/// [`take_events`], and `ctx` is woken for it. Returns whether the view took them.
pub(super) fn install(view: &UIView, ctx: &egui::Context) -> bool {
    let Some(class) = text_view_class(view.class()) else {
        return false;
    };
    with(|state| state.ctx = Some(ctx.clone()));
    // SAFETY: the class is a subclass of the view's own with no instance variables of its
    // own, so the view is laid out as one.
    unsafe {
        objc2::ffi::object_setClass(
            (view as *const UIView).cast_mut().cast(),
            (class as *const AnyClass).cast(),
        );
    }
    true
}

/// The events since the last call, for egui's next frame.
pub(super) fn take_events() -> Vec<egui::Event> {
    with(|state| state.marked.take_events())
}

/// The text cursor is at `rect`, in the view's points.
pub(super) fn set_caret(rect: egui::Rect) {
    with(|state| {
        state.caret = CGRect::new(
            CGPoint::new(rect.min.x.into(), rect.min.y.into()),
            CGSize::new(rect.width().into(), rect.height().into()),
        );
    });
}

/// Focus moved to another field: drop the composition, and tell UIKit the text changed so
/// the keyboard drops it too.
pub(super) fn reset(view: &UIView) {
    let delegate = with(|state| {
        if !state.marked.is_marked() {
            return None;
        }
        state.input_delegate.as_ref().and_then(Weak::load)
    });
    // Outside the state: the delegate asks the view about its text in between.
    // SAFETY: `UITextInputDelegate`'s methods, given the view, which adopts `UITextInput`.
    if let Some(delegate) = &delegate {
        unsafe {
            let _: () = msg_send![delegate, textWillChange: view];
        }
    }
    with(|state| state.marked.reset());
    if let Some(delegate) = &delegate {
        unsafe {
            let _: () = msg_send![delegate, textDidChange: view];
        }
    }
}

/// The subclass of the view's class `base` that adopts `UITextInput`, made once. `base` is
/// winit's view class, or AccessKit's subclass of it, which the view already is by then.
/// `None` when the view is not winit's or UIKit has no such protocol.
fn text_view_class(base: &AnyClass) -> Option<&'static AnyClass> {
    static CLASS: OnceLock<Option<&'static AnyClass>> = OnceLock::new();
    let winit = std::iter::successors(Some(base), |class| class.superclass())
        .any(|class| class.name() == c"WinitUIView");
    if !winit {
        tracing::warn!(
            "the window's view is a {:?}, not winit's: keyboards that compose are left out",
            base.name()
        );
        return None;
    }
    *CLASS.get_or_init(|| {
        let Some(protocol) = AnyProtocol::get(c"UITextInput") else {
            tracing::warn!("UIKit has no UITextInput: keyboards that compose are left out");
            return None;
        };
        let mut builder = ClassBuilder::new(TEXT_VIEW, base)?;
        builder.add_protocol(protocol);
        // SAFETY: each function has the signature of the `UIKeyInput` or `UITextInput`
        // method it is added as.
        unsafe { add_methods(&mut builder) };
        Some(builder.register())
    })
}

/// The methods of `UITextInput`, and `UIKeyInput`'s two that change text.
unsafe fn add_methods(builder: &mut ClassBuilder) {
    unsafe {
        builder.add_method(
            sel!(insertText:),
            insert_text as extern "C-unwind" fn(_, _, _),
        );
        builder.add_method(
            sel!(deleteBackward),
            delete_backward as extern "C-unwind" fn(_, _),
        );
        builder.add_method(
            sel!(textInRange:),
            text_in_range as extern "C-unwind" fn(_, _, _) -> _,
        );
        builder.add_method(
            sel!(replaceRange:withText:),
            replace_range as extern "C-unwind" fn(_, _, _, _),
        );
        builder.add_method(
            sel!(selectedTextRange),
            selected_text_range as extern "C-unwind" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(setSelectedTextRange:),
            set_selected_text_range as extern "C-unwind" fn(_, _, _),
        );
        builder.add_method(
            sel!(markedTextRange),
            marked_text_range as extern "C-unwind" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(markedTextStyle),
            nothing as extern "C-unwind" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(setMarkedTextStyle:),
            ignore as extern "C-unwind" fn(_, _, _),
        );
        builder.add_method(
            sel!(setMarkedText:selectedRange:),
            set_marked_text as extern "C-unwind" fn(_, _, _, _),
        );
        builder.add_method(sel!(unmarkText), unmark_text as extern "C-unwind" fn(_, _));
        builder.add_method(
            sel!(beginningOfDocument),
            beginning_of_document as extern "C-unwind" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(endOfDocument),
            end_of_document as extern "C-unwind" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(textRangeFromPosition:toPosition:),
            text_range_from_position as extern "C-unwind" fn(_, _, _, _) -> _,
        );
        builder.add_method(
            sel!(positionFromPosition:offset:),
            position_from_position as extern "C-unwind" fn(_, _, _, _) -> _,
        );
        builder.add_method(
            sel!(positionFromPosition:inDirection:offset:),
            position_in_direction as extern "C-unwind" fn(_, _, _, _, _) -> _,
        );
        builder.add_method(
            sel!(comparePosition:toPosition:),
            compare_position as extern "C-unwind" fn(_, _, _, _) -> _,
        );
        builder.add_method(
            sel!(offsetFromPosition:toPosition:),
            offset_from_position as extern "C-unwind" fn(_, _, _, _) -> _,
        );
        builder.add_method(
            sel!(inputDelegate),
            input_delegate as extern "C-unwind" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(setInputDelegate:),
            set_input_delegate as extern "C-unwind" fn(_, _, _),
        );
        builder.add_method(
            sel!(tokenizer),
            tokenizer as extern "C-unwind" fn(_, _) -> _,
        );
        builder.add_method(
            sel!(positionWithinRange:farthestInDirection:),
            position_within_range as extern "C-unwind" fn(_, _, _, _) -> _,
        );
        builder.add_method(
            sel!(characterRangeByExtendingPosition:inDirection:),
            character_range_by_extending as extern "C-unwind" fn(_, _, _, _) -> _,
        );
        builder.add_method(
            sel!(baseWritingDirectionForPosition:inDirection:),
            base_writing_direction as extern "C-unwind" fn(_, _, _, _) -> _,
        );
        builder.add_method(
            sel!(setBaseWritingDirection:forRange:),
            set_base_writing_direction as extern "C-unwind" fn(_, _, _, _),
        );
        builder.add_method(
            sel!(firstRectForRange:),
            first_rect_for_range as extern "C-unwind" fn(_, _, _) -> _,
        );
        builder.add_method(
            sel!(caretRectForPosition:),
            caret_rect_for_position as extern "C-unwind" fn(_, _, _) -> _,
        );
        builder.add_method(
            sel!(selectionRectsForRange:),
            selection_rects_for_range as extern "C-unwind" fn(_, _, _) -> _,
        );
        builder.add_method(
            sel!(closestPositionToPoint:),
            closest_position_to_point as extern "C-unwind" fn(_, _, _) -> _,
        );
        builder.add_method(
            sel!(closestPositionToPoint:withinRange:),
            closest_position_within_range as extern "C-unwind" fn(_, _, _, _) -> _,
        );
        builder.add_method(
            sel!(characterRangeAtPoint:),
            character_range_at_point as extern "C-unwind" fn(_, _, _) -> _,
        );
    }
}

/// `UITextLayoutDirection`'s left and up: towards the document's start.
fn is_backward(direction: isize) -> bool {
    matches!(direction, 3 | 4)
}

fn mtm(_view: &AnyObject) -> MainThreadMarker {
    // SAFETY: UIKit sends a view's text input methods on the main thread.
    unsafe { MainThreadMarker::new_unchecked() }
}

fn offset_of(position: Option<&AnyObject>) -> Option<usize> {
    Some(position?.downcast_ref::<TextPosition>()?.ivars().get())
}

fn range_of(range: Option<&AnyObject>) -> Option<(usize, usize)> {
    Some(range?.downcast_ref::<TextRange>()?.ivars().get())
}

/// The text of `text`, an `NSString` or an `NSAttributedString`.
fn string_of(text: Option<&AnyObject>) -> String {
    let Some(text) = text else {
        return String::new();
    };
    if let Some(text) = text.downcast_ref::<NSString>() {
        return text.to_string();
    }
    // SAFETY: an object that answers `string` with an NSString, as NSAttributedString does.
    if text.class().responds_to(sel!(string)) {
        let string: Option<Retained<NSString>> = unsafe { msg_send![text, string] };
        return string.map(|s| s.to_string()).unwrap_or_default();
    }
    String::new()
}

fn position(view: &AnyObject, offset: usize) -> *mut AnyObject {
    Retained::autorelease_return(TextPosition::new(mtm(view), offset)).cast()
}

fn range(view: &AnyObject, start: usize, end: usize) -> *mut AnyObject {
    Retained::autorelease_return(TextRange::new(mtm(view), start, end)).cast()
}

fn document_len() -> usize {
    with(|state| state.marked.len())
}

extern "C-unwind" fn insert_text(_this: &AnyObject, _cmd: Sel, text: Option<&AnyObject>) {
    let text = string_of(text);
    edit(|marked| marked.insert(&text));
}

extern "C-unwind" fn delete_backward(_this: &AnyObject, _cmd: Sel) {
    edit(MarkedText::delete_backward);
}

extern "C-unwind" fn text_in_range(
    _this: &AnyObject,
    _cmd: Sel,
    range: Option<&AnyObject>,
) -> *mut AnyObject {
    let text = match range_of(range) {
        Some((start, end)) => with(|state| state.marked.slice(start, end)),
        None => String::new(),
    };
    Retained::autorelease_return(NSString::from_str(&text)).cast()
}

extern "C-unwind" fn replace_range(
    _this: &AnyObject,
    _cmd: Sel,
    range: Option<&AnyObject>,
    text: Option<&AnyObject>,
) {
    let text = string_of(text);
    let (start, end) = range_of(range).unwrap_or((0, document_len()));
    edit(|marked| marked.replace(start, end, &text));
}

extern "C-unwind" fn selected_text_range(this: &AnyObject, _cmd: Sel) -> *mut AnyObject {
    let (start, end) = with(|state| {
        if state.marked.is_marked() {
            state.marked.selection()
        } else {
            (0, 0)
        }
    });
    range(this, start, end)
}

extern "C-unwind" fn set_selected_text_range(
    _this: &AnyObject,
    _cmd: Sel,
    range: Option<&AnyObject>,
) {
    if let Some((start, end)) = range_of(range) {
        with(|state| state.marked.select(start, end));
    }
}

extern "C-unwind" fn marked_text_range(this: &AnyObject, _cmd: Sel) -> *mut AnyObject {
    let len = with(|state| state.marked.is_marked().then(|| state.marked.len()));
    match len {
        Some(len) => range(this, 0, len),
        None => std::ptr::null_mut(),
    }
}

extern "C-unwind" fn nothing(_this: &AnyObject, _cmd: Sel) -> *mut AnyObject {
    std::ptr::null_mut()
}

extern "C-unwind" fn ignore(_this: &AnyObject, _cmd: Sel, _value: Option<&AnyObject>) {}

extern "C-unwind" fn set_marked_text(
    _this: &AnyObject,
    _cmd: Sel,
    text: Option<&AnyObject>,
    selected: NSRange,
) {
    let text = string_of(text);
    edit(|marked| marked.set(&text, (selected.location, selected.length)));
}

extern "C-unwind" fn unmark_text(_this: &AnyObject, _cmd: Sel) {
    edit(MarkedText::unmark);
}

extern "C-unwind" fn beginning_of_document(this: &AnyObject, _cmd: Sel) -> *mut AnyObject {
    position(this, 0)
}

extern "C-unwind" fn end_of_document(this: &AnyObject, _cmd: Sel) -> *mut AnyObject {
    position(this, document_len())
}

extern "C-unwind" fn text_range_from_position(
    this: &AnyObject,
    _cmd: Sel,
    from: Option<&AnyObject>,
    to: Option<&AnyObject>,
) -> *mut AnyObject {
    match (offset_of(from), offset_of(to)) {
        (Some(from), Some(to)) => range(this, from, to),
        _ => std::ptr::null_mut(),
    }
}

/// The position `offset` units from `from`, if it is in the document.
fn moved(this: &AnyObject, from: Option<&AnyObject>, offset: isize) -> *mut AnyObject {
    let Some(from) = offset_of(from) else {
        return std::ptr::null_mut();
    };
    match from.checked_add_signed(offset) {
        Some(to) if to <= document_len() => position(this, to),
        _ => std::ptr::null_mut(),
    }
}

extern "C-unwind" fn position_from_position(
    this: &AnyObject,
    _cmd: Sel,
    from: Option<&AnyObject>,
    offset: isize,
) -> *mut AnyObject {
    moved(this, from, offset)
}

extern "C-unwind" fn position_in_direction(
    this: &AnyObject,
    _cmd: Sel,
    from: Option<&AnyObject>,
    direction: isize,
    offset: isize,
) -> *mut AnyObject {
    let offset = if is_backward(direction) {
        -offset
    } else {
        offset
    };
    moved(this, from, offset)
}

extern "C-unwind" fn compare_position(
    _this: &AnyObject,
    _cmd: Sel,
    a: Option<&AnyObject>,
    b: Option<&AnyObject>,
) -> isize {
    // NSComparisonResult: ascending is -1, the same 0, descending 1.
    offset_of(a).cmp(&offset_of(b)) as isize
}

extern "C-unwind" fn offset_from_position(
    _this: &AnyObject,
    _cmd: Sel,
    from: Option<&AnyObject>,
    to: Option<&AnyObject>,
) -> isize {
    let from = offset_of(from).unwrap_or(0) as isize;
    let to = offset_of(to).unwrap_or(0) as isize;
    to - from
}

extern "C-unwind" fn input_delegate(_this: &AnyObject, _cmd: Sel) -> *mut AnyObject {
    let delegate = with(|state| state.input_delegate.as_ref().and_then(Weak::load));
    delegate.map_or(std::ptr::null_mut(), Retained::autorelease_return)
}

extern "C-unwind" fn set_input_delegate(
    _this: &AnyObject,
    _cmd: Sel,
    delegate: Option<&AnyObject>,
) {
    with(|state| state.input_delegate = delegate.map(Weak::new));
}

extern "C-unwind" fn tokenizer(this: &AnyObject, _cmd: Sel) -> *mut AnyObject {
    let tokenizer = with(|state| state.tokenizer.clone()).unwrap_or_else(|| {
        // SAFETY: UITextInputStringTokenizer's initializer, given the view, which adopts
        // UITextInput.
        let tokenizer: Retained<AnyObject> = unsafe {
            let tokenizer: Allocated<AnyObject> =
                msg_send![class!(UITextInputStringTokenizer), alloc];
            msg_send![tokenizer, initWithTextInput: this]
        };
        with(|state| state.tokenizer = Some(tokenizer.clone()));
        tokenizer
    });
    Retained::autorelease_return(tokenizer)
}

extern "C-unwind" fn position_within_range(
    this: &AnyObject,
    _cmd: Sel,
    range: Option<&AnyObject>,
    direction: isize,
) -> *mut AnyObject {
    match range_of(range) {
        Some((start, _)) if is_backward(direction) => position(this, start),
        Some((_, end)) => position(this, end),
        None => std::ptr::null_mut(),
    }
}

extern "C-unwind" fn character_range_by_extending(
    this: &AnyObject,
    _cmd: Sel,
    from: Option<&AnyObject>,
    direction: isize,
) -> *mut AnyObject {
    let Some(from) = offset_of(from) else {
        return std::ptr::null_mut();
    };
    if is_backward(direction) {
        range(this, from.saturating_sub(1), from)
    } else {
        range(this, from, (from + 1).min(document_len()))
    }
}

extern "C-unwind" fn base_writing_direction(
    _this: &AnyObject,
    _cmd: Sel,
    _position: Option<&AnyObject>,
    _direction: isize,
) -> isize {
    // NSWritingDirectionLeftToRight.
    0
}

extern "C-unwind" fn set_base_writing_direction(
    _this: &AnyObject,
    _cmd: Sel,
    _direction: isize,
    _range: Option<&AnyObject>,
) {
}

fn caret() -> CGRect {
    with(|state| state.caret)
}

extern "C-unwind" fn first_rect_for_range(
    _this: &AnyObject,
    _cmd: Sel,
    _range: Option<&AnyObject>,
) -> CGRect {
    caret()
}

extern "C-unwind" fn caret_rect_for_position(
    _this: &AnyObject,
    _cmd: Sel,
    _position: Option<&AnyObject>,
) -> CGRect {
    caret()
}

extern "C-unwind" fn selection_rects_for_range(
    _this: &AnyObject,
    _cmd: Sel,
    _range: Option<&AnyObject>,
) -> *mut AnyObject {
    Retained::autorelease_return(NSArray::<AnyObject>::new()).cast()
}

extern "C-unwind" fn closest_position_to_point(
    this: &AnyObject,
    _cmd: Sel,
    _point: CGPoint,
) -> *mut AnyObject {
    position(this, document_len())
}

extern "C-unwind" fn closest_position_within_range(
    this: &AnyObject,
    _cmd: Sel,
    _point: CGPoint,
    range: Option<&AnyObject>,
) -> *mut AnyObject {
    match range_of(range) {
        Some((_, end)) => position(this, end),
        None => std::ptr::null_mut(),
    }
}

extern "C-unwind" fn character_range_at_point(
    _this: &AnyObject,
    _cmd: Sel,
    _point: CGPoint,
) -> *mut AnyObject {
    std::ptr::null_mut()
}
