//! `JobQueue`: the durable, priority-ordered set of jobs (§54).
//!
//! Every job row lives in the index store (`repo::jobs`) so it survives a
//! restart; an in-memory mirror of the non-terminal rows keeps scheduling
//! decisions off SQLite. Global pauses (sleep, offline, budget, fault) are
//! runtime state, not persisted status: the store keeps the job `queued`,
//! and only the view the Task Centre sees is overlaid with `Paused`. That
//! way a crash while asleep never leaves jobs paused forever. A pause by the
//! user is different: it is the user's intent, so it is persisted.
//!
//! Enqueue dedupes on (kind, Dig, payload hash) among non-terminal jobs: an
//! agent that retries "sync this Dig" gets the job that is already there
//! (§52). Cancellation is cooperative through `CancelToken`; the runner
//! records the final status when the handler returns.

use crate::runner::CancelToken;
use crate::spec::{payload_hash, JobEnvelope, JobSpec, PauseReason, USER_PAUSE};
use parking_lot::Mutex;
use riddle_core::{Clock, EventSink, Job, JobId, JobStatus, Priority, Progress, RiddleEvent};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rdebug, rerror, rinfo, rwarn, Category};
use riddle_store::{repo, Store};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Default)]
struct Inner {
    jobs: HashMap<JobId, Job>,
    /// Tokens of jobs currently running.
    tokens: HashMap<JobId, CancelToken>,
    /// Retry back-off: not before this instant.
    not_before: HashMap<JobId, tokio::time::Instant>,
    /// Jobs whose last outcome was a cooperative yield (no attempt bump).
    yielded: std::collections::HashSet<JobId>,
    /// Held by the scheduler with a reason (budget) until conditions change.
    held: HashMap<JobId, &'static str>,
    paused: BTreeSet<PauseReason>,
}

pub struct JobQueue {
    store: Arc<Store>,
    clock: Arc<dyn Clock>,
    events: Arc<dyn EventSink>,
    inner: Mutex<Inner>,
    wake: Notify,
}

impl JobQueue {
    /// Load the durable queue: running jobs from a previous process are
    /// requeued to resume from their checkpoints; system pauses are dropped.
    pub fn open(
        store: Arc<Store>,
        clock: Arc<dyn Clock>,
        events: Arc<dyn EventSink>,
    ) -> Result<Arc<Self>> {
        let orphans = repo::jobs::requeue_orphans(&store)?;
        if orphans > 0 {
            rinfo!(Category::Jobs, "requeued {orphans} orphaned jobs");
        }
        let mut inner = Inner::default();
        for mut job in repo::jobs::active(&store)? {
            if let JobStatus::Paused { reason } = &job.status {
                if reason != USER_PAUSE {
                    job.status = JobStatus::Queued;
                    job.updated_at = clock.now();
                    repo::jobs::upsert(&store, &job)?;
                }
            }
            inner.jobs.insert(job.id.clone(), job);
        }
        rinfo!(
            Category::Jobs,
            "queue loaded with {} active jobs",
            inner.jobs.len()
        );
        Ok(Arc::new(Self {
            store,
            clock,
            events,
            inner: Mutex::new(inner),
            wake: Notify::new(),
        }))
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }
    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    /// Wake the scheduler; also awaited by it.
    pub fn wake(&self) {
        self.wake.notify_one();
    }
    pub async fn wait_for_wake(&self) {
        self.wake.notified().await
    }

