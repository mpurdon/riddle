//! Find Actions: resolve, dismiss, snooze, retype. All reversible; dismiss
//! records a correction that undo retracts. `close_finds` is the ledger's
//! handle on Finds a rule closed (a newer one in the series replaced them,
//! or they expired), so that closing is undone from Activity like the rest.

use crate::service::FindService;
use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn all() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(ResolveFind),
        Arc::new(DismissFind),
        Arc::new(SnoozeFind),
        Arc::new(RetypeFind),
        Arc::new(CloseFinds),
    ]
}

fn service(ctx: &ActionContext) -> FindService {
    FindService::new(
        ctx.services.stores.clone(),
        ctx.services.clock.clone(),
        ctx.services.events.clone(),
    )
}

fn find_id(args: &Value) -> Result<FindId> {
    args.get("findId")
        .and_then(|v| v.as_str())
        .map(|s| FindId(s.to_string()))
        .ok_or_else(|| RiddleError::invalid("missing argument findId"))
}

fn find_ref(f: &Find) -> EntityRef {
    EntityRef::new(EntityKind::Find, &f.id)
}

fn resolved_by(p: &Principal) -> ResolvedBy {
    match p {
        Principal::Director { .. } | Principal::Expedition { .. } => ResolvedBy::Director,
        Principal::Temporal => ResolvedBy::Temporal,
        _ => ResolvedBy::User,
    }
}

pub struct ResolveFind;

#[async_trait]
impl Action for ResolveFind {
    fn meta(&self) -> ActionMeta {
        meta("resolve_find", "Mark a Find as dealt with")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = find_id(&args)?;
        let f = service(ctx).resolve(&id, resolved_by(&ctx.principal))?;
        Ok(
            ActionResult::ok(serde_json::to_value(&f)?, format!("Resolved: {}", f.title))
                .with_affected(vec![find_ref(&f)])
                .with_inputs(json!({ "findId": f.id }))
                .with_undo(json!({ "findId": f.id }), "Reopen"),
        )
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let f = service(ctx).reopen(&find_id(&restore)?)?;
        Ok(
            ActionResult::ok(serde_json::to_value(&f)?, format!("Reopened: {}", f.title))
                .with_affected(vec![find_ref(&f)]),
        )
    }
}

pub struct DismissFind;

