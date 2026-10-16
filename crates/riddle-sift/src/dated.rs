//! The days a message is about, read from it: a hotel's check-in and
//! check-out, a parking booking's start and end, an event's day. A rule
//! with `+ Expires: after its dates` uses them to keep the Find out of the
//! way until the day and close it after, and `dates_passed` says when what
//! a message was about is behind the user. Without them a Telpark parking
//! booking made in June sat among the open Finds for months, and a Hampton
//! Inn confirmation was still open a week after the stay (15 Oct 2026).
//!
//! Only dates written in full count: a named month with a year
//! (`Oct-07-2026`, `Wednesday Oct 07, 2026`, `7 October 2026`), an ISO date,
//! or a numeric date whose order the message settles (`27/06/2026` can only
//! be day first, and then so is `30/06/2026`). `07-10-2026` alone is left
//! out rather than guessed. A date after "cancel by" or "booked on" is not
//! the one the message is about, nor is one before it was sent. The span
//! runs from the earliest date left to the latest within `MAX_SPAN_DAYS`.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use regex::Regex;
use riddle_core::DateSpan;
use std::sync::OnceLock;

/// Words shortly before a date that make it another date than the one the
/// message is about: the last day to cancel, when it was booked or paid.
const NOT_ITS_DATE: [&str; 11] = [
    "cancel",
    "booked on",
    "booking date",
    "order date",
    "ordered on",
    "issued",
    "purchase",
    "paid on",
    "payment date",
    "invoice date",
    "made on",
];

/// How far before a date those words still apply, in bytes.
const LOOK_BACK: usize = 60;

/// The furthest ahead of the message its dates can be.
const MAX_AHEAD_DAYS: i64 = 550;

/// The longest one span runs (a stay, a trip); a date further on is
/// something else.
const MAX_SPAN_DAYS: i64 = 60;

struct Patterns {
    month_first: Regex,
    day_first: Regex,
    iso: Regex,
    numeric: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| {
        const MONTH: &str = r"(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec)[a-z]*\.?";
        Patterns {
            month_first: Regex::new(&format!(
                r"\b{MONTH}[\s-]+(\d{{1,2}})(?:st|nd|rd|th)?,?[\s-]+(\d{{4}})\b"
            ))
            .unwrap(),
            day_first: Regex::new(&format!(
                r"\b(\d{{1,2}})(?:st|nd|rd|th)?[\s-]+(?:of\s+)?{MONTH},?[\s-]+(\d{{4}})\b"
            ))
            .unwrap(),
            iso: Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})\b").unwrap(),
            numeric: Regex::new(r"\b(\d{1,2})[/.-](\d{1,2})[/.-](\d{4})\b").unwrap(),
        }
    })
}

fn month_number(name: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    MONTHS
        .iter()
        .position(|m| name.starts_with(m))
        .map(|i| i as u32 + 1)
}

/// Whether the words just before byte `at` say the date is another one.
fn another_date(text: &str, at: usize) -> bool {
    let from = riddle_core::floor_char_boundary(text, at.saturating_sub(LOOK_BACK));
    let before = &text[from..at];
    NOT_ITS_DATE.iter().any(|w| before.contains(w))
}

