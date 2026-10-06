//! Meshes: versioned policy over facts and Signals (ARCHITECTURE.md §2,
//! §36–§38). The text DSL is what the user reads and what the Director
//! proposes; the parser turns it into the AST in `riddle_core::mesh`, the
//! evaluator applies scope resolution and guards, the simulator replays a
//! candidate over cached Signals, and the shadow helpers keep the score a
//! Mesh must earn before it acts (§51).
//!
//! Two decisions the evaluator makes that are easy to get wrong: a route
//! whose Signal is missing is *unknown*, not false, so an `ELSE` below it
//! cannot fire on absent evidence (three-valued logic, the reason judge
//! outages produce Unresolved and never Spoil); and every guard from every
//! scope runs on the routed result, because a guard is the user's veto and a
//! more specific Mesh must not be able to route around it (§36 step 3).

pub mod actions;
pub mod defaults;
pub mod eval;
pub mod format;
pub mod humanize;
pub mod order;
pub mod packs;
pub mod parse;
pub mod policy;
pub mod service;
pub mod shadow;
pub mod simulate;
pub mod validate;

pub use eval::{
    check_routes, deterministic_projection, evaluate, evaluate_with, signals_needed,
    ListMembership, MeshDecision, ProfileLists, Truth,
};
pub use format::format;
pub use parse::{parse, parse_full, ParsedHeader};
pub use policy::ConfidencePolicy;
pub use service::MeshService;
pub use simulate::{simulate, SimulationInput};
pub use validate::{validate, RegistryResolver, SignalResolver};

/// The Actions this crate exposes, for registration by the integrator.
pub fn actions() -> Vec<std::sync::Arc<dyn riddle_actions::Action>> {
    actions::all()
}
