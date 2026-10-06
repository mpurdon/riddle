//! Temporal Sift (§19): dispositions that change because time passed or a
//! thread changed, not because a message arrived. Runs from the scheduler as
//! a durable job; every outcome carries an Activity-ready summary so the
//! integrator writes the ledger without re-deriving what happened.

use crate::service::FindService;
use riddle_core::*;
use riddle_errors::Result;
use riddle_observability::{rinfo, rwarn, Category};
use riddle_store::repo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct TemporalOptions {
    /// Days without a reply to an open ask of mine before a Follow Up Find.
    pub follow_up_after_days: u32,
    pub follow_up_priority: u8,
}

impl Default for TemporalOptions {
    fn default() -> Self {
        Self {
            follow_up_after_days: 3,
            follow_up_priority: 45,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalOutcomeKind {
    Unsnoozed,
    Expired,
    FollowUpCreated,
    RespondResolved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemporalOutcome {
    pub kind: TemporalOutcomeKind,
    pub find_id: FindId,
    pub dig_id: Option<DigId>,
    pub thread_id: Option<ThreadId>,
    /// "Snooze ended: Re: budget", "Follow Up: no reply in 4 days to ..."
    pub summary: String,
}

impl TemporalOutcome {
    pub fn of(kind: TemporalOutcomeKind, f: &Find, summary: String) -> Self {
        Self {
            kind,
            find_id: f.id.clone(),
            dig_id: f.dig_ids.first().cloned(),
            thread_id: f.sources.first().map(|s| s.thread_id.clone()),
            summary,
        }
    }
}

impl FindService {
    fn expire(&self, id: &FindId) -> Result<TemporalOutcome> {
        self.transition(id, FindStatus::Expired, Some(ResolvedBy::Temporal))
            .map(|f| {
                let summary = format!("{} expired: {}", f.find_type.label(), f.title);
                TemporalOutcome::of(TemporalOutcomeKind::Expired, &f, summary)
            })
    }

    /// One Temporal Sift pass. Each step is independent; a failure in one is
    /// logged and the others still run.
    pub fn tick(&self, opts: &TemporalOptions) -> Result<Vec<TemporalOutcome>> {
        let mut out = Vec::new();
        let now = self.clock.now();
        let index = self.stores.index();

        // 1. Finds past their `+ Expires:` time (snoozed or not), snoozes
        //    that ended, and dated Finds past their date.
        for f in repo::finds::due_for_temporal(&index, &now)? {
            // The query leaves kept Finds out of expiry, but a kept Find
            // whose snooze ended comes back for that; it must not expire.
            let past_expiry = f.kept_open_at.is_none() && f.expires_at.is_some_and(|at| at <= now);
            let result = match f.status {
                FindStatus::Open | FindStatus::Snoozed if past_expiry => self.expire(&f.id),
                FindStatus::Snoozed if f.snoozed_until.is_some_and(|u| u <= now) => {
                    self.transition(&f.id, FindStatus::Open, None).map(|f| {
                        let summary = format!("Snooze ended: {}", f.title);
                        TemporalOutcome::of(TemporalOutcomeKind::Unsnoozed, &f, summary)
                    })
                }
                FindStatus::Open if f.find_type.is_dated() => self.expire(&f.id),
                _ => continue,
            };
            match result {
                Ok(o) => out.push(o),
                Err(e) => rwarn!(
                    Category::Sift,
                    "temporal transition failed for find {}: {}",
                    f.id,
                    e.message
                ),
            }
        }

        // 2. Follow Up Finds from stalled asks of mine, per Dig.
        let digs = repo::digs::list_enabled(&index)?;
        let stale_before = now - chrono::Duration::days(opts.follow_up_after_days as i64);
        for dig in &digs {
            let store = match self.stores.dig(&dig.id) {
                Ok(s) => s,
                Err(e) => {
                    rwarn!(
                        Category::Sift,
                        "temporal: cannot open dig {}: {}",
                        dig.id,
                        e.message
                    );
                    continue;
                }
            };
            for ts in repo::thread_state::with_open_asks_from_me(&store)? {
                if ts.last_speaker != Speaker::Me {
                    continue;
                }
                let Some(last) = ts.last_message_at else {
                    continue;
                };
                if last > stale_before {
                    continue;
                }
                let key = format!("followup:{}", ts.thread_id);
                let existing = repo::finds::any_by_identity(&index, &key)?;
                let already = existing.iter().any(|f| {
                    matches!(f.status, FindStatus::Open | FindStatus::Snoozed)
                        || f.created_at >= last
                });
                if already {
                    continue;
                }
                let last_message = repo::messages::thread_messages(&store, &dig.id, &ts.thread_id)?
                    .into_iter()
                    .last();
                let days = (now - last).num_days().max(1);
                let ask = ts.open_asks_from_me.first().cloned().unwrap_or_default();
                let title = last_message
                    .as_ref()
                    .map(|m| m.subject.clone())
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| format!("Follow up: {ask}"));
                let mut find = Find {
                    code: None,
                    id: FindId::new(),
                    find_type: FindType::FollowUp,
                    title,
                    summary: format!(
                        "No reply in {days} days to: {}",
                        ts.open_asks_from_me.join("; ")
                    ),
                    priority: opts.follow_up_priority,
                    status: FindStatus::Open,
                    due_at: None,
                    snoozed_until: None,
                    created_at: now,
                    updated_at: now,
                    resolved_at: None,
                    resolved_by: None,
                    sources: vec![],
                    dig_ids: vec![dig.id.clone()],
                    identity_key: key,
                    reason: format!(
                        "Temporal Sift: you asked \"{ask}\" and nobody replied in {days} days"
                    ),
                    confidence: None,
                    classification_id: None,
                    trace_id: None,
                    superseded_by: None,
                    series: None,
                    expires_at: None,
                    kept_open_at: None,
                    notify: false,
                };
                if let Some(m) = &last_message {
                    find.sources.push(FindSource {
                        message_id: m.id.clone(),
                        thread_id: m.thread_id.clone(),
                        dig_id: dig.id.clone(),
                    });
                }
                repo::finds::upsert(&index, &find)?;
                self.events
                    .emit(RiddleEvent::FindCreated { find: find.clone() });
                out.push(TemporalOutcome {
                    kind: TemporalOutcomeKind::FollowUpCreated,
                    find_id: find.id.clone(),
                    dig_id: Some(dig.id.clone()),
                    thread_id: Some(ts.thread_id.clone()),
                    summary: format!("Follow Up: no reply in {days} days to \"{ask}\""),
                });
            }
        }

        // 3. Respond Finds resolved by my reply from any client.
        for f in repo::finds::list_by_status(&index, FindStatus::Open, Some(FindType::Respond))? {
            let mut replied = false;
            for src in &f.sources {
                let Ok(store) = self.stores.dig(&src.dig_id) else {
                    continue;
                };
                if let Some(ts) = repo::thread_state::get(&store, &src.thread_id)? {
                    if ts.last_reply_by_me_at.is_some_and(|t| t > f.created_at) {
                        replied = true;
                        break;
                    }
                }
            }
            if replied {
                match self.transition(&f.id, FindStatus::Resolved, Some(ResolvedBy::ThreadState)) {
                    Ok(f) => out.push(TemporalOutcome {
                        kind: TemporalOutcomeKind::RespondResolved,
                        find_id: f.id.clone(),
                        dig_id: f.dig_ids.first().cloned(),
                        thread_id: f.sources.first().map(|s| s.thread_id.clone()),
                        summary: format!("Resolved: you replied to {}", f.title),
                    }),
                    Err(e) => rwarn!(
                        Category::Sift,
                        "temporal resolve failed for find {}: {}",
                        f.id,
                        e.message
                    ),
                }
            }
        }
        if !out.is_empty() {
            rinfo!(Category::Sift, "temporal sift: {} outcomes", out.len());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::Applied;
    use riddle_store::Stores;
    use std::sync::Arc;

    fn setup() -> (FindService, Arc<FixedClock>, DigId) {
        let stores = Arc::new(Stores::in_memory().unwrap());
        let clock = Arc::new(FixedClock::at(chrono::Utc::now()));
        let dig = DigId("d1".into());
        repo::digs::upsert(
            &stores.index(),
            &Dig {
                id: dig.clone(),
                name: "d".into(),
                email: "me@x".into(),
                provider: ProviderKind::Fixture,
                group: None,
                privacy: PrivacyMode::CloudJudgment,
                projection: ProjectionMode::None,
                status: DigStatus::Synced,
                shadow_started_at: None,
                created_at: clock.now(),
                disabled: false,
                mailbox: None,
            },
        )
        .unwrap();
        (
            FindService::new(stores, clock.clone(), Arc::new(RecordingSink::default())),
            clock,
            dig,
        )
    }

    fn proposal(
        dig: &DigId,
        pid: &str,
        find_type: FindType,
        due_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> FindProposal {
        FindProposal {
            code: None,
            find_type,
            title: format!("t-{pid}"),
            summary: "s".into(),
            priority: 50,
            reason: "r".into(),
            identity_key: format!("k-{pid}"),
            due_at,
            source: FindSource {
                message_id: MessageId::for_provider(dig, pid),
                thread_id: ThreadId::for_provider(dig, &format!("th-{pid}")),
                dig_id: dig.clone(),
            },
            confidence: None,
            classification_id: None,
            trace_id: None,
            series: None,
            expires_at: None,
            notify: false,
        }
    }

    #[test]
    fn temporal_sift_creates_a_follow_up_expires_a_dated_find_and_unsnoozes() {
        let (svc, clock, dig) = setup();
        let now = clock.now();
        let store = svc.stores.dig(&dig).unwrap();
        // A thread where I asked something four days ago and nobody replied.
        let thread = ThreadId::for_provider(&dig, "stalled");
        repo::thread_state::put(
            &store,
            &ThreadState {
                thread_id: thread.clone(),
                dig_id: dig.clone(),
                last_speaker: Speaker::Me,
                open_asks_from_them: vec![],
                open_asks_from_me: vec!["send the contract".into()],
                my_commitments: vec![],
                last_reply_by_me_at: Some(now - chrono::Duration::days(4)),
                last_message_at: Some(now - chrono::Duration::days(4)),
                message_count: 2,
                updated_at: now,
            },
        )
        .unwrap();
        // A Pay Find due yesterday and a snoozed Find whose snooze ended.
        let Applied { find: pay, .. } = svc
            .apply_proposal(&proposal(
                &dig,
                "pay",
                FindType::Pay,
                Some(now - chrono::Duration::days(1)),
            ))
            .unwrap();
        let Applied { find: snoozed, .. } = svc
            .apply_proposal(&proposal(&dig, "snz", FindType::Respond, None))
            .unwrap();
        svc.snooze(&snoozed.id, now - chrono::Duration::minutes(1))
            .unwrap();
        let outcomes = svc.tick(&TemporalOptions::default()).unwrap();
        let kinds: Vec<_> = outcomes.iter().map(|o| o.kind).collect();
        assert!(
            kinds.contains(&TemporalOutcomeKind::FollowUpCreated),
            "{kinds:?}"
        );
        assert!(kinds.contains(&TemporalOutcomeKind::Expired));
        assert!(kinds.contains(&TemporalOutcomeKind::Unsnoozed));
        assert_eq!(svc.get(&pay.id).unwrap().status, FindStatus::Expired);
        assert_eq!(svc.get(&snoozed.id).unwrap().status, FindStatus::Open);
        let follow = outcomes
            .iter()
            .find(|o| o.kind == TemporalOutcomeKind::FollowUpCreated)
            .unwrap();
        assert!(follow.summary.contains("send the contract"));
        // A second tick does not create a second Follow Up for the same stall.
        let again = svc.tick(&TemporalOptions::default()).unwrap();
        assert!(!again
            .iter()
            .any(|o| o.kind == TemporalOutcomeKind::FollowUpCreated));
    }

    #[test]
    fn a_respond_find_resolves_when_thread_state_shows_my_later_reply() {
        let (svc, clock, dig) = setup();
        let Applied { find: f, .. } = svc
            .apply_proposal(&proposal(&dig, "r1", FindType::Respond, None))
            .unwrap();
        let store = svc.stores.dig(&dig).unwrap();
        let thread = f.sources[0].thread_id.clone();
        clock.advance(chrono::Duration::hours(2));
        repo::thread_state::put(
            &store,
            &ThreadState {
                thread_id: thread,
                dig_id: dig.clone(),
                last_speaker: Speaker::Me,
                open_asks_from_them: vec![],
                open_asks_from_me: vec![],
                my_commitments: vec![],
                last_reply_by_me_at: Some(clock.now()),
                last_message_at: Some(clock.now()),
                message_count: 2,
                updated_at: clock.now(),
            },
        )
        .unwrap();
        let outcomes = svc.tick(&TemporalOptions::default()).unwrap();
        assert!(outcomes
            .iter()
            .any(|o| o.kind == TemporalOutcomeKind::RespondResolved));
        let f = svc.get(&f.id).unwrap();
        assert_eq!(f.status, FindStatus::Resolved);
        assert_eq!(f.resolved_by, Some(ResolvedBy::ThreadState));
    }

    #[test]
    fn a_find_of_any_type_expires_at_its_expires_time_even_while_snoozed() {
        let (svc, clock, dig) = setup();
        let now = clock.now();
        let expiring = |pid: &str| FindProposal {
            expires_at: Some(now + chrono::Duration::hours(18)),
            ..proposal(&dig, pid, FindType::Do, None)
        };
        let Applied { find: open, .. } = svc.apply_proposal(&expiring("open")).unwrap();
        let Applied { find: snoozed, .. } = svc.apply_proposal(&expiring("snz")).unwrap();
        svc.snooze(&snoozed.id, now + chrono::Duration::days(3))
            .unwrap();
        let Applied { find: lasting, .. } = svc
            .apply_proposal(&proposal(&dig, "keep", FindType::Do, None))
            .unwrap();

        assert!(svc.tick(&TemporalOptions::default()).unwrap().is_empty());
        clock.advance(chrono::Duration::hours(18));
        let outcomes = svc.tick(&TemporalOptions::default()).unwrap();
        assert_eq!(outcomes.len(), 2, "{outcomes:?}");
        assert!(outcomes
            .iter()
            .all(|o| o.kind == TemporalOutcomeKind::Expired));
        for id in [&open.id, &snoozed.id] {
            let f = svc.get(id).unwrap();
            assert_eq!(f.status, FindStatus::Expired);
            assert_eq!(f.resolved_by, Some(ResolvedBy::Temporal));
        }
        assert_eq!(svc.get(&lasting.id).unwrap().status, FindStatus::Open);
    }

    #[test]
    fn mail_decided_after_its_expiry_makes_a_find_that_is_born_expired() {
        let (svc, clock, dig) = setup();
        let stale = FindProposal {
            expires_at: Some(clock.now() - chrono::Duration::hours(2)),
            ..proposal(&dig, "old", FindType::Do, None)
        };
        let Applied { find, .. } = svc.apply_proposal(&stale).unwrap();
        assert_eq!(find.status, FindStatus::Expired);
        assert!(svc.tick(&TemporalOptions::default()).unwrap().is_empty());
    }

    #[test]
    fn a_later_message_in_the_thread_extends_the_expiry_but_never_adds_one() {
        let (svc, clock, dig) = setup();
        let now = clock.now();
        let in_thread = |pid: &str, hours: Option<i64>| FindProposal {
            identity_key: "thread".into(),
            expires_at: hours.map(|h| now + chrono::Duration::hours(h)),
            ..proposal(&dig, pid, FindType::Do, None)
        };
        let Applied { find, .. } = svc.apply_proposal(&in_thread("a", Some(4))).unwrap();
        svc.apply_proposal(&in_thread("b", Some(10))).unwrap();
        assert_eq!(
            svc.get(&find.id).unwrap().expires_at,
            Some(now + chrono::Duration::hours(10))
        );

        let Applied { find: plain, .. } = svc
            .apply_proposal(&FindProposal {
                identity_key: "other".into(),
                ..proposal(&dig, "c", FindType::Do, None)
            })
            .unwrap();
        svc.apply_proposal(&FindProposal {
            identity_key: "other".into(),
            expires_at: Some(now + chrono::Duration::hours(1)),
            ..proposal(&dig, "d", FindType::Do, None)
        })
        .unwrap();
        assert_eq!(svc.get(&plain.id).unwrap().expires_at, None);
    }

    #[test]
    fn a_find_the_user_kept_open_is_not_expired_again() {
        let (svc, clock, dig) = setup();
        let now = clock.now();
        let Applied { find: reminder, .. } = svc
            .apply_proposal(&FindProposal {
                expires_at: Some(now + chrono::Duration::hours(1)),
                ..proposal(&dig, "rem", FindType::Do, None)
            })
            .unwrap();
        let Applied { find: bill, .. } = svc
            .apply_proposal(&proposal(
                &dig,
                "bill",
                FindType::Pay,
                Some(now + chrono::Duration::hours(1)),
            ))
            .unwrap();
        clock.advance(chrono::Duration::hours(2));
        assert_eq!(svc.tick(&TemporalOptions::default()).unwrap().len(), 2);

        // The user undoes both expiries, then time passes again.
        svc.keep_open(&reminder.id).unwrap();
        svc.keep_open(&bill.id).unwrap();
        svc.snooze(&reminder.id, clock.now() + chrono::Duration::hours(3))
            .unwrap();
        clock.advance(chrono::Duration::hours(1));
        assert!(svc.tick(&TemporalOptions::default()).unwrap().is_empty());
        assert_eq!(
            svc.get(&reminder.id).unwrap().status,
            FindStatus::Snoozed,
            "still snoozed: its snooze has not ended"
        );
        assert_eq!(svc.get(&bill.id).unwrap().status, FindStatus::Open);
    }
}
