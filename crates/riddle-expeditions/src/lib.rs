//! Expeditions (`ARCHITECTURE.md` §42): durable delegated work over the
//! mailbox. An Expedition declares its goal, scope (Digs, date range, tools)
//! and budget (cost, seconds, tool calls) before it starts; the runner plans
//! it with one model call, executes the steps with the Director's tool loop,
//! checkpoints after every step, withdraws tools when the budget is spent
//! and finishes as `Partial`, honours cancellation, and resumes from the
//! checkpoint after a restart. Results become findings on the Expedition
//! and, when the `create_find` Action is registered, Finds of type Know.
//!
//! Why the checkpoint is the unit of durability: the machine sleeps, the app
//! is closed, the model provider blips. A step that completed is never
//! repeated; a step that did not is repeated from its start. Findings are
//! written only at the end, from the recorded step summaries, so a resumed
//! Expedition concludes from the same evidence a continuous one would.
//!
//! Security: an Expedition runs as `Principal::Expedition` with the same
//! untrusted marking and egress gating as the Director (§58). Its tool list
//! is the read tools plus whatever `scope.tools_allowed` names; mutating
//! Actions outside that list are never offered.

pub mod actions;
pub mod runner;

pub use actions::{actions, CancelExpedition, CreateFind, StartExpedition};
pub use runner::{ExpeditionConfig, ExpeditionHandle, ExpeditionRunner};
