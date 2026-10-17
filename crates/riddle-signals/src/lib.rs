//! Semantic Signals as managed domain objects (ARCHITECTURE.md §22–§32).
//!
//! The system vocabulary, the Signal Sets that decide what state the judge
//! sees, the registry that versions and promotes Signals, the calibration
//! that turns a raw score into something a Mesh threshold can trust, the
//! test harness that runs before promotion, and the cache that makes a Mesh
//! change free when the Signals it needs were already evaluated.
//!
//! Two decisions shape the crate. Definitions are the prompt, so the
//! authoring rules are enforced by `Signal::validate` at proposal time rather
//! than reviewed by hand (§23): a definition with a negation reads
//! differently to a literal judge, and the incident that taught it was a
//! `not_spam` Signal that scored spam high. And a Signal that declares
//! message-only inputs is cached forever (§26): the cache key carries the
//! engine identity so a model change invalidates everything at once instead
//! of mixing two models' scores under one threshold.

pub mod actions;
pub mod cache;
pub mod calibration;
pub mod registry;
pub mod sets;
pub mod testing;
pub mod vocabulary;

pub use cache::{cached_or_evaluate, CacheOutcome};
pub use calibration::{fit_platt, LabelledSet, PlattFit};
pub use registry::{PromoteOptions, Registry, RegistrySnapshot};
pub use sets::{assign_to_sets, select_sets, BuiltState, SetId, StateBuilder, StateInput};
pub use testing::{test_signal, AdversarialExample, SignalCorpus, SignalTestReport, TestSample};
pub use vocabulary::{system_signal, system_signal_names, system_signals};

/// The Actions this crate exposes, for registration by the integrator.
pub fn actions() -> Vec<std::sync::Arc<dyn riddle_actions::Action>> {
    actions::all()
}
