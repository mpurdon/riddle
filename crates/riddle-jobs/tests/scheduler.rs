//! Scheduler behaviour end to end with fake handlers and an in-memory store:
//! ordering, pre-emption, checkpointed yields, cancellation, sleep recovery,
//! retries, dedupe, orphan requeue, budget holds, notifications, progress
//! throttling and the Task Centre snapshot.

use async_trait::async_trait;
use parking_lot::Mutex;
use riddle_actions::Services;
use riddle_core::*;
use riddle_errors::{ErrorKind, Result, RiddleError};
use riddle_jobs::*;
use riddle_store::{repo, Stores};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

struct Harness {
    stores: Arc<Stores>,
    events: Arc<RecordingSink>,
    queue: Arc<JobQueue>,
    runner: Arc<Runner>,
    clock: Arc<FixedClock>,
}

fn harness(max_concurrent: usize) -> Harness {
    let stores = Arc::new(Stores::in_memory().unwrap());
    let events = Arc::new(RecordingSink::default());
    let clock = Arc::new(FixedClock::at(chrono::Utc::now()));
    let services = Arc::new(Services::new(
        stores.clone(),
        events.clone(),
        BuildStamp::from_env_vars("0", None, None, None, "test"),
    ));
    let queue = JobQueue::open(stores.index(), clock.clone(), events.clone()).unwrap();
    let resources = Resources::new(ResourceConfig::default());
    let runner = Runner::new(queue.clone(), services, resources)
        .with_max_concurrent(max_concurrent)
        .with_backoff_base(Duration::from_millis(10));
    Harness {
        stores,
        events,
        queue,
        runner,
        clock,
    }
}

/// Records the order handlers ran in and completes immediately.
struct Recorder(Arc<Mutex<Vec<String>>>);
#[async_trait]
impl JobHandler for Recorder {
    async fn run(&self, ctx: JobContext) -> Result<JobOutcome> {
        self.0.lock().push(ctx.job.label.clone());
        Ok(JobOutcome::done())
    }
}

/// Counts to `steps`, yielding with a checkpoint after each step.
struct Stepper {
    steps: u64,
    log: Arc<Mutex<Vec<String>>>,
}
#[async_trait]
impl JobHandler for Stepper {
    async fn run(&self, ctx: JobContext) -> Result<JobOutcome> {
        let n = ctx
            .checkpoint
            .get("n")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            + 1;
        self.log.lock().push(format!("{} step {n}", ctx.job.label));
        ctx.progress.report(n, Some(self.steps), "stepping");
        if n >= self.steps {
            Ok(JobOutcome::done_with(json!({ "n": n })))
        } else {
            Ok(JobOutcome::yield_with(json!({ "n": n })))
        }
    }
}

/// Runs until told to stop, checkpointing what it had.
struct Waiter {
    runs: Arc<AtomicUsize>,
}
#[async_trait]
impl JobHandler for Waiter {
    async fn run(&self, ctx: JobContext) -> Result<JobOutcome> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        ctx.progress.report(1, None, "waiting");
        ctx.cancel.signalled().await;
        Ok(JobOutcome::yield_with(json!({ "partial": true })))
    }
}

struct Failing(ErrorKind);
#[async_trait]
impl JobHandler for Failing {
    async fn run(&self, _ctx: JobContext) -> Result<JobOutcome> {
        Err(RiddleError::new(self.0, "nope"))
    }
}

struct Chatty;
#[async_trait]
impl JobHandler for Chatty {
    async fn run(&self, ctx: JobContext) -> Result<JobOutcome> {
        for i in 0..100 {
            ctx.progress.report(i + 1, Some(100), "chatting");
        }
        Ok(JobOutcome::done())
    }
}

struct NeedsJudge;
#[async_trait]
impl JobHandler for NeedsJudge {
    async fn run(&self, ctx: JobContext) -> Result<JobOutcome> {
        ctx.resources.acquire(Resource::Judge, 1.0).await?;
        Ok(JobOutcome::done())
    }
}

