//! User correction Actions: `move_to_spoil`, `mark_contamination` and
//! `set_disposition`. Each writes a Classification with mechanism User (a
//! new row, so history is kept), records a structured Correction (§46), and
//! is reversible: undo restores the prior classification, retracts the
//! correction, and skips messages a later Activity touched (§50). Projection
//! goes through the `ProjectionHook` when one is registered; a projection
//! failure is reported and never fails the correction.

use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_finds::FindService;
use riddle_observability::{rwarn, Category};
use riddle_store::{repo, Store};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn all() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(MoveToSpoil),
        Arc::new(MarkContamination),
        Arc::new(SetDisposition),
    ]
}

fn dig_of(args: &Value) -> Result<DigId> {
    args.get("digId")
        .and_then(|v| v.as_str())
        .map(|s| DigId(s.to_string()))
        .ok_or_else(|| RiddleError::invalid("missing argument digId"))
}

fn message_ids(args: &Value) -> Result<Vec<MessageId>> {
    let ids: Vec<MessageId> = args
        .get("messageIds")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(|s| MessageId(s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Err(RiddleError::invalid(
            "messageIds must name at least one message",
        ));
    }
    Ok(ids)
}

fn projection(ctx: &ActionContext) -> Option<Arc<dyn ProjectionHook>> {
    ctx.services
        .get::<ProjectionHookHandle>()
        .map(|h| h.0.clone())
}

/// Write a user classification for one message, plus its trace and
/// correction. Returns the new classification and the prior one.
fn classify_as_user(
    ctx: &ActionContext,
    store: &Store,
    dig: &DigId,
    message_id: &MessageId,
    disposition: Disposition,
    action: &str,
    kind: CorrectionKind,
) -> Result<(Classification, Option<Classification>)> {
    let now = ctx.services.clock.now();
    let prior = repo::classifications::current(store, message_id)?;
    let facts = repo::facts::get(store, message_id)?.unwrap_or_default();
    let trace = DecisionTrace {
        id: TraceId::new(),
        message_id: message_id.clone(),
        dig_id: dig.clone(),
        objective: format!("user correction via {action}"),
        strategy: "user".into(),
        facts,
        gate: None,
        signals: vec![],
        mesh_id: None,
        mesh_version: None,
        mesh_name: None,
        matched_rule: Some(format!("USER {action}")),
        guard_vetoes: vec![],
        escalation: None,
        conclusion: disposition.clone(),
        confidence: Some(1.0),
        mechanism: DecisionMechanism::User,
        timing: TraceTiming::default(),
        cost: TraceCost {
            judge_cost_usd: Some(0.0),
            ..Default::default()
        },
        judge_identity: None,
        app_version: ctx.services.build.version.clone(),
        build_stamp: ctx.services.build.git_sha.clone(),
        shadow: false,
        created_at: now,
    };
    let strata = prior.as_ref().map(|p| p.strata.clone()).unwrap_or_default();
    let k = Classification {
        id: ClassificationId::new(),
        message_id: message_id.clone(),
        dig_id: dig.clone(),
        disposition: disposition.clone(),
        strata,
        notify: false,
        mechanism: DecisionMechanism::User,
        mesh_id: None,
        mesh_version: None,
        matched_rule: Some(format!("USER {action}")),
        confidence: Some(1.0),
        reason: format!("You chose {} ({action}).", disposition.kind().as_str()),
        trace_id: trace.id.clone(),
        shadow: false,
        created_at: now,
        superseded_at: None,
    };
    repo::traces::record(store, &trace)?;
    repo::classifications::record(store, &k)?;
    let correction = Correction {
        id: CorrectionId::new(),
        kind,
        dig_id: dig.clone(),
        message_id: message_id.clone(),
        from_disposition: prior.as_ref().map(|p| p.disposition.clone()),
        to_disposition: Some(disposition),
        from_find_type: prior.as_ref().and_then(|p| match &p.disposition {
            Disposition::Find { find_type } => Some(*find_type),
            _ => None,
        }),
        to_find_type: match &k.disposition {
            Disposition::Find { find_type } => Some(*find_type),
            _ => None,
        },
        mesh_id: prior.as_ref().and_then(|p| p.mesh_id.clone()),
        mesh_version: prior.as_ref().and_then(|p| p.mesh_version),
        activity_id: Some(ctx.activity_id.clone()),
        created_at: now,
        retracted_at: None,
    };
    if let Err(e) = repo::corrections::record(store, &correction) {
        rwarn!(
            Category::Actions,
            "could not record correction for {}: {}",
            message_id,
            e.message
        );
    }
    ctx.services.emit(RiddleEvent::MessageClassified {
        message_id: message_id.clone(),
        dig_id: dig.clone(),
        disposition: k.disposition.clone(),
        shadow: false,
    });
    if let Some(hook) = projection(ctx) {
        if let Err(e) = hook.project(dig, &k) {
            rwarn!(
                Category::Projection,
                "projection of {} failed: {}",
                message_id,
                e.message
            );
        }
    }
    Ok((k, prior))
}

/// Bulk user classification with one Activity and one undo.
async fn bulk(
    ctx: &ActionContext,
    args: Value,
    disposition: Disposition,
    action: &str,
    kind: CorrectionKind,
    label: &str,
) -> Result<ActionResult> {
    let dig = dig_of(&args)?;
    let ids = message_ids(&args)?;
    let store = ctx.services.stores.dig(&dig)?;
    let mut affected = Vec::new();
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    for id in &ids {
        match classify_as_user(ctx, &store, &dig, id, disposition.clone(), action, kind) {
            Ok((k, prior)) => {
                affected.push(EntityRef::new(EntityKind::Message, id));
                items.push(json!({ "messageId": id, "newId": k.id, "prior": prior }));
            }
            Err(e) => skipped.push(SkippedEntity {
                entity: EntityRef::new(EntityKind::Message, id),
                reason: e.message,
            }),
        }
    }
    let n = items.len();
    Ok(ActionResult::ok(
        json!({ "moved": n, "skipped": skipped.len() }),
        format!(
            "Moved {n} message{} to {label}",
            if n == 1 { "" } else { "s" }
        ),
    )
    .with_affected(affected)
    .with_inputs(json!({ "digId": dig, "count": ids.len() }))
    .with_undo(
        json!({ "digId": dig, "activityId": ctx.activity_id, "items": items }),
        format!("Undo{}", if n > 1 { " All" } else { "" }),
    )
    .partial(skipped))
}

/// Restore prior classifications from undo data. Messages whose current
/// classification is no longer the one this action wrote are skipped.
async fn restore(ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
    let dig = dig_of(&restore)?;
    let store = ctx.services.stores.dig(&dig)?;
    let now = ctx.services.clock.now();
    let mut affected = Vec::new();
    let mut skipped = Vec::new();
    let hook = projection(ctx);
    for item in restore
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let Some(id) = item
            .get("messageId")
            .and_then(|v| v.as_str())
            .map(|s| MessageId(s.to_string()))
        else {
            continue;
        };
        let written = item.get("newId").and_then(|v| v.as_str()).unwrap_or("");
        let current = repo::classifications::current(&store, &id)?;
        if current.as_ref().map(|c| c.id.as_str()) != Some(written) {
            skipped.push(SkippedEntity {
                entity: EntityRef::new(EntityKind::Message, &id),
                reason: "changed since; handled later by you or by Riddle".into(),
            });
            continue;
        }
        let prior: Option<Classification> = item
            .get("prior")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .unwrap_or(None);
        match prior {
            Some(mut p) => {
                p.id = ClassificationId::new();
                p.created_at = now;
                p.superseded_at = None;
                repo::classifications::record(&store, &p)?;
                ctx.services.emit(RiddleEvent::MessageClassified {
                    message_id: id.clone(),
                    dig_id: dig.clone(),
                    disposition: p.disposition.clone(),
                    shadow: false,
                });
                if let Some(h) = &hook {
                    if let Err(e) = h.project(&dig, &p) {
                        rwarn!(
                            Category::Projection,
                            "projection of {} failed on undo: {}",
                            id,
                            e.message
                        );
                    }
                }
            }
            None => {
                repo::classifications::supersede_current(&store, &id)?;
                if let Some(h) = &hook {
                    if let Err(e) = h.unproject(&dig, &id) {
                        rwarn!(
                            Category::Projection,
                            "unproject of {} failed on undo: {}",
                            id,
                            e.message
                        );
                    }
                }
            }
        }
        affected.push(EntityRef::new(EntityKind::Message, &id));
    }
    if let Some(activity) = restore.get("activityId").and_then(|v| v.as_str()) {
        if let Err(e) =
            repo::corrections::retract_by_activity(&store, &ActivityId(activity.to_string()))
        {
            rwarn!(
                Category::Actions,
                "could not retract corrections for activity {}: {}",
                activity,
                e.message
            );
        }
    }
    let n = affected.len();
    Ok(ActionResult::ok(
        json!({ "restored": n, "skipped": skipped.len() }),
        format!(
            "Restored {n} message{}{}",
            if n == 1 { "" } else { "s" },
            if skipped.is_empty() {
                String::new()
            } else {
                format!("; {} skipped", skipped.len())
            }
        ),
    )
    .with_affected(affected)
    .partial(skipped))
}

