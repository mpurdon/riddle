//! The Find lifecycle and cross-Dig joining.

use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, Category};
use riddle_store::{repo, Stores};
use std::sync::Arc;

/// What `apply_proposal` did.
#[derive(Debug, Clone)]
pub struct Applied {
    pub find: Find,
    /// The proposal joined an open Find rather than creating one.
    pub joined: bool,
    /// Finds settling closed, each carrying why in its status: older members
    /// of its `+ Replaces` series (Superseded), or the new Find itself when a
    /// newer one is already open (Superseded) or its expiry has passed
    /// (Expired).
    pub closed: Vec<Find>,
}

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
    /// one and, when the proposal belongs to a `+ Replaces` series, settle
    /// the series.
    pub fn apply_proposal(&self, p: &FindProposal) -> Result<Applied> {
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
            // A newer code replaces the one before it: only the latest works.
            if p.code.is_some() {
                existing.code = p.code.clone();
            }
            // A later message in the thread keeps an expiring Find alive for
            // its own span; a Find raised without an expiry never gains one.
            if let (Some(have), Some(new)) = (existing.expires_at, p.expires_at) {
                existing.expires_at = Some(have.max(new));
            }
            // Likewise its place in a series is its newest message's.
            if let (Some(have), Some(new)) = (existing.series.as_mut(), &p.series) {
                have.message_at = have.message_at.max(new.message_at);
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
            return Ok(Applied {
                find: existing,
                joined: true,
                closed: vec![],
            });
        }
        let find = Find {
            code: p.code.clone(),
            id: FindId::new(),
            find_type: p.find_type,
            title: if p.title.trim().is_empty() {
                "(no subject)".into()
            } else {
                p.title.clone()
            },
            summary: p.summary.clone(),
            priority: p.priority,
            status: if p.snoozed_until.is_some_and(|u| u > now) {
                FindStatus::Snoozed
            } else {
                FindStatus::Open
            },
            due_at: p.due_at,
            snoozed_until: p.snoozed_until.filter(|u| *u > now),
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
            series: p.series.clone(),
            expires_at: p.expires_at,
            kept_open_at: None,
            notify: p.notify,
        };
        repo::finds::upsert(&index, &find)?;
        rinfo!(
            Category::Sift,
            "created find {} ({:?}) from message {}",
            find.id,
            find.find_type,
            p.source.message_id
        );
        let closed = self.settle(&find)?;
        let find = closed
            .iter()
            .find(|f| f.id == find.id)
            .cloned()
            .unwrap_or(find);
        // After settling, so listeners see the Find as it ends up.
        self.events
            .emit(RiddleEvent::FindCreated { find: find.clone() });
        Ok(Applied {
            find,
            joined: false,
            closed,
        })
    }

    /// Close what a new Find makes moot, through the ordinary lifecycle so
    /// Activity and undo treat it like any other closing. Its series keeps
    /// the Find from the newest message open and supersedes the rest; the
    /// newest message wins whatever order Sift saw them in, so a Find made
    /// for an old message (history being re-decided) is the one that
    /// closes. A Find whose expiry has already passed expires at once.
    fn settle(&self, new: &Find) -> Result<Vec<Find>> {
        let mut closed = Vec::new();
        let mut new_open = true;
        if let Some(series) = &new.series {
            // Kept Finds are not members: `open_in_series` leaves them out.
            let members = repo::finds::open_in_series(&self.stores.index(), &series.key)?;
            if let Some(keeper) = members
                .iter()
                .max_by_key(|f| (f.series.as_ref().map(|s| s.message_at), f.created_at))
                .map(|f| f.id.clone())
            {
                new_open = keeper == new.id;
                for f in members.into_iter().filter(|f| f.id != keeper) {
                    rinfo!(Category::Sift, "find {} replaced by find {}", f.id, keeper);
                    let by = keeper.clone();
                    closed.push(self.apply_transition(f, FindStatus::Superseded, None, |f| {
                        f.superseded_by = Some(by)
                    })?);
                }
            }
        }
        if new_open && new.expires_at.is_some_and(|at| at <= self.clock.now()) {
            closed.push(self.apply_transition(
                new.clone(),
                FindStatus::Expired,
                Some(ResolvedBy::Temporal),
                |_| {},
            )?);
        }
        Ok(closed)
    }

    fn allowed(from: FindStatus, to: FindStatus) -> bool {
        use FindStatus::*;
        matches!(
            (from, to),
            (Open, Snoozed | Resolved | Dismissed | Expired | Superseded)
                | (Snoozed, Open | Resolved | Dismissed | Expired | Superseded)
                | (Resolved | Dismissed | Expired | Superseded, Open)
        )
    }

    /// Validated lifecycle transition. Reopening a resolved, dismissed,
    /// expired or superseded Find is allowed because undo needs it: undoing
    /// a re-decision puts the Find it closed back.
    pub fn transition(&self, id: &FindId, to: FindStatus, by: Option<ResolvedBy>) -> Result<Find> {
        self.apply_transition(self.get(id)?, to, by, |_| {})
    }

    /// `transition` on a Find already in hand; `edit` sets what belongs
    /// with the new status (who superseded it) in the same write.
    fn apply_transition(
        &self,
        mut f: Find,
        to: FindStatus,
        by: Option<ResolvedBy>,
        edit: impl FnOnce(&mut Find),
    ) -> Result<Find> {
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
        edit(&mut f);
        repo::finds::upsert(&self.stores.index(), &f)?;
        self.events.emit(RiddleEvent::FindChanged {
            find_id: f.id.clone(),
        });
        Ok(f)
    }

    pub fn resolve(&self, id: &FindId, by: ResolvedBy) -> Result<Find> {
        self.transition(id, FindStatus::Resolved, Some(by))
    }

    /// Close a Find because its message was re-decided as something else.
    pub fn close_superseded(&self, id: &FindId) -> Result<Find> {
        self.transition(id, FindStatus::Superseded, None)
    }

    pub fn reopen(&self, id: &FindId) -> Result<Find> {
        self.transition(id, FindStatus::Open, None)
    }

    /// Reopen a Find a rule closed (a newer one of its series, an expiry)
    /// because the user undid that, and keep it: no rule closes it again.
    pub fn keep_open(&self, id: &FindId) -> Result<Find> {
        let now = self.clock.now();
        self.apply_transition(self.get(id)?, FindStatus::Open, None, |f| {
            f.kept_open_at = Some(now)
        })
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
            let (mesh_id, mesh_version) = self.deciding_mesh(&src.dig_id, &src.message_id);
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
                mesh_id,
                mesh_version,
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

    /// The Mesh version whose decision raised this message, if a Mesh did.
    /// Corrections carry it so repeated ones against the same Mesh add up to
    /// a Director suggestion (§28); without it a dismissal counted toward
    /// nothing. A lookup failure only loses the link, never the dismissal.
    pub fn deciding_mesh(&self, dig: &DigId, message: &MessageId) -> (Option<MeshId>, Option<u32>) {
        self.stores
            .dig(dig)
            .and_then(|s| repo::classifications::current(&s, message))
            .ok()
            .flatten()
            .map(|k| (k.mesh_id, k.mesh_version))
            .unwrap_or((None, None))
    }

    pub fn snooze(&self, id: &FindId, until: chrono::DateTime<chrono::Utc>) -> Result<Find> {
        let mut f = self.transition(id, FindStatus::Snoozed, None)?;
        f.snoozed_until = Some(until);
        f.updated_at = self.clock.now();
        repo::finds::upsert(&self.stores.index(), &f)?;
        Ok(f)
    }

    /// Set or move the due date. `apply_proposal` keeps the date an open
    /// Find already has, so a Sift proposal never overwrites one the user
    /// chose; a user's own "follow up by…" is the opposite case, and the
    /// newer date wins.
    pub fn set_due(
        &self,
        id: &FindId,
        due_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Find> {
        let mut f = self.get(id)?;
        if f.due_at == due_at {
            return Ok(f);
        }
        f.due_at = due_at;
        f.updated_at = self.clock.now();
        repo::finds::upsert(&self.stores.index(), &f)?;
        self.events.emit(RiddleEvent::FindChanged {
            find_id: f.id.clone(),
        });
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
            code: None,
            find_type: FindType::Respond,
            title: "Re: plan".into(),
            summary: "s".into(),
            priority,
            reason: "r".into(),
            identity_key: key.into(),
            due_at: None,
            snoozed_until: None,
            source: FindSource {
                message_id: MessageId::for_provider(&dig, pid),
                thread_id: ThreadId::for_provider(&dig, "t"),
                dig_id: dig.clone(),
            },
            confidence: Some(0.9),
            classification_id: None,
            trace_id: None,
            series: None,
            expires_at: None,
            notify: false,
        }
    }

    #[test]
    fn the_same_thread_arriving_in_two_digs_is_one_find() {
        let svc = service();
        let Applied {
            find: a, joined, ..
        } = svc
            .apply_proposal(&proposal("work", "m1", "<root@example.com>", 50))
            .unwrap();
        assert!(!joined);
        let Applied {
            find: b, joined, ..
        } = svc
            .apply_proposal(&proposal("personal", "m9", "<root@example.com>", 70))
            .unwrap();
        assert!(joined);
        assert_eq!(a.id, b.id);
        assert_eq!(b.sources.len(), 2);
        assert_eq!(b.dig_ids.len(), 2);
        assert_eq!(b.priority, 70);
        assert_eq!(
            repo::finds::count_open(&svc.stores.index(), None).unwrap(),
            1
        );
    }

    #[test]
    fn a_dismissal_names_the_mesh_that_raised_the_find_so_repeats_can_add_up() {
        let svc = service();
        let p = proposal("work", "m1", "k", 50);
        let dig = svc.stores.dig(&p.source.dig_id).unwrap();
        repo::classifications::record(
            &dig,
            &Classification {
                id: ClassificationId::new(),
                message_id: p.source.message_id.clone(),
                dig_id: p.source.dig_id.clone(),
                disposition: Disposition::Find {
                    find_type: FindType::Respond,
                },
                importance: Importance::Normal,
                strata: vec![],
                notify: false,
                mechanism: DecisionMechanism::Mesh,
                mesh_id: Some(MeshId("mesh_github".into())),
                mesh_version: Some(7),
                matched_rule: None,
                confidence: None,
                reason: String::new(),
                trace_id: TraceId::new(),
                shadow: false,
                created_at: chrono::Utc::now(),
                superseded_at: None,
                series: None,
            },
        )
        .unwrap();
        let Applied { find: f, .. } = svc.apply_proposal(&p).unwrap();
        svc.dismiss(&f.id, None).unwrap();
        let k = &repo::corrections::for_message(&dig, &p.source.message_id).unwrap()[0];
        assert_eq!(k.mesh_id.as_ref().map(|m| m.as_str()), Some("mesh_github"));
        assert_eq!(k.mesh_version, Some(7));
    }

    #[test]
    fn lifecycle_transitions_are_validated_and_dismiss_records_a_correction() {
        let svc = service();
        let Applied { find: f, .. } = svc
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
        let Applied {
            find: again,
            joined,
            ..
        } = svc
            .apply_proposal(&proposal("work", "m2", "k", 50))
            .unwrap();
        assert!(!joined);
        assert_ne!(again.id, f.id);
    }

    #[test]
    fn supersede_moves_sources_and_marks_the_old_find() {
        let svc = service();
        let Applied { find: a, .. } = svc
            .apply_proposal(&proposal("work", "m1", "ka", 40))
            .unwrap();
        let Applied { find: b, .. } = svc
            .apply_proposal(&proposal("work", "m2", "kb", 60))
            .unwrap();
        let merged = svc.supersede(&a.id, &b.id).unwrap();
        assert_eq!(merged.sources.len(), 2);
        let old = svc.get(&a.id).unwrap();
        assert_eq!(old.status, FindStatus::Superseded);
        assert_eq!(old.superseded_by, Some(b.id));
    }

    /// A daily reminder: its own thread each day, one Mesh, one sender.
    fn reminder(pid: &str, sender: &str, days_ago: i64) -> FindProposal {
        let at = chrono::Utc::now() - chrono::Duration::days(days_ago);
        FindProposal {
            series: Some(FindSeries::new(&MeshId::from("mesh_daily"), sender, at)),
            ..proposal("personal", pid, &format!("<{pid}@example.com>"), 50)
        }
    }

    fn open(svc: &FindService) -> Vec<Find> {
        repo::finds::list_by_status(&svc.stores.index(), FindStatus::Open, None).unwrap()
    }

    #[test]
    fn a_newer_find_in_a_series_supersedes_the_open_one() {
        let svc = service();
        let first = svc
            .apply_proposal(&reminder("d1", "coach@example.com", 1))
            .unwrap();
        assert!(first.closed.is_empty());
        let second = svc
            .apply_proposal(&reminder("d2", "Coach@Example.com ", 0))
            .unwrap();
        assert_eq!(second.closed.len(), 1);
        let old = svc.get(&first.find.id).unwrap();
        assert_eq!(old.status, FindStatus::Superseded);
        assert_eq!(old.superseded_by, Some(second.find.id.clone()));
        assert_eq!(open(&svc).len(), 1);
    }

    #[test]
    fn an_older_message_decided_late_closes_itself_not_the_newer_find() {
        let svc = service();
        let today = svc
            .apply_proposal(&reminder("d2", "coach@example.com", 0))
            .unwrap();
        let history = svc
            .apply_proposal(&reminder("d1", "coach@example.com", 1))
            .unwrap();
        assert_eq!(history.find.status, FindStatus::Superseded);
        assert_eq!(history.closed.len(), 1);
        assert_eq!(history.closed[0].id, history.find.id);
        assert_eq!(svc.get(&today.find.id).unwrap().status, FindStatus::Open);
    }

    #[test]
    fn a_series_is_per_sender_and_a_snoozed_member_is_replaced_too() {
        let svc = service();
        let coach = svc
            .apply_proposal(&reminder("d1", "coach@example.com", 2))
            .unwrap();
        svc.snooze(
            &coach.find.id,
            chrono::Utc::now() + chrono::Duration::hours(3),
        )
        .unwrap();
        let other = svc
            .apply_proposal(&reminder("o1", "editor@example.com", 1))
            .unwrap();
        assert!(other.closed.is_empty(), "another sender is another series");
        let next = svc
            .apply_proposal(&reminder("d2", "coach@example.com", 0))
            .unwrap();
        assert_eq!(next.closed.len(), 1);
        assert_eq!(
            svc.get(&coach.find.id).unwrap().status,
            FindStatus::Superseded
        );
        assert_eq!(svc.get(&other.find.id).unwrap().status, FindStatus::Open);
    }

    #[test]
    fn a_find_without_a_series_is_never_replaced() {
        let svc = service();
        svc.apply_proposal(&proposal("work", "m1", "ka", 50))
            .unwrap();
        let b = svc
            .apply_proposal(&proposal("work", "m2", "kb", 50))
            .unwrap();
        assert!(b.closed.is_empty());
        assert_eq!(open(&svc).len(), 2);
    }

    #[test]
    fn a_newer_find_in_a_series_leaves_one_the_user_kept_open() {
        let svc = service();
        let first = svc
            .apply_proposal(&reminder("d1", "coach@example.com", 2))
            .unwrap();
        let second = svc
            .apply_proposal(&reminder("d2", "coach@example.com", 1))
            .unwrap();
        assert_eq!(second.closed[0].id, first.find.id);
        let kept = svc.keep_open(&first.find.id).unwrap();
        assert!(kept.kept_open_at.is_some());
        assert_eq!(kept.superseded_by, None);

        let third = svc
            .apply_proposal(&reminder("d3", "coach@example.com", 0))
            .unwrap();
        assert_eq!(third.closed.len(), 1, "only the unkept older one closes");
        assert_eq!(third.closed[0].id, second.find.id);
        assert_eq!(svc.get(&first.find.id).unwrap().status, FindStatus::Open);
    }

    /// A booking's Find is raised asleep until its first day.
    #[test]
    fn a_proposal_with_a_future_wake_time_is_raised_snoozed() {
        let svc = service();
        let mut p = proposal("d", "b1", "booking:telpark.com:2026-10-07", 60);
        let wake = svc.clock.now() + chrono::Duration::days(3);
        p.snoozed_until = Some(wake);
        let f = svc.apply_proposal(&p).unwrap().find;
        assert_eq!(
            (f.status, f.snoozed_until),
            (FindStatus::Snoozed, Some(wake))
        );

        let mut late = proposal("d", "b2", "k-late", 60);
        late.snoozed_until = Some(svc.clock.now() - chrono::Duration::hours(1));
        let f = svc.apply_proposal(&late).unwrap().find;
        assert_eq!((f.status, f.snoozed_until), (FindStatus::Open, None));
    }
}
