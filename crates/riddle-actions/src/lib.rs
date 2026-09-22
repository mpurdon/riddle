//! The Action layer: Riddle's capability boundary (ARCHITECTURE.md §8).
//!
//! An Action owns its input and output contracts, validation, authorization,
//! execution, risk metadata, idempotency, audit and undo. The UI, the CLI, the
//! Director and the MCP server all call `Runtime::invoke`; nothing else
//! mutates the store on a principal's behalf. Prompts are not authorization:
//! the principal and the per-Dig autonomy decide, here.
//!
//! This module holds the trait, the registry and the invocation path. The
//! concrete actions live in `actions::*`, one module per domain.

pub mod actions;
pub mod policy;
pub mod runtime;

use async_trait::async_trait;
use riddle_core::{ActionMeta, ActivityResult, EntityRef, Principal, Provenance, UndoInfo};
use riddle_errors::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub use runtime::{Runtime, RuntimeBuilder, Services};

/// What an Action hands back. The runtime turns it into an Activity.
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub value: Value,
    /// Human sentence for the Activity: "Moved 14 messages to Spoil".
    pub summary: String,
    pub why: Option<String>,
    pub affected: Vec<EntityRef>,
    pub undo: UndoInfo,
    pub provenance: Option<Provenance>,
    pub result: ActivityResult,
    /// Redacted inputs for the Activity: identifiers and counts only.
    pub inputs: Value,
    /// Set by `undo_activity`: the Activity this result undid, so the
    /// runtime records `undoes` on the Activity it appends.
    pub undoes: Option<riddle_core::ActivityId>,
}

impl ActionResult {
    pub fn ok(value: Value, summary: impl Into<String>) -> Self {
        Self {
            value,
            summary: summary.into(),
            why: None,
            affected: vec![],
            undo: UndoInfo::NotUndoable {
                reason: "read-only".into(),
            },
            provenance: None,
            result: ActivityResult::Ok,
            inputs: Value::Null,
            undoes: None,
        }
    }
    pub fn with_undoes(mut self, undoes: riddle_core::ActivityId) -> Self {
        self.undoes = Some(undoes);
        self
    }
    pub fn with_affected(mut self, affected: Vec<EntityRef>) -> Self {
        self.affected = affected;
        self
    }
    pub fn with_undo(mut self, restore: Value, label: impl Into<String>) -> Self {
        self.undo = UndoInfo::Undoable {
            restore,
            label: label.into(),
        };
        self
    }
    pub fn with_provenance(mut self, p: Provenance) -> Self {
        self.provenance = Some(p);
        self
    }
    pub fn with_inputs(mut self, inputs: Value) -> Self {
        self.inputs = inputs;
        self
    }
    pub fn with_why(mut self, why: impl Into<String>) -> Self {
        self.why = Some(why.into());
        self
    }
    pub fn partial(mut self, skipped: Vec<riddle_core::SkippedEntity>) -> Self {
        if !skipped.is_empty() {
            self.result = ActivityResult::Partial { skipped };
        }
        self
    }
}

/// What the confirmation dialog shows: the Action layer's real arguments.
#[derive(Debug, Clone)]
pub struct Preview {
    pub summary: String,
    pub real_arguments: Value,
}

/// The context an Action executes in.
pub struct ActionContext {
    pub services: Arc<Services>,
    pub principal: Principal,
    pub dig_id: Option<riddle_core::DigId>,
    pub idempotency_key: Option<String>,
    /// The Activity id the runtime will assign, so Actions can reference it
    /// in corrections they record.
    pub activity_id: riddle_core::ActivityId,
}

#[async_trait]
pub trait Action: Send + Sync {
    fn meta(&self) -> ActionMeta;
    /// The confirmation tier for these particular arguments. Defaults to the
    /// static policy in `meta()`; an Action whose risk depends on its input
    /// (`save_settings` is PressHold only when it raises a Dig to Trusted)
    /// overrides it. The runtime consults this before deciding.
    fn confirmation_for(
        &self,
        _ctx: &ActionContext,
        _args: &Value,
    ) -> riddle_core::ConfirmationPolicy {
        self.meta().confirmation
    }
    /// Validate and describe what would really happen, for confirmation.
    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        Ok(Preview {
            summary: self.meta().description,
            real_arguments: args.clone(),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult>;
    /// Reverse a previous execution from its restoration data.
    async fn undo(&self, _ctx: &ActionContext, _restore: Value) -> Result<ActionResult> {
        Err(riddle_errors::RiddleError::invalid(format!(
            "{} cannot be undone",
            self.meta().name
        )))
    }
}

#[derive(Default)]
pub struct Registry {
    actions: HashMap<String, Arc<dyn Action>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, action: Arc<dyn Action>) {
        let name = action.meta().name;
        self.actions.insert(name, action);
    }
    pub fn get(&self, name: &str) -> Option<Arc<dyn Action>> {
        self.actions.get(name).cloned()
    }
    pub fn list(&self) -> Vec<ActionMeta> {
        let mut v: Vec<ActionMeta> = self.actions.values().map(|a| a.meta()).collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }
    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.actions.keys().cloned().collect();
        v.sort();
        v
    }
}

/// Helper for declaring metadata tersely in action modules.
pub fn meta(name: &str, description: &str) -> ActionMetaBuilder {
    ActionMetaBuilder(ActionMeta {
        name: name.into(),
        description: description.into(),
        mutating: false,
        reversible: false,
        external_side_effect: false,
        risk: riddle_core::Risk::Low,
        confirmation: riddle_core::ConfirmationPolicy::None,
        minimum_autonomy: riddle_core::Autonomy::Observe,
        egress: false,
        read_only: true,
    })
}

pub struct ActionMetaBuilder(ActionMeta);
impl ActionMetaBuilder {
    pub fn mutating(mut self) -> Self {
        self.0.mutating = true;
        self.0.read_only = false;
        self
    }
    pub fn reversible(mut self) -> Self {
        self.0.reversible = true;
        self
    }
    pub fn external(mut self) -> Self {
        self.0.external_side_effect = true;
        self
    }
    pub fn risk(mut self, r: riddle_core::Risk) -> Self {
        self.0.risk = r;
        self
    }
    pub fn confirm(mut self, c: riddle_core::ConfirmationPolicy) -> Self {
        self.0.confirmation = c;
        self
    }
    pub fn autonomy(mut self, a: riddle_core::Autonomy) -> Self {
        self.0.minimum_autonomy = a;
        self
    }
    pub fn egress(mut self) -> Self {
        self.0.egress = true;
        self
    }
    pub fn build(self) -> ActionMeta {
        self.0
    }
}
