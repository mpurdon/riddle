//! The Find lifecycle and cross-Dig joining.

use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, Category};
use riddle_store::{repo, Stores};
use std::sync::Arc;

pub struct FindService {
    pub stores: Arc<Stores>,
    pub clock: Arc<dyn Clock>,
    pub events: Arc<dyn EventSink>,
}

impl FindService {
    pub fn new(stores: Arc<Stores>, clock: Arc<dyn Clock>, events: Arc<dyn EventSink>) -> Self {
        Self {
            stores,
            clock,
            events,
        }
    }

    pub fn get(&self, id: &FindId) -> Result<Find> {
        repo::finds::get(&self.stores.index(), id)?
            .ok_or_else(|| RiddleError::not_found(format!("find {id}")))
    }

    /// Join an OPEN or SNOOZED Find with the same identity key, else create
    /// one. Returns the Find and whether it was joined.
    pub fn apply_proposal(&self, p: &FindProposal) -> Result<(Find, bool)> {
        let index = self.stores.index();
        let now = self.clock.now();
        if let Some(mut existing) = repo::finds::by_identity(&index, &p.identity_key)? {
            if !existing
                .sources
                .iter()
                .any(|s| s.message_id == p.source.message_id)
            {
                existing.sources.push(p.source.clone());
            }
            if !existing.dig_ids.contains(&p.source.dig_id) {
                existing.dig_ids.push(p.source.dig_id.clone());
            }
            if p.priority > existing.priority {
                existing.priority = p.priority;
                existing.reason = p.reason.clone();
            }
            if existing.due_at.is_none() {
                existing.due_at = p.due_at;
            }
            existing.updated_at = now;
            repo::finds::upsert(&index, &existing)?;
            self.events.emit(RiddleEvent::FindChanged {
                find_id: existing.id.clone(),
            });
            rinfo!(
                Category::Sift,
                "message {} joined find {}",
                p.source.message_id,
                existing.id
            );
            return Ok((existing, true));
        }
        let find = Find {
            id: FindId::new(),
            find_type: p.find_type,
            title: if p.title.trim().is_empty() {
                "(no subject)".into()
            } else {
                p.title.clone()
            },
            summary: p.summary.clone(),
            priority: p.priority,
            status: FindStatus::Open,
            due_at: p.due_at,
            snoozed_until: None,
            created_at: now,
            updated_at: now,
            resolved_at: None,
            resolved_by: None,
            sources: vec![p.source.clone()],
            dig_ids: vec![p.source.dig_id.clone()],
            identity_key: p.identity_key.clone(),
            reason: p.reason.clone(),
            confidence: p.confidence,
            classification_id: p.classification_id.clone(),
            trace_id: p.trace_id.clone(),
            superseded_by: None,
        };
        repo::finds::upsert(&index, &find)?;
        self.events
            .emit(RiddleEvent::FindCreated { find: find.clone() });
        rinfo!(
            Category::Sift,
            "created find {} ({:?}) from message {}",
            find.id,
            find.find_type,
            p.source.message_id
        );
        Ok((find, false))
    }

    fn allowed(from: FindStatus, to: FindStatus) -> bool {
        use FindStatus::*;
        matches!(
            (from, to),
            (Open, Snoozed | Resolved | Dismissed | Expired | Superseded)
                | (Snoozed, Open | Resolved | Dismissed | Expired | Superseded)
                | (Resolved | Dismissed | Expired, Open)
        )
    }

