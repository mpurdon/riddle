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
use std::collections::BTreeSet;
use std::sync::Arc;

pub fn all() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(MoveToSpoil),
        Arc::new(MarkContamination),
        Arc::new(SetDisposition),
        Arc::new(ReviewShadow),
        Arc::new(SetImportance),
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

/// "by Tue Sep 30", in the user's own day: the picker offers "tomorrow
/// 8:00" in local time, and a UTC date could name the day before it.
fn due_phrase(due: chrono::DateTime<chrono::Utc>) -> String {
    due.with_timezone(&chrono::Local)
        .format("%a %b %-d")
        .to_string()
}

/// What the user decided about a message, and how the correction is filed.
struct UserDecision {
    disposition: Disposition,
    /// The Action that carried it, for the trace and the rule text.
    action: &'static str,
    kind: CorrectionKind,
    /// The Mesh version the correction is held against when it is not the
    /// one that made the current decision: a Shadow proposal comes from a
    /// Mesh that decided nothing yet.
    against: Option<(MeshId, u32)>,
    /// A Find's due date, when the user gave one.
    due_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// What `classify_as_user` wrote: the new classification, the one it
/// superseded, and the Mesh version the correction counts against, when
/// one was recorded against a Mesh. That version is what the demotion
/// check reads; the caller runs the check, once per message or once per
/// version, as suits it.
struct Written {
    classification: Classification,
    prior: Option<Classification>,
    counted_against: Option<(MeshId, u32)>,
}

/// Write a user classification for one message, plus its trace and
/// correction, tell the projection, and emit the change.
fn classify_as_user(
    ctx: &ActionContext,
    store: &Store,
    dig: &DigId,
    message_id: &MessageId,
    decision: &UserDecision,
) -> Result<Written> {
    let UserDecision {
        disposition,
        action,
        kind,
        against,
        ..
    } = decision;
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
        importance: Importance::default_for(&disposition.clone()),
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
        series: None,
    };
    repo::traces::record(store, &trace)?;
    repo::classifications::record(store, &k)?;
    let correction = Correction {
        id: CorrectionId::new(),
        kind: *kind,
        dig_id: dig.clone(),
        message_id: message_id.clone(),
        from_disposition: prior.as_ref().map(|p| p.disposition.clone()),
        to_disposition: Some(disposition.clone()),
        from_find_type: prior.as_ref().and_then(|p| match &p.disposition {
            Disposition::Find { find_type } => Some(*find_type),
            _ => None,
        }),
        to_find_type: match &k.disposition {
            Disposition::Find { find_type } => Some(*find_type),
            _ => None,
        },
        mesh_id: against
            .as_ref()
            .map(|(m, _)| m.clone())
            .or_else(|| prior.as_ref().and_then(|p| p.mesh_id.clone())),
        mesh_version: against
            .as_ref()
            .map(|(_, v)| *v)
            .or_else(|| prior.as_ref().and_then(|p| p.mesh_version)),
        activity_id: Some(ctx.activity_id.clone()),
        created_at: now,
        retracted_at: None,
    };
    let counted_against = match repo::corrections::record(store, &correction) {
        Ok(()) => correction.mesh_id.clone().zip(correction.mesh_version),
        Err(e) => {
            rwarn!(
                Category::Actions,
                "could not record correction for {}: {}",
                message_id,
                e.message
            );
            None
        }
    };
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
    Ok(Written {
        classification: k,
        prior,
        counted_against,
    })
}

/// One message, and the demotion check for the Mesh version its correction
/// counts against, at once: the single-message Actions.
fn classify_one(
    ctx: &ActionContext,
    store: &Store,
    dig: &DigId,
    message_id: &MessageId,
    decision: &UserDecision,
) -> Result<Written> {
    let written = classify_as_user(ctx, store, dig, message_id, decision)?;
    if let Some(mesh) = &written.counted_against {
        check_demotion(ctx, store, mesh);
    }
    Ok(written)
}

