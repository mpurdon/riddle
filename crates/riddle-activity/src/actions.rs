//! `undo_activity`: the one Action behind every Undo button (§50). In-place
//! confirmation; the preview lists what will and will not be restored. The
//! Action reaches the runtime through `RuntimeHandle` (registered in
//! `Services` by the integrator, like the settings crate's `ArmControl`),
//! since an Action only sees its context.

use crate::undo::UndoEngine;
use async_trait::async_trait;
use riddle_actions::policy::ensure_args;
use riddle_actions::runtime::Runtime;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview};
use riddle_core::{ActionMeta, ActivityId, Autonomy, ConfirmationPolicy, Risk};
use riddle_errors::{Result, RiddleError};
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Weak};

pub struct RuntimeHandle(Weak<Runtime>);

impl RuntimeHandle {
    pub fn new(runtime: &Arc<Runtime>) -> Self {
        Self(Arc::downgrade(runtime))
    }
    pub fn runtime(&self) -> Result<Arc<Runtime>> {
        self.0
            .upgrade()
            .ok_or_else(|| RiddleError::internal("runtime has gone away"))
    }
}

pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![Arc::new(UndoActivity)]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Args {
    activity_id: ActivityId,
}

pub struct UndoActivity;

#[async_trait]
impl Action for UndoActivity {
    fn meta(&self) -> ActionMeta {
        meta("undo_activity", "Undo an Activity")
            .mutating()
            .risk(Risk::Medium)
            .confirm(ConfirmationPolicy::InPlaceUndo)
            .autonomy(Autonomy::Act)
            .changes_rules()
            .build()
    }

    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let a: Args = ensure_args(args)?;
        let engine = ctx.services.require::<UndoEngine>()?;
        let p = engine.preview(&ctx.services.stores, &a.activity_id).await?;
        let summary = match (p.will_skip.len(), p.conflicts.len()) {
            (0, 0) => format!("Undo: {}", p.summary),
            (s, c) => format!(
                "Undo: {} — restores {}, skips {}",
                p.summary,
                p.will_restore.len(),
                s + c
            ),
        };
        Ok(Preview {
            summary,
            real_arguments: serde_json::to_value(&p)?,
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: Args = ensure_args(&args)?;
        let engine = ctx.services.require::<UndoEngine>()?;
        let runtime = ctx.services.require::<RuntimeHandle>()?.runtime()?;
        let (_report, result) = engine.undo_within(&runtime, &a.activity_id, ctx).await?;
        Ok(result)
    }
}
