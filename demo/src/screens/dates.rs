//! The Dates: alumina-datetime's calendar and pickers, in corundum's words and icons.
//!
//! Every widget takes `gui::datetime::config(ctx)`: the labels follow the language and the
//! week starts on Monday, so the week numbers are ISO.

use corundum_framework::gui::datetime::chrono::{
    Datelike as _, Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike as _,
};
use corundum_framework::gui::datetime::{
    self, Calendar, CalendarState, DatePickerButton, DateTimePickerButton, Period,
    PeriodPickerButton, TimePickerButton,
};
use corundum_framework::gui::{egui, tr};

use super::{heading, section};

/// What the screen keeps between frames.
pub(crate) struct State {
    calendar: CalendarState,
    picked: Option<Period>,
    date: Option<NaiveDate>,
    period: Option<Period>,
    time: NaiveTime,
    datetime: NaiveDateTime,
}

impl Default for State {
    fn default() -> State {
        let now = Local::now()
            .naive_local()
            .with_second(0)
            .and_then(|t| t.with_nanosecond(0))
            .unwrap_or_default();
        State {
            calendar: CalendarState::new(now.date()),
            picked: None,
            date: Some(now.date()),
            period: Some(Period::month_of(now.date())),
            time: now.time(),
            datetime: now,
        }
    }
}

/// A made-up count of entries on a day: one dot on every 7th day.
fn badge(day: NaiveDate) -> usize {
    usize::from(day.day().is_multiple_of(7))
}

pub(super) fn ui(ui: &mut egui::Ui, state: &mut State) {
    heading(ui, "demo-screen-dates");
    ui.label(tr!(ui, "demo-dates-intro"));
    let config = datetime::config(ui.ctx());
    let badge: &dyn Fn(NaiveDate) -> usize = &badge;
    let none = tr!(ui, "demo-dates-none");

    section(ui, "demo-dates-calendar");
    let output = Calendar::new("dates-calendar", &mut state.calendar)
        .selection(state.picked)
        .config(config.clone().show_week_numbers(true))
        .day_badge(badge)
        .show(ui);
    if let Some(period) = output.picked {
        state.picked = Some(period);
    }
    ui.monospace(match state.picked {
        Some(period) => period.to_string(),
        None => none.clone(),
    });

    section(ui, "demo-dates-buttons");
    egui::Grid::new("dates-buttons")
        .num_columns(3)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label(tr!(ui, "demo-dates-date"));
            ui.add(DatePickerButton::new("dates-date", &mut state.date).config(config.clone()));
            ui.monospace(state.date.map_or_else(|| none.clone(), |d| d.to_string()));
            ui.end_row();

            ui.label(tr!(ui, "demo-dates-period"));
            ui.add(
                PeriodPickerButton::new("dates-period", &mut state.period).config(config.clone()),
            );
            ui.monospace(state.period.map_or_else(|| none.clone(), |p| p.to_string()));
            ui.end_row();

            ui.label(tr!(ui, "demo-dates-time"));
            ui.add(TimePickerButton::new("dates-time", &mut state.time).config(config.clone()));
            ui.monospace(state.time.to_string());
            ui.end_row();

            ui.label(tr!(ui, "demo-dates-datetime"));
            ui.add(
                DateTimePickerButton::new("dates-datetime", &mut state.datetime)
                    .config(config.clone()),
            );
            ui.monospace(state.datetime.to_string());
            ui.end_row();
        });
}
