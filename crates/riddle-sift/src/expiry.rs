//! When a Find a route's `+ Expires:` raised stops mattering, as an instant.
//! "End of day" and "next morning" are the user's day, not UTC's: a daily
//! reminder that arrived at 11 pm used to keep a fixed 18 hours and outlive
//! its day, and one at 7 am lapsed at 1 am. The timezone is the profile's
//! (recorded from the system at boot); an unknown or missing one falls back
//! to UTC rather than guessing.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use riddle_core::{DateSpan, Expiry, DATED_DAY_HOUR, NEXT_MORNING_HOUR};

pub fn expires_at(
    e: Expiry,
    arrived: DateTime<Utc>,
    dated: Option<&DateSpan>,
    timezone: Option<&str>,
) -> Option<DateTime<Utc>> {
    Some(match e {
        Expiry::After { hours } => arrived + Duration::hours(i64::from(hours)),
        Expiry::AfterMinutes { minutes } => arrived + Duration::minutes(i64::from(minutes)),
        Expiry::EndOfDay => next_local(arrived, timezone, 0),
        Expiry::NextMorning => next_local(arrived, timezone, NEXT_MORNING_HOUR),
        // Midnight at the end of the last day the message is about.
        Expiry::AfterDates => at_local(dated?.end + Duration::days(1), 0, timezone),
    })
}

/// When the Find a route raised wakes: `DATED_DAY_HOUR` on the first day
/// the message is about, for `Expires: after its dates`; `None` for every
/// other expiry or a message without dates.
pub fn wakes_at(
    e: Expiry,
    dated: Option<&DateSpan>,
    timezone: Option<&str>,
) -> Option<DateTime<Utc>> {
    match e {
        Expiry::AfterDates => Some(at_local(dated?.start, DATED_DAY_HOUR, timezone)),
        _ => None,
    }
}

/// `hour` o'clock on `day` where the user is.
fn at_local(day: NaiveDate, hour: u32, timezone: Option<&str>) -> DateTime<Utc> {
    let zone = riddle_core::user_zone(timezone);
    local(
        zone,
        day.and_hms_opt(hour, 0, 0).expect("a whole hour exists"),
    )
}

/// `hour` o'clock on the local day after the one `arrived` falls on.
fn next_local(arrived: DateTime<Utc>, timezone: Option<&str>, hour: u32) -> DateTime<Utc> {
    let zone = riddle_core::user_zone(timezone);
    let day = arrived.with_timezone(&zone).date_naive() + Duration::days(1);
    let wanted = day.and_hms_opt(hour, 0, 0).expect("a whole hour exists");
    local(zone, wanted)
}

/// A wall-clock time in `zone`. Where daylight saving skips it (a few zones
/// spring forward at midnight), the first moment after the gap.
fn local(zone: Tz, wanted: NaiveDateTime) -> DateTime<Utc> {
    (0..=2)
        .find_map(|h| {
            zone.from_local_datetime(&(wanted + Duration::hours(h)))
                .earliest()
        })
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn end_of_day_is_midnight_where_the_user_is_not_in_utc() {
        // 7 am and 11 pm in Toronto (UTC-4 in September): both end at local midnight.
        let tz = Some("America/Toronto");
        assert_eq!(
            expires_at(Expiry::EndOfDay, utc("2026-09-27T11:00:00Z"), None, tz).unwrap(),
            utc("2026-09-28T04:00:00Z")
        );
        assert_eq!(
            expires_at(Expiry::EndOfDay, utc("2026-09-28T03:00:00Z"), None, tz).unwrap(),
            utc("2026-09-28T04:00:00Z"),
            "11 pm on the 27th is still the 27th in Toronto"
        );
    }

    #[test]
    fn next_morning_gives_a_late_message_a_morning() {
        let tz = Some("America/Toronto");
        assert_eq!(
            expires_at(Expiry::NextMorning, utc("2026-09-28T03:50:00Z"), None, tz).unwrap(),
            utc("2026-09-28T13:00:00Z"),
            "11:50 pm on the 27th lasts until 9 am on the 28th"
        );
    }

    #[test]
    fn a_daylight_saving_change_moves_the_utc_instant_not_the_local_hour() {
        // Toronto falls back on 1 November 2026: 9 am is UTC-5 that morning.
        assert_eq!(
            expires_at(
                Expiry::NextMorning,
                utc("2026-10-31T15:00:00Z"),
                None,
                Some("America/Toronto")
            )
            .unwrap(),
            utc("2026-11-01T14:00:00Z")
        );
    }

    #[test]
    fn a_span_ignores_the_timezone_and_an_unknown_zone_falls_back_to_utc() {
        let at = utc("2026-09-27T11:00:00Z");
        assert_eq!(
            expires_at(
                Expiry::After { hours: 18 },
                at,
                None,
                Some("America/Toronto")
            )
            .unwrap(),
            utc("2026-09-28T05:00:00Z")
        );
        assert_eq!(
            expires_at(Expiry::EndOfDay, at, None, Some("Mars/Olympus")).unwrap(),
            utc("2026-09-28T00:00:00Z")
        );
        assert_eq!(
            expires_at(Expiry::EndOfDay, at, None, None).unwrap(),
            utc("2026-09-28T00:00:00Z")
        );
    }

    #[test]
    fn a_booking_wakes_on_its_first_morning_and_expires_after_its_last_day() {
        let tz = Some("America/Toronto");
        let b = DateSpan {
            start: "2026-10-07".parse().unwrap(),
            end: "2026-10-08".parse().unwrap(),
        };
        let arrived = utc("2026-07-04T12:00:00Z");
        assert_eq!(
            wakes_at(Expiry::AfterDates, Some(&b), tz),
            Some(utc("2026-10-07T10:00:00Z")),
            "6 am in Toronto"
        );
        assert_eq!(
            expires_at(Expiry::AfterDates, arrived, Some(&b), tz),
            Some(utc("2026-10-09T04:00:00Z")),
            "midnight after check-out"
        );
        assert_eq!(expires_at(Expiry::AfterDates, arrived, None, tz), None);
        assert_eq!(wakes_at(Expiry::EndOfDay, Some(&b), tz), None);
    }
}