pub struct MoveToSpoil;

#[async_trait]
impl Action for MoveToSpoil {
    fn meta(&self) -> ActionMeta {
        meta("move_to_spoil", "Move messages to Spoil")
            .mutating()
            .reversible()
            .external()
            .risk(Risk::Low)
            .confirm(ConfirmationPolicy::InPlaceUndo)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let ids = message_ids(args)?;
        Ok(Preview {
            summary: format!(
                "Move {} message{} to Spoil",
                ids.len(),
                if ids.len() == 1 { "" } else { "s" }
            ),
            real_arguments: json!({ "digId": dig_of(args)?, "count": ids.len() }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        bulk(
            ctx,
            args,
            Disposition::Spoil,
            "move_to_spoil",
            CorrectionKind::Move,
            "Spoil",
        )
        .await
    }
    async fn undo(&self, ctx: &ActionContext, data: Value) -> Result<ActionResult> {
        restore(ctx, data).await
    }
}

pub struct MarkContamination;

#[async_trait]
impl Action for MarkContamination {
    fn meta(&self) -> ActionMeta {
        meta("mark_contamination", "Mark messages as Contamination")
            .mutating()
            .reversible()
            .external()
            .risk(Risk::Medium)
            .confirm(ConfirmationPolicy::InPlaceUndo)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let ids = message_ids(args)?;
        Ok(Preview {
            summary: format!(
                "Mark {} message{} as Contamination",
                ids.len(),
                if ids.len() == 1 { "" } else { "s" }
            ),
            real_arguments: json!({ "digId": dig_of(args)?, "count": ids.len() }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        bulk(
            ctx,
            args,
            Disposition::Contamination,
            "mark_contamination",
            CorrectionKind::Move,
            "Contamination",
        )
        .await
    }
    async fn undo(&self, ctx: &ActionContext, data: Value) -> Result<ActionResult> {
        restore(ctx, data).await
    }
}

fn parse_disposition(args: &Value) -> Result<Disposition> {
    let v = args
        .get("disposition")
        .ok_or_else(|| RiddleError::invalid("missing argument disposition"))?;
    if let Some(s) = v.as_str() {
        return Ok(match s.to_ascii_lowercase().as_str() {
            "find" => Disposition::Find {
                find_type: args
                    .get("findType")
                    .and_then(|f| f.as_str())
                    .and_then(FindType::parse)
                    .ok_or_else(|| RiddleError::invalid("a Find needs findType"))?,
            },
            "filed" => Disposition::Filed,
            "spoil" => Disposition::Spoil,
            "contamination" => Disposition::Contamination,
            "unresolved" => Disposition::Unresolved,
            other => return Err(RiddleError::invalid(format!("unknown disposition {other}"))),
        });
    }
    serde_json::from_value(v.clone()).map_err(|e| RiddleError::invalid(format!("disposition: {e}")))
}

pub struct SetDisposition;

#[async_trait]
impl Action for SetDisposition {
    fn meta(&self) -> ActionMeta {
        meta("set_disposition", "Correct the disposition of one message")
            .mutating()
            .reversible()
            .external()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let dig = dig_of(&args)?;
        let id = args
            .get("messageId")
            .and_then(|v| v.as_str())
            .map(|s| MessageId(s.to_string()))
            .ok_or_else(|| RiddleError::invalid("missing argument messageId"))?;
        let disposition = parse_disposition(&args)?;
        let store = ctx.services.stores.dig(&dig)?;
        let prior = repo::classifications::current(&store, &id)?;
        let kind = match (prior.as_ref().map(|p| &p.disposition), &disposition) {
            (Some(Disposition::Spoil | Disposition::Contamination), d)
                if !matches!(d, Disposition::Spoil | Disposition::Contamination) =>
            {
                CorrectionKind::Restore
            }
            (Some(Disposition::Find { .. }), Disposition::Find { .. }) => CorrectionKind::Retype,
            _ => CorrectionKind::Move,
        };
        let (k, prior) = classify_as_user(
            ctx,
            &store,
            &dig,
            &id,
            disposition.clone(),
            "set_disposition",
            kind,
        )?;
        let mut find_id = None;
        if let Disposition::Find { find_type } = &disposition {
            if let Some(m) = repo::messages::get(&store, &dig, &id)? {
                let svc = FindService::new(
                    ctx.services.stores.clone(),
                    ctx.services.clock.clone(),
                    ctx.services.events.clone(),
                );
                let proposal = FindProposal {
                    find_type: *find_type,
                    title: m.subject.clone(),
                    summary: m.snippet.clone(),
                    priority: 60,
                    reason: "You marked it as a Find.".into(),
                    identity_key: m
                        .headers
                        .references
                        .first()
                        .cloned()
                        .or_else(|| m.headers.in_reply_to.clone())
                        .or_else(|| m.headers.message_id.clone())
                        .unwrap_or_else(|| m.thread_id.provider_id().to_string()),
                    due_at: None,
                    source: FindSource {
                        message_id: id.clone(),
                        thread_id: m.thread_id.clone(),
                        dig_id: dig.clone(),
                    },
                    confidence: Some(1.0),
                    classification_id: Some(k.id.clone()),
                    trace_id: Some(k.trace_id.clone()),
                };
                match svc.apply_proposal(&proposal) {
                    Ok((f, _)) => find_id = Some(f.id),
                    Err(e) => rwarn!(
                        Category::Actions,
                        "could not create a Find for {}: {}",
                        id,
                        e.message
                    ),
                }
            }
        }
        Ok(ActionResult::ok(json!({ "classification": k, "findId": find_id }), format!("Set {} to {}", id.provider_id(), disposition.kind().as_str()))
            .with_affected(vec![EntityRef::new(EntityKind::Message, &id)])
            .with_inputs(json!({ "digId": dig, "messageId": id, "disposition": disposition.kind() }))
            .with_undo(json!({ "digId": dig, "activityId": ctx.activity_id, "items": [{ "messageId": id, "newId": k.id, "prior": prior }] }), "Undo"))
    }
    async fn undo(&self, ctx: &ActionContext, data: Value) -> Result<ActionResult> {
        restore(ctx, data).await
    }
}
