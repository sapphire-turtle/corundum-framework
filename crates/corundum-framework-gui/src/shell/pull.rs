//! [`pull_to_refresh`]: a screen's scroll area that refreshes the screen when pulled down
//! at its top, as on a phone.

use super::Refresh;
use super::refresh;

/// How far a pull goes down before letting go refreshes.
const THRESHOLD: f32 = 64.0;

/// The size of the pull's icon and of the spinner.
const ICON: f32 = 18.0;

/// A pull under way: where it was pressed, and whether letting go now refreshes.
#[derive(Clone, Copy, Debug)]
struct Pull {
    origin: egui::Pos2,
    armed: bool,
}

/// A vertical scroll area over `add_contents` that refreshes the open screen when pulled
/// down at its top, by a finger or the mouse: the screen's main scroll, in place of an
/// `egui::ScrollArea::vertical()`. Past a short way down, an icon says letting go will
/// refresh; while the refresh runs, a spinner shows at the top.
///
/// It refreshes through the shell ([`ShellApp::refresh`](super::ShellApp::refresh)), so
/// draw it in [`ShellApp::screen_ui`](super::ShellApp::screen_ui). While the screen
/// cannot be refreshed, or outside a shell, it is a plain scroll area. Screens that drag
/// vertically themselves — a map, a canvas — keep their own scroll instead.
pub fn pull_to_refresh<R>(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let output = egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, add_contents);
    let ctx = ui.ctx().clone();
    let state = refresh::state(&ctx);
    let id = output.id.with("corundum_pull");
    let rect = output.inner_rect;
    let mut pull = ctx.data(|d| d.get_temp::<Pull>(id));
    let (pressed, released, down, origin, latest) = ctx.input(|i| {
        let p = &i.pointer;
        (
            p.primary_pressed(),
            p.primary_released(),
            p.primary_down(),
            p.press_origin(),
            p.latest_pos(),
        )
    });
    let at_top = output.state.clamped_offset().y <= 0.0;
    if state != Refresh::Idle || !at_top {
        pull = None;
    } else if pressed
        && let Some(origin) = origin.filter(|o| rect.contains(*o))
        && ui.rect_contains_pointer(rect)
    {
        pull = Some(Pull {
            origin,
            armed: false,
        });
    }
    let mut shown = None;
    if let Some(p) = &mut pull
        && let Some(pos) = latest
    {
        let dy = pos.y - p.origin.y;
        let dx = (pos.x - p.origin.x).abs();
        if dy < 0.0 {
            pull = None;
        } else {
            p.armed = dy >= THRESHOLD && dy > dx;
            shown = Some((dy, p.armed));
            if released {
                if p.armed {
                    refresh::request(&ctx);
                }
                pull = None;
            } else if !down {
                pull = None;
            }
        }
    }
    ctx.data_mut(|d| {
        if let Some(p) = pull {
            d.insert_temp(id, p);
        } else {
            d.remove::<Pull>(id);
        }
    });
    let top = rect.center_top();
    if let Some((dy, armed)) = shown.filter(|_| pull.is_some()) {
        let progress = (dy / THRESHOLD).min(1.0);
        let down = dy.min(THRESHOLD * 1.5) * 0.5;
        let square =
            egui::Rect::from_center_size(top + egui::vec2(0.0, down), egui::Vec2::splat(ICON));
        let visuals = ui.visuals();
        let colour = if armed {
            visuals.strong_text_color()
        } else {
            visuals.weak_text_color()
        };
        egui::Image::new(crate::icons::refresh_cw())
            .rotate(progress * std::f32::consts::TAU, egui::Vec2::splat(0.5))
            .tint(colour)
            .paint_at(ui, square);
    }
    if state == Refresh::Running {
        let square =
            egui::Rect::from_center_size(top + egui::vec2(0.0, ICON), egui::Vec2::splat(ICON));
        egui::Spinner::new().size(ICON).paint_at(ui, square);
    }
    output.inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::Refresh;
    use crate::shell::refresh::tests::{App, Run, Screen, WIDE};

    fn long(ui: &mut egui::Ui) {
        pull_to_refresh(ui, |ui| {
            for i in 0..60 {
                ui.label(format!("row {i}"));
            }
        });
    }

    fn run(state: Refresh) -> Run {
        let mut app = App::with(&[(Screen::Entries, state)]);
        app.screen = Some(long);
        Run::new(app, WIDE)
    }

    /// Press at `from`, move by `by` in 10-point steps, and let go.
    fn drag(run: &mut Run, from: egui::Pos2, by: egui::Vec2) {
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        run.frame(vec![egui::Event::PointerMoved(from)]);
        run.frame(vec![button(from, true)]);
        let steps = (by.length() / 10.0).ceil().max(1.0) as usize;
        for i in 1..=steps {
            let at = from + by * (i as f32 / steps as f32);
            run.frame(vec![egui::Event::PointerMoved(at)]);
        }
        run.frame(vec![button(from + by, false)]);
        run.frame(vec![]);
    }

    const FROM: egui::Pos2 = egui::pos2(400.0, 100.0);

    #[test]
    fn pulling_past_the_threshold_refreshes_once() {
        let mut run = run(Refresh::Idle);
        drag(&mut run, FROM, egui::vec2(0.0, 100.0));
        assert_eq!(run.app.refreshed, [Screen::Entries]);
    }

    #[test]
    fn a_short_pull_does_not_refresh() {
        let mut run = run(Refresh::Idle);
        drag(&mut run, FROM, egui::vec2(0.0, 40.0));
        assert_eq!(run.app.refreshed, []);
    }

    #[test]
    fn a_drag_begun_scrolled_down_does_not_pull() {
        let mut run = run(Refresh::Idle);
        run.frame(vec![egui::Event::PointerMoved(FROM)]);
        run.frame(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -200.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        }]);
        for _ in 0..30 {
            run.frame(vec![]);
        }
        drag(&mut run, FROM, egui::vec2(0.0, 300.0));
        assert_eq!(run.app.refreshed, []);
    }

    #[test]
    fn a_sideways_drag_does_not_pull() {
        let mut run = run(Refresh::Idle);
        drag(&mut run, FROM, egui::vec2(150.0, 80.0));
        assert_eq!(run.app.refreshed, []);
    }

    #[test]
    fn an_unrefreshable_screen_is_a_plain_scroll_area() {
        let mut run = run(Refresh::Unsupported);
        drag(&mut run, FROM, egui::vec2(0.0, 100.0));
        assert_eq!(run.app.refreshed, []);
    }

    #[test]
    fn nothing_is_pulled_while_running() {
        let mut run = run(Refresh::Running);
        drag(&mut run, FROM, egui::vec2(0.0, 100.0));
        assert_eq!(run.app.refreshed, []);
    }
}