fn spec(kind: JobKind, label: &str) -> JobSpec {
    JobSpec::new(kind, label).payload(json!({ "label": label }))
}

async fn wait_for(what: &str, mut cond: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !cond() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

#[tokio::test]
async fn jobs_run_in_priority_order_then_by_age() {
    let h = harness(1);
    let order = Arc::new(Mutex::new(Vec::new()));
    for kind in [
        JobKind::Replay,
        JobKind::Embeddings,
        JobKind::BulkAction,
        JobKind::Sift,
        JobKind::Maintenance,
    ] {
        h.runner.register(kind, Arc::new(Recorder(order.clone())));
    }
    h.queue
        .enqueue(spec(JobKind::Replay, "experiment"))
        .unwrap();
    h.queue
        .enqueue(spec(JobKind::Embeddings, "backfill-1"))
        .unwrap();
    h.queue
        .enqueue(spec(JobKind::Maintenance, "backfill-2"))
        .unwrap();
    h.queue.enqueue(spec(JobKind::Sift, "incoming")).unwrap();
    h.queue
        .enqueue(spec(JobKind::BulkAction, "interactive"))
        .unwrap();
    assert!(h.runner.run_until_idle(Duration::from_secs(5)).await);
    assert_eq!(
        order.lock().clone(),
        vec![
            "interactive",
            "incoming",
            "backfill-1",
            "backfill-2",
            "experiment"
        ]
    );
}

#[tokio::test]
async fn an_interactive_job_preempts_a_yielding_backfill() {
    let h = harness(1);
    let log = Arc::new(Mutex::new(Vec::new()));
    h.runner.register(
        JobKind::Embeddings,
        Arc::new(Stepper {
            steps: 3,
            log: log.clone(),
        }),
    );
    h.runner
        .register(JobKind::BulkAction, Arc::new(Recorder(log.clone())));
    let backfill = h
        .queue
        .enqueue(spec(JobKind::Embeddings, "backfill"))
        .unwrap();
    // Let the backfill take exactly one step.
    h.runner.tick();
    wait_for("first step", || !log.lock().is_empty()).await;
    h.queue
        .enqueue(spec(JobKind::BulkAction, "interactive"))
        .unwrap();
    assert!(h.runner.run_until_idle(Duration::from_secs(5)).await);
    let log = log.lock().clone();
    assert_eq!(log[0], "backfill step 1");
    assert_eq!(log[1], "interactive", "{log:?}");
    assert_eq!(&log[2..], ["backfill step 2", "backfill step 3"]);
    let job = h.queue.get(&backfill).unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Completed);
    assert_eq!(job.attempts, 1, "yields are not retries");
    assert_eq!(JobEnvelope::of(&job).checkpoint["n"], 3);
    assert_eq!(job.progress.done, 3);
}

#[tokio::test]
async fn a_cancelled_running_job_ends_cancelled_and_a_queued_one_immediately() {
    let h = harness(1);
    let runs = Arc::new(AtomicUsize::new(0));
    h.runner.register(
        JobKind::HistoricalSift,
        Arc::new(Waiter { runs: runs.clone() }),
    );
    let running = h
        .queue
        .enqueue(spec(JobKind::HistoricalSift, "running"))
        .unwrap();
    let queued = h
        .queue
        .enqueue(spec(JobKind::HistoricalSift, "queued").dig(DigId::from("other")))
        .unwrap();
    let never = h
        .queue
        .enqueue(
            spec(JobKind::HistoricalSift, "fixed")
                .not_cancellable()
                .dig(DigId::from("third")),
        )
        .unwrap();
    h.runner.tick();
    wait_for("one running", || runs.load(Ordering::SeqCst) >= 1).await;
    assert_eq!(
        h.queue.get(&running).unwrap().unwrap().status,
        JobStatus::Running
    );
    assert_eq!(
        h.queue.get(&queued).unwrap().unwrap().status,
        JobStatus::Queued
    );
    h.queue.cancel(&running).unwrap();
    h.queue.cancel(&queued).unwrap();
    assert_eq!(
        h.queue.get(&queued).unwrap().unwrap().status,
        JobStatus::Cancelled
    );
    let err = h.queue.cancel(&never).unwrap_err();
    assert_eq!(err.kind, ErrorKind::PolicyDenied);
    wait_for("running job cancelled", || {
        h.queue.get(&running).unwrap().unwrap().status == JobStatus::Cancelled
    })
    .await;
    let from_store = repo::jobs::get(&h.stores.index(), &running)
        .unwrap()
        .unwrap();
    assert_eq!(from_store.status, JobStatus::Cancelled);
    assert!(from_store.finished_at.is_some());
    h.queue.cancel(&running).unwrap_err();
    h.queue.cancel(&never).unwrap_err();
}

