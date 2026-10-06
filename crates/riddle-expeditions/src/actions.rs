//! The Expedition Actions: `start_expedition`, `cancel_expedition` and the
//! small additive `create_find` that turns findings into Finds of type Know.
//! Starting an Expedition is SUGGEST-level work: creating the record is
//! harmless; what it may do is gated tool by tool inside the run.

use crate::runner::ExpeditionHandle;
use async_trait::async_trait;
use riddle_actions::policy::ensure_args;
use riddle_actions::{Action, ActionContext, ActionResult, Preview};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// The Actions this crate contributes to the registry.
pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(StartExpedition),
        Arc::new(CancelExpedition),
        Arc::new(CreateFind),
    ]
}

/// Lenient argument shapes: the Director sends partial objects, the UI
/// sends full ones; every field defaults.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct ScopeArg {
    dig_ids: Vec<DigId>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    until: Option<chrono::DateTime<chrono::Utc>>,
    tools_allowed: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct BudgetArg {
    cost_usd: f64,
    seconds: u64,
    tool_calls: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartArgs {
    goal: String,
    #[serde(default)]
    scope: ScopeArg,
    #[serde(default)]
    budget: BudgetArg,
}

impl StartArgs {
    fn scope(&self) -> ExpeditionScope {
        ExpeditionScope {
            dig_ids: self.scope.dig_ids.clone(),
            since: self.scope.since,
            until: self.scope.until,
            tools_allowed: self.scope.tools_allowed.clone(),
        }
    }
    fn budget(&self) -> ExpeditionBudget {
        ExpeditionBudget {
            cost_usd: self.budget.cost_usd,
            seconds: self.budget.seconds,
            tool_calls: self.budget.tool_calls,
        }
    }
}

pub struct StartExpedition;

#[async_trait]
impl Action for StartExpedition {
    fn meta(&self) -> ActionMeta {
        riddle_actions::meta("start_expedition", "start an Expedition: durable delegated work over the mailbox with a declared scope and budget")
            .mutating()
            .risk(Risk::Low)
            .confirm(ConfirmationPolicy::None)
            .autonomy(Autonomy::Suggest)
            .build()
    }

    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let a: StartArgs = ensure_args(args)?;
        Ok(Preview {
            summary: format!(
                "start an Expedition ({} chars of goal) over {} Dig(s) with a budget of ${:.2}, {} seconds and {} tool calls",
                a.goal.chars().count(),
                if a.scope.dig_ids.is_empty() { "all permitted".to_string() } else { a.scope.dig_ids.len().to_string() },
                a.budget.cost_usd,
                a.budget.seconds,
                a.budget.tool_calls
            ),
            real_arguments: args.clone(),
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: StartArgs = ensure_args(&args)?;
        let runner = ctx.services.require::<ExpeditionHandle>()?.runner()?;
        let e = runner.create(&a.goal, a.scope(), a.budget())?;
        runner.spawn(e.id.clone());
        let value = serde_json::to_value(&e)?;
        Ok(ActionResult::ok(value, "Started an Expedition")
            .with_affected(vec![EntityRef::new(EntityKind::Expedition, &e.id)])
            .with_inputs(
                json!({"expeditionId": e.id, "digIds": e.scope.dig_ids, "budget": e.budget}),
            ))
    }
}

pub struct CancelExpedition;

#[async_trait]
impl Action for CancelExpedition {
    fn meta(&self) -> ActionMeta {
        riddle_actions::meta(
            "cancel_expedition",
            "cancel a running Expedition; its checkpoint and findings so far are kept",
        )
        .mutating()
        .risk(Risk::Low)
        .confirm(ConfirmationPolicy::None)
        .autonomy(Autonomy::Suggest)
        .build()
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = args
            .get("expeditionId")
            .and_then(|v| v.as_str())
            .map(ExpeditionId::from)
            .ok_or_else(|| RiddleError::invalid("expeditionId is required"))?;
        let runner = ctx.services.require::<ExpeditionHandle>()?.runner()?;
        let e = runner.cancel(&id)?;
        Ok(ActionResult::ok(
            json!({"expeditionId": e.id, "status": e.status}),
            "Cancelled an Expedition",
        )
        .with_affected(vec![EntityRef::new(EntityKind::Expedition, &e.id)])
        .with_inputs(json!({"expeditionId": e.id})))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateFindArgs {
    #[serde(default)]
    find_type: Option<String>,
    title: String,
    summary: String,
    #[serde(default)]
    sources: Vec<SourceArg>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    identity_key: Option<String>,
    #[serde(default)]
    priority: Option<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceArg {
    message_id: String,
    #[serde(default)]
    dig_id: Option<String>,
    #[serde(default)]
    thread_id: Option<String>,
}

/// Writes a Find (default type Know) with the given sources. Deduplicated on
/// the identity key so a re-run of an Expedition does not duplicate Finds.
pub struct CreateFind;

#[async_trait]
impl Action for CreateFind {
    fn meta(&self) -> ActionMeta {
        riddle_actions::meta(
            "create_find",
            "create a Find from evidence (default type Know)",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .confirm(ConfirmationPolicy::None)
        .autonomy(Autonomy::Act)
        .build()
    }

    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let a: CreateFindArgs = ensure_args(args)?;
        Ok(Preview {
            summary: format!(
                "create a {} Find from {} source message(s)",
                a.find_type.as_deref().unwrap_or("know"),
                a.sources.len()
            ),
            real_arguments: args.clone(),
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: CreateFindArgs = ensure_args(&args)?;
        if a.title.trim().is_empty() || a.summary.trim().is_empty() {
            return Err(RiddleError::invalid("a Find needs a title and a summary"));
        }
        let find_type = match &a.find_type {
            Some(t) => FindType::parse(t)
                .ok_or_else(|| RiddleError::invalid(format!("unknown find type {t}")))?,
            None => FindType::Know,
        };
        let index = ctx.services.stores.index();
        let identity_key = a.identity_key.clone().unwrap_or_else(|| {
            format!(
                "find:{}",
                riddle_core::sha256_hex(format!("{}\n{}", a.title.trim(), a.summary.trim()))
            )
        });
        if let Some(existing) = riddle_store::repo::finds::by_identity(&index, &identity_key)? {
            return Ok(
                ActionResult::ok(serde_json::to_value(&existing)?, "Find already exists")
                    .with_affected(vec![EntityRef::new(EntityKind::Find, &existing.id)]),
            );
        }
        let now = ctx.services.clock.now();
        let sources: Vec<FindSource> = a
            .sources
            .iter()
            .map(|s| {
                let dig = s
                    .dig_id
                    .clone()
                    .or_else(|| ctx.dig_id.as_ref().map(|d| d.as_str().to_string()))
                    .or_else(|| s.message_id.split_once(':').map(|(d, _)| d.to_string()))
                    .unwrap_or_default();
                FindSource {
                    message_id: MessageId::from(s.message_id.as_str()),
                    thread_id: ThreadId::from(
                        s.thread_id
                            .clone()
                            .unwrap_or_else(|| s.message_id.clone())
                            .as_str(),
                    ),
                    dig_id: DigId::from(dig.as_str()),
                }
            })
            .collect();
        let mut dig_ids: Vec<DigId> = sources.iter().map(|s| s.dig_id.clone()).collect();
        dig_ids.sort();
        dig_ids.dedup();
        let find = Find {
            code: None,
            id: FindId::new(),
            find_type,
            title: a.title.trim().to_string(),
            summary: a.summary.trim().to_string(),
            priority: a.priority.unwrap_or(40),
            status: FindStatus::Open,
            due_at: None,
            snoozed_until: None,
            created_at: now,
            updated_at: now,
            resolved_at: None,
            resolved_by: None,
            sources,
            dig_ids,
            identity_key,
            reason: a.reason.unwrap_or_else(|| "created by the Director".into()),
            confidence: None,
            classification_id: None,
            trace_id: None,
            superseded_by: None,
            series: None,
            expires_at: None,
            kept_open_at: None,
            notify: false,
        };
        riddle_store::repo::finds::upsert(&index, &find)?;
        ctx.services
            .emit(RiddleEvent::FindCreated { find: find.clone() });
        Ok(ActionResult::ok(serde_json::to_value(&find)?, format!("Created a {} Find", find_type.label()))
            .with_affected(vec![EntityRef::new(EntityKind::Find, &find.id)])
            .with_inputs(json!({"findId": find.id, "findType": find_type.as_str(), "sources": find.sources.len()}))
            .with_undo(json!({"findId": find.id}), "Dismiss the Find")
            .with_provenance(Provenance { mechanism: DecisionMechanism::Director, mesh_id: None, mesh_version: None, mesh_name: None, trace_id: None, note: Some("Expedition finding".into()) }))
    }

    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let id = restore
            .get("findId")
            .and_then(|v| v.as_str())
            .map(FindId::from)
            .ok_or_else(|| RiddleError::invalid("findId missing from restore data"))?;
        let index = ctx.services.stores.index();
        let mut find = riddle_store::repo::finds::get(&index, &id)?
            .ok_or_else(|| RiddleError::not_found("no such Find"))?;
        find.status = FindStatus::Dismissed;
        find.updated_at = ctx.services.clock.now();
        riddle_store::repo::finds::upsert(&index, &find)?;
        ctx.services.emit(RiddleEvent::FindChanged {
            find_id: id.clone(),
        });
        Ok(
            ActionResult::ok(json!({"findId": id}), "Dismissed the Find")
                .with_affected(vec![EntityRef::new(EntityKind::Find, &id)]),
        )
    }
}
