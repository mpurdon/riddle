//! Sync: initial, incremental, history backfill and full resync per Dig,
//! and attribution of provider-side changes (`ARCHITECTURE.md` §5, §15,
//! §53). The loop that runs them is the job runner (`riddle-jobs`, driven
//! by `riddle-app`'s sync jobs): the crate's own poll loop, `run_poller`,
//! had no caller but its test once the runner took over the timer, the
//! refresh trigger, back-off and parking, so it went (17 Oct 2026).

pub mod actions;
pub mod engine;
#[cfg(test)]
mod tests;

pub use actions::actions;
pub use engine::{
    CancelFlag, Difference, ExternalMove, SyncEngine, SyncKind, SyncOptions, SyncPhase,
    SyncProgress, SyncReport,
};
