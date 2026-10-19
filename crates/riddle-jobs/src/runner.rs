//! The scheduler and the handler contract (§54–§55).
//!
//! `Runner` picks the highest-priority runnable job whose Dig has no other
//! job of the same resource class running, up to `max_concurrent` at once,
//! and runs its `JobHandler`. Handlers are cooperative: they report progress,
//! check the `CancelToken` at safe points, and may return `done: false` with
//! a checkpoint to yield and be rescheduled. That one convention gives us
//! pause, cancel, sleep recovery and crash resume without a second code path.
//!
//! Progress is persisted at most once a second per job (the incident: a
//! sync that wrote a row per message and starved the interactive reads);
//! ETA comes from the rate over the last minute. Failures retry with
//! exponential back-off when the error kind is transient (§8) and fail the
//! job otherwise. A panic in a handler fails the job; it never takes the
//! scheduler down.

use crate::queue::JobQueue;
use crate::resources::{BudgetClass, Resources};
use crate::spec::{is_long_running, resource_class, JobEnvelope, ResourceClass};
use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};
use riddle_actions::Services;
use riddle_core::{DigId, Job, JobId, JobKind, JobStatus, Progress};
use riddle_errors::{ErrorKind, Result, RiddleError};
use riddle_observability::{rdebug, rerror, rinfo, rwarn, Category};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;

/// What a handler returns. `done: false` yields: the checkpoint is kept and
/// the job goes back to the queue.
#[derive(Debug, Clone, Default)]
pub struct JobOutcome {
    pub checkpoint: Value,
    pub done: bool,
}

impl JobOutcome {
    pub fn done() -> Self {
        Self {
            checkpoint: Value::Null,
            done: true,
        }
    }
    pub fn done_with(checkpoint: Value) -> Self {
        Self {
            checkpoint,
            done: true,
        }
    }
    pub fn yield_with(checkpoint: Value) -> Self {
        Self {
            checkpoint,
            done: false,
        }
    }
}

#[async_trait]
pub trait JobHandler: Send + Sync {
    async fn run(&self, ctx: JobContext) -> Result<JobOutcome>;
}

/// Cooperative cancellation and pause. Handlers call `check()` at safe
/// points; `should_yield()` covers both signals.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<TokenInner>);