    /// Enqueue, or return the id of an identical non-terminal job.
    pub fn enqueue(&self, spec: JobSpec) -> Result<JobId> {
        let hash = payload_hash(&spec.payload);
        let mut inner = self.inner.lock();
        if let Some(existing) = inner.jobs.values().find(|j| {
            j.kind == spec.kind
                && j.dig_id == spec.dig_id
                && payload_hash(&JobEnvelope::of(j).payload) == hash
        }) {
            rdebug!(
                Category::Jobs,
                "deduped {} for dig {:?} onto job {}",
                spec.kind.as_str(),
                spec.dig_id,
                existing.id
            );
            return Ok(existing.id.clone());
        }
        let now = self.clock.now();
        let job = Job {
            id: JobId::new(),
            kind: spec.kind,
            priority: spec.effective_priority(),
            status: JobStatus::Queued,
            dig_id: spec.dig_id.clone(),
            label: spec.label.clone(),
            progress: Progress {
                unit: "items".into(),
                ..Default::default()
            },
            checkpoint: JobEnvelope {
                payload: spec.payload,
                checkpoint: serde_json::Value::Null,
            }
            .to_value(),
            cancellable: spec.cancellable,
            attempts: 0,
            created_at: now,
            started_at: None,
            finished_at: None,
            updated_at: now,
        };
        repo::jobs::upsert(&self.store, &job)?;
        rinfo!(
            Category::Jobs,
            "enqueued {} job {} ({:?}) for dig {:?}",
            job.kind.as_str(),
            job.id,
            job.priority,
            job.dig_id
        );
        let visible = self.visible_locked(&inner, &job);
        inner.jobs.insert(job.id.clone(), job.clone());
        drop(inner);
        self.events.emit(RiddleEvent::JobChanged { job: visible });
        self.wake();
        Ok(job.id)
    }

    pub fn get(&self, id: &JobId) -> Result<Option<Job>> {
        let cached = {
            let inner = self.inner.lock();
            inner.jobs.get(id).map(|j| self.visible_locked(&inner, j))
        };
        match cached {
            Some(j) => Ok(Some(j)),
            None => repo::jobs::get(&self.store, id),
        }
    }

