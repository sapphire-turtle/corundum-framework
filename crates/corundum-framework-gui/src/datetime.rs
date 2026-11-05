//! alumina-datetime's pickers (a calendar, drums, and buttons reading a date, a period, a
//! time or a timestamp) re-exported, so an app depends on corundum alone. [alumina-datetime]
//! is egui widgets for picking dates and times.
//!
//! The pickers' colours follow the visuals, and corundum's theme sets the selection from its
//! accent. Use [`config`] with alumina's widgets (`Calendar::config`,
//! `DatePickerButton::config`, ...) for corundum's look: the buttons are its Lucide icons
//! ([`icons`](crate::icons)), and every word is in the context's language.
//!
//! Weeks start on Monday, always: the week numbers are ISO numbers, which count Monday
//! weeks, and alumina shows the column only then. Weeks starting on Sunday wait for
//! [alumina#36].
//!
//! In Japanese the calendar's heading reads month then year, and a 12-hour time shows "PM"
//! before the hour, until [alumina#37]. corundum's config leaves alumina's default of 24-hour
//! time alone, so the AM / PM order only shows for an app that turns `hour12` on.
//!
//! Timestamps are [`chrono::NaiveDateTime`], local wall time: the app converts to and from a
//! time zone.
//!
//! [alumina-datetime]: https://crates.io/crates/alumina-datetime
//! [alumina#36]: https://github.com/sapphire-turtle/alumina/issues/36
//! [alumina#37]: https://github.com/sapphire-turtle/alumina/issues/37

pub use alumina_datetime::*;

use crate::{icons, tr};

/// The calendar's buttons as corundum's Lucide icons.
///
/// The icons need the image loaders (`icons::install_loaders`, which corundum's shell
/// already installs); without them the buttons show no icon.
pub fn icons() -> CalendarIcons {
    let mut i = CalendarIcons::default();
    i.previous = Some(icons::chevron_left());
    i.next = Some(icons::chevron_right());
    i.today = Some(icons::calendar_check());
    i.clear = Some(icons::x());
    i.now = Some(icons::clock());
    i
}

/// Every word the pickers show, in the context's language.
pub fn labels(ctx: &egui::Context) -> Labels {
    let mut l = Labels::default();
    for (i, month) in l.months.iter_mut().enumerate() {
        *month = crate::i18n::tr(ctx, &format!("corundum-datetime-month-{}", i + 1));
    }
    for (i, month) in l.months_short.iter_mut().enumerate() {
        *month = crate::i18n::tr(ctx, &format!("corundum-datetime-month-short-{}", i + 1));
    }
    l.weekdays_short = [
        tr!(ctx, "corundum-datetime-weekday-mon"),
        tr!(ctx, "corundum-datetime-weekday-tue"),
        tr!(ctx, "corundum-datetime-weekday-wed"),
        tr!(ctx, "corundum-datetime-weekday-thu"),
        tr!(ctx, "corundum-datetime-weekday-fri"),
        tr!(ctx, "corundum-datetime-weekday-sat"),
        tr!(ctx, "corundum-datetime-weekday-sun"),
    ];
    l.week_column = tr!(ctx, "corundum-datetime-week-column");
    l.day = tr!(ctx, "corundum-datetime-day");
    l.week = tr!(ctx, "corundum-datetime-week");
    l.month = tr!(ctx, "corundum-datetime-month");
    l.year = tr!(ctx, "corundum-datetime-year");
    l.range = tr!(ctx, "corundum-datetime-range");
    l.today = tr!(ctx, "corundum-datetime-today");
    l.now = tr!(ctx, "corundum-datetime-now");
    l.clear = tr!(ctx, "corundum-datetime-clear");
    l.previous = tr!(ctx, "corundum-datetime-previous");
    l.next = tr!(ctx, "corundum-datetime-next");
    l.choose_month = tr!(ctx, "corundum-datetime-choose-month");
    l.choose_year = tr!(ctx, "corundum-datetime-choose-year");
    l.am = tr!(ctx, "corundum-datetime-am");
    l.pm = tr!(ctx, "corundum-datetime-pm");
    l.none = tr!(ctx, "corundum-datetime-none");
    l.period_hint = tr!(ctx, "corundum-datetime-period-hint");
    l
}