struct NoHook;
impl WakeHook for NoHook {
    fn on_wake(&self, _r: WakeReason) {}
}

#[tokio::test]
async fn a_sleep_gap_makes_running_jobs_yield_and_restart_from_their_checkpoints() {
    let h = harness(2);
    let runs = Arc::new(AtomicUsize::new(0));
    h.runner.register(
        JobKind::InitialSync,
        Arc::new(Waiter { runs: runs.clone() }),
    );
    let id = h
        .queue
        .enqueue(spec(JobKind::InitialSync, "initial sync"))
        .unwrap();
    h.runner.tick();
    wait_for("first run", || runs.load(Ordering::SeqCst) >= 1).await;
    let detector = SleepDetector::new(h.queue.clone(), h.clock.clone(), Arc::new(NoHook));
    h.clock.advance(chrono::Duration::minutes(10));
    assert!(detector.check().is_some());
    // The handler yields with its checkpoint, the job is queued again, and
    // the next tick restarts it.
    wait_for("restart after wake", || {
        h.runner.tick();
        runs.load(Ordering::SeqCst) >= 2
    })
    .await;
    let job = h.queue.get(&id).unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Running);
    assert_eq!(JobEnvelope::of(&job).checkpoint["partial"], true);
    assert!(h.events.take().iter().any(|e| matches!(e, RiddleEvent::JobChanged { job } if matches!(&job.status, JobStatus::Paused { reason } if reason == "sleep"))));
    h.queue.cancel(&id).unwrap();
}

#[tokio::test]
async fn a_user_pause_persists_and_resume_requeues() {
    let h = harness(1);
    let runs = Arc::new(AtomicUsize::new(0));
    h.runner
        .register(JobKind::Expedition, Arc::new(Waiter { runs: runs.clone() }));
    let id = h
        .queue
        .enqueue(spec(JobKind::Expedition, "expedition"))
        .unwrap();
    h.runner.tick();
    wait_for("first run", || runs.load(Ordering::SeqCst) >= 1).await;
    h.queue.pause(&id).unwrap();
    wait_for("handler yielded", || h.runner.running_count() == 0).await;
    assert_eq!(
        h.queue.get(&id).unwrap().unwrap().status,
        JobStatus::Paused {
            reason: USER_PAUSE.into()
        }
    );
    assert!(
        h.runner.run_until_idle(Duration::from_millis(200)).await,
        "a paused job is idle"
    );
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    // A restart keeps the user's pause.
    let reopened = JobQueue::open(h.stores.index(), h.clock.clone(), h.events.clone()).unwrap();
    assert_eq!(
        reopened.get(&id).unwrap().unwrap().status,
        JobStatus::Paused {
            reason: USER_PAUSE.into()
        }
    );
    h.queue.resume(&id).unwrap();
    h.runner.tick();
    wait_for("second run", || runs.load(Ordering::SeqCst) >= 2).await;
    h.queue.cancel(&id).unwrap();
}

