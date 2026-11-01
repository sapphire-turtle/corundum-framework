//! The first-run flow: a sequence of setup pages an app composes, shown over the whole
//! window until the person has been through them. That the flow was finished is kept in the
//! [store](crate::store), so it is not shown again on the next start.
//!
//! The app hands [`FirstRun::new`] its name and its pages, and each frame, until [`FirstRun::done`] says
//! so, draws [`FirstRun::ui`] in place of its window's content.

#[cfg(all(
    feature = "service",
    not(any(target_os = "ios", target_os = "android"))
))]
mod service;

#[cfg(all(
    feature = "service",
    not(any(target_os = "ios", target_os = "android"))
))]
pub use service::ServicePage;

use crate::store;
use crate::tr;

/// One page of the [`FirstRun`] flow.
pub trait FirstRunPage {
    /// The page's heading.
    fn title(&self, ctx: &egui::Context) -> String;

    /// Draw the page. It may ask to move on or back by itself (a page whose own button
    /// answers a question, say); the footer's buttons do so otherwise.
    fn ui(&mut self, ui: &mut egui::Ui) -> PageAction;

    /// Whether the person may move on from the page: `false` holds the footer's **Next**
    /// disabled until what the page asks for is done.
    fn can_continue(&self) -> bool {
        true
    }

    /// Whether to leave the page out: a skipped page is neither drawn nor counted among
    /// the steps, and **Next** and **Back** pass over it. It is asked each frame, so a page
    /// can be skipped by what the person answered on an earlier one (an answer the pages
    /// share, through an `Rc<Cell<_>>` say).
    fn skip(&self) -> bool {
        false
    }
}

/// What a page or the footer asks the flow to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageAction {
    /// Stay on the page.
    Stay,
    /// Go on to the next page, or finish after the last.
    Next,
    /// Go back to the previous page; on the first page, stay.
    Back,
    /// Finish the flow here, leaving the pages after this one unshown. Unlike
    /// [`Next`](Self::Next), it is not held by [`FirstRunPage::can_continue`]: a page asks for
    /// it when an answer makes the rest of the flow needless.
    Finish,
}

/// The first-run flow over a list of pages.
pub struct FirstRun {
    /// The app's name, shown above each page's title.
    app_name: String,
    pages: Vec<Box<dyn FirstRunPage>>,
    /// The page shown. Kept only while the app runs: a flow left unfinished starts again.
    current: usize,
    /// The footer button pressed, taken up by the next [`ui`](Self::ui), for the tests.
    #[cfg(test)]
    pressed: Option<PageAction>,
}

impl FirstRun {
    /// The flow over `pages`, in order, for the app called `app_name`: the name heads every
    /// page, above its title.
    pub fn new(app_name: impl Into<String>, pages: Vec<Box<dyn FirstRunPage>>) -> FirstRun {
        FirstRun {
            app_name: app_name.into(),
            pages,
            current: 0,
            #[cfg(test)]
            pressed: None,
        }
    }

    /// Whether the flow has been finished on this device.
    pub fn done(ctx: &egui::Context) -> bool {
        store::state(ctx).first_run == Some(true)
    }

    /// Forget that the flow was finished, so the app shows it again.
    pub fn reset(ctx: &egui::Context) {
        store::update(ctx, |stored| stored.first_run = None);
    }

