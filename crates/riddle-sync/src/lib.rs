//! Sync: initial, incremental and full resync per Dig, attribution of
//! provider-side changes, and the poll loop (`ARCHITECTURE.md` §5, §15, §53).

pub mod actions;
pub mod engine;
pub mod poller;
#[cfg(test)]
mod tests;

pub use actions::actions;
pub use engine::{
    CancelFlag, Difference, ExternalMove, SyncEngine, SyncKind, SyncOptions, SyncPhase,
    SyncProgress, SyncReport,
};
pub use poller::{run_poller, PollerConfig, PollerHooks};