/// Bulk user classification with one Activity and one undo.
async fn bulk(
    ctx: &ActionContext,
    args: Value,
    decision: UserDecision,
    label: &str,
) -> Result<ActionResult> {
    let dig = dig_of(&args)?;
    let ids = message_ids(&args)?;
    let store = ctx.services.stores.dig(&dig)?;
    let mut affected = Vec::new();
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    let mut counted: BTreeSet<(MeshId, u32)> = BTreeSet::new();
    for id in &ids {
        match classify_as_user(ctx, &store, &dig, id, &decision) {
            Ok(w) => {
                affected.push(EntityRef::new(EntityKind::Message, id));
                items.push(
                    json!({ "messageId": id, "newId": w.classification.id, "prior": w.prior }),
                );
                counted.extend(w.counted_against);
            }
            Err(e) => skipped.push(SkippedEntity {
                entity: EntityRef::new(EntityKind::Message, id),
                reason: e.message,
            }),
        }
    }
    // The demotion check is a Mesh read and two aggregate scans of the
    // corrections, so it runs once per Mesh version the loop corrected
    // against, not once per message. The counts only grow, so the verdict
    // after the loop is the one the last message would have reached.
    for mesh in &counted {
        check_demotion(ctx, &store, mesh);
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
        restore_due_date(ctx, &item);
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

/// A decision that moved the due date of a Find it joined carries the
/// earlier date; undo puts it back. Bookkeeping on the Find never fails
/// the restore of the classification.
fn restore_due_date(ctx: &ActionContext, item: &Value) {
    let (Some(find_id), Some(prior)) = (
        item.get("findId").and_then(|v| v.as_str()),
        item.get("priorDueAt"),
    ) else {
        return;
    };
    let prior: Option<chrono::DateTime<chrono::Utc>> = prior.as_str().and_then(|s| s.parse().ok());
    let svc = FindService::new(
        ctx.services.stores.clone(),
        ctx.services.clock.clone(),
        ctx.services.events.clone(),
    );
    if let Err(e) = svc.set_due(&FindId(find_id.to_string()), prior) {
        rwarn!(
            Category::Actions,
            "could not restore the due date of find {} on undo: {}",
            find_id,
            e.message
        );
    }
}

pub struct MoveToSpoil;

#[async_trait]
impl Action for MoveToSpoil {
    fn meta(&self) -> ActionMeta {
        meta("move_to_spoil", "Move messages to Spoil")
            .mutating()
            .reversible()
            .projected()
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
        let decision = UserDecision {
            disposition: Disposition::Spoil,
            action: "move_to_spoil",
            kind: CorrectionKind::Move,
            against: None,
            due_at: None,
        };
        bulk(ctx, args, decision, "Spoil").await
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
            .projected()
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
        let decision = UserDecision {
            disposition: Disposition::Contamination,
            action: "mark_contamination",
            kind: CorrectionKind::Move,
            against: None,
            due_at: None,
        };
        bulk(ctx, args, decision, "Contamination").await
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
            .projected()
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
        // A follow-up due in the past would open a Find already overdue.
        let due_at = future_timestamp(&args, "dueAt", ctx.services.clock.now())?;
        if due_at.is_some() && !matches!(disposition, Disposition::Find { .. }) {
            return Err(RiddleError::invalid("dueAt applies to a Find"));
        }
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
        let decision = UserDecision {
            disposition,
            action: "set_disposition",
            kind,
            against: None,
            due_at,
        };
        let Written {
            classification: k,
            prior,
            ..
        } = classify_one(ctx, &store, &dig, &id, &decision)?;
        let touched = find_for_user_decision(ctx, &store, &dig, &id, &k, &decision);
        let find_id = touched.as_ref().map(|t| t.id.clone());
        let disposition = decision.disposition;
        let summary = match (&disposition, due_at) {
            (
                Disposition::Find {
                    find_type: FindType::FollowUp,
                },
                Some(due),
            ) => format!("Marked for follow-up by {}", due_phrase(due)),
            (
                Disposition::Find {
                    find_type: FindType::FollowUp,
                },
                None,
            ) => "Marked for follow-up".to_string(),
            _ => format!(
                "Set {} to {}",
                id.provider_id(),
                disposition.kind().as_str()
            ),
        };
        let mut item = json!({ "messageId": id, "newId": k.id, "prior": prior });
        if let Some(t) = touched.as_ref().filter(|t| t.due_moved) {
            item["findId"] = json!(t.id);
            item["priorDueAt"] = json!(t.prior_due_at);
        }
        Ok(ActionResult::ok(json!({ "classification": k, "findId": find_id }), summary)
            .with_affected(vec![EntityRef::new(EntityKind::Message, &id)])
            .with_inputs(json!({ "digId": dig, "messageId": id, "disposition": disposition.kind(), "dueAt": due_at }))
            .with_undo(json!({ "digId": dig, "activityId": ctx.activity_id, "items": [item] }), "Undo"))
    }
    async fn undo(&self, ctx: &ActionContext, data: Value) -> Result<ActionResult> {
        restore(ctx, data).await
    }
}

/// The Find a user decision opened or joined, and what it did to the due
/// date, so the undo data can put the earlier date back.
struct FindTouch {
    id: FindId,
    prior_due_at: Option<chrono::DateTime<chrono::Utc>>,
    due_moved: bool,
}

/// A user decision that a message is a Find opens the Find as well, so the
/// correction and the thing to act on arrive together. A `due_at` lands on
/// a new Find through the proposal; on a joined one it replaces whatever
/// date was there, since `apply_proposal` keeps the first date it saw and
/// this is the user choosing again. Without a date the joined Find keeps
/// its own.
fn find_for_user_decision(
    ctx: &ActionContext,
    store: &Store,
    dig: &DigId,
    id: &MessageId,
    k: &Classification,
    decision: &UserDecision,
) -> Option<FindTouch> {
    let Disposition::Find { find_type } = decision.disposition else {
        return None;
    };
    let due_at = decision.due_at;
    let m = match repo::messages::get(store, dig, id) {
        Ok(Some(m)) => m,
        _ => return None,
    };
    let svc = FindService::new(
        ctx.services.stores.clone(),
        ctx.services.clock.clone(),
        ctx.services.events.clone(),
    );
    let proposal = FindProposal {
        code: None,
        find_type,
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
        due_at,
        snoozed_until: None,
        source: FindSource {
            message_id: id.clone(),
            thread_id: m.thread_id.clone(),
            dig_id: dig.clone(),
        },
        confidence: Some(1.0),
        classification_id: Some(k.id.clone()),
        trace_id: Some(k.trace_id.clone()),
        series: None,
        expires_at: None,
        notify: false,
    };
    let (f, joined) = match svc.apply_proposal(&proposal) {
        Ok(a) => (a.find, a.joined),
        Err(e) => {
            rwarn!(
                Category::Actions,
                "could not create a Find for {}: {}",
                id,
                e.message
            );
            return None;
        }
    };
    let mut touch = FindTouch {
        id: f.id.clone(),
        prior_due_at: f.due_at,
        due_moved: false,
    };
    if joined && due_at.is_some() && f.due_at != due_at {
        match svc.set_due(&f.id, due_at) {
            Ok(_) => touch.due_moved = true,
            // The Find is open and joined; only the date failed to move.
            Err(e) => rwarn!(
                Category::Actions,
                "could not move the due date of find {}: {}",
                f.id,
                e.message
            ),
        }
    }
    Some(touch)
}

/// Bookkeeping: a failed demotion check never fails the correction.
fn check_demotion(ctx: &ActionContext, store: &Store, mesh: &(MeshId, u32)) {
    if let Err(e) = demote_if_reversed_often(ctx, store, &mesh.0, mesh.1) {
        rwarn!(
            Category::Actions,
            "demotion check failed for mesh {} v{}: {}",
            mesh.0,
            mesh.1,
            e.message
        );
    }
}

/// Reversals demote a rule (Flow 7): when the user has reversed a quarter or
/// more of an Active Mesh version's last decisions on this account, with at
/// least `DEMOTE_MIN_DECISIONS` to judge by, the Mesh steps back to Shadow
/// and the user is told. Promotion is earned in the Shadow Period; demotion
/// is earned the same way, on evidence, never silently.
fn demote_if_reversed_often(
    ctx: &ActionContext,
    store: &Store,
    mid: &MeshId,
    version: u32,
) -> Result<()> {
    let index = ctx.services.stores.index();
    let Some(mesh) = repo::meshes::get(&index, mid, version)? else {
        return Ok(());
    };
    if mesh.status != MeshStatus::Active {
        return Ok(());
    }
    // Distinct messages: a Retype or Dismiss correction leaves the Mesh's
    // classification standing, so the two sets are unioned in SQL.
    let (decided, reversed) =
        repo::corrections::decided_and_reversed_by_mesh(store, mid.as_str(), version)?;
    if !reversed_often(decided, reversed) {
        return Ok(());
    }
    repo::meshes::demote(
        &index,
        mid,
        version,
        Demotion {
            at: ctx.services.clock.now(),
            reversed: reversed as u32,
            decided: decided as u32,
        },
    )?;
    rwarn!(
        Category::Mesh,
        "mesh {} v{} demoted to shadow: {} of {} reversed",
        mid,
        version,
        reversed,
        decided
    );
    ctx.services.emit(RiddleEvent::Notify {
        title: "A rule stepped back".into(),
        body: format!(
            "Mesh {} v{} is back in Shadow: you reversed {} of its last {} decisions. It proposes until it earns Active again.",
            mesh.name, version, reversed, decided
        ),
        find_id: None,
    });
    ctx.services.emit(RiddleEvent::SummaryChanged);
    Ok(())
}

/// Agree or disagree with what a Shadow Mesh would have done (Flow 7,
/// Shadow review). Agreement is what a Mesh must earn to act, and until
/// this Action nothing recorded it: the Shadow inbox was a plain list whose
/// buttons acted for real. Agree counts for the Mesh and marks the row;
/// Disagree records the user's own decision as a correction, which is what
/// the Mesh is judged against.
pub struct ReviewShadow;

#[async_trait]
impl Action for ReviewShadow {
    fn meta(&self) -> ActionMeta {
        // Not `.external()`: Agree writes a mark and a counter, and Disagree's
        // decision reaches the mailbox only through the outbox, which holds
        // it in an unarmed dev build on its own. Marking it external made
        // every review fail in a dev build that had nothing to arm.
        meta("review_shadow", "Agree or disagree with a Shadow proposal")
            .mutating()
            .reversible()
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
        let agree = args.get("agree").and_then(|v| v.as_bool()).unwrap_or(true);
        let store = ctx.services.stores.dig(&dig)?;
        let Some(shadow) = repo::classifications::current_shadow(&store, &id)? else {
            return Err(RiddleError::invalid(
                "this message has no Shadow proposal to review",
            ));
        };
        let index = ctx.services.stores.index();
        if agree {
            repo::shadow_reviews::set(&store, &id, ShadowReview::Agreed)?;
            if let Some(mid) = &shadow.mesh_id {
                if let Err(e) = riddle_mesh::shadow::record_agreement(&index, mid) {
                    rwarn!(
                        Category::Mesh,
                        "agreement not counted for {}: {}",
                        mid,
                        e.message
                    );
                }
            }
            let mut r = ActionResult::ok(
                json!({ "review": "agreed", "proposed": shadow.disposition }),
                format!(
                    "Agreed with the Shadow proposal: {}",
                    shadow.disposition.kind().as_str()
                ),
            )
            .with_affected(vec![EntityRef::new(EntityKind::Message, &id)])
            .with_inputs(json!({ "digId": dig, "messageId": id, "agree": true }));
            r.undo = UndoInfo::NotUndoable {
                reason: "agreement is a count the Mesh earns; disagree from the same row to record the opposite".into(),
            };
            return Ok(r);
        }
        let decision = UserDecision {
            disposition: parse_disposition(&args)?,
            action: "review_shadow",
            kind: CorrectionKind::Move,
            against: shadow.mesh_id.clone().zip(shadow.mesh_version),
            due_at: None,
        };
        let Written {
            classification: k,
            prior,
            ..
        } = classify_one(ctx, &store, &dig, &id, &decision)?;
        repo::shadow_reviews::set(&store, &id, ShadowReview::Disagreed)?;
        if let Some(mid) = &shadow.mesh_id {
            if let Err(e) = riddle_mesh::shadow::record_disagreement(&index, mid) {
                rwarn!(
                    Category::Mesh,
                    "disagreement not counted for {}: {}",
                    mid,
                    e.message
                );
            }
        }
        let find_id = find_for_user_decision(ctx, &store, &dig, &id, &k, &decision).map(|t| t.id);
        let disposition = decision.disposition;
        Ok(ActionResult::ok(
            json!({ "review": "disagreed", "classification": k, "findId": find_id }),
            format!(
                "Disagreed with the Shadow proposal ({}): {} instead",
                shadow.disposition.kind().as_str(),
                disposition.kind().as_str()
            ),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Message, &id)])
        .with_inputs(json!({ "digId": dig, "messageId": id, "agree": false, "disposition": disposition.kind() }))
        .with_undo(json!({ "digId": dig, "activityId": ctx.activity_id, "items": [{ "messageId": id, "newId": k.id, "prior": prior }] }), "Undo"))
    }
    async fn undo(&self, ctx: &ActionContext, data: Value) -> Result<ActionResult> {
        restore(ctx, data).await
    }
}