    /// Non-terminal jobs as the Task Centre sees them, highest priority first,
    /// oldest first within a class.
    pub fn active(&self) -> Vec<Job> {
        let inner = self.inner.lock();
        let mut v: Vec<Job> = inner
            .jobs
            .values()
            .map(|j| self.visible_locked(&inner, j))
            .collect();
        v.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then(a.created_at.cmp(&b.created_at))
                .then(a.id.cmp(&b.id))
        });
        v
    }

    pub fn recent(&self, limit: u32) -> Result<Vec<Job>> {
        repo::jobs::recent(&self.store, limit)
    }

    /// Jobs the scheduler may start now: queued, not held, not backing off,
    /// and not globally paused.
    pub fn runnable(&self) -> Vec<Job> {
        let inner = self.inner.lock();
        if !inner.paused.is_empty() {
            return vec![];
        }
        let now = tokio::time::Instant::now();
        let mut v: Vec<Job> = inner
            .jobs
            .values()
            .filter(|j| j.status == JobStatus::Queued)
            .filter(|j| !inner.held.contains_key(&j.id))
            .filter(|j| inner.not_before.get(&j.id).is_none_or(|t| *t <= now))
            .cloned()
            .collect();
        // Ids are UUIDv7, so they break ties between jobs created in the same
        // millisecond in creation order.
        v.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then(a.created_at.cmp(&b.created_at))
                .then(a.id.cmp(&b.id))
        });
        v
    }

    /// The earliest back-off expiry, so the scheduler can sleep until then.
    pub fn next_backoff(&self) -> Option<tokio::time::Instant> {
        let inner = self.inner.lock();
        inner
            .not_before
            .iter()
            .filter(|(id, _)| {
                inner
                    .jobs
                    .get(*id)
                    .is_some_and(|j| j.status == JobStatus::Queued)
            })
            .map(|(_, t)| *t)
            .min()
    }

    // ------------------------------------------------------------ transitions

    fn visible_locked(&self, inner: &Inner, job: &Job) -> Job {
        let mut j = job.clone();
        if matches!(j.status, JobStatus::Queued | JobStatus::Running) {
            if let Some(reason) = inner.paused.iter().next() {
                j.status = JobStatus::Paused {
                    reason: reason.as_str().into(),
                };
            } else if let Some(reason) = inner.held.get(&j.id) {
                j.status = JobStatus::Paused {
                    reason: (*reason).into(),
                };
            }
        }
        j
    }

    /// Persist and publish a changed job. Bookkeeping never fails the work:
    /// a store error is logged and the in-memory state still moves on.
    fn commit(&self, inner: &mut Inner, job: Job) {
        if let Err(e) = repo::jobs::upsert(&self.store, &job) {
            rerror!(
                Category::Jobs,
                "could not persist job {}: {}",
                job.id,
                e.message
            );
        }
        let visible = self.visible_locked(inner, &job);
        if job.status.is_terminal() {
            inner.jobs.remove(&job.id);
            inner.tokens.remove(&job.id);
            inner.not_before.remove(&job.id);
            inner.yielded.remove(&job.id);
            inner.held.remove(&job.id);
        } else {
            inner.jobs.insert(job.id.clone(), job);
        }
        self.events.emit(RiddleEvent::JobChanged { job: visible });
    }

    fn with_job(&self, id: &JobId, f: impl FnOnce(&mut Inner, &mut Job)) -> Result<Job> {
        let mut inner = self.inner.lock();
        let mut job = inner
            .jobs
            .get(id)
            .cloned()
            .ok_or_else(|| RiddleError::not_found(format!("job {id} is not active")))?;
        f(&mut inner, &mut job);
        job.updated_at = self.clock.now();
        self.commit(&mut inner, job.clone());
        Ok(job)
    }

    /// Called by the runner when a job starts.
    pub(crate) fn mark_running(&self, id: &JobId, token: CancelToken) -> Result<Job> {
        let now = self.clock.now();
        self.with_job(id, |inner, job| {
            job.status = JobStatus::Running;
            job.started_at.get_or_insert(now);
            if !inner.yielded.remove(&job.id) {
                job.attempts += 1;
            }
            inner.not_before.remove(&job.id);
            inner.tokens.insert(job.id.clone(), token);
        })
    }

    /// Persist progress (throttled by the runner) and publish it.
    pub(crate) fn record_progress(&self, id: &JobId, progress: Progress) {
        let mut inner = self.inner.lock();
        let Some(job) = inner.jobs.get_mut(id) else {
            return;
        };
        job.progress = progress.clone();
        job.updated_at = self.clock.now();
        let snapshot = job.clone();
        drop(inner);
        if let Err(e) = repo::jobs::upsert(&self.store, &snapshot) {
            rerror!(
                Category::Jobs,
                "could not persist progress for job {id}: {}",
                e.message
            );
        }
        self.events.emit(RiddleEvent::JobProgress {
            job_id: id.clone(),
            done: progress.done,
            total: progress.total,
            step: progress.step,
        });
    }

    /// The handler yielded (`done: false`): keep the checkpoint, go back to
    /// the queue unless the user paused it meanwhile.
    pub(crate) fn mark_yielded(
        &self,
        id: &JobId,
        checkpoint: serde_json::Value,
        progress: Progress,
    ) -> Result<Job> {
        self.with_job(id, |inner, job| {
            let mut env = JobEnvelope::of(job);
            env.checkpoint = checkpoint;
            job.checkpoint = env.to_value();
            job.progress = progress;
            if !matches!(job.status, JobStatus::Paused { .. }) {
                job.status = JobStatus::Queued;
            }
            inner.tokens.remove(&job.id);
            inner.yielded.insert(job.id.clone());
        })
    }

    pub(crate) fn mark_completed(
        &self,
        id: &JobId,
        checkpoint: serde_json::Value,
        progress: Progress,
    ) -> Result<Job> {
        let now = self.clock.now();
        self.with_job(id, |_, job| {
            let mut env = JobEnvelope::of(job);
            env.checkpoint = checkpoint;
            job.checkpoint = env.to_value();
            job.progress = progress;
            job.status = JobStatus::Completed;
            job.finished_at = Some(now);
        })
    }

    pub(crate) fn mark_failed(&self, id: &JobId, error: String) -> Result<Job> {
        let now = self.clock.now();
        self.with_job(id, |_, job| {
            job.status = JobStatus::Failed { error };
            job.finished_at = Some(now);
        })
    }

    pub(crate) fn mark_cancelled(&self, id: &JobId) -> Result<Job> {
        let now = self.clock.now();
        self.with_job(id, |_, job| {
            job.status = JobStatus::Cancelled;
            job.finished_at = Some(now);
        })
    }

    /// A transient failure: back to the queue, not before `delay`.
    pub(crate) fn mark_retry(&self, id: &JobId, delay: std::time::Duration) -> Result<Job> {
        self.with_job(id, |inner, job| {
            // A pause the user asked for meanwhile stays a pause.
            if !matches!(job.status, JobStatus::Paused { .. }) {
                job.status = JobStatus::Queued;
            }
            inner.tokens.remove(&job.id);
            inner
                .not_before
                .insert(job.id.clone(), tokio::time::Instant::now() + delay);
        })
    }

    /// Hold a queued job with a reason (the budget gate) until released.
    pub(crate) fn hold(&self, id: &JobId, reason: &'static str) {
        let mut inner = self.inner.lock();
        if inner.held.insert(id.clone(), reason).is_none() {
            if let Some(job) = inner.jobs.get(id).cloned() {
                let visible = self.visible_locked(&inner, &job);
                self.events.emit(RiddleEvent::JobChanged { job: visible });
            }
        }
    }

    pub(crate) fn release_holds(&self, reason: &'static str) {
        let mut inner = self.inner.lock();
        let ids: Vec<JobId> = inner
            .held
            .iter()
            .filter(|(_, r)| **r == reason)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &ids {
            inner.held.remove(id);
        }
        if !ids.is_empty() {
            drop(inner);
            self.wake();
        }
    }

    pub fn is_held(&self, id: &JobId) -> bool {
        self.inner.lock().held.contains_key(id)
    }

    // --------------------------------------------------------- user controls

    pub fn cancel(&self, id: &JobId) -> Result<Job> {
        let job = self
            .get(id)?
            .ok_or_else(|| RiddleError::not_found(format!("job {id} not found")))?;
        if job.status.is_terminal() {
            return Err(RiddleError::invalid(format!(
                "job {id} has already finished"
            )));
        }
        if !job.cancellable {
            return Err(RiddleError::policy_denied(format!(
                "job {id} cannot be cancelled"
            )));
        }
        let token = self.inner.lock().tokens.get(id).cloned();
        match token {
            Some(token) => {
                // Running: the handler stops at its next check and the runner
                // records Cancelled.
                token.cancel();
                rinfo!(Category::Jobs, "cancel requested for running job {id}");
                Ok(job)
            }
            None => {
                rinfo!(Category::Jobs, "cancelled queued job {id}");
                self.mark_cancelled(id)
            }
        }
    }

    pub fn pause(&self, id: &JobId) -> Result<Job> {
        let updated = self.with_job(id, |inner, job| {
            if let Some(token) = inner.tokens.get(&job.id) {
                token.request_pause();
            }
            job.status = JobStatus::Paused {
                reason: USER_PAUSE.into(),
            };
        })?;
        rinfo!(Category::Jobs, "paused job {id}");
        Ok(updated)
    }

    pub fn resume(&self, id: &JobId) -> Result<Job> {
        let updated = self.with_job(id, |inner, job| {
            if matches!(job.status, JobStatus::Paused { .. }) {
                job.status = JobStatus::Queued;
                inner.not_before.remove(&job.id);
            }
        })?;
        rinfo!(Category::Jobs, "resumed job {id}");
        self.wake();
        Ok(updated)
    }

    // ----------------------------------------------------------- global pause

    pub fn pause_all(&self, reason: PauseReason) {
        let mut inner = self.inner.lock();
        let first = inner.paused.insert(reason);
        for token in inner.tokens.values() {
            token.request_pause();
        }
        if first {
            rwarn!(Category::Jobs, "all jobs paused: {}", reason.as_str());
            let jobs: Vec<Job> = inner
                .jobs
                .values()
                .map(|j| self.visible_locked(&inner, j))
                .collect();
            drop(inner);
            for job in jobs {
                self.events.emit(RiddleEvent::JobChanged { job });
            }
        }
    }

    /// Clear every pause reason.
    pub fn resume_all(&self) {
        let mut inner = self.inner.lock();
        if inner.paused.is_empty() {
            return;
        }
        inner.paused.clear();
        rinfo!(Category::Jobs, "all jobs resumed");
        let jobs: Vec<Job> = inner
            .jobs
            .values()
            .map(|j| self.visible_locked(&inner, j))
            .collect();
        drop(inner);
        for job in jobs {
            self.events.emit(RiddleEvent::JobChanged { job });
        }
        self.wake();
    }

    /// Clear one reason; others (offline while asleep) keep the pause.
    pub fn clear_pause(&self, reason: PauseReason) {
        let mut inner = self.inner.lock();
        if !inner.paused.remove(&reason) {
            return;
        }
        rinfo!(Category::Jobs, "pause cleared: {}", reason.as_str());
        let jobs: Vec<Job> = inner
            .jobs
            .values()
            .map(|j| self.visible_locked(&inner, j))
            .collect();
        let now_running = inner.paused.is_empty();
        drop(inner);
        for job in jobs {
            self.events.emit(RiddleEvent::JobChanged { job });
        }
        if now_running {
            self.wake();
        }
    }

    /// Pause on the persistence gate's fault flag (§11) and resume when it
    /// clears. Call once per opened store; Dig stores open lazily.
    pub fn watch_store_faults(self: &Arc<Self>, store: &Store) {
        let me = Arc::downgrade(self);
        store.on_fault(Arc::new(move |fault| {
            if let Some(q) = me.upgrade() {
                match fault {
                    Some(_) => q.pause_all(PauseReason::Fault),
                    None => q.clear_pause(PauseReason::Fault),
                }
            }
        }));
        if store.fault().is_some() {
            self.pause_all(PauseReason::Fault);
        }
    }

    pub fn pause_reasons(&self) -> Vec<PauseReason> {
        self.inner.lock().paused.iter().copied().collect()
    }
    pub fn is_paused(&self) -> bool {
        !self.inner.lock().paused.is_empty()
    }

    pub fn running_count(&self) -> usize {
        self.inner.lock().tokens.len()
    }
    pub fn running_of(&self, priority: Priority) -> usize {
        let inner = self.inner.lock();
        inner
            .tokens
            .keys()
            .filter(|id| inner.jobs.get(*id).is_some_and(|j| j.priority == priority))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::{RecordingSink, SystemClock};
    use riddle_store::StoreKind;

    #[test]
    fn a_store_fault_pauses_the_queue_and_clearing_it_resumes() {
        let store = Arc::new(Store::in_memory(StoreKind::Index).unwrap());
        let queue = JobQueue::open(
            store.clone(),
            Arc::new(SystemClock),
            Arc::new(RecordingSink::default()),
        )
        .unwrap();
        queue.watch_store_faults(&store);
        for _ in 0..3 {
            let _ = store.write(|c| c.execute("INSERT INTO nope VALUES (1)", []));
        }
        assert_eq!(queue.pause_reasons(), vec![PauseReason::Fault]);
        store.clear_fault();
        assert!(!queue.is_paused());
    }
}
