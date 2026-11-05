//! How often a recurring reminder (`recurs:`) comes back.

use std::fmt;

use chrono::{Days, Months, NaiveDateTime};

/// `recurs: daily | weekly | monthly | <n>d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recurrence {
    Days(u32),
    Months(u32),
}

impl Recurrence {
    /// `daily`, `weekly`, `monthly`, `3d`; `None` for anything else (and `0d`).
    pub fn parse(text: &str) -> Option<Recurrence> {
        let text = text.trim().to_ascii_lowercase();
        match text.as_str() {
            "daily" => Some(Recurrence::Days(1)),
            "weekly" => Some(Recurrence::Days(7)),
            "monthly" => Some(Recurrence::Months(1)),
            _ => text
                .strip_suffix('d')
                .and_then(|n| n.trim().parse::<u32>().ok())
                .filter(|&n| n > 0)
                .map(Recurrence::Days),
        }
    }

    /// The first occurrence after `now`, counting whole intervals on from `due` (the one that
    /// fired): `due:` keeps its time of day, and firings missed while the server was off are
    /// skipped rather than delivered one by one. Months add as calendar months from `due`
    /// (the 31st becomes the last day of a shorter month).
    pub fn next_after(self, due: NaiveDateTime, now: NaiveDateTime) -> Option<NaiveDateTime> {
        let mut steps = 1u32;
        loop {
            let next = match self {
                Recurrence::Days(n) => due.checked_add_days(Days::new(u64::from(n * steps)))?,
                Recurrence::Months(n) => due.checked_add_months(Months::new(n * steps))?,
            };
            if next > now {
                return Some(next);
            }
            steps = steps.checked_add(1)?;
        }
    }
}

impl fmt::Display for Recurrence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Recurrence::Days(1) => f.write_str("daily"),
            Recurrence::Days(7) => f.write_str("weekly"),
            Recurrence::Months(1) => f.write_str("monthly"),
            Recurrence::Days(n) => write!(f, "{n}d"),
            Recurrence::Months(n) => write!(f, "{n} months"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn parses_the_intervals() {
        assert_eq!(Recurrence::parse("daily"), Some(Recurrence::Days(1)));
        assert_eq!(Recurrence::parse(" Weekly "), Some(Recurrence::Days(7)));
        assert_eq!(Recurrence::parse("monthly"), Some(Recurrence::Months(1)));
        assert_eq!(Recurrence::parse("3d"), Some(Recurrence::Days(3)));
        assert_eq!(Recurrence::parse("0d"), None);
        assert_eq!(Recurrence::parse("sometimes"), None);
        assert_eq!(Recurrence::parse(""), None);
    }

    #[test]
    fn the_next_firing_keeps_the_phase() {
        let daily = Recurrence::Days(1);
        assert_eq!(
            daily.next_after(at("2099-10-19 20:00"), at("2099-10-19 20:00")),
            Some(at("2099-10-20 20:00"))
        );
        // Three days late: one delivery, the next firing is still at 20:00.
        assert_eq!(
            daily.next_after(at("2099-10-19 20:00"), at("2099-10-22 21:00")),
            Some(at("2099-10-23 20:00"))
        );
        assert_eq!(
            Recurrence::Days(3).next_after(at("2099-10-19 08:00"), at("2099-10-25 09:00")),
            Some(at("2099-10-28 08:00"))
        );
        assert_eq!(
            Recurrence::Months(1).next_after(at("2099-01-31 09:00"), at("2099-01-31 09:00")),
            Some(at("2099-02-28 09:00"))
        );
        assert_eq!(
            Recurrence::Months(1).next_after(at("2099-01-31 09:00"), at("2099-03-01 09:00")),
            Some(at("2099-03-31 09:00"))
        );
    }
}
