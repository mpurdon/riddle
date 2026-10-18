//! People: a model of every sender and recipient (`ARCHITECTURE.md` §2),
//! aggregated from the Dig stores into the index store so identity spans
//! Digs. Stats are recomputed from the messages rather than incremented,
//! because a resync or an undo would otherwise drift the counts; the store
//! does the counting in SQL and never loads bodies. A refresh takes a set
//! of addresses (`refresh_addresses`): every sync page refreshed each of
//! its addresses one at a time, seven queries with `json_extract` over
//! every header each, which is where a 45-second poll spent most of its
//! time (17 Oct 2026). The set goes to `address_stats_many` as one
//! parameter, three queries per Dig store whatever its size.
//!
//! Relationship strength is a bounded score from what the user did (replied,
//! wrote first) and how recently, not from volume alone: a newsletter that
//! sends daily is not a relationship.
//!
//! Cadence is how regularly a sender writes, from arrival dates alone. It
//! answers "does only the newest one matter?" for a Mesh; whether the mail
//! matters at all is a Signal's question. On the mailbox that prompted it,
//! eighteen senders wrote daily and one of them (a journaling reminder) was
//! a to-do; `personally_relevant` told them apart where cadence could not.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc, Weekday};
use riddle_core::{Cadence, DigId, Person, PersonId, ThreadId, UserProfile};
use riddle_errors::Result;
use riddle_observability::{rdebug, rinfo, Category};
use riddle_store::{repo, Store};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// A Dig store to aggregate over.
pub type DigStores<'a> = &'a [(DigId, Arc<Store>)];

/// 0..1 from reply counts and recency. Automated senders are capped low.
pub fn relationship_strength(
    messages_to: u64,
    messages_from: u64,
    replied: bool,
    last_contact: Option<DateTime<Utc>>,
    automated: bool,
    now: DateTime<Utc>,
) -> f64 {
    if automated {
        return 0.05_f64.min(if replied { 0.05 } else { 0.02 });
    }
    let engagement =
        2.0 * messages_to as f64 + 0.3 * messages_from as f64 + if replied { 2.0 } else { 0.0 };
    let base = 1.0 - (-engagement / 8.0).exp();
    let recency = match last_contact {
        Some(t) => {
            let days = (now - t).num_days().max(0) as f64;
            if days <= 30.0 {
                1.0
            } else {
                (0.3_f64).max(1.0 - (days - 30.0) / 365.0 * 0.7)
            }
        }
        None => 0.5,
    };
    (base * recency).clamp(0.0, 1.0)
}

/// How far back cadence looks: long enough for four monthly letters.
pub const CADENCE_WINDOW_DAYS: i64 = 120;

/// Classify a sender's rhythm from when their mail arrived, by the user's
/// local day. `None` until mail has come on three different days.
pub fn cadence_of(arrivals: &[DateTime<Utc>], timezone: Option<&str>) -> Option<Cadence> {
    let zone = riddle_core::user_zone(timezone);
    let mut days: Vec<NaiveDate> = arrivals
        .iter()
        .map(|t| t.with_timezone(&zone).date_naive())
        .collect();
    days.sort();
    days.dedup();
    if days.len() < 3 {
        return None;
    }
    let (first, last) = (days[0], days[days.len() - 1]);
    let span = (last - first).num_days() + 1;
    let weekend = |d: &NaiveDate| matches!(d.weekday(), Weekday::Sat | Weekday::Sun);
    let on_weekends = days.iter().filter(|d| weekend(d)).count();
    if span >= 7 {
        let covered = days.len() as f64;
        if on_weekends > 0 && covered / span as f64 >= 0.6 {
            return Some(Cadence::Daily);
        }
        let weekdays_in_span = (0..span)
            .map(|i| first + Duration::days(i))
            .filter(|d| !weekend(d))
            .count();
        if on_weekends == 0 && covered / weekdays_in_span as f64 >= 0.6 {
            return Some(Cadence::Weekdays);
        }
    }
    let mut gaps: Vec<i64> = days.windows(2).map(|w| (w[1] - w[0]).num_days()).collect();
    gaps.sort();
    Some(match gaps[gaps.len() / 2] {
        2..=4 => Cadence::EveryFewDays,
        5..=9 => Cadence::Weekly,
        25..=35 => Cadence::Monthly,
        _ => Cadence::Irregular,
    })
}

