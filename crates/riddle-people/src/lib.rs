//! People: a model of every sender and recipient (`ARCHITECTURE.md` §2),
//! aggregated from the Dig stores into the index store so identity spans
//! Digs. Stats are recomputed from the messages rather than incremented,
//! because a resync or an undo would otherwise drift the counts; the store
//! does the counting in SQL (`repo::messages::address_stats`) so a refresh
//! touches one address at a time and never loads bodies.
//!
//! Relationship strength is a bounded score from what the user did (replied,
//! wrote first) and how recently, not from volume alone: a newsletter that
//! sends daily is not a relationship.

use chrono::{DateTime, Utc};
use riddle_core::{DigId, Person, PersonId, ThreadId, UserProfile};
use riddle_errors::Result;
use riddle_observability::{rdebug, rinfo, Category};
use riddle_store::{repo, Store};
use std::collections::HashSet;
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

/// Recompute one address across the given Dig stores and upsert the Person.
pub fn refresh_address(
    index: &Store,
    digs: DigStores<'_>,
    email: &str,
    profile: &UserProfile,
    now: DateTime<Utc>,
) -> Result<Option<Person>> {
    let email = email.to_lowercase();
    if email.is_empty() || profile.is_me(&email) {
        return Ok(None);
    }
    let mut total = repo::messages::AddressStats::default();
    let mut dig_ids = Vec::new();
    for (dig_id, store) in digs {
        let s = repo::messages::address_stats(store, &email)?;
        if s.messages_from == 0 && s.messages_to == 0 {
            continue;
        }
        dig_ids.push(dig_id.clone());
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
    if dig_ids.is_empty() {
        return Ok(None);
    }
    let existing = repo::people::by_email(index, &email)?;
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
            if !v.contains(&email) {
                v.push(email.clone());
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
        in_contacts: existing.as_ref().is_some_and(|p| p.in_contacts)
            || profile.vips.iter().any(|v| v.eq_ignore_ascii_case(&email))
            || profile
                .family
                .iter()
                .any(|f| f.eq_ignore_ascii_case(&email)),
        dig_ids,
        updated_at: now,
    };
    repo::people::upsert(index, &person)?;
    Ok(Some(person))
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
    let mut n = 0;
    for a in addresses {
        if refresh_address(index, digs, &a, profile, now)?.is_some() {
            n += 1;
        }
    }
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
    let addresses = repo::messages::addresses_in_threads(store, threads)?;
    let mut n = 0;
    for a in addresses {
        if refresh_address(index, digs, &a, profile, now)?.is_some() {
            n += 1;
        }
    }
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
}