#[async_trait]
impl Action for DismissFind {
    fn meta(&self) -> ActionMeta {
        meta("dismiss_find", "Dismiss a Find as not important")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = find_id(&args)?;
        let (f, correction) = service(ctx).dismiss(&id, Some(&ctx.activity_id))?;
        Ok(ActionResult::ok(serde_json::to_value(&f)?, format!("Dismissed: {}", f.title))
            .with_affected(vec![find_ref(&f)])
            .with_inputs(json!({ "findId": f.id }))
            .with_undo(json!({ "findId": f.id, "activityId": ctx.activity_id, "correctionId": correction, "digId": f.sources.first().map(|s| s.dig_id.clone()) }), "Restore the Find"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let f = service(ctx).reopen(&find_id(&restore)?)?;
        if let (Some(activity), Some(dig)) = (
            restore.get("activityId").and_then(|v| v.as_str()),
            restore.get("digId").and_then(|v| v.as_str()),
        ) {
            if let Ok(store) = ctx.services.stores.dig(&DigId(dig.to_string())) {
                if let Err(e) = repo::corrections::retract_by_activity(
                    &store,
                    &ActivityId(activity.to_string()),
                ) {
                    riddle_observability::rwarn!(
                        riddle_observability::Category::Actions,
                        "could not retract dismiss correction: {}",
                        e.message
                    );
                }
            }
        }
        Ok(
            ActionResult::ok(serde_json::to_value(&f)?, format!("Restored: {}", f.title))
                .with_affected(vec![find_ref(&f)]),
        )
    }
}

pub struct SnoozeFind;

#[async_trait]
impl Action for SnoozeFind {
    fn meta(&self) -> ActionMeta {
        meta("snooze_find", "Snooze a Find until a time")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = find_id(&args)?;
        let until = required_future_timestamp(&args, "until", ctx.services.clock.now())?;
        let f = service(ctx).snooze(&id, until)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&f)?,
            format!(
                "Snoozed until {}: {}",
                until.format("%Y-%m-%d %H:%M"),
                f.title
            ),
        )
        .with_affected(vec![find_ref(&f)])
        .with_inputs(json!({ "findId": f.id, "until": until }))
        .with_undo(json!({ "findId": f.id }), "Unsnooze"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let f = service(ctx).reopen(&find_id(&restore)?)?;
        Ok(
            ActionResult::ok(serde_json::to_value(&f)?, format!("Unsnoozed: {}", f.title))
                .with_affected(vec![find_ref(&f)]),
        )
    }
}

pub struct RetypeFind;

#[async_trait]
impl Action for RetypeFind {
    fn meta(&self) -> ActionMeta {
        meta("retype_find", "Change the type of a Find")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = find_id(&args)?;
        let ft = args.get("findType").and_then(|v| v.as_str()).and_then(FindType::parse).ok_or_else(|| RiddleError::invalid("findType must be one of respond, decide, review, do, pay, attend, follow_up, know"))?;
        let svc = service(ctx);
        let prior = svc.get(&id)?;
        let f = svc.retype(&id, ft)?;
        if let Some(src) = f.sources.first() {
            let (mesh_id, mesh_version) = service(ctx).deciding_mesh(&src.dig_id, &src.message_id);
            let correction = Correction {
                id: CorrectionId::new(),
                kind: CorrectionKind::Retype,
                dig_id: src.dig_id.clone(),
                message_id: src.message_id.clone(),
                from_disposition: Some(Disposition::Find {
                    find_type: prior.find_type,
                }),
                to_disposition: Some(Disposition::Find { find_type: ft }),
                from_find_type: Some(prior.find_type),
                to_find_type: Some(ft),
                mesh_id,
                mesh_version,
                activity_id: Some(ctx.activity_id.clone()),
                created_at: ctx.services.clock.now(),
                retracted_at: None,
            };
            if let Err(e) = ctx
                .services
                .stores
                .dig(&src.dig_id)
                .and_then(|s| repo::corrections::record(&s, &correction))
            {
                riddle_observability::rwarn!(
                    riddle_observability::Category::Actions,
                    "could not record retype correction: {}",
                    e.message
                );
            }
        }
        Ok(ActionResult::ok(serde_json::to_value(&f)?, format!("Retyped {} to {}", f.title, ft.label()))
            .with_affected(vec![find_ref(&f)])
            .with_inputs(json!({ "findId": f.id, "findType": ft }))
            .with_undo(json!({ "findId": f.id, "findType": prior.find_type, "activityId": ctx.activity_id, "digId": f.sources.first().map(|s| s.dig_id.clone()) }), format!("Back to {}", prior.find_type.label())))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let ft: FindType = serde_json::from_value(restore["findType"].clone())
            .map_err(|e| RiddleError::invalid(format!("findType: {e}")))?;
        let f = service(ctx).retype(&find_id(&restore)?, ft)?;
        if let (Some(activity), Some(dig)) = (
            restore.get("activityId").and_then(|v| v.as_str()),
            restore.get("digId").and_then(|v| v.as_str()),
        ) {
            if let Ok(store) = ctx.services.stores.dig(&DigId(dig.to_string())) {
                let _ = repo::corrections::retract_by_activity(
                    &store,
                    &ActivityId(activity.to_string()),
                );
            }
        }
        Ok(ActionResult::ok(
            serde_json::to_value(&f)?,
            format!("Retyped {} back to {}", f.title, ft.label()),
        )
        .with_affected(vec![find_ref(&f)]))
    }
}

/// Finds closed by a rule rather than a person: superseded by a newer one
/// of their `+ Replaces` series, or expired. Never invoked by a client; the
/// engine records it so the Activity row has an Action to undo through.
/// Undo reopens the Finds that are still in the state the rule left them,
/// and leaves alone any the user has handled since. An undo is a choice to
/// keep them, so they are kept open: no rule closes them again.
pub struct CloseFinds;

#[async_trait]
impl Action for CloseFinds {
    fn meta(&self) -> ActionMeta {
        meta(
            "close_finds",
            "Close Finds a newer one replaced or that expired",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Act)
        .build()
    }
    async fn execute(&self, _ctx: &ActionContext, _args: Value) -> Result<ActionResult> {
        Err(RiddleError::policy_denied(
            "close_finds is recorded by the engine, not invoked",
        ))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let ids: Vec<FindId> =
            serde_json::from_value(restore.get("findIds").cloned().unwrap_or(json!([])))?;
        let svc = service(ctx);
        let mut reopened = Vec::new();
        for id in &ids {
            let f = svc.get(id)?;
            if matches!(f.status, FindStatus::Superseded | FindStatus::Expired) {
                reopened.push(find_ref(&svc.keep_open(id)?));
            }
        }
        let n = reopened.len();
        Ok(ActionResult::ok(
            json!({ "reopened": n }),
            format!("Kept {} open", riddle_core::text::plural(n, "Find")),
        )
        .with_affected(reopened))
    }
}