/// `set_importance`: the user's own level for one message (`null` clears it
/// back to the decision's). It lives beside the classification, so a rule
/// re-deciding the message never overwrites it, and an open Find from the
/// message moves into the level's priority band. Riddle-only: nothing is
/// written to the mailbox.
pub struct SetImportance;

#[async_trait]
impl Action for SetImportance {
    fn meta(&self) -> ActionMeta {
        meta(
            "set_importance",
            "Set how much one message matters: Low, Normal, High or Critical",
        )
        .mutating()
        .reversible()
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
        let level = match args.get("importance") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_str().and_then(Importance::parse).ok_or_else(|| {
                RiddleError::invalid("importance is low, normal, high, critical or null")
            })?),
        };
        let store = ctx.services.stores.dig(&dig)?;
        if repo::messages::get(&store, &dig, &id)?.is_none() {
            return Err(RiddleError::not_found(format!("message {id}")));
        }
        let prior = repo::importance::get(&store, &id)?;
        repo::importance::set(&store, &id, level)?;
        let effective = match level {
            Some(l) => l,
            None => repo::classifications::current(&store, &id)?
                .map(|k| k.importance)
                .unwrap_or_default(),
        };
        let index = ctx.services.stores.index();
        let mut finds = Vec::new();
        for mut f in repo::finds::for_message(&index, &id)? {
            if f.status != FindStatus::Open {
                continue;
            }
            let p = effective.clamp_priority(f.priority);
            if p != f.priority {
                finds.push(json!({ "findId": f.id, "priority": f.priority }));
                f.priority = p;
                repo::finds::upsert(&index, &f)?;
            }
        }
        let summary = match level {
            Some(l) => format!("Set {} to {} importance", id.provider_id(), l.as_str()),
            None => format!("Cleared your importance for {}", id.provider_id()),
        };
        Ok(
            ActionResult::ok(json!({ "importance": effective }), summary)
                .with_affected(vec![EntityRef::new(EntityKind::Message, &id)])
                .with_inputs(json!({ "digId": dig, "messageId": id, "importance": level }))
                .with_undo(
                    json!({ "digId": dig, "messageId": id, "prior": prior, "finds": finds }),
                    "Undo",
                ),
        )
    }
    async fn undo(&self, ctx: &ActionContext, data: Value) -> Result<ActionResult> {
        let dig = dig_of(&data)?;
        let id = data
            .get("messageId")
            .and_then(|v| v.as_str())
            .map(|s| MessageId(s.to_string()))
            .ok_or_else(|| RiddleError::invalid("undo data has no messageId"))?;
        let prior: Option<Importance> =
            serde_json::from_value(data.get("prior").cloned().unwrap_or(Value::Null))
                .unwrap_or(None);
        let store = ctx.services.stores.dig(&dig)?;
        repo::importance::set(&store, &id, prior)?;
        let index = ctx.services.stores.index();
        for f in data
            .get("finds")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let (Some(fid), Some(p)) = (
                f.get("findId").and_then(|v| v.as_str()),
                f.get("priority").and_then(|v| v.as_u64()),
            ) else {
                continue;
            };
            if let Some(mut find) = repo::finds::get(&index, &FindId(fid.to_string()))? {
                find.priority = p.min(100) as u8;
                repo::finds::upsert(&index, &find)?;
            }
        }
        Ok(ActionResult::ok(json!({}), "Importance restored")
            .with_affected(vec![EntityRef::new(EntityKind::Message, &id)]))
    }
}