/// Mail on a timer: at least five arrivals, no more than 1.1 a day across
/// the span they cover, and 80% of them within an hour of the sender's usual local
/// time. Minutes wrap at midnight, so 23:50 and 00:10 are twenty apart.
pub fn on_schedule(arrivals: &[DateTime<Utc>], timezone: Option<&str>) -> bool {
    if arrivals.len() < 5 {
        return false;
    }
    let zone = riddle_core::user_zone(timezone);
    let local: Vec<_> = arrivals.iter().map(|t| t.with_timezone(&zone)).collect();
    // Per day of the whole span, not per day with mail: a reminder that
    // wobbles either side of midnight puts two on one date and none on the
    // next, and is still one a day.
    let first = local
        .iter()
        .map(|t| t.date_naive())
        .min()
        .unwrap_or_default();
    let last = local
        .iter()
        .map(|t| t.date_naive())
        .max()
        .unwrap_or_default();
    let span_days = (last - first).num_days() + 1;
    if arrivals.len() as f64 > span_days as f64 * 1.1 {
        return false;
    }
    const DAY: i64 = 24 * 60;
    let minutes: Vec<i64> = local
        .iter()
        .map(|t| i64::from(t.hour() * 60 + t.minute()))
        .collect();
    let apart = |a: i64, b: i64| {
        let d = (a - b).rem_euclid(DAY);
        d.min(DAY - d)
    };
    // The usual time: the arrival minute closest, in total, to all the others.
    let usual = minutes
        .iter()
        .copied()
        .min_by_key(|&m| minutes.iter().map(|&o| apart(m, o)).sum::<i64>())
        .unwrap_or_default();
    let near = minutes.iter().filter(|&&m| apart(m, usual) <= 60).count();
    near as f64 >= minutes.len() as f64 * 0.8
}

/// Addresses per round of the stats queries: the set travels as one JSON
/// parameter, and a sync page's few hundred addresses fit in one round.
const REFRESH_BATCH: usize = 500;

/// Recompute one address across the given Dig stores and upsert the Person.
pub fn refresh_address(
    index: &Store,
    digs: DigStores<'_>,
    email: &str,
    profile: &UserProfile,
    now: DateTime<Utc>,
) -> Result<Option<Person>> {
    Ok(refresh_addresses(index, digs, &[email.to_string()], profile, now)?.pop())
}

/// What one address added up to across the Dig stores: its stats, the
/// Digs it has mail in, and when its mail arrived (for cadence).
struct Tally {
    stats: repo::messages::AddressStats,
    dig_ids: Vec<DigId>,
    arrivals: Vec<DateTime<Utc>>,
}

/// Recompute a set of addresses across the given Dig stores and upsert
/// their People, in the order given. The user's own addresses, empty ones
/// and ones with no mail are skipped and not returned.
pub fn refresh_addresses(
    index: &Store,
    digs: DigStores<'_>,
    emails: &[String],
    profile: &UserProfile,
    now: DateTime<Utc>,
) -> Result<Vec<Person>> {
    let mut wanted: Vec<String> = Vec::with_capacity(emails.len());
    for e in emails {
        let e = e.to_lowercase();
        if !e.is_empty() && !profile.is_me(&e) && !wanted.contains(&e) {
            wanted.push(e);
        }
    }
    let since = now - Duration::days(CADENCE_WINDOW_DAYS);
    let mut out = Vec::new();
    for chunk in wanted.chunks(REFRESH_BATCH) {
        let mut tallies: HashMap<String, Tally> = HashMap::new();
        for (dig_id, store) in digs {
            let stats = repo::messages::address_stats_many(store, chunk)?;
            let senders: Vec<String> = stats
                .iter()
                .filter(|(_, s)| s.messages_from > 0)
                .map(|(e, _)| e.clone())
                .collect();
            let mut arrivals = if senders.is_empty() {
                HashMap::new()
            } else {
                repo::messages::arrivals_from_many(store, &senders, &since)?
            };
            for (email, s) in stats {
                if s.messages_from == 0 && s.messages_to == 0 {
                    continue;
                }
                let t = tallies.entry(email.clone()).or_insert_with(|| Tally {
                    stats: repo::messages::AddressStats::default(),
                    dig_ids: Vec::new(),
                    arrivals: Vec::new(),
                });
                t.dig_ids.push(dig_id.clone());
                if let Some(a) = arrivals.remove(&email) {
                    t.arrivals.extend(a);
                }
                let total = &mut t.stats;
                total.messages_from += s.messages_from;
                total.messages_to += s.messages_to;
                total.automated += s.automated;
                total.mailing_list += s.mailing_list;
                total.auth_failures += s.auth_failures;
                total.spam_verdicts += s.spam_verdicts;
                total.first_at = match (total.first_at, s.first_at) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
                total.last_at = match (total.last_at, s.last_at) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                };
                if total.name.is_none() {
                    total.name = s.name;
                }
            }
        }
        for email in chunk {
            let Some(mut t) = tallies.remove(email) else {
                continue;
            };
            t.arrivals.sort();
            let person = person_from(index, email, t, profile, now)?;
            repo::people::upsert(index, &person)?;
            out.push(person);
        }
    }
    Ok(out)
}

