//! `refresh_now`: wake the poller(s). The Notify lives in `riddle-mail`'s
//! `RefreshTriggers` so Demo Mode's injection can wake the same loop.

use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult};
use riddle_core::{ActionMeta, Autonomy, DigId, Risk};
use riddle_errors::Result;
use riddle_mail::actions::util::opt;
use riddle_mail::RefreshTriggers;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct RefreshNow;
#[async_trait]
impl Action for RefreshNow {
    fn meta(&self) -> ActionMeta {
        // Mutating only in the sense that it schedules work; nothing in the store changes.
        meta("refresh_now", "Sync now")
            .mutating()
            .risk(Risk::Low)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let dig: Option<DigId> = opt(&args, "digId")?;
        let dig = dig.or_else(|| ctx.dig_id.clone());
        let triggers = ctx.services.require::<RefreshTriggers>()?;
        let woken = triggers.trigger(dig.as_ref());
        let mut r = ActionResult::ok(
            json!({"woken": woken}),
            if woken == 1 {
                "Refreshing".to_string()
            } else {
                format!("Refreshing {woken} Digs")
            },
        )
        .with_inputs(json!({"digId": dig}));
        r.undo = riddle_core::UndoInfo::NotUndoable {
            reason: "a refresh only reads".into(),
        };
        Ok(r)
    }
}

pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![Arc::new(RefreshNow)]
}
