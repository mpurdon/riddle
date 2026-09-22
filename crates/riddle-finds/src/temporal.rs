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

impl FindService {
    /// One Temporal Sift pass. Each step is independent; a failure in one is
    /// logged and the others still run.
    pub fn tick(&self, opts: &TemporalOptions) -> Result<Vec<TemporalOutcome>> {
        let mut out = Vec::new();
        let now = self.clock.now();
        let index = self.stores.index();

        // 1. Snoozes that ended and dated Finds past their date.
        for f in repo::finds::due_for_temporal(&index, &now)? {
            let result = match f.status {
                FindStatus::Snoozed => {
                    self.transition(&f.id, FindStatus::Open, None)
                        .map(|f| TemporalOutcome {
                            kind: TemporalOutcomeKind::Unsnoozed,
                            find_id: f.id.clone(),
                            dig_id: f.dig_ids.first().cloned(),
                            thread_id: f.sources.first().map(|s| s.thread_id.clone()),
                            summary: format!("Snooze ended: {}", f.title),
                        })
                }
                FindStatus::Open if f.find_type.is_dated() => self
                    .transition(&f.id, FindStatus::Expired, Some(ResolvedBy::Temporal))
                    .map(|f| TemporalOutcome {
                        kind: TemporalOutcomeKind::Expired,
                        find_id: f.id.clone(),
                        dig_id: f.dig_ids.first().cloned(),
                        thread_id: f.sources.first().map(|s| s.thread_id.clone()),
                        summary: format!("{} expired: {}", f.find_type.label(), f.title),
                    }),
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
        let digs = repo::digs::list(&index)?;
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
        let (pay, _) = svc
            .apply_proposal(&proposal(
                &dig,
                "pay",
                FindType::Pay,
                Some(now - chrono::Duration::days(1)),
            ))
            .unwrap();
        let (snoozed, _) = svc
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
        let (f, _) = svc
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
}