/// The Person one address adds up to, keeping the identity and the
/// contacts flag of the one already stored.
fn person_from(
    index: &Store,
    email: &str,
    tally: Tally,
    profile: &UserProfile,
    now: DateTime<Utc>,
) -> Result<Person> {
    let Tally {
        stats: total,
        dig_ids,
        arrivals,
    } = tally;
    let existing = repo::people::by_email(index, email)?;
    let replied = total.messages_to > 0;
    // Automated when most received mail carries list or bulk markers.
    let automated = total.messages_from > 0 && total.automated * 2 >= total.messages_from;
    let mailing_list = total.messages_from > 0 && total.mailing_list * 2 >= total.messages_from;
    let organisation = email
        .rsplit_once('@')
        .map(|(_, d)| d.to_string())
        .filter(|d| !is_public_mail_domain(d));
    let person = Person {
        id: existing
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_else(PersonId::new),
        emails: {
            let mut v = existing
                .as_ref()
                .map(|p| p.emails.clone())
                .unwrap_or_default();
            if !v.iter().any(|e| e == email) {
                v.push(email.to_string());
            }
            v
        },
        name: total
            .name
            .or(existing.as_ref().and_then(|p| p.name.clone())),
        organisation,
        relationship_strength: relationship_strength(
            total.messages_to,
            total.messages_from,
            replied,
            total.last_at,
            automated,
            now,
        ),
        first_contact_at: total.first_at,
        last_contact_at: total.last_at,
        replied_to_by_user: replied,
        messages_from: total.messages_from,
        messages_to: total.messages_to,
        automated,
        mailing_list,
        auth_failures: total.auth_failures as u32,
        spam_verdicts: total.spam_verdicts as u32,
        in_contacts: existing.as_ref().is_some_and(|p| p.in_contacts) || profile.is_contact(email),
        dig_ids,
        updated_at: now,
        cadence: cadence_of(&arrivals, profile.timezone.as_deref()),
        on_schedule: on_schedule(&arrivals, profile.timezone.as_deref()),
    };
    Ok(person)
}

fn is_public_mail_domain(d: &str) -> bool {
    const PUBLIC: &[&str] = &[
        "gmail.com",
        "googlemail.com",
        "outlook.com",
        "hotmail.com",
        "live.com",
        "yahoo.com",
        "icloud.com",
        "me.com",
        "proton.me",
        "protonmail.com",
        "fastmail.com",
        "hey.com",
        "aol.com",
        "gmx.com",
        "gmx.de",
        "web.de",
        "mail.com",
    ];
    let d = d.to_ascii_lowercase();
    PUBLIC.contains(&d.as_str())
        || d.ends_with(".example")
            && (d.starts_with("gmail") || d.starts_with("outlook") || d.starts_with("fastmail"))
}

/// Aggregate every address seen in one Dig (senders and sent-to recipients),
/// summing across all open Digs. Returns how many People were written.
pub fn rebuild_for_dig(
    index: &Store,
    digs: DigStores<'_>,
    dig: &DigId,
    profile: &UserProfile,
    now: DateTime<Utc>,
) -> Result<usize> {
    let Some((_, store)) = digs.iter().find(|(d, _)| d == dig) else {
        return Ok(0);
    };
    let mut addresses: HashSet<String> = repo::messages::distinct_senders(store)?
        .into_iter()
        .map(|s| s.to_lowercase())
        .collect();
    addresses.extend(repo::messages::replied_to_senders(store)?);
    let addresses: Vec<String> = addresses.into_iter().collect();
    let n = refresh_addresses(index, digs, &addresses, profile, now)?.len();
    rinfo!(
        Category::Sync,
        "people rebuilt for dig {}: {} people",
        dig,
        n
    );
    Ok(n)
}

/// Refresh the People touched by a set of threads (after a sync page).
pub fn update_for_threads(
    index: &Store,
    digs: DigStores<'_>,
    dig: &DigId,
    threads: &[ThreadId],
    profile: &UserProfile,
    now: DateTime<Utc>,
) -> Result<usize> {
    let Some((_, store)) = digs.iter().find(|(d, _)| d == dig) else {
        return Ok(0);
    };
    let addresses: Vec<String> = repo::messages::addresses_in_threads(store, threads)?
        .into_iter()
        .collect();
    let n = refresh_addresses(index, digs, &addresses, profile, now)?.len();
    rdebug!(
        Category::Sync,
        "people updated for {} threads in dig {}: {} people",
        threads.len(),
        dig,
        n
    );
    Ok(n)
}

