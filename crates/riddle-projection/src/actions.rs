//! The user's own moves: archive, restore, label, snooze. Each writes the
//! store first (Riddle's understanding), then enqueues the provider write
//! (the projection), then answers with restoration data so `undo` can put
//! both back. Provider labels in the store are never written optimistically:
//! the outbox processor updates them once the provider has, because the
//! store's labels are what conflict detection compares against. A restore from Spoil or Contamination is a structured
//! correction (§46) tied to the Activity, so undoing the restore retracts
//! the evidence too (§50).

use crate::outbox::{enqueue, enqueue_classification, undo_projection};
use crate::plan::{plan, stratum_label, ProjectionPlan, INBOX};
use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_mail::actions::util::{arg, dig_and_store, message_ids, message_refs, opt};
use riddle_observability::{rinfo, Category};
use riddle_store::{repo, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

/// What one message looked like before an Action touched it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSnapshot {
    pub message_id: MessageId,
    pub classification: Option<Classification>,
    pub labels: Vec<String>,
    pub strata: Vec<String>,
    pub snoozed_until: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreState {
    pub dig_id: DigId,
    pub items: Vec<MessageSnapshot>,
}

fn snapshot(store: &Store, m: &Message) -> Result<MessageSnapshot> {
    let classification = repo::classifications::current(store, &m.id)?;
    Ok(MessageSnapshot {
        message_id: m.id.clone(),
        strata: classification
            .as_ref()
            .map(|k| k.strata.clone())
            .unwrap_or_default(),
        classification,
        labels: m.labels.clone(),
        snoozed_until: repo::drafts::snoozed_until(store, m.id.as_str())?,
    })
}

/// A user decision is a classification with a trace like any other (§92).
fn record_user_classification(
    ctx: &ActionContext,
    store: &Store,
    dig: &Dig,
    m: &Message,
    disposition: Disposition,
    strata: Vec<String>,
    reason: &str,
) -> Result<Classification> {
    let now = ctx.services.clock.now();
    let trace = DecisionTrace {
        id: TraceId::new(),
        message_id: m.id.clone(),
        dig_id: dig.id.clone(),
        objective: "apply the user's decision".into(),
        strategy: "user action".into(),
        facts: repo::facts::get(store, &m.id)?.unwrap_or_default(),
        gate: None,
        signals: vec![],
        mesh_id: None,
        mesh_version: None,
        mesh_name: None,
        matched_rule: None,
        guard_vetoes: vec![],
        escalation: None,
        conclusion: disposition.clone(),
        confidence: Some(1.0),
        mechanism: DecisionMechanism::User,
        timing: TraceTiming::default(),
        cost: TraceCost::default(),
        judge_identity: None,
        app_version: ctx.services.build.version.clone(),
        build_stamp: ctx.services.build.git_sha.clone(),
        shadow: false,
        created_at: now,
        pass_id: String::new(),
        list_results: vec![],
        policy: None,
        margin: None,
        mesh_outcomes: vec![],
    };
    let k = Classification {
        id: ClassificationId::new(),
        message_id: m.id.clone(),
        dig_id: dig.id.clone(),
        importance: Importance::default_for(&disposition),
        disposition,
        strata,
        notify: false,
        mechanism: DecisionMechanism::User,
        mesh_id: None,
        mesh_version: None,
        matched_rule: None,
        confidence: Some(1.0),
        reason: reason.into(),
        trace_id: trace.id.clone(),
        shadow: false,
        created_at: now,
        superseded_at: None,
        series: None,
    };
    store.write(|c| {
        repo::traces::record_in(c, &trace)?;
        repo::classifications::record_in(c, &k)
    })?;
    ctx.services.emit(RiddleEvent::MessageClassified {
        message_id: m.id.clone(),
        dig_id: dig.id.clone(),
        disposition: k.disposition.clone(),
        shadow: false,
    });
    Ok(k)
}

/// Put a message back to what a snapshot says, in the store and the provider.
fn restore_snapshot(
    ctx: &ActionContext,
    store: &Store,
    dig: &Dig,
    snap: &MessageSnapshot,
    key_suffix: &str,
) -> Result<Option<Message>> {
    let Some(m) = repo::messages::get(store, &dig.id, &snap.message_id)? else {
        return Ok(None);
    };
    match &snap.classification {
        Some(k) => {
            let restored = Classification {
                id: ClassificationId::new(),
                created_at: ctx.services.clock.now(),
                superseded_at: None,
                series: None,
                ..k.clone()
            };
            repo::classifications::record(store, &restored)?;
            let p = plan(
                &restored.disposition,
                &restored.strata,
                dig.projection,
                false,
            );
            // Undo the current projection, then re-apply the old one.
            undo_projection(
                store,
                dig,
                &m,
                snap.labels.contains(&INBOX.to_string()),
                key_suffix,
            )?;
            enqueue(store, &m, &p, "undo", Some(key_suffix))?;
        }
        None => {
            record_user_classification(
                ctx,
                store,
                dig,
                &m,
                Disposition::Unresolved,
                snap.strata.clone(),
                "action undone",
            )?;
            undo_projection(
                store,
                dig,
                &m,
                snap.labels.contains(&INBOX.to_string()),
                key_suffix,
            )?;
        }
    }
    repo::messages::set_strata(store, &m.id, &snap.strata)?;
    // Labels are the provider's; the outbox processor updates them after the
    // undo projection applies. Writing them here would look like a conflict.
    match snap.snoozed_until {
        Some(u) => repo::drafts::snooze(store, m.id.as_str(), &u)?,
        None => repo::drafts::unsnooze(store, m.id.as_str())?,
    }
    Ok(Some(m))
}

fn restore_all(ctx: &ActionContext, restore: Value, label: &str) -> Result<ActionResult> {
    let state: RestoreState = serde_json::from_value(restore)
        .map_err(|e| RiddleError::invalid(format!("restore data: {e}")))?;
    let dig = repo::digs::get(&ctx.services.stores.index(), &state.dig_id)?
        .ok_or_else(|| RiddleError::not_found("Dig is gone"))?;
    let store = ctx.services.stores.dig(&dig.id)?;
    let mut affected = Vec::new();
    let mut skipped = Vec::new();
    for snap in &state.items {
        match restore_snapshot(ctx, &store, &dig, snap, ctx.activity_id.as_str())? {
            Some(m) => affected.push(EntityRef::new(EntityKind::Message, &m.id)),
            None => skipped.push(SkippedEntity {
                entity: EntityRef::new(EntityKind::Message, &snap.message_id),
                reason: "message no longer in the store".into(),
            }),
        }
    }
    ctx.services.emit(RiddleEvent::MessagesChanged {
        dig_id: dig.id.clone(),
        count: affected.len() as u64,
    });
    ctx.services.emit(RiddleEvent::SummaryChanged);
    let n = affected.len();
    Ok(ActionResult::ok(
        json!({"restored": n}),
        format!("{label} {n} message{}", if n == 1 { "" } else { "s" }),
    )
    .with_affected(affected)
    .with_inputs(json!({"digId": dig.id, "count": n}))
    .partial(skipped))
}

fn load(store: &Store, dig: &Dig, ids: &[MessageId]) -> Result<(Vec<Message>, Vec<SkippedEntity>)> {
    let mut found = Vec::new();
    let mut skipped = Vec::new();
    for id in ids {
        match repo::messages::get(store, &dig.id, id)? {
            Some(m) => found.push(m),
            None => skipped.push(SkippedEntity {
                entity: EntityRef::new(EntityKind::Message, id),
                reason: "not found".into(),
            }),
        }
    }
    if found.is_empty() {
        return Err(RiddleError::not_found("none of the messages exist"));
    }
    Ok((found, skipped))
}

fn user_move_meta(name: &str, description: &str) -> ActionMeta {
    meta(name, description)
        .mutating()
        .reversible()
        .projected()
        .risk(Risk::Medium)
        .confirm(ConfirmationPolicy::InPlaceUndo)
        .autonomy(Autonomy::Act)
        .build()
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

pub struct ArchiveMessages;
#[async_trait]
impl Action for ArchiveMessages {
    fn meta(&self) -> ActionMeta {
        user_move_meta("archive_messages", "Archive messages (Filed)")
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, store) = dig_and_store(ctx, &args)?;
        let ids = message_ids(&args)?;
        let (messages, skipped) = load(&store, &dig, &ids)?;
        let mut items = Vec::new();
        for m in &messages {
            let snap = snapshot(&store, m)?;
            let k = record_user_classification(
                ctx,
                &store,
                &dig,
                m,
                Disposition::Filed,
                snap.strata.clone(),
                "archived by the user",
            )?;
            enqueue_classification(&store, &dig, m, &k.disposition, &k.strata)?;
            items.push(snap);
        }
        let n = items.len();
        rinfo!(
            Category::Actions,
            "archived {} messages in dig {}",
            n,
            dig.id
        );
        ctx.services.emit(RiddleEvent::MessagesChanged {
            dig_id: dig.id.clone(),
            count: n as u64,
        });
        ctx.services.emit(RiddleEvent::SummaryChanged);
        let restore = serde_json::to_value(RestoreState {
            dig_id: dig.id.clone(),
            items,
        })?;
        Ok(ActionResult::ok(
            json!({"archived": n}),
            format!("Archived {n} message{}", plural(n)),
        )
        .with_affected(message_refs(
            &messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        ))
        .with_inputs(json!({"digId": dig.id, "count": n}))
        .with_undo(restore, "Unarchive")
        .partial(skipped))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        restore_all(ctx, restore, "Unarchived")
    }
}

pub struct RestoreMessages;
#[async_trait]
impl Action for RestoreMessages {
    fn meta(&self) -> ActionMeta {
        user_move_meta(
            "restore_messages",
            "Restore messages from Spoil or Contamination",
        )
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, store) = dig_and_store(ctx, &args)?;
        let ids = message_ids(&args)?;
        let (messages, mut skipped) = load(&store, &dig, &ids)?;
        let mut items = Vec::new();
        let mut affected = Vec::new();
        for m in &messages {
            let snap = snapshot(&store, m)?;
            let from = snap.classification.as_ref().map(|k| k.disposition.clone());
            if !matches!(
                from,
                Some(Disposition::Spoil)
                    | Some(Disposition::Contamination)
                    | Some(Disposition::Filed)
            ) {
                skipped.push(SkippedEntity {
                    entity: EntityRef::new(EntityKind::Message, &m.id),
                    reason: "not hidden".into(),
                });
                continue;
            }
            let k = record_user_classification(
                ctx,
                &store,
                &dig,
                m,
                Disposition::Unresolved,
                snap.strata.clone(),
                "restored by the user",
            )?;
            let correction = Correction {
                id: CorrectionId::new(),
                kind: CorrectionKind::Restore,
                dig_id: dig.id.clone(),
                message_id: m.id.clone(),
                from_disposition: from.clone(),
                to_disposition: Some(k.disposition.clone()),
                from_find_type: None,
                to_find_type: None,
                mesh_id: snap.classification.as_ref().and_then(|k| k.mesh_id.clone()),
                mesh_version: snap.classification.as_ref().and_then(|k| k.mesh_version),
                activity_id: Some(ctx.activity_id.clone()),
                created_at: ctx.services.clock.now(),
                retracted_at: None,
            };
            repo::corrections::record(&store, &correction)?;
            undo_projection(&store, &dig, m, true, ctx.activity_id.as_str())?;
            affected.push(EntityRef::new(EntityKind::Message, &m.id));
            items.push(snap);
        }
        let n = items.len();
        rinfo!(
            Category::Actions,
            "restored {} messages in dig {}",
            n,
            dig.id
        );
        ctx.services.emit(RiddleEvent::MessagesChanged {
            dig_id: dig.id.clone(),
            count: n as u64,
        });
        ctx.services.emit(RiddleEvent::SummaryChanged);
        let restore = serde_json::to_value(RestoreState {
            dig_id: dig.id.clone(),
            items,
        })?;
        Ok(ActionResult::ok(
            json!({"restored": n}),
            format!("Restored {n} message{}", plural(n)),
        )
        .with_affected(affected)
        .with_inputs(json!({"digId": dig.id, "count": n}))
        .with_undo(restore, "Hide again")
        .partial(skipped))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        // Retract the corrections this restore generated (§50), then put the
        // previous classifications back.
        if let Ok(state) = serde_json::from_value::<RestoreState>(restore.clone()) {
            if let Ok(store) = ctx.services.stores.dig(&state.dig_id) {
                for snap in &state.items {
                    for k in repo::corrections::for_message(&store, &snap.message_id)? {
                        if k.kind == CorrectionKind::Restore && k.retracted_at.is_none() {
                            if let Some(a) = &k.activity_id {
                                repo::corrections::retract_by_activity(&store, a)?;
                            }
                        }
                    }
                }
            }
        }
        restore_all(ctx, restore, "Hid")
    }
}

