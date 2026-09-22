//! The timing policy for Temporal Sift and the other clock-driven work
//! (§19, §55). It decides *what is due*, with last-run bookkeeping in the
//! index store's `kv` table; the job queue (Track D) decides when and how it
//! runs. Intervals are data so the Task Centre and tests can read them, and
//! the clock is injected so a test never sleeps.

use riddle_core::Clock;
use riddle_errors::Result;
use riddle_observability::{rdebug, Category};
use riddle_store::{repo, Store};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Task {
    /// Snooze expiry, dated Find expiry, Follow Ups, Respond resolution.
    TemporalTick,
    /// Promote Shadow Meshes that earned ACT; refresh the Shadow summary.
    ShadowPeriodCheck,
    /// Re-evaluate Context-dependent Signals whose slice changed (§26).
    ContextReevaluation,
    /// Store checkpoint and vacuum.
    StoreMaintenance,
}

impl Task {
    pub const ALL: [Task; 4] = [
        Task::TemporalTick,
        Task::ShadowPeriodCheck,
        Task::ContextReevaluation,
        Task::StoreMaintenance,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Task::TemporalTick => "temporal_tick",
            Task::ShadowPeriodCheck => "shadow_period_check",
            Task::ContextReevaluation => "context_reevaluation",
            Task::StoreMaintenance => "store_maintenance",
        }
    }

    fn kv_key(self) -> String {
        format!("scheduler:last_run:{}", self.as_str())
    }
}

/// Intervals between runs, in seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub temporal_tick_secs: u64,
    pub shadow_period_check_secs: u64,
    pub context_reevaluation_secs: u64,
    pub store_maintenance_secs: u64,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            temporal_tick_secs: 15 * 60,
            shadow_period_check_secs: 24 * 60 * 60,
            context_reevaluation_secs: 60 * 60,
            store_maintenance_secs: 24 * 60 * 60,
        }
    }
}

impl Schedule {
    pub fn interval(&self, task: Task) -> chrono::Duration {
        let secs = match task {
            Task::TemporalTick => self.temporal_tick_secs,
            Task::ShadowPeriodCheck => self.shadow_period_check_secs,
            Task::ContextReevaluation => self.context_reevaluation_secs,
            Task::StoreMaintenance => self.store_maintenance_secs,
        };
        chrono::Duration::seconds(secs as i64)
    }
}

pub fn last_run(kv: &Store, task: Task) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    Ok(repo::kv::get(kv, &task.kv_key())?.and_then(|s| s.parse().ok()))
}

pub fn mark_ran(kv: &Store, task: Task, at: chrono::DateTime<chrono::Utc>) -> Result<()> {
    repo::kv::set(kv, &task.kv_key(), &at.to_rfc3339())
}

/// The tasks whose interval has elapsed since their last run (a task that
/// never ran is due). Order is `Task::ALL`, temporal first.
pub fn due_now(kv: &Store, clock: &dyn Clock, schedule: &Schedule) -> Result<Vec<Task>> {
    let now = clock.now();
    let mut due = Vec::new();
    for task in Task::ALL {
        let is_due = match last_run(kv, task)? {
            None => true,
            Some(last) => now - last >= schedule.interval(task),
        };
        if is_due {
            due.push(task);
        }
    }
    rdebug!(Category::Jobs, "scheduler: {} tasks due", due.len());
    Ok(due)
}

/// When each task next runs, for the Task Centre.
pub fn next_runs(
    kv: &Store,
    schedule: &Schedule,
) -> Result<Vec<(Task, Option<chrono::DateTime<chrono::Utc>>)>> {
    let mut out = Vec::new();
    for task in Task::ALL {
        out.push((
            task,
            last_run(kv, task)?.map(|l| l + schedule.interval(task)),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::FixedClock;
    use riddle_store::StoreKind;

    #[test]
    fn tasks_are_due_when_never_run_and_again_after_their_interval() {
        let kv = Store::in_memory(StoreKind::Index).unwrap();
        let clock = FixedClock::at(chrono::Utc::now());
        let schedule = Schedule::default();
        assert_eq!(due_now(&kv, &clock, &schedule).unwrap(), Task::ALL.to_vec());
        for t in Task::ALL {
            mark_ran(&kv, t, clock.now()).unwrap();
        }
        assert!(due_now(&kv, &clock, &schedule).unwrap().is_empty());
        clock.advance(chrono::Duration::minutes(16));
        assert_eq!(
            due_now(&kv, &clock, &schedule).unwrap(),
            vec![Task::TemporalTick]
        );
        clock.advance(chrono::Duration::hours(24));
        let due = due_now(&kv, &clock, &schedule).unwrap();
        assert!(due.contains(&Task::ShadowPeriodCheck) && due.contains(&Task::ContextReevaluation));
        let next = next_runs(&kv, &schedule).unwrap();
        assert!(next.iter().all(|(_, n)| n.is_some()));
    }
}