#[tokio::test]
async fn transient_failures_retry_with_backoff_and_others_fail_at_once() {
    let h = harness(2);
    h.runner
        .register(JobKind::CatchUp, Arc::new(Failing(ErrorKind::Provider)));
    h.runner
        .register(JobKind::Projection, Arc::new(Failing(ErrorKind::Invalid)));
    let transient = h.queue.enqueue(spec(JobKind::CatchUp, "catch up")).unwrap();
    let fatal = h
        .queue
        .enqueue(spec(JobKind::Projection, "project"))
        .unwrap();
    assert!(h.runner.run_until_idle(Duration::from_secs(10)).await);
    let t = repo::jobs::get(&h.stores.index(), &transient)
        .unwrap()
        .unwrap();
    assert_eq!(t.attempts, MAX_ATTEMPTS);
    assert!(matches!(t.status, JobStatus::Failed { .. }));
    let f = repo::jobs::get(&h.stores.index(), &fatal).unwrap().unwrap();
    assert_eq!(f.attempts, 1);
    assert_eq!(
        f.status,
        JobStatus::Failed {
            error: "nope".into()
        }
    );
}

#[tokio::test]
async fn enqueue_dedupes_identical_active_jobs_but_not_finished_ones() {
    let h = harness(1);
    let order = Arc::new(Mutex::new(Vec::new()));
    h.runner
        .register(JobKind::Sift, Arc::new(Recorder(order.clone())));
    let a = h
        .queue
        .enqueue(JobSpec::new(JobKind::Sift, "a").payload(json!({ "ids": [1, 2], "dig": "x" })))
        .unwrap();
    let b = h
        .queue
        .enqueue(JobSpec::new(JobKind::Sift, "b").payload(json!({ "dig": "x", "ids": [1, 2] })))
        .unwrap();
    assert_eq!(a, b, "same kind, dig and payload");
    let c = h
        .queue
        .enqueue(
            JobSpec::new(JobKind::Sift, "c")
                .payload(json!({ "ids": [1, 2], "dig": "x" }))
                .dig(DigId::from("d")),
        )
        .unwrap();
    assert_ne!(a, c, "a different Dig is a different job");
    assert!(h.runner.run_until_idle(Duration::from_secs(5)).await);
    let again = h
        .queue
        .enqueue(JobSpec::new(JobKind::Sift, "a").payload(json!({ "ids": [1, 2], "dig": "x" })))
        .unwrap();
    assert_ne!(a, again, "a finished job does not absorb a new one");
}

#[tokio::test]
async fn orphaned_running_jobs_are_requeued_on_open() {
    let h = harness(1);
    let id = h.queue.enqueue(spec(JobKind::InitialSync, "sync")).unwrap();
    let mut job = repo::jobs::get(&h.stores.index(), &id).unwrap().unwrap();
    job.status = JobStatus::Running;
    repo::jobs::upsert(&h.stores.index(), &job).unwrap();
    let reopened = JobQueue::open(h.stores.index(), h.clock.clone(), h.events.clone()).unwrap();
    assert_eq!(
        reopened.get(&id).unwrap().unwrap().status,
        JobStatus::Queued
    );
    assert_eq!(reopened.runnable().len(), 1);
}

struct Exhausted;
impl BudgetGate for Exhausted {
    fn remaining(&self, _d: Option<&DigId>, _c: BudgetClass) -> f64 {
        0.0
    }
}

