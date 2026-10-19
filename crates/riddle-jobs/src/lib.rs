//! riddle-jobs: durable background work and the Task Centre
//! (ARCHITECTURE.md §5, §54, §55, §56).
//!
//! The pieces: `JobQueue` (durable, priority-ordered, deduped), `Runner`
//! (the scheduler and the `JobHandler` contract with cooperative
//! checkpoints), `Resources` (priority-aware rate limiters and the budget
//! gate), `task_centre::snapshot`, `SleepDetector` and
//! `ConnectivityMonitor` (the pause reasons that come from the machine, not
//! the user), and the three job-control Actions.
//!
//! Integrator seams: `JobHandler` per `JobKind`, `DigStatusSource`,
//! `Connectivity` (a default DNS probe exists), `Notifier`, `BudgetGate`
//! (`UsageBudgetGate` reads the usage ledger), `WakeHook`. Register the
//! `JobQueue` in `Services` so the Actions can reach it.

pub mod actions;
pub mod connectivity;
pub mod queue;
pub mod resources;
pub mod runner;
pub mod sleep;
pub mod spec;
pub mod task_centre;

pub use actions::actions;
pub use connectivity::{Connectivity, ConnectivityMonitor, DnsProbe, FixedConnectivity};
pub use queue::JobQueue;
pub use resources::{BudgetClass, BudgetGate, Resources, UnlimitedBudget, UsageBudgetGate};
pub use runner::{
    CancelToken, JobContext, JobHandler, JobOutcome, Notifier, NullNotifier, ProgressReporter,
    RecordingNotifier, Runner, DEFAULT_MAX_CONCURRENT, MAX_ATTEMPTS,
};
pub use sleep::{SleepDetector, WakeHook, WakeReason, SLEEP_GAP};
pub use spec::{
    is_long_running, payload_hash, resource_class, JobEnvelope, JobSpec, PauseReason,
    ResourceClass, USER_PAUSE,
};
pub use task_centre::{snapshot, DigStatusSource, FixedDigStatuses};
