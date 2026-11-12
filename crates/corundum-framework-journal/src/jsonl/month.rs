//! The months that name the journal's files.

/// A month of the UTC calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Month {
    pub(crate) year: i32,
    /// 1 to 12.
    pub(crate) month: u32,
}

impl Month {
    /// The UTC month of `at_ms`, milliseconds since the Unix epoch.
    pub(crate) fn of(at_ms: i64) -> Month {
        // Howard Hinnant's `civil_from_days`, keeping the year and the month.
        let days = at_ms.div_euclid(86_400_000) + 719_468;
        let era = days.div_euclid(146_097);
        let doe = days.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        Month {
            year: i32::try_from(year).unwrap_or(if year < 0 { i32::MIN } else { i32::MAX }),
            // In 1..=12 by construction.
            month: month as u32,
        }
    }

    /// The name of its file: `2026-11.jsonl`.
    pub(crate) fn file_name(&self) -> String {
        format!("{:04}-{:02}.jsonl", self.year, self.month)
    }

    /// The month a file name stands for: exactly four digits, `-`, two digits from `01` to
    /// `12`, and `.jsonl`.
    pub(crate) fn parse(file_name: &str) -> Option<Month> {
        let stem = file_name.strip_suffix(".jsonl")?;
        let (year, month) = stem.split_once('-')?;
        let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(year, 4) || !digits(month, 2) {
            return None;
        }
        let (year, month) = (year.parse().ok()?, month.parse().ok()?);
        (1..=12).contains(&month).then_some(Month { year, month })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_are_utc() {
        assert_eq!(Month::of(0).file_name(), "1970-01.jsonl");
        assert_eq!(Month::of(-1).file_name(), "1969-12.jsonl");
        assert_eq!(Month::of(1_796_083_199_999).file_name(), "2026-11.jsonl");
        assert_eq!(Month::of(1_796_083_200_000).file_name(), "2026-12.jsonl");
        assert_eq!(Month::of(1_709_208_000_000).file_name(), "2024-02.jsonl");
        assert_eq!(Month::of(951_868_800_000).file_name(), "2000-03.jsonl");
    }

    #[test]
    fn only_month_names_parse() {
        assert_eq!(
            Month::parse("2026-11.jsonl"),
            Some(Month {
                year: 2026,
                month: 11
            })
        );
        for bad in [
            "2026-13.jsonl",
            "2026-00.jsonl",
            "2026-1.jsonl",
            "notes.jsonl",
            "2026-11.json",
            "2026-11.jsonl.tmp",
            "journal.lock",
            ".DS_Store",
            "+026-11.jsonl",
        ] {
            assert_eq!(Month::parse(bad), None, "{bad}");
        }
        assert!(Month::parse("2026-10.jsonl") < Month::parse("2026-11.jsonl"));
    }
}