#[tokio::test]
async fn an_exhausted_budget_holds_backfill_and_lets_incoming_run() {
    let h = harness(2);
    h.runner.resources().set_budget_gate(Arc::new(Exhausted));
    h.runner
        .register(JobKind::HistoricalSift, Arc::new(NeedsJudge));
    h.runner.register(JobKind::Sift, Arc::new(NeedsJudge));
    let backfill = h
        .queue
        .enqueue(spec(JobKind::HistoricalSift, "historical"))
        .unwrap();
    let incoming = h.queue.enqueue(spec(JobKind::Sift, "incoming")).unwrap();
    assert!(h.runner.run_until_idle(Duration::from_secs(5)).await);
    assert_eq!(
        h.queue.get(&incoming).unwrap().unwrap().status,
        JobStatus::Completed
    );
    let held = h.queue.get(&backfill).unwrap().unwrap();
    assert_eq!(
        held.status,
        JobStatus::Paused {
            reason: "budget".into()
        }
    );
    assert!(h.queue.is_held(&backfill));
    // Budget restored: the hold is released and the job runs.
    h.runner
        .resources()
        .set_budget_gate(Arc::new(UnlimitedBudget));
    h.runner.recheck_budgets();
    assert!(h.runner.run_until_idle(Duration::from_secs(5)).await);
    assert_eq!(
        h.queue.get(&backfill).unwrap().unwrap().status,
        JobStatus::Completed
    );
}

#[tokio::test]
async fn a_long_running_job_notifies_on_completion() {
    let h = harness(1);
    let notifier = Arc::new(RecordingNotifier::default());
    h.runner.set_notifier(notifier.clone());
    let log = Arc::new(Mutex::new(Vec::new()));
    h.runner.register(
        JobKind::InitialSync,
        Arc::new(Stepper {
            steps: 2,
            log: log.clone(),
        }),
    );
    h.runner.register(JobKind::Sift, Arc::new(Recorder(log)));
    h.queue
        .enqueue(spec(JobKind::InitialSync, "Initial sync of Work"))
        .unwrap();
    h.queue.enqueue(spec(JobKind::Sift, "sift")).unwrap();
    assert!(h.runner.run_until_idle(Duration::from_secs(5)).await);
    let n = notifier.0.lock().clone();
    assert_eq!(n.len(), 1, "only the long job notifies");
    assert_eq!(n[0].0, "Initial sync of Work finished");
    assert_eq!(n[0].1, "2 of 2 items done");
}

#[tokio::test]
async fn progress_is_throttled_and_the_final_value_is_persisted() {
    let h = harness(1);
    h.runner.register(JobKind::Embeddings, Arc::new(Chatty));
    let id = h.queue.enqueue(spec(JobKind::Embeddings, "embed")).unwrap();
    assert!(h.runner.run_until_idle(Duration::from_secs(5)).await);
    let progress_events = h
        .events
        .take()
        .iter()
        .filter(|e| matches!(e, RiddleEvent::JobProgress { .. }))
        .count();
    assert!(
        progress_events <= 1,
        "100 reports in under a second produce at most one event, got {progress_events}"
    );
    let job = repo::jobs::get(&h.stores.index(), &id).unwrap().unwrap();
    assert_eq!(job.progress.done, 100);
    assert_eq!(job.status, JobStatus::Completed);
}

#[tokio::test]
async fn the_task_centre_snapshot_derives_overall_status() {
    let h = harness(1);
    let runs = Arc::new(AtomicUsize::new(0));
    h.runner
        .register(JobKind::CatchUp, Arc::new(Waiter { runs: runs.clone() }));
    let digs = FixedDigStatuses(vec![DigStatusRow {
        dig_id: DigId::from("d"),
        name: "Work".into(),
        status: DigStatus::Synced,
    }]);
    let before = snapshot(&h.stores, &h.queue, &digs, h.clock.as_ref());
    assert_eq!(before.overall, OverallStatus::CaughtUp);
    assert!(before.jobs.is_empty());

    let id = h.queue.enqueue(spec(JobKind::CatchUp, "catch up")).unwrap();
    h.runner.tick();
    wait_for("running", || runs.load(Ordering::SeqCst) >= 1).await;
    let during = snapshot(&h.stores, &h.queue, &digs, h.clock.as_ref());
    assert_eq!(during.overall, OverallStatus::CatchingUp);
    assert_eq!(during.jobs[0].id, id);
    assert_eq!(during.jobs[0].status, JobStatus::Running);

    h.queue.pause_all(PauseReason::Offline);
    let paused = snapshot(&h.stores, &h.queue, &digs, h.clock.as_ref());
    assert_eq!(
        paused.jobs[0].status,
        JobStatus::Paused {
            reason: "offline".into()
        }
    );
    h.queue.resume_all();

    let degraded = FixedDigStatuses(vec![DigStatusRow {
        dig_id: DigId::from("d"),
        name: "Work".into(),
        status: DigStatus::Degraded {
            reason: "judge unavailable".into(),
        },
    }]);
    assert_eq!(
        snapshot(&h.stores, &h.queue, &degraded, h.clock.as_ref()).overall,
        OverallStatus::Degraded
    );
    h.queue.cancel(&id).unwrap();
}