/// corundum's look for the pickers: its icons and words, weeks from Monday (ISO numbers).
///
/// Each call looks up every word, so build it once per frame (or when the language changes)
/// and `.clone()` it into each widget: the clone shares the labels.
pub fn config(ctx: &egui::Context) -> CalendarConfig {
    CalendarConfig::default()
        .labels(labels(ctx))
        .icons(icons())
        .first_weekday(chrono::Weekday::Mon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, LanguageSettings};

    fn set_language(ctx: &egui::Context, tag: &str) {
        crate::i18n::install(
            ctx,
            &LanguageSettings {
                language: Language::Explicit(tag.into()),
            },
        );
    }

    fn ctx_in(tag: &str) -> egui::Context {
        let ctx = egui::Context::default();
        set_language(&ctx, tag);
        ctx
    }

    #[test]
    fn every_calendar_button_has_an_icon() {
        let i = icons();
        let all = [&i.previous, &i.next, &i.today, &i.clear, &i.now];
        assert!(all.iter().all(|icon| icon.is_some()));
    }

    #[test]
    fn japanese_words_in_alumina_s_order() {
        let l = labels(&ctx_in("ja"));
        assert_eq!(l.months[0], "1月");
        assert_eq!(l.months[11], "12月");
        assert_eq!(l.weekdays_short[0], "月");
        assert_eq!(l.weekdays_short[6], "日");
        assert_eq!(l.today, "今日");
    }

    #[test]
    fn english_is_alumina_s_default() {
        let l = labels(&ctx_in("en"));
        let d = Labels::default();
        assert_eq!(l, d);
    }

    #[test]
    fn the_config_follows_the_language() {
        let ctx = ctx_in("en");
        assert_eq!(config(&ctx).labels.today, "Today");
        set_language(&ctx, "ja");
        assert_eq!(config(&ctx).labels.today, "今日");
    }

    #[test]
    fn weeks_start_on_monday() {
        assert_eq!(config(&ctx_in("en")).first_weekday, chrono::Weekday::Mon);
    }

    #[test]
    fn week_numbers_show_with_corundum_s_config() {
        let ctx = ctx_in("en");
        let cfg = config(&ctx).show_week_numbers(true);
        let mut state = CalendarState::new(chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap());
        let mut found = false;
        let mut first = ctx.run_ui(egui::RawInput::default(), |ui| {
            Calendar::new("t", &mut state).config(cfg.clone()).show(ui);
        });
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            Calendar::new("t", &mut state).config(cfg.clone()).show(ui);
        });
        // No renderer takes the texture deltas; dropping them unapplied panics.
        first.textures_delta.clear();
        out.textures_delta.clear();
        for clipped in &out.shapes {
            if let egui::epaint::Shape::Text(t) = &clipped.shape
                && t.galley.text() == "W"
            {
                found = true;
            }
        }
        assert!(found, "no \"W\" heading drawn");
    }

    #[test]
    fn the_datetime_strings_exist_in_both_languages() {
        let mut ids: Vec<String> = Vec::new();
        for i in 1..=12 {
            ids.push(format!("month-{i}"));
            ids.push(format!("month-short-{i}"));
        }
        for d in ["mon", "tue", "wed", "thu", "fri", "sat", "sun"] {
            ids.push(format!("weekday-{d}"));
        }
        for n in [
            "week-column",
            "day",
            "week",
            "month",
            "year",
            "range",
            "today",
            "now",
            "clear",
            "previous",
            "next",
            "choose-month",
            "choose-year",
            "am",
            "pm",
            "none",
            "period-hint",
        ] {
            ids.push(n.into());
        }
        for tag in ["en", "ja"] {
            let ctx = ctx_in(tag);
            for n in &ids {
                let id = format!("corundum-datetime-{n}");
                assert_ne!(crate::i18n::tr(&ctx, &id), id, "{tag} lacks {id}");
            }
        }
    }
}