#[derive(Default)]
struct TokenInner {
    cancelled: AtomicBool,
    pause: AtomicBool,
    notify: Notify,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }
    pub fn request_pause(&self) {
        self.0.pause.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::SeqCst)
    }
    pub fn pause_requested(&self) -> bool {
        self.0.pause.load(Ordering::SeqCst)
    }
    /// True when the handler should checkpoint and return `done: false`.
    pub fn should_yield(&self) -> bool {
        self.is_cancelled() || self.pause_requested()
    }
    /// `Err` once cancelled, for `?` at safe points.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(RiddleError::new(ErrorKind::Internal, "cancelled"))
        } else {
            Ok(())
        }
    }
    /// Resolves when cancel or pause is requested, for `select!`.
    pub async fn signalled(&self) {
        loop {
            // Register before checking so a signal between the check and the
            // await is not lost (`notify_waiters` stores no permit).
            let notified = self.0.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.should_yield() {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Default)]
struct ProgressState {
    progress: Progress,
    dirty: bool,
    samples: VecDeque<(Instant, u64)>,
}

/// Progress reporting with a one-minute rate window for the ETA.
#[derive(Clone, Default)]
pub struct ProgressReporter(Arc<Mutex<ProgressState>>);

impl ProgressReporter {
    pub fn new(initial: Progress) -> Self {
        let mut st = ProgressState {
            progress: initial,
            ..Default::default()
        };
        st.samples.push_back((Instant::now(), st.progress.done));
        Self(Arc::new(Mutex::new(st)))
    }
    pub fn report(&self, done: u64, total: Option<u64>, step: impl Into<String>) {
        let mut st = self.0.lock();
        st.progress.done = done;
        st.progress.total = total;
        st.progress.step = step.into();
        st.dirty = true;
        let now = Instant::now();
        st.samples.push_back((now, done));
        while st.samples.len() > 1
            && st
                .samples
                .front()
                .is_some_and(|(t, _)| now.duration_since(*t) > Duration::from_secs(60))
        {
            st.samples.pop_front();
        }
    }
    pub fn set_unit(&self, unit: impl Into<String>) {
        let mut st = self.0.lock();
        st.progress.unit = unit.into();
        st.dirty = true;
    }
    pub fn step(&self, step: impl Into<String>) {
        let mut st = self.0.lock();
        st.progress.step = step.into();
        st.dirty = true;
    }
    pub fn add(&self, n: u64) {
        let (done, total, step) = {
            let st = self.0.lock();
            (
                st.progress.done + n,
                st.progress.total,
                st.progress.step.clone(),
            )
        };
        self.report(done, total, step);
    }
    pub fn current(&self) -> Progress {
        self.snapshot().0
    }

    /// The progress with the ETA computed, and whether it changed since the
    /// last snapshot.
    pub fn snapshot(&self) -> (Progress, bool) {
        let mut st = self.0.lock();
        let dirty = std::mem::take(&mut st.dirty);
        let eta = match (st.samples.front(), st.samples.back(), st.progress.total) {
            (Some((t0, d0)), Some((t1, d1)), Some(total)) if d1 > d0 && t1 > t0 => {
                let rate = (d1 - d0) as f64 / t1.duration_since(*t0).as_secs_f64();
                let remaining = total.saturating_sub(*d1) as f64;
                if rate > 0.0 {
                    Some((remaining / rate).round() as u64)
                } else {
                    None
                }
            }
            _ => None,
        };
        st.progress.eta_seconds = eta;
        (st.progress.clone(), dirty)
    }
}

pub struct JobContext {
    pub job: Job,
    pub payload: Value,
    pub checkpoint: Value,
    pub progress: ProgressReporter,
    pub cancel: CancelToken,
    pub services: Arc<Services>,
}

/// Completion notifications for long jobs (§55). The desktop app routes
/// this to the notifier helper; tests record.
pub trait Notifier: Send + Sync {
    fn notify(&self, title: &str, body: &str);
}

pub struct NullNotifier;
impl Notifier for NullNotifier {
    fn notify(&self, _title: &str, _body: &str) {}
}

#[derive(Default)]
pub struct RecordingNotifier(pub Mutex<Vec<(String, String)>>);
impl Notifier for RecordingNotifier {
    fn notify(&self, title: &str, body: &str) {
        self.0.lock().push((title.into(), body.into()));
    }
}

pub const DEFAULT_MAX_CONCURRENT: usize = 3;
pub const MAX_ATTEMPTS: u32 = 5;
const PROGRESS_FLUSH: Duration = Duration::from_secs(1);
const IDLE_TICK: Duration = Duration::from_secs(5);

pub const BACKOFF_BASE: Duration = Duration::from_secs(5);
pub const BACKOFF_CAP: Duration = Duration::from_secs(600);

/// `base × 2^(attempts-1)`, capped.
pub fn backoff_for(attempts: u32, base: Duration) -> Duration {
    base.saturating_mul(2u32.saturating_pow(attempts.saturating_sub(1).min(16)))
        .min(BACKOFF_CAP)
}

struct RunningEntry {
    dig: Option<DigId>,
    class: ResourceClass,
    handle: tokio::task::JoinHandle<()>,
}

pub struct Runner {
    queue: Arc<JobQueue>,
    services: Arc<Services>,
    resources: Arc<Resources>,
    handlers: RwLock<HashMap<JobKind, Arc<dyn JobHandler>>>,
    notifier: RwLock<Arc<dyn Notifier>>,
    max_concurrent: usize,
    backoff_base: Duration,
    running: Mutex<HashMap<JobId, RunningEntry>>,
    stopped: AtomicBool,
    settled: Notify,
}

impl Runner {
    pub fn new(
        queue: Arc<JobQueue>,
        services: Arc<Services>,
        resources: Arc<Resources>,
    ) -> Arc<Self> {
        Arc::new(Self {
            queue,
            services,
            resources,
            handlers: RwLock::new(HashMap::new()),
            notifier: RwLock::new(Arc::new(NullNotifier)),
            max_concurrent: DEFAULT_MAX_CONCURRENT,
            backoff_base: BACKOFF_BASE,
            running: Mutex::new(HashMap::new()),
            stopped: AtomicBool::new(false),
            settled: Notify::new(),
        })
    }

    /// Only meaningful before `start`; rebuilt rather than mutated.
    pub fn with_max_concurrent(self: Arc<Self>, n: usize) -> Arc<Self> {
        self.rebuild(n.max(1), None)
    }
    /// Shorter retries for tests.
    pub fn with_backoff_base(self: Arc<Self>, base: Duration) -> Arc<Self> {
        self.rebuild(self.max_concurrent, Some(base))
    }

    fn rebuild(&self, max_concurrent: usize, backoff_base: Option<Duration>) -> Arc<Self> {
        Arc::new(Self {
            queue: self.queue.clone(),
            services: self.services.clone(),
            resources: self.resources.clone(),
            handlers: RwLock::new(self.handlers.read().clone()),
            notifier: RwLock::new(self.notifier.read().clone()),
            max_concurrent,
            backoff_base: backoff_base.unwrap_or(self.backoff_base),
            running: Mutex::new(HashMap::new()),
            stopped: AtomicBool::new(false),
            settled: Notify::new(),
        })
    }

    pub fn register(&self, kind: JobKind, handler: Arc<dyn JobHandler>) {
        self.handlers.write().insert(kind, handler);
    }
    pub fn set_notifier(&self, n: Arc<dyn Notifier>) {
        *self.notifier.write() = n;
    }
    pub fn queue(&self) -> &Arc<JobQueue> {
        &self.queue
    }
    pub fn resources(&self) -> &Arc<Resources> {
        &self.resources
    }
    pub fn running_count(&self) -> usize {
        self.running.lock().len()
    }

    /// Spawn the scheduler loop. Returns its handle; `stop()` ends it.
    pub fn start(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let me = self.clone();
        tokio::spawn(async move {
            rinfo!(
                Category::Jobs,
                "scheduler started (max {} concurrent)",
                me.max_concurrent
            );
            while !me.stopped.load(Ordering::SeqCst) {
                me.tick();
                let until_backoff = me
                    .queue
                    .next_backoff()
                    .map(|t| t.saturating_duration_since(Instant::now()))
                    .unwrap_or(IDLE_TICK)
                    .min(IDLE_TICK);
                tokio::select! {
                    _ = me.queue.wait_for_wake() => {}
                    _ = tokio::time::sleep(until_backoff) => {}
                }
            }
            rinfo!(Category::Jobs, "scheduler stopped");
        })
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.queue.wake();
    }

    /// One scheduling pass. Returns the number of jobs started.
    pub fn tick(self: &Arc<Self>) -> usize {
        let mut started = 0;
        for job in self.queue.runnable() {
            let mut running = self.running.lock();
            if running.len() >= self.max_concurrent {
                break;
            }
            if running.contains_key(&job.id) {
                continue;
            }
            let Some(handler) = self.handlers.read().get(&job.kind).cloned() else {
                rdebug!(
                    Category::Jobs,
                    "no handler registered for {}; job {} waits",
                    job.kind.as_str(),
                    job.id
                );
                continue;
            };
            let class = resource_class(job.kind);
            if running
                .values()
                .any(|r| r.dig == job.dig_id && r.class == class)
            {
                continue;
            }
            let budget_class = match class {
                ResourceClass::Judge => Some(BudgetClass::Judge),
                ResourceClass::Llm => Some(BudgetClass::Llm),
                _ => None,
            };
            if let Some(bc) = budget_class {
                if self
                    .resources
                    .budget_exhausted(job.dig_id.as_ref(), bc, job.priority)
                {
                    drop(running);
                    self.queue.hold(&job.id, "budget");
                    continue;
                }
            }
            let token = CancelToken::new();
            let job = match self.queue.mark_running(&job.id, token.clone()) {
                Ok(j) => j,
                Err(e) => {
                    rwarn!(
                        Category::Jobs,
                        "could not start job {}: {}",
                        job.id,
                        e.message
                    );
                    continue;
                }
            };
            let me = self.clone();
            let id = job.id.clone();
            let dig = job.dig_id.clone();
            let handle = tokio::spawn(async move {
                me.run_job(job, handler, token).await;
            });
            running.insert(id, RunningEntry { dig, class, handle });
            started += 1;
        }
        started
    }

    /// Release budget holds so the next tick re-checks them; call when the
    /// day rolls over or the budget setting changes.
    pub fn recheck_budgets(&self) {
        self.queue.release_holds("budget");
    }

    async fn run_job(self: Arc<Self>, job: Job, handler: Arc<dyn JobHandler>, token: CancelToken) {
        let id = job.id.clone();
        let env = JobEnvelope::of(&job);
        let progress = ProgressReporter::new(job.progress.clone());
        let ctx = JobContext {
            payload: env.payload,
            checkpoint: env.checkpoint,
            progress: progress.clone(),
            cancel: token.clone(),
            services: self.services.clone(),
            job: job.clone(),
        };
        rinfo!(
            Category::Jobs,
            "job {} ({}) running, attempt {}",
            id,
            job.kind.as_str(),
            job.attempts
        );
        let started = Instant::now();
        let mut work = tokio::spawn(async move { handler.run(ctx).await });
        let mut flush = tokio::time::interval(PROGRESS_FLUSH);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        flush.tick().await;
        let outcome = loop {
            tokio::select! {
                res = &mut work => break res,
                _ = flush.tick() => {
                    let (p, dirty) = progress.snapshot();
                    if dirty { self.queue.record_progress(&id, p); }
                }
            }
        };
        let (final_progress, _) = progress.snapshot();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        let result = match outcome {
            Ok(r) => r,
            Err(join) => Err(RiddleError::internal(if join.is_panic() {
                "job handler panicked"
            } else {
                "job handler aborted"
            })),
        };
        let finished = match result {
            Ok(out) if out.done => {
                rinfo!(
                    Category::Jobs,
                    "job {} completed in {elapsed_ms}ms ({} {})",
                    id,
                    final_progress.done,
                    final_progress.unit
                );
                let done = self
                    .queue
                    .mark_completed(&id, out.checkpoint, final_progress.clone());
                if is_long_running(job.kind) {
                    let body = match final_progress.total {
                        Some(t) => format!(
                            "{} of {} {} done",
                            final_progress.done, t, final_progress.unit
                        ),
                        None => format!("{} {} done", final_progress.done, final_progress.unit),
                    };
                    self.notifier
                        .read()
                        .notify(&format!("{} finished", job.label), &body);
                }
                done
            }
            Ok(out) => {
                if token.is_cancelled() {
                    rinfo!(Category::Jobs, "job {} cancelled after {elapsed_ms}ms", id);
                    self.queue.mark_cancelled(&id)
                } else {
                    rdebug!(Category::Jobs, "job {} yielded after {elapsed_ms}ms", id);
                    self.queue.mark_yielded(&id, out.checkpoint, final_progress)
                }
            }
            Err(e) if token.is_cancelled() => {
                rinfo!(
                    Category::Jobs,
                    "job {} cancelled ({}) after {elapsed_ms}ms",
                    id,
                    e.kind.as_str()
                );
                self.queue.mark_cancelled(&id)
            }
            Err(e) if e.kind == ErrorKind::BudgetExhausted => {
                rwarn!(Category::Jobs, "job {} paused on budget", id);
                let r = self
                    .queue
                    .mark_yielded(&id, job_checkpoint(&job), final_progress);
                self.queue.hold(&id, "budget");
                r
            }
            Err(e) if e.kind.is_transient() && job.attempts < MAX_ATTEMPTS => {
                let delay = backoff_for(job.attempts, self.backoff_base);
                rwarn!(
                    Category::Jobs,
                    "job {} failed ({}), retry {} in {}s",
                    id,
                    e.kind.as_str(),
                    job.attempts + 1,
                    delay.as_secs()
                );
                self.queue.mark_retry(&id, delay)
            }
            Err(e) => {
                rerror!(
                    Category::Jobs,
                    "job {} failed ({}) after {} attempts",
                    id,
                    e.kind.as_str(),
                    job.attempts
                );
                let r = self.queue.mark_failed(&id, e.message.clone());
                if is_long_running(job.kind) {
                    self.notifier
                        .read()
                        .notify(&format!("{} failed", job.label), &e.message);
                }
                r
            }
        };
        if let Err(e) = finished {
            riddle_observability::swallowed!(
                Category::Jobs,
                "job_outcome",
                e,
                "could not record outcome for job {}",
                id
            );
        }
        if let Some(entry) = self.running.lock().remove(&id) {
            drop(entry.handle);
        }
        self.settled.notify_waiters();
        self.queue.wake();
    }

    /// For tests and shutdown: tick until nothing is running or queued
    /// (paused and held jobs do not count), up to `timeout`.
    pub async fn run_until_idle(self: &Arc<Self>, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            let settled = self.settled.notified();
            tokio::pin!(settled);
            settled.as_mut().enable();
            self.tick();
            let idle = self.running.lock().is_empty()
                && self
                    .queue
                    .active()
                    .iter()
                    .all(|j| matches!(j.status, JobStatus::Paused { .. }));
            if idle {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            tokio::select! {
                _ = &mut settled => {}
                _ = self.queue.wait_for_wake() => {}
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
        }
    }

    /// Whether any job of a kind is running or queued, for the Task Centre.
    pub fn has_active(&self, kinds: &[JobKind]) -> bool {
        self.queue.active().iter().any(|j| {
            kinds.contains(&j.kind) && matches!(j.status, JobStatus::Running | JobStatus::Queued)
        })
    }
}

fn job_checkpoint(job: &Job) -> Value {
    JobEnvelope::of(job).checkpoint
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_from_five_seconds_and_caps_at_ten_minutes() {
        assert_eq!(backoff_for(1, BACKOFF_BASE), Duration::from_secs(5));
        assert_eq!(backoff_for(2, BACKOFF_BASE), Duration::from_secs(10));
        assert_eq!(backoff_for(4, BACKOFF_BASE), Duration::from_secs(40));
        assert_eq!(backoff_for(20, BACKOFF_BASE), Duration::from_secs(600));
    }

    #[tokio::test(start_paused = true)]
    async fn the_eta_comes_from_the_rate_over_the_window() {
        let p = ProgressReporter::new(Progress::default());
        p.report(0, Some(100), "fetch");
        tokio::time::advance(Duration::from_secs(10)).await;
        p.report(20, Some(100), "fetch");
        let (snap, dirty) = p.snapshot();
        assert!(dirty);
        assert_eq!(snap.eta_seconds, Some(40));
        let (_, dirty) = p.snapshot();
        assert!(!dirty, "a second snapshot without a report is clean");
    }

    #[tokio::test]
    async fn a_token_signals_cancel_and_pause() {
        let t = CancelToken::new();
        assert!(t.check().is_ok());
        t.request_pause();
        assert!(t.should_yield());
        assert!(t.check().is_ok(), "a pause is not a cancel");
        t.cancel();
        assert!(t.check().is_err());
        tokio::time::timeout(Duration::from_millis(100), t.signalled())
            .await
            .unwrap();
    }
}