pub fn lookup(index: &Store, email: &str) -> Result<Option<Person>> {
    repo::people::by_email(index, email)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use riddle_mail::{FixtureProvider, FixtureSpec};
    use riddle_store::{StoreKind, Stores};

    fn seeded() -> (Arc<Store>, Arc<Store>, DigId, FixtureProvider, UserProfile) {
        let stores = Stores::in_memory().unwrap();
        let dig = DigId::from("d1");
        let fx = FixtureProvider::new(
            dig.clone(),
            FixtureSpec {
                count: 200,
                seed: 9,
                ..Default::default()
            },
        );
        let store = stores.dig(&dig).unwrap();
        let all = fx.snapshot();
        store
            .write(|c| {
                for m in &all {
                    repo::messages::upsert_in(c, m)?;
                }
                Ok(())
            })
            .unwrap();
        let profile = UserProfile {
            addresses: vec!["matt@example.com".into()],
            organisation_domains: vec!["example.com".into()],
            ..Default::default()
        };
        let _ = StoreKind::Dig;
        (stores.index(), store, dig, fx, profile)
    }

    #[test]
    fn a_colleague_the_user_replied_to_outranks_a_newsletter() {
        let (index, store, dig, _fx, profile) = seeded();
        let digs = vec![(dig.clone(), store.clone())];
        let n = rebuild_for_dig(&index, &digs, &dig, &profile, Utc::now()).unwrap();
        assert!(n > 10);
        let priya = lookup(&index, "priya@example.com")
            .unwrap()
            .expect("priya is a person");
        assert!(priya.replied_to_by_user);
        assert!(priya.messages_to > 0);
        assert_eq!(priya.organisation.as_deref(), Some("example.com"));
        assert!(!priya.automated);
        let github = lookup(&index, "notifications@github.com")
            .unwrap()
            .expect("github is a sender");
        assert!(github.automated && github.mailing_list);
        assert!(!github.replied_to_by_user);
        assert!(priya.relationship_strength > github.relationship_strength);
        assert!(github.relationship_strength <= 0.05);
        assert!(
            lookup(&index, "matt@example.com").unwrap().is_none(),
            "the user is not a Person"
        );
        let phisher = lookup(&index, "security@micros0ft-support.example").unwrap();
        if let Some(p) = phisher {
            assert!(p.auth_failures > 0);
        }
    }

    #[test]
    fn incremental_updates_touch_only_the_threads_given_and_keep_identity() {
        let (index, store, dig, fx, profile) = seeded();
        let digs = vec![(dig.clone(), store.clone())];
        rebuild_for_dig(&index, &digs, &dig, &profile, Utc::now()).unwrap();
        let before = lookup(&index, "priya@example.com").unwrap().unwrap();
        let thread = fx
            .snapshot()
            .into_iter()
            .find(|m| m.from.email == "priya@example.com")
            .unwrap()
            .thread_id;
        let n = update_for_threads(&index, &digs, &dig, &[thread], &profile, Utc::now()).unwrap();
        assert!(n >= 1);
        let after = lookup(&index, "priya@example.com").unwrap().unwrap();
        assert_eq!(before.id, after.id, "identity is stable across refreshes");
        assert_eq!(before.messages_from, after.messages_from);
    }

    #[test]
    fn a_batch_refresh_says_what_the_single_refreshes_say_and_skips_the_user_and_strangers() {
        let (index, store, dig, _fx, profile) = seeded();
        let digs = vec![(dig.clone(), store.clone())];
        let now = Utc::now();
        let set: Vec<String> = [
            "priya@example.com",
            "notifications@github.com",
            "matt@example.com",
            "nobody@nowhere.example",
            "PRIYA@example.com",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let people = refresh_addresses(&index, &digs, &set, &profile, now).unwrap();
        let emails: Vec<&str> = people.iter().map(|p| p.emails[0].as_str()).collect();
        assert_eq!(
            emails,
            ["priya@example.com", "notifications@github.com"],
            "in the order asked, once each, without the user or an address with no mail"
        );
        for p in &people {
            let single = refresh_address(&index, &digs, &p.emails[0], &profile, now)
                .unwrap()
                .unwrap();
            assert_eq!(single, *p, "{}", p.emails[0]);
        }
    }

    #[test]
    fn relationship_strength_decays_with_time_and_rewards_replies() {
        let now = Utc::now();
        let fresh = relationship_strength(5, 10, true, Some(now - Duration::days(2)), false, now);
        let stale = relationship_strength(5, 10, true, Some(now - Duration::days(300)), false, now);
        let one_way =
            relationship_strength(0, 10, false, Some(now - Duration::days(2)), false, now);
        assert!(fresh > stale);
        assert!(fresh > one_way);
        assert!(relationship_strength(0, 500, false, Some(now), true, now) < 0.1);
    }

    /// `n` arrivals `every` days apart from 1 June 2026 (a Monday), at `hour` UTC.
    fn every(n: i64, every: i64, hour: u32) -> Vec<DateTime<Utc>> {
        let start: DateTime<Utc> = "2026-06-01T00:00:00Z".parse().unwrap();
        (0..n)
            .map(|i| start + Duration::days(i * every) + Duration::hours(hour.into()))
            .collect()
    }

    #[test]
    fn cadence_reads_daily_weekly_monthly_and_irregular_from_arrivals() {
        assert_eq!(cadence_of(&every(60, 1, 11), None), Some(Cadence::Daily));
        assert_eq!(cadence_of(&every(10, 7, 9), None), Some(Cadence::Weekly));
        assert_eq!(
            cadence_of(&every(30, 3, 9), None),
            Some(Cadence::EveryFewDays),
            "a sale every three days is not irregular"
        );
        assert_eq!(cadence_of(&every(4, 30, 9), None), Some(Cadence::Monthly));
        let scattered: Vec<_> = [0, 1, 13, 14, 40, 41, 90]
            .iter()
            .map(|d| every(1, 1, 9)[0] + Duration::days(*d))
            .collect();
        assert_eq!(cadence_of(&scattered, None), Some(Cadence::Irregular));
        assert_eq!(
            cadence_of(&every(2, 1, 9), None),
            None,
            "two days is too little to say"
        );
    }

    #[test]
    fn a_sender_that_skips_every_weekend_is_weekdays_not_daily() {
        let weekdays: Vec<_> = every(56, 1, 9)
            .into_iter()
            .filter(|t| !matches!(t.weekday(), Weekday::Sat | Weekday::Sun))
            .collect();
        assert_eq!(cadence_of(&weekdays, None), Some(Cadence::Weekdays));
    }

    #[test]
    fn several_a_day_is_still_daily_and_days_are_the_user_s_days() {
        let mut busy = every(30, 1, 9);
        busy.extend(every(30, 1, 15));
        assert_eq!(cadence_of(&busy, None), Some(Cadence::Daily));
        // 03:00 UTC is the evening before in Toronto: every second day in UTC
        // at 03:00 and 23:00 is one local day each time, i.e. every other day.
        let pairs: Vec<_> = (0..20)
            .flat_map(|i| {
                let d = every(1, 1, 0)[0] + Duration::days(i * 2);
                [d + Duration::hours(3), d - Duration::hours(1)]
            })
            .collect();
        assert_eq!(cadence_of(&pairs, None), Some(Cadence::Daily));
        assert_eq!(
            cadence_of(&pairs, Some("America/Toronto")),
            Some(Cadence::EveryFewDays),
            "in Toronto both land on the same evening, every other day"
        );
    }

    #[test]
    fn mail_on_a_timer_is_on_schedule_and_mail_set_off_by_the_user_is_not() {
        // A reminder at 7:00 in Toronto every day, give or take a few minutes.
        let reminder: Vec<_> = every(30, 1, 11)
            .into_iter()
            .enumerate()
            .map(|(i, t)| t + Duration::minutes((i % 5) as i64))
            .collect();
        assert!(on_schedule(&reminder, Some("America/Toronto")));
        // Confirmations: most days, at whatever hour the user bought something, often twice.
        let confirmations: Vec<_> = every(30, 1, 0)
            .into_iter()
            .enumerate()
            .flat_map(|(i, t)| {
                let first = t + Duration::hours((i * 7 % 24) as i64);
                if i % 2 == 0 {
                    vec![first, first + Duration::hours(3)]
                } else {
                    vec![first]
                }
            })
            .collect();
        assert!(!on_schedule(&confirmations, None));
        assert!(
            !on_schedule(&reminder[..4], None),
            "four is too few to call it a schedule"
        );
    }

    #[test]
    fn a_schedule_around_midnight_is_still_a_schedule() {
        let around_midnight: Vec<_> = every(20, 1, 0)
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                if i % 2 == 0 {
                    t - Duration::minutes(10)
                } else {
                    t + Duration::minutes(10)
                }
            })
            .collect();
        assert!(on_schedule(&around_midnight, None));
    }
}