pub struct LabelMessages;
#[async_trait]
impl Action for LabelMessages {
    fn meta(&self) -> ActionMeta {
        user_move_meta("label_messages", "Add Strata to messages")
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, store) = dig_and_store(ctx, &args)?;
        let ids = message_ids(&args)?;
        let strata: Vec<String> = arg(&args, "strata")?;
        let strata: Vec<String> = strata
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if strata.is_empty() {
            return Err(RiddleError::invalid("strata is empty"));
        }
        let (messages, skipped) = load(&store, &dig, &ids)?;
        let mut items = Vec::new();
        for m in &messages {
            let snap = snapshot(&store, m)?;
            let mut merged = snap.strata.clone();
            for s in &strata {
                if !merged.contains(s) {
                    merged.push(s.clone());
                }
            }
            match &snap.classification {
                Some(k) => {
                    let updated = Classification {
                        id: ClassificationId::new(),
                        strata: merged.clone(),
                        created_at: ctx.services.clock.now(),
                        superseded_at: None,
                        series: None,
                        ..k.clone()
                    };
                    repo::classifications::record(&store, &updated)?;
                }
                None => repo::messages::set_strata(&store, &m.id, &merged)?,
            }
            if dig.projection != ProjectionMode::None {
                let p = ProjectionPlan {
                    add: strata.iter().map(|s| stratum_label(s)).collect(),
                    remove: vec![],
                };
                enqueue(&store, m, &p, "strata", None)?;
            }
            items.push(snap);
        }
        let n = items.len();
        ctx.services.emit(RiddleEvent::MessagesChanged {
            dig_id: dig.id.clone(),
            count: n as u64,
        });
        let restore = serde_json::to_value(RestoreState {
            dig_id: dig.id.clone(),
            items,
        })?;
        Ok(ActionResult::ok(
            json!({"labelled": n, "strata": strata}),
            format!("Added {} to {n} message{}", strata.join(", "), plural(n)),
        )
        .with_affected(message_refs(
            &messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        ))
        .with_inputs(json!({"digId": dig.id, "count": n, "strata": strata}))
        .with_undo(restore, "Remove Strata")
        .partial(skipped))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let state: RestoreState = serde_json::from_value(restore)
            .map_err(|e| RiddleError::invalid(format!("restore data: {e}")))?;
        let dig = repo::digs::get(&ctx.services.stores.index(), &state.dig_id)?
            .ok_or_else(|| RiddleError::not_found("Dig is gone"))?;
        let store = ctx.services.stores.dig(&dig.id)?;
        let mut affected = Vec::new();
        for snap in &state.items {
            let Some(m) = repo::messages::get(&store, &dig.id, &snap.message_id)? else {
                continue;
            };
            let current = repo::classifications::current(&store, &m.id)?;
            let now_strata = current
                .as_ref()
                .map(|k| k.strata.clone())
                .unwrap_or_default();
            let removed: Vec<String> = now_strata
                .iter()
                .filter(|s| !snap.strata.contains(s))
                .cloned()
                .collect();
            match current {
                Some(k) => repo::classifications::record(
                    &store,
                    &Classification {
                        id: ClassificationId::new(),
                        strata: snap.strata.clone(),
                        created_at: ctx.services.clock.now(),
                        superseded_at: None,
                        series: None,
                        ..k
                    },
                )?,
                None => repo::messages::set_strata(&store, &m.id, &snap.strata)?,
            }
            if dig.projection != ProjectionMode::None && !removed.is_empty() {
                repo::outbox::cancel_for_message(&store, m.id.as_str())?;
                let p = ProjectionPlan {
                    add: vec![],
                    remove: removed.iter().map(|s| stratum_label(s)).collect(),
                };
                enqueue(
                    &store,
                    &m,
                    &p,
                    "undo strata",
                    Some(ctx.activity_id.as_str()),
                )?;
            }
            affected.push(EntityRef::new(EntityKind::Message, &m.id));
        }
        let n = affected.len();
        ctx.services.emit(RiddleEvent::MessagesChanged {
            dig_id: dig.id.clone(),
            count: n as u64,
        });
        Ok(ActionResult::ok(
            json!({"restored": n}),
            format!("Removed Strata from {n} message{}", plural(n)),
        )
        .with_affected(affected)
        .with_inputs(json!({"digId": dig.id, "count": n})))
    }
}