#[tokio::test]
async fn one_job_per_dig_per_resource_class_at_a_time() {
    let h = harness(3);
    let runs = Arc::new(AtomicUsize::new(0));
    h.runner
        .register(JobKind::CatchUp, Arc::new(Waiter { runs: runs.clone() }));
    h.runner.register(
        JobKind::IncrementalSync,
        Arc::new(Waiter { runs: runs.clone() }),
    );
    h.runner
        .register(JobKind::Sift, Arc::new(Waiter { runs: runs.clone() }));
    let dig = DigId::from("d");
    let a = h
        .queue
        .enqueue(spec(JobKind::CatchUp, "catch up").dig(dig.clone()))
        .unwrap();
    let b = h
        .queue
        .enqueue(spec(JobKind::IncrementalSync, "incremental").dig(dig.clone()))
        .unwrap();
    let c = h
        .queue
        .enqueue(spec(JobKind::Sift, "sift").dig(dig.clone()))
        .unwrap();
    h.runner.tick();
    wait_for("two running", || runs.load(Ordering::SeqCst) >= 2).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    h.runner.tick();
    assert_eq!(
        h.runner.running_count(),
        2,
        "two provider jobs on one Dig cannot both run"
    );
    assert_eq!(
        h.queue.get(&b).unwrap().unwrap().status,
        JobStatus::Running,
        "incoming beats catch-up"
    );
    assert_eq!(
        h.queue.get(&c).unwrap().unwrap().status,
        JobStatus::Running,
        "a judge job on the same Dig runs alongside"
    );
    assert_eq!(h.queue.get(&a).unwrap().unwrap().status, JobStatus::Queued);
    for id in [&a, &b, &c] {
        let _ = h.queue.cancel(id);
    }
}

#[tokio::test]
async fn the_job_actions_go_through_the_queue() {
    let h = harness(1);
    let runs = Arc::new(AtomicUsize::new(0));
    h.runner
        .register(JobKind::Replay, Arc::new(Waiter { runs: runs.clone() }));
    let services = Arc::new(Services::new(
        h.stores.clone(),
        h.events.clone(),
        BuildStamp::from_env_vars("0", None, None, None, "test"),
    ));
    services.insert(h.queue.clone());
    let mut builder = riddle_actions::RuntimeBuilder::new(services);
    for a in actions() {
        builder = builder.register(a);
    }
    let rt = builder.build();
    let id = h.queue.enqueue(spec(JobKind::Replay, "replay")).unwrap();
    let paused = rt
        .invoke_as_user("pause_job", json!({ "jobId": id }))
        .await
        .unwrap();
    assert!(paused.activity_id.is_some());
    assert_eq!(
        h.queue.get(&id).unwrap().unwrap().status,
        JobStatus::Paused {
            reason: USER_PAUSE.into()
        }
    );
    rt.invoke_as_user("resume_job", json!({ "jobId": id }))
        .await
        .unwrap();
    assert_eq!(h.queue.get(&id).unwrap().unwrap().status, JobStatus::Queued);
    rt.invoke_as_user("cancel_job", json!({ "jobId": id }))
        .await
        .unwrap();
    assert_eq!(
        h.queue.get(&id).unwrap().unwrap().status,
        JobStatus::Cancelled
    );
    let err = rt
        .invoke_as_user("cancel_job", json!({ "jobId": "nope" }))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}