/// The first and last day the message is about, or `None` when it names
/// no date that could be one. `sent` is the message's date.
pub fn dated_span(subject: &str, body: &str, sent: DateTime<Utc>) -> Option<DateSpan> {
    let text = format!("{subject}\n{body}").to_lowercase();
    let p = patterns();
    let mut dates: Vec<NaiveDate> = Vec::new();
    let mut keep = |at: usize, d: Option<NaiveDate>| {
        if let Some(d) = d.filter(|_| !another_date(&text, at)) {
            dates.push(d);
        }
    };
    for c in p.month_first.captures_iter(&text) {
        let d = match (month_number(&c[1]), c[2].parse(), c[3].parse()) {
            (Some(m), Ok(day), Ok(y)) => NaiveDate::from_ymd_opt(y, m, day),
            _ => None,
        };
        keep(c.get(0).unwrap().start(), d);
    }
    for c in p.day_first.captures_iter(&text) {
        let d = match (c[1].parse(), month_number(&c[2]), c[3].parse()) {
            (Ok(day), Some(m), Ok(y)) => NaiveDate::from_ymd_opt(y, m, day),
            _ => None,
        };
        keep(c.get(0).unwrap().start(), d);
    }
    for c in p.iso.captures_iter(&text) {
        let d = match (c[1].parse(), c[2].parse(), c[3].parse()) {
            (Ok(y), Ok(m), Ok(day)) => NaiveDate::from_ymd_opt(y, m, day),
            _ => None,
        };
        keep(c.get(0).unwrap().start(), d);
    }
    // Numeric dates, once the message shows which number is the day.
    let numeric: Vec<(usize, u32, u32, i32)> = p
        .numeric
        .captures_iter(&text)
        .filter_map(|c| {
            Some((
                c.get(0)?.start(),
                c[1].parse().ok()?,
                c[2].parse().ok()?,
                c[3].parse().ok()?,
            ))
        })
        .collect();
    let day_first = numeric.iter().any(|&(_, a, b, _)| a > 12 && b <= 12);
    let month_first = numeric.iter().any(|&(_, a, b, _)| b > 12 && a <= 12);
    if day_first != month_first {
        for (at, a, b, y) in numeric {
            let (day, month) = if day_first { (a, b) } else { (b, a) };
            keep(at, NaiveDate::from_ymd_opt(y, month, day));
        }
    }

    let sent_day = sent.date_naive();
    let earliest = sent_day - Duration::days(1);
    let latest = sent_day + Duration::days(MAX_AHEAD_DAYS);
    dates.retain(|d| (earliest..=latest).contains(d));
    let start = *dates.iter().min()?;
    let end = dates
        .iter()
        .copied()
        .filter(|d| *d - start <= Duration::days(MAX_SPAN_DAYS))
        .max()
        .unwrap_or(start);
    Some(DateSpan { start, end })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn sent(s: &str) -> DateTime<Utc> {
        format!("{s}T12:00:00Z").parse().unwrap()
    }

    #[test]
    fn a_parking_booking_runs_from_its_start_to_its_end_day_first() {
        let body = "Your Booking information\nCODE #T100554641\nSTART\n16:00 -\n27/06/2026\nEND\n16:00 -\n30/06/2026\nPARKING\nAlbufeira - P1";
        assert_eq!(
            dated_span("Your booking has been made!", body, sent("2026-06-27")),
            Some(DateSpan {
                start: day("2026-06-27"),
                end: day("2026-06-30")
            })
        );
    }

    #[test]
    fn a_hotel_stay_ignores_the_last_day_to_cancel() {
        let body = "Your reservation for Wednesday Oct 07, 2026 has been confirmed.\nRate per night Oct-07-2026 - Oct-08-2026 189.00 CAD\nIf you wish to cancel, please do by 11:59 p.m. on Oct-06-2026, to avoid a charge.";
        assert_eq!(
            dated_span(
                "Your Oct-07-2026 Confirmation #88080681",
                body,
                sent("2026-07-04")
            ),
            Some(DateSpan {
                start: day("2026-10-07"),
                end: day("2026-10-08")
            })
        );
    }

    #[test]
    fn a_named_month_after_the_day_and_an_iso_date_both_count() {
        let b = dated_span(
            "Check-in 7 October 2026",
            "Check-out: 2026-10-09",
            sent("2026-09-01"),
        );
        assert_eq!(
            b,
            Some(DateSpan {
                start: day("2026-10-07"),
                end: day("2026-10-09")
            })
        );
    }

    #[test]
    fn a_numeric_date_whose_order_is_unknown_is_left_out() {
        assert_eq!(
            dated_span("Your stay on 07-10-2026", "", sent("2026-10-01")),
            None
        );
    }

    #[test]
    fn dates_before_the_message_and_when_it_was_booked_are_not_the_booking() {
        assert_eq!(
            dated_span(
                "Thanks for staying with us",
                "Your stay ended on Oct 3, 2026. Booked on Oct 10, 2026.",
                sent("2026-10-12")
            ),
            None
        );
        assert_eq!(
            dated_span("Big sale", "Book now and save!", sent("2026-10-12")),
            None
        );
    }

    #[test]
    fn a_date_far_beyond_the_first_is_not_part_of_the_stay() {
        let b = dated_span(
            "Booking confirmed",
            "Arrive Nov 2, 2026, depart Nov 4, 2026. Offer valid until Dec 31, 2027.",
            sent("2026-10-12"),
        )
        .unwrap();
        assert_eq!((b.start, b.end), (day("2026-11-02"), day("2026-11-04")));
    }
}