    /// Validated lifecycle transition. Reopening a resolved, dismissed or
    /// expired Find is allowed because undo needs it.
    pub fn transition(&self, id: &FindId, to: FindStatus, by: Option<ResolvedBy>) -> Result<Find> {
        let mut f = self.get(id)?;
        if f.status == to {
            return Ok(f);
        }
        if !Self::allowed(f.status, to) {
            return Err(RiddleError::invalid(format!(
                "a {} Find cannot become {}",
                f.status.as_str(),
                to.as_str()
            )));
        }
        let now = self.clock.now();
        f.status = to;
        f.updated_at = now;
        match to {
            FindStatus::Resolved | FindStatus::Dismissed | FindStatus::Expired => {
                f.resolved_at = Some(now);
                f.resolved_by = by;
            }
            FindStatus::Open => {
                f.resolved_at = None;
                f.resolved_by = None;
                f.snoozed_until = None;
                f.superseded_by = None;
            }
            _ => {}
        }
        repo::finds::upsert(&self.stores.index(), &f)?;
        self.events.emit(RiddleEvent::FindChanged {
            find_id: f.id.clone(),
        });
        Ok(f)
    }

    pub fn resolve(&self, id: &FindId, by: ResolvedBy) -> Result<Find> {
        self.transition(id, FindStatus::Resolved, Some(by))
    }

    pub fn reopen(&self, id: &FindId) -> Result<Find> {
        self.transition(id, FindStatus::Open, None)
    }

    /// Dismiss and record the correction (§46). Returns the Find and the
    /// correction id, so undo can retract it.
    pub fn dismiss(
        &self,
        id: &FindId,
        activity_id: Option<&ActivityId>,
    ) -> Result<(Find, Option<CorrectionId>)> {
        let f = self.transition(id, FindStatus::Dismissed, Some(ResolvedBy::User))?;
        let mut correction_id = None;
        if let Some(src) = f.sources.first() {
            let correction = Correction {
                id: CorrectionId::new(),
                kind: CorrectionKind::Dismiss,
                dig_id: src.dig_id.clone(),
                message_id: src.message_id.clone(),
                from_disposition: Some(Disposition::Find {
                    find_type: f.find_type,
                }),
                to_disposition: None,
                from_find_type: Some(f.find_type),
                to_find_type: None,
                mesh_id: None,
                mesh_version: None,
                activity_id: activity_id.cloned(),
                created_at: self.clock.now(),
                retracted_at: None,
            };
            match self
                .stores
                .dig(&src.dig_id)
                .and_then(|s| repo::corrections::record(&s, &correction))
            {
                Ok(()) => correction_id = Some(correction.id),
                // Bookkeeping never fails the work.
                Err(e) => riddle_observability::rwarn!(
                    Category::Sift,
                    "could not record dismiss correction for find {}: {}",
                    f.id,
                    e.message
                ),
            }
        }
        Ok((f, correction_id))
    }

    pub fn snooze(&self, id: &FindId, until: chrono::DateTime<chrono::Utc>) -> Result<Find> {
        let mut f = self.transition(id, FindStatus::Snoozed, None)?;
        f.snoozed_until = Some(until);
        f.updated_at = self.clock.now();
        repo::finds::upsert(&self.stores.index(), &f)?;
        Ok(f)
    }

    pub fn retype(&self, id: &FindId, find_type: FindType) -> Result<Find> {
        let mut f = self.get(id)?;
        f.find_type = find_type;
        f.updated_at = self.clock.now();
        repo::finds::upsert(&self.stores.index(), &f)?;
        self.events.emit(RiddleEvent::FindChanged {
            find_id: f.id.clone(),
        });
        Ok(f)
    }

