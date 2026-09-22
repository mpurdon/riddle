//! Projection: Riddle's decisions written into the provider through an
//! outbox (`ARCHITECTURE.md` §15), and the user's own moves as Actions.

pub mod actions;
pub mod outbox;
pub mod plan;
#[cfg(test)]
mod tests;

pub use actions::{actions, RestoreState};
pub use outbox::{
    bootstrap_managed_labels, enqueue, enqueue_classification, expected_managed_labels,
    undo_projection, Conflict, LabelResolver, OutboxProcessor, ProcessReport, ProjectionOp,
};
pub use plan::{
    expected_after, plan, stratum_label, ProjectionPlan, CONTAMINATION_LABEL, FILED_LABEL,
    SPOIL_LABEL, VOLATILE_LABELS,
};
