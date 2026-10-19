//! The Task Centre snapshot (§55): one honest status line and the work
//! behind it. Overall status is derived, never set: Degraded when any Dig is
//! degraded or a store has faulted, Catching up while a catch-up or initial
//! sync runs (or a Dig says it is catching up), otherwise Caught up. The
//! morning summary reads this to say whether processing is complete.

use crate::queue::JobQueue;
use riddle_core::{Clock, DigStatus, DigStatusRow, JobKind, JobStatus, OverallStatus, TaskCentre};
use riddle_errors::Result;
use riddle_store::Stores;

/// Per-Dig status rows. The integrator derives them from sync state, the
/// outbox backlog, the Shadow Period and judge availability.
pub trait DigStatusSource: Send + Sync {
    fn dig_statuses(&self) -> Result<Vec<DigStatusRow>>;
}

/// A fixed list, for tests and the fixture profile.
pub struct FixedDigStatuses(pub Vec<DigStatusRow>);
impl DigStatusSource for FixedDigStatuses {
    fn dig_statuses(&self) -> Result<Vec<DigStatusRow>> {
        Ok(self.0.clone())
    }
}

pub const CATCH_UP_KINDS: [JobKind; 2] = [JobKind::CatchUp, JobKind::InitialSync];

pub fn overall_status(
    digs: &[DigStatusRow],
    jobs: &[riddle_core::Job],
    store_fault: Option<&str>,
) -> OverallStatus {
    if store_fault.is_some()
        || digs.iter().any(|d| {
            matches!(
                d.status,
                DigStatus::Degraded { .. } | DigStatus::SyncFailing { .. }
            )
        })
    {
        return OverallStatus::Degraded;
    }
    let catching_up_job = jobs.iter().any(|j| {
        CATCH_UP_KINDS.contains(&j.kind)
            && matches!(
                j.status,
                JobStatus::Running | JobStatus::Queued | JobStatus::Paused { .. }
            )
    });
    let catching_up_dig = digs.iter().any(|d| {
        matches!(
            d.status,
            DigStatus::CatchingUp { .. } | DigStatus::Sifting { .. }
        )
    });
    if catching_up_job || catching_up_dig {
        OverallStatus::CatchingUp
    } else {
        OverallStatus::CaughtUp
    }
}

pub fn snapshot(
    stores: &Stores,
    queue: &JobQueue,
    digs: &dyn DigStatusSource,
    clock: &dyn Clock,
) -> TaskCentre {
    // A status source that fails must not blank the Task Centre; the jobs
    // and the fault flag still tell the truth.
    let digs = digs.dig_statuses().unwrap_or_else(|e| {
        riddle_observability::rerror!(
            riddle_observability::Category::Jobs,
            "dig statuses unavailable: {}",
            e.message
        );
        vec![]
    });
    let jobs = queue.active();
    let store_fault = stores.any_fault();
    let overall = overall_status(&digs, &jobs, store_fault.as_deref());
    TaskCentre {
        overall,
        digs,
        jobs,
        store_fault,
        // The app fills these from the judge and LLM clients it holds.
        judge: None,
        llm: None,
        as_of: clock.now(),
        booting: None,
        boot_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::{DigId, Job, JobId, Progress};

    fn row(status: DigStatus) -> DigStatusRow {
        DigStatusRow {
            dig_id: DigId::from("d"),
            name: "Work".into(),
            status,
        }
    }
    fn job(kind: JobKind, status: JobStatus) -> Job {
        let now = chrono::Utc::now();
        Job {
            id: JobId::new(),
            kind,
            priority: kind.default_priority(),
            status,
            dig_id: None,
            label: "x".into(),
            progress: Progress::default(),
            checkpoint: serde_json::Value::Null,
            cancellable: true,
            attempts: 0,
            created_at: now,
            started_at: None,
            finished_at: None,
            updated_at: now,
        }
    }

    #[test]
    fn a_degraded_dig_or_a_store_fault_wins_over_everything() {
        assert_eq!(
            overall_status(
                &[row(DigStatus::Degraded {
                    reason: "judge".into()
                })],
                &[job(JobKind::CatchUp, JobStatus::Running)],
                None
            ),
            OverallStatus::Degraded
        );
        assert_eq!(
            overall_status(&[row(DigStatus::Synced)], &[], Some("fault")),
            OverallStatus::Degraded
        );
        // An account that is not syncing is never "Caught up".
        assert_eq!(
            overall_status(
                &[row(DigStatus::SyncFailing {
                    failure: riddle_core::SyncFailure::SignIn,
                    since: chrono::Utc::now(),
                    last_synced_at: None,
                })],
                &[],
                None
            ),
            OverallStatus::Degraded
        );
    }

    #[test]
    fn a_running_catch_up_or_initial_sync_means_catching_up() {
        assert_eq!(
            overall_status(
                &[row(DigStatus::Synced)],
                &[job(JobKind::InitialSync, JobStatus::Queued)],
                None
            ),
            OverallStatus::CatchingUp
        );
        assert_eq!(
            overall_status(
                &[row(DigStatus::Synced)],
                &[job(JobKind::Embeddings, JobStatus::Running)],
                None
            ),
            OverallStatus::CaughtUp
        );
        assert_eq!(
            overall_status(
                &[row(DigStatus::CatchingUp {
                    remaining: 3,
                    total: 9
                })],
                &[],
                None
            ),
            OverallStatus::CatchingUp
        );
        assert_eq!(
            overall_status(
                &[row(DigStatus::Synced)],
                &[job(JobKind::CatchUp, JobStatus::Completed)],
                None
            ),
            OverallStatus::CaughtUp
        );
    }

    #[test]
    fn a_running_history_backfill_leaves_an_account_with_its_recent_mail_sorted_caught_up() {
        assert_eq!(
            overall_status(
                &[row(DigStatus::Synced)],
                &[job(JobKind::HistoryBackfill, JobStatus::Running)],
                None
            ),
            OverallStatus::CaughtUp
        );
    }
}