pub struct SnoozeMessages;
#[async_trait]
impl Action for SnoozeMessages {
    fn meta(&self) -> ActionMeta {
        // Riddle-only (§15): no provider write, so no external side effect.
        meta("snooze_messages", "Snooze messages until a time")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .confirm(ConfirmationPolicy::InPlaceUndo)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, store) = dig_and_store(ctx, &args)?;
        let ids = message_ids(&args)?;
        let until = required_future_timestamp(&args, "until", ctx.services.clock.now())?;
        let (messages, skipped) = load(&store, &dig, &ids)?;
        let mut items = Vec::new();
        for m in &messages {
            items.push(snapshot(&store, m)?);
            repo::drafts::snooze(&store, m.id.as_str(), &until)?;
        }
        let n = items.len();
        let _ = opt::<String>(&args, "note");
        ctx.services.emit(RiddleEvent::MessagesChanged {
            dig_id: dig.id.clone(),
            count: n as u64,
        });
        let restore = serde_json::to_value(RestoreState {
            dig_id: dig.id.clone(),
            items,
        })?;
        Ok(ActionResult::ok(
            json!({"snoozed": n, "until": until}),
            format!(
                "Snoozed {n} message{} until {}",
                plural(n),
                until.format("%b %-d %H:%M")
            ),
        )
        .with_affected(message_refs(
            &messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        ))
        .with_inputs(json!({"digId": dig.id, "count": n, "until": until}))
        .with_undo(restore, "Unsnooze")
        .partial(skipped))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let state: RestoreState = serde_json::from_value(restore)
            .map_err(|e| RiddleError::invalid(format!("restore data: {e}")))?;
        let store = ctx.services.stores.dig(&state.dig_id)?;
        let mut affected = Vec::new();
        for snap in &state.items {
            match snap.snoozed_until {
                Some(u) => repo::drafts::snooze(&store, snap.message_id.as_str(), &u)?,
                None => repo::drafts::unsnooze(&store, snap.message_id.as_str())?,
            }
            affected.push(EntityRef::new(EntityKind::Message, &snap.message_id));
        }
        let n = affected.len();
        ctx.services.emit(RiddleEvent::MessagesChanged {
            dig_id: state.dig_id.clone(),
            count: n as u64,
        });
        Ok(ActionResult::ok(
            json!({"unsnoozed": n}),
            format!("Unsnoozed {n} message{}", plural(n)),
        )
        .with_affected(affected)
        .with_inputs(json!({"digId": state.dig_id, "count": n})))
    }
}

pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(ArchiveMessages),
        Arc::new(RestoreMessages),
        Arc::new(LabelMessages),
        Arc::new(SnoozeMessages),
    ]
}
