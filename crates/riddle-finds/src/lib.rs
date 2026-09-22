//! Finds as first-class entities (ARCHITECTURE.md §41) and Temporal Sift
//! (§19). A Find is derived from messages, joined across Digs by its
//! identity key, and has a lifecycle the service validates. Temporal Sift is
//! message-free: it reads Thread State and Finds and changes Finds because
//! time passed (snooze ended, a dated Find expired, an ask of mine went
//! unanswered) or because the thread changed (my reply from any client
//! resolves a Respond Find).
//!
//! The dismiss path records a `Correction { kind: Dismiss }` because a
//! dismissed Find is the user telling Riddle the Find was wrong (§46), and
//! undo retracts it.

pub mod actions;
pub mod service;
pub mod temporal;

pub use service::FindService;
pub use temporal::{TemporalOptions, TemporalOutcome, TemporalOutcomeKind};

/// The Actions this crate exposes, for registration by the integrator.
pub fn actions() -> Vec<std::sync::Arc<dyn riddle_actions::Action>> {
    actions::all()
}