    /// Draw the current page with its header and footer over the whole of `ui`. Returns
    /// `true` once the flow is finished (and, from then on, without drawing anything).
    /// A current page that has come to be [skipped](FirstRunPage::skip) is passed over
    /// as **Next** would; with no page left to show, the flow is finished.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        if Self::done(ui.ctx()) || self.pages.is_empty() {
            return true;
        }
        match self.shown_from(self.current) {
            Some(index) => self.current = index,
            None => return self.finish(ui.ctx()),
        }
        let current = self.current;
        let (step, count) = self.step();
        let on_last = self.on_last();
        let can_go_back = self.can_go_back();
        let mut action = PageAction::Stay;

        egui::Panel::top("corundum_first_run_header").show(ui, |ui| {
            let title = self.pages[current].title(ui.ctx());
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&self.app_name).small().weak());
            ui.heading(title);
            ui.label(tr!(
                ui,
                "corundum-first-run-step",
                step = step,
                count = count
            ));
            ui.add_space(8.0);
        });
        let can_continue = self.pages[current].can_continue();
        egui::Panel::bottom("corundum_first_run_footer").show(ui, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if can_go_back && ui.button(tr!(ui, "corundum-first-run-back")).clicked() {
                    action = PageAction::Back;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let id = if on_last {
                        "corundum-first-run-finish"
                    } else {
                        "corundum-first-run-next"
                    };
                    if ui
                        .add_enabled(can_continue, egui::Button::new(tr!(ui, id)))
                        .clicked()
                    {
                        action = PageAction::Next;
                    }
                });
            });
            ui.add_space(8.0);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    let asked = self.pages[current].ui(ui);
                    if asked != PageAction::Stay {
                        action = asked;
                    }
                });
        });
        #[cfg(test)]
        if let Some(pressed) = self.pressed.take()
            && pressed != PageAction::Stay
        {
            action = pressed;
        }
        self.apply(ui.ctx(), action)
    }

    /// Carry out `action`; whether that finished the flow. A **Next** the page cannot
    /// continue from is dropped, as the disabled button would not have sent it. Skipped
    /// pages are asked after the page's own `ui`, so an answer given this frame counts.
    fn apply(&mut self, ctx: &egui::Context, action: PageAction) -> bool {
        match action {
            PageAction::Stay => {}
            PageAction::Back => {
                if let Some(index) = self.shown_before(self.current) {
                    self.current = index;
                }
            }
            PageAction::Next if !self.pages[self.current].can_continue() => {}
            PageAction::Next => match self.shown_from(self.current + 1) {
                Some(index) => self.current = index,
                None => return self.finish(ctx),
            },
            PageAction::Finish => return self.finish(ctx),
        }
        false
    }

    /// Mark the flow finished on this device; always `true`, for [`ui`](Self::ui) to return.
    fn finish(&mut self, ctx: &egui::Context) -> bool {
        store::update(ctx, |stored| stored.first_run = Some(true));
        true
    }

    /// The first page at or after `index` that is not skipped.
    fn shown_from(&self, index: usize) -> Option<usize> {
        (index..self.pages.len()).find(|&i| !self.pages[i].skip())
    }

    /// The last page before `index` that is not skipped.
    fn shown_before(&self, index: usize) -> Option<usize> {
        (0..index).rev().find(|&i| !self.pages[i].skip())
    }

    /// The current page's place among the pages not skipped, from 1, and their number.
    fn step(&self) -> (usize, usize) {
        let shown =
            |range: std::ops::Range<usize>| range.filter(|&i| !self.pages[i].skip()).count();
        (
            shown(0..self.current) + 1,
            shown(0..self.pages.len()).max(1),
        )
    }

    /// Whether no page after the current one is shown, so **Next** finishes.
    fn on_last(&self) -> bool {
        self.shown_from(self.current + 1).is_none()
    }

    /// Whether a page before the current one is shown, for **Back** to go to.
    fn can_go_back(&self) -> bool {
        self.shown_before(self.current).is_some()
    }

    /// Press a footer button on the next frame, as a click would.
    #[cfg(test)]
    fn press(&mut self, action: PageAction) {
        self.pressed = Some(action);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;
    use crate::test_support::render_in;

    /// A page that says its number as its title and whether it may be left through shared cells.
    struct Page {
        number: usize,
        open: Rc<Cell<bool>>,
        drawn: Rc<Cell<usize>>,
        /// What the page asks for on its next frame, once.
        asks: Rc<Cell<Option<PageAction>>>,
        /// The numbers of the pages to skip.
        skipped: Rc<RefCell<Vec<usize>>>,
        /// The numbers of the pages whose title or content was asked for, in order.
        seen: Rc<RefCell<Vec<usize>>>,
    }

    impl FirstRunPage for Page {
        fn title(&self, _: &egui::Context) -> String {
            self.seen.borrow_mut().push(self.number);
            format!("page {}", self.number)
        }
        fn ui(&mut self, _: &mut egui::Ui) -> PageAction {
            self.seen.borrow_mut().push(self.number);
            self.drawn.set(self.number);
            self.asks.take().unwrap_or(PageAction::Stay)
        }
        fn can_continue(&self) -> bool {
            self.open.get()
        }
        fn skip(&self) -> bool {
            self.skipped.borrow().contains(&self.number)
        }
    }

    struct Flow {
        flow: FirstRun,
        ctx: egui::Context,
        open: Rc<Cell<bool>>,
        drawn: Rc<Cell<usize>>,
        asks: Rc<Cell<Option<PageAction>>>,
        skipped: Rc<RefCell<Vec<usize>>>,
        seen: Rc<RefCell<Vec<usize>>>,
    }

    impl Flow {
        fn new(count: usize) -> Flow {
            let open = Rc::new(Cell::new(true));
            let drawn = Rc::new(Cell::new(0));
            let asks = Rc::new(Cell::new(None));
            let skipped = Rc::new(RefCell::new(Vec::new()));
            let seen = Rc::new(RefCell::new(Vec::new()));
            let pages = (1..=count)
                .map(|number| {
                    Box::new(Page {
                        number,
                        open: open.clone(),
                        drawn: drawn.clone(),
                        asks: asks.clone(),
                        skipped: skipped.clone(),
                        seen: seen.clone(),
                    }) as Box<dyn FirstRunPage>
                })
                .collect();
            Flow {
                flow: FirstRun::new("App", pages),
                ctx: egui::Context::default(),
                open,
                drawn,
                asks,
                skipped,
                seen,
            }
        }

        /// Skip the pages numbered `numbers` from now on.
        fn skip(&self, numbers: &[usize]) {
            *self.skipped.borrow_mut() = numbers.to_vec();
        }

        /// Run a frame with `action` pressed; whether the flow is finished.
        fn press(&mut self, action: PageAction) -> bool {
            let mut finished = false;
            let flow = &mut self.flow;
            flow.press(action);
            render_in(&self.ctx, [400.0, 300.0], |ui| {
                finished |= flow.ui(ui);
            });
            finished
        }

        /// The page shown, as last drawn.
        fn page(&mut self) -> usize {
            self.press(PageAction::Stay);
            self.drawn.get()
        }
    }

    #[test]
    fn pages_go_in_order_and_back() {
        let mut flow = Flow::new(3);
        assert_eq!(flow.page(), 1);
        flow.press(PageAction::Next);
        assert_eq!(flow.page(), 2);
        flow.press(PageAction::Next);
        assert_eq!(flow.page(), 3);
        flow.press(PageAction::Back);
        assert_eq!(flow.page(), 2);
    }

    #[test]
    fn back_on_the_first_page_stays() {
        let mut flow = Flow::new(2);
        flow.press(PageAction::Back);
        assert_eq!(flow.page(), 1);
    }

    #[test]
    fn next_is_held_while_the_page_cannot_continue() {
        let mut flow = Flow::new(2);
        flow.open.set(false);
        flow.press(PageAction::Next);
        assert_eq!(flow.page(), 1);
        flow.open.set(true);
        flow.press(PageAction::Next);
        assert_eq!(flow.page(), 2);
    }

    #[test]
    fn a_page_can_ask_to_move_on() {
        let mut flow = Flow::new(2);
        flow.asks.set(Some(PageAction::Next));
        assert_eq!(flow.page(), 2);
    }

    #[test]
    fn a_page_asking_to_move_on_is_held_while_it_cannot_continue() {
        let mut flow = Flow::new(2);
        flow.open.set(false);
        flow.asks.set(Some(PageAction::Next));
        assert_eq!(flow.page(), 1);
    }

    #[test]
    fn a_page_can_ask_to_go_back() {
        let mut flow = Flow::new(2);
        flow.press(PageAction::Next);
        flow.asks.set(Some(PageAction::Back));
        assert_eq!(flow.page(), 1);
    }

    #[test]
    fn finishing_is_stored_and_not_shown_again() {
        let mut flow = Flow::new(2);
        assert!(!FirstRun::done(&flow.ctx));
        assert!(!flow.press(PageAction::Next));
        assert!(flow.press(PageAction::Next));
        assert!(FirstRun::done(&flow.ctx));
        assert_eq!(store::state(&flow.ctx).first_run, Some(true));
        flow.drawn.set(0);
        assert!(flow.press(PageAction::Stay));
        assert_eq!(flow.drawn.get(), 0);
    }

    #[test]
    fn reset_shows_it_again() {
        let mut flow = Flow::new(1);
        assert!(flow.press(PageAction::Next));
        FirstRun::reset(&flow.ctx);
        assert!(!FirstRun::done(&flow.ctx));
        assert!(!flow.press(PageAction::Stay));
        assert_eq!(flow.drawn.get(), 1);
    }

    #[test]
    fn an_unfinished_flow_starts_at_page_one() {
        let mut left = Flow::new(3);
        left.press(PageAction::Next);
        assert_eq!(left.page(), 2);
        let mut again = Flow::new(3);
        again.ctx = left.ctx.clone();
        assert_eq!(again.page(), 1);
    }

    #[test]
    fn a_finished_flow_is_held_even_when_the_store_cannot_save() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the directory should be.
        std::fs::write(dir.path().join("app"), "").unwrap();
        let mut flow = Flow::new(1);
        store::attach(
            &flow.ctx,
            store::Store::at(dir.path().join("app").join(store::FILE_NAME)),
        );
        assert!(flow.press(PageAction::Next));
        assert!(FirstRun::done(&flow.ctx));
    }

    #[test]
    fn a_page_can_finish_the_flow() {
        let mut flow = Flow::new(3);
        flow.asks.set(Some(PageAction::Finish));
        assert!(flow.press(PageAction::Stay));
        assert!(FirstRun::done(&flow.ctx));
        flow.drawn.set(0);
        assert!(flow.press(PageAction::Stay));
        assert_eq!(flow.drawn.get(), 0);
    }

    #[test]
    fn finish_is_not_held_by_can_continue() {
        let mut flow = Flow::new(3);
        flow.open.set(false);
        flow.asks.set(Some(PageAction::Finish));
        assert!(flow.press(PageAction::Stay));
        assert!(FirstRun::done(&flow.ctx));
    }

    #[test]
    fn a_skipped_page_is_never_drawn() {
        let mut flow = Flow::new(3);
        flow.skip(&[2]);
        assert_eq!(flow.page(), 1);
        flow.press(PageAction::Next);
        assert_eq!(flow.page(), 3);
        assert!(!flow.seen.borrow().contains(&2));
    }

    #[test]
    fn skipped_pages_are_not_counted() {
        let mut flow = Flow::new(4);
        flow.skip(&[2]);
        flow.page();
        assert_eq!(flow.flow.step(), (1, 3));
        flow.press(PageAction::Next);
        flow.page();
        assert_eq!(flow.flow.step(), (2, 3));
        flow.press(PageAction::Next);
        flow.page();
        assert_eq!(flow.flow.step(), (3, 3));
        assert!(flow.flow.on_last());
    }

    #[test]
    fn next_over_only_skipped_pages_finishes() {
        let mut flow = Flow::new(4);
        flow.skip(&[2, 3, 4]);
        assert!(flow.press(PageAction::Next));
        assert!(FirstRun::done(&flow.ctx));
        assert!(!flow.seen.borrow().iter().any(|&n| n > 1));
    }

    #[test]
    fn a_page_answer_can_skip_the_rest() {
        // Page 1's answer makes 2 and 3 skipped in the same frame it asks to move on.
        let mut flow = Flow::new(3);
        flow.page();
        flow.skip(&[2, 3]);
        flow.asks.set(Some(PageAction::Next));
        assert!(flow.press(PageAction::Stay));
        assert!(FirstRun::done(&flow.ctx));
    }

    #[test]
    fn a_skipped_first_page_is_passed_over() {
        let mut flow = Flow::new(3);
        flow.skip(&[1]);
        assert_eq!(flow.page(), 2);
        assert!(!flow.seen.borrow().contains(&1));
        assert_eq!(flow.flow.step(), (1, 2));
    }

    #[test]
    fn back_steps_over_skipped_pages() {
        let mut flow = Flow::new(4);
        flow.skip(&[2, 3]);
        flow.press(PageAction::Next);
        assert_eq!(flow.page(), 4);
        flow.press(PageAction::Back);
        assert_eq!(flow.page(), 1);
    }

    #[test]
    fn back_with_only_skipped_pages_before_stays() {
        let mut flow = Flow::new(3);
        flow.skip(&[1]);
        assert_eq!(flow.page(), 2);
        assert!(!flow.flow.can_go_back());
        flow.press(PageAction::Back);
        assert_eq!(flow.page(), 2);
    }

    #[test]
    fn a_page_no_longer_skipped_is_shown_again_on_back() {
        let mut flow = Flow::new(3);
        flow.skip(&[2]);
        flow.press(PageAction::Next);
        assert_eq!(flow.page(), 3);
        flow.skip(&[]);
        flow.press(PageAction::Back);
        assert_eq!(flow.page(), 2);
    }
}
