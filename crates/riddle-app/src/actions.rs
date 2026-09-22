//! Actions that belong to the assembled application rather than one
//! domain crate.

use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use serde_json::{json, Value};
use std::sync::Arc;

/// Permanent delete: the provider trashes the messages and Riddle forgets
/// them. Typing the exact count is the confirmation (§51); there is no
/// undo, and the Action says so before it runs.
pub struct PermanentDelete;

#[async_trait]
impl Action for PermanentDelete {
    fn meta(&self) -> ActionMeta {
        meta("permanent_delete", "Delete messages permanently (trash at the provider, forget locally)")
            .mutating()
            .external()
            .risk(Risk::Critical)
            .confirm(ConfirmationPolicy::TypeCount)
            .autonomy(Autonomy::Trusted)
            .build()
    }

    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let ids: Vec<MessageId> = serde_json::from_value(args.get("messageIds").cloned().unwrap_or(json!([])))?;
        Ok(Preview { summary: format!("Permanently delete {} messages. This cannot be undone.", ids.len()), real_arguments: json!({ "count": ids.len(), "messageIds": ids }) })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let dig_id: DigId = serde_json::from_value(args.get("digId").cloned().unwrap_or(Value::Null)).map_err(|_| RiddleError::invalid("digId required"))?;
        let ids: Vec<MessageId> = serde_json::from_value(args.get("messageIds").cloned().unwrap_or(json!([])))?;
        if ids.is_empty() {
            return Err(RiddleError::invalid("no messages given"));
        }
        let dig = repo::digs::get(&ctx.services.stores.index(), &dig_id)?.ok_or_else(|| RiddleError::not_found("dig"))?;
        let domains = ctx.services.require::<crate::domains::Domains>()?;
        let provider = domains.providers.provider_for(&dig)?;
        let provider_ids: Vec<String> = ids.iter().map(|i| i.provider_id().to_string()).collect();
        provider.trash(&provider_ids).await?;
        let store = ctx.services.stores.dig(&dig_id)?;
        for id in &ids {
            repo::messages::mark_deleted(&store, id)?;
        }
        let affected = ids.iter().map(|i| EntityRef::new(EntityKind::Message, i)).collect();
        let mut r = ActionResult::ok(json!({ "deleted": ids.len() }), format!("Permanently deleted {} messages", ids.len()))
            .with_affected(affected)
            .with_inputs(json!({ "digId": dig_id, "count": ids.len() }));
        r.undo = UndoInfo::NotUndoable { reason: "permanent deletion has no undo".into() };
        Ok(r)
    }
}

pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![Arc::new(PermanentDelete)]
}