    /// Merge `a` into `b`: sources move, `a` is superseded.
    pub fn supersede(&self, a: &FindId, into: &FindId) -> Result<Find> {
        if a == into {
            return Err(RiddleError::invalid("a Find cannot supersede itself"));
        }
        let mut from = self.get(a)?;
        let mut target = self.get(into)?;
        for s in &from.sources {
            if !target.sources.iter().any(|t| t.message_id == s.message_id) {
                target.sources.push(s.clone());
            }
        }
        for d in &from.dig_ids {
            if !target.dig_ids.contains(d) {
                target.dig_ids.push(d.clone());
            }
        }
        target.priority = target.priority.max(from.priority);
        target.updated_at = self.clock.now();
        from.status = FindStatus::Superseded;
        from.superseded_by = Some(target.id.clone());
        from.updated_at = target.updated_at;
        let index = self.stores.index();
        repo::finds::upsert(&index, &target)?;
        repo::finds::upsert(&index, &from)?;
        self.events.emit(RiddleEvent::FindChanged {
            find_id: target.id.clone(),
        });
        self.events.emit(RiddleEvent::FindChanged {
            find_id: from.id.clone(),
        });
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> FindService {
        FindService::new(
            Arc::new(Stores::in_memory().unwrap()),
            Arc::new(SystemClock),
            Arc::new(RecordingSink::default()),
        )
    }

    pub fn proposal(dig: &str, pid: &str, key: &str, priority: u8) -> FindProposal {
        let dig = DigId(dig.into());
        FindProposal {
            find_type: FindType::Respond,
            title: "Re: plan".into(),
            summary: "s".into(),
            priority,
            reason: "r".into(),
            identity_key: key.into(),
            due_at: None,
            source: FindSource {
                message_id: MessageId::for_provider(&dig, pid),
                thread_id: ThreadId::for_provider(&dig, "t"),
                dig_id: dig.clone(),
            },
            confidence: Some(0.9),
            classification_id: None,
            trace_id: None,
        }
    }

    #[test]
    fn the_same_thread_arriving_in_two_digs_is_one_find() {
        let svc = service();
        let (a, joined) = svc
            .apply_proposal(&proposal("work", "m1", "<root@example.com>", 50))
            .unwrap();
        assert!(!joined);
        let (b, joined) = svc
            .apply_proposal(&proposal("personal", "m9", "<root@example.com>", 70))
            .unwrap();
        assert!(joined);
        assert_eq!(a.id, b.id);
        assert_eq!(b.sources.len(), 2);
        assert_eq!(b.dig_ids.len(), 2);
        assert_eq!(b.priority, 70);
        assert_eq!(repo::finds::count_open(&svc.stores.index()).unwrap(), 1);
    }

    #[test]
    fn lifecycle_transitions_are_validated_and_dismiss_records_a_correction() {
        let svc = service();
        let (f, _) = svc
            .apply_proposal(&proposal("work", "m1", "k", 50))
            .unwrap();
        let snoozed = svc
            .snooze(&f.id, chrono::Utc::now() + chrono::Duration::hours(1))
            .unwrap();
        assert_eq!(snoozed.status, FindStatus::Snoozed);
        assert!(snoozed.snoozed_until.is_some());
        let reopened = svc.reopen(&f.id).unwrap();
        assert_eq!(reopened.status, FindStatus::Open);
        let (dismissed, correction) = svc.dismiss(&f.id, None).unwrap();
        assert_eq!(dismissed.status, FindStatus::Dismissed);
        assert!(correction.is_some());
        let err = svc.snooze(&f.id, chrono::Utc::now()).unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
        let dig = svc.stores.dig(&DigId("work".into())).unwrap();
        let ks = repo::corrections::for_message(&dig, &dismissed.sources[0].message_id).unwrap();
        assert_eq!(ks[0].kind, CorrectionKind::Dismiss);
        // A dismissed Find does not absorb a new proposal with the same key.
        let (again, joined) = svc
            .apply_proposal(&proposal("work", "m2", "k", 50))
            .unwrap();
        assert!(!joined);
        assert_ne!(again.id, f.id);
    }

    #[test]
    fn supersede_moves_sources_and_marks_the_old_find() {
        let svc = service();
        let (a, _) = svc
            .apply_proposal(&proposal("work", "m1", "ka", 40))
            .unwrap();
        let (b, _) = svc
            .apply_proposal(&proposal("work", "m2", "kb", 60))
            .unwrap();
        let merged = svc.supersede(&a.id, &b.id).unwrap();
        assert_eq!(merged.sources.len(), 2);
        let old = svc.get(&a.id).unwrap();
        assert_eq!(old.status, FindStatus::Superseded);
        assert_eq!(old.superseded_by, Some(b.id));
    }
}
