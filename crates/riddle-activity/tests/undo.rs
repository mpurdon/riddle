//! Undo semantics (§50) against a fake bulk Action: non-linear skipping,
//! Undo All over the remainder, refusal when already undone or not
//! undoable, correction retraction, provider conflicts, all-or-nothing
//! restore data, and the `undo_activity` Action through the runtime.

use async_trait::async_trait;
use parking_lot::Mutex;
use riddle_actions::policy::AutonomyPolicy;
use riddle_actions::{
    meta, Action, ActionContext, ActionResult, Preview, Runtime, RuntimeBuilder, Services,
};
use riddle_activity::{
    actions, Entry, Ledger, NoProviderCheck, ProviderStateCheck, RuntimeHandle, UndoEngine,
    ALL_OR_NOTHING, HANDLED_SINCE,
};
use riddle_core::*;
use riddle_errors::{ErrorKind, Result, RiddleError};
use riddle_store::{repo, Stores};
use serde_json::{json, Value};
use std::sync::Arc;

/// A fake "move to Spoil": records dispositions and, on undo, which ids it
/// restored.
#[derive(Default)]
struct Mailbox {
    dispositions: Mutex<std::collections::BTreeMap<String, String>>,
    restored: Mutex<Vec<String>>,
}

struct Spoil(Arc<Mailbox>, bool);

#[async_trait]
impl Action for Spoil {
    fn meta(&self) -> ActionMeta {
        let m = meta("move_to_spoil", "Move messages to Spoil")
            .mutating()
            .reversible()
            .confirm(ConfirmationPolicy::InPlaceUndo)
            .autonomy(Autonomy::Act)
            .risk(Risk::Medium);
        if self.1 {
            m.external().build()
        } else {
            m.build()
        }
    }
    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        Ok(Preview {
            summary: "move".into(),
            real_arguments: args.clone(),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let ids: Vec<String> = serde_json::from_value(args["ids"].clone()).unwrap();
        let per_entity = args
            .get("perEntity")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut map = RestoreMap::new();
        let mut opaque = Vec::new();
        for id in &ids {
            let prev = self
                .0
                .dispositions
                .lock()
                .insert(id.clone(), "spoil".into())
                .unwrap_or_else(|| "unresolved".into());
            map = map.entity(id, json!({ "disposition": prev }));
            opaque.push(json!({ "id": id, "disposition": prev }));
        }
        let affected = ids
            .iter()
            .map(|i| EntityRef::new(EntityKind::Message, i))
            .collect();
        let restore = if per_entity {
            map.to_value()
        } else {
            json!({ "list": opaque })
        };
        Ok(ActionResult::ok(
            json!({ "moved": ids.len() }),
            format!(
                "{} moved {} messages to Spoil",
                riddle_activity::actor_name(ctx.principal.actor()),
                ids.len()
            ),
        )
        .with_affected(affected)
        .with_undo(restore, "Undo"))
    }
    async fn undo(&self, _ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let restored: Vec<String> = match RestoreMap::from_value(&restore) {
            Some(map) => map
                .entities
                .into_iter()
                .map(|(id, v)| {
                    self.0
                        .dispositions
                        .lock()
                        .insert(id.clone(), v["disposition"].as_str().unwrap().into());
                    id
                })
                .collect(),
            None => restore["list"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    let id = v["id"].as_str().unwrap().to_string();
                    self.0
                        .dispositions
                        .lock()
                        .insert(id.clone(), v["disposition"].as_str().unwrap().into());
                    id
                })
                .collect(),
        };
        self.0.restored.lock().extend(restored.iter().cloned());
        Ok(
            ActionResult::ok(json!({ "restored": restored.len() }), "restored").with_affected(
                restored
                    .iter()
                    .map(|i| EntityRef::new(EntityKind::Message, i))
                    .collect(),
            ),
        )
    }
}

struct Harness {
    stores: Arc<Stores>,
    events: Arc<RecordingSink>,
    clock: Arc<FixedClock>,
    runtime: Arc<Runtime>,
    mailbox: Arc<Mailbox>,
    engine: Arc<UndoEngine>,
}

fn harness_with(provider: Arc<dyn ProviderStateCheck>, external: bool, dev: bool) -> Harness {
    let stores = Arc::new(Stores::in_memory().unwrap());
    let events = Arc::new(RecordingSink::default());
    let clock = Arc::new(FixedClock::at(chrono::Utc::now()));
    let mut services = Services::new(
        stores.clone(),
        events.clone(),
        BuildStamp::from_env_vars("0", None, None, None, "test"),
    );
    services.clock = clock.clone();
    let services = Arc::new(services);
    let mailbox = Arc::new(Mailbox::default());
    let engine = Arc::new(UndoEngine::new(provider));
    services.insert(engine.clone());
    let mut builder = RuntimeBuilder::new(services.clone())
        .register(Arc::new(Spoil(mailbox.clone(), external)))
        .policy(AutonomyPolicy {
            default: Autonomy::Act,
            dev,
            armed: false,
            ..Default::default()
        });
    for a in actions() {
        builder = builder.register(a);
    }
    let runtime = builder.build();
    services.insert(Arc::new(RuntimeHandle::new(&runtime)));
    Harness {
        stores,
        events,
        clock,
        runtime,
        mailbox,
        engine,
    }
}

fn harness() -> Harness {
    harness_with(Arc::new(NoProviderCheck), false, false)
}

impl Harness {
    async fn spoil(&self, ids: &[&str]) -> ActivityId {
        self.spoil_with(ids, true).await
    }
    async fn spoil_with(&self, ids: &[&str], per_entity: bool) -> ActivityId {
        let out = self
            .runtime
            .invoke_as_user(
                "move_to_spoil",
                json!({ "ids": ids, "perEntity": per_entity }),
            )
            .await
            .unwrap();
        self.clock.advance(chrono::Duration::seconds(1));
        out.activity_id.unwrap()
    }
    fn activity(&self, id: &ActivityId) -> Activity {
        repo::activities::get(&self.stores.index(), id)
            .unwrap()
            .unwrap()
    }
    fn ledger(&self) -> Ledger {
        Ledger::new(self.stores.index(), self.events.clone(), self.clock.clone())
    }
}

#[tokio::test]
async fn undo_all_skips_the_entity_handled_since_and_lists_it() {
    let h = harness();
    let bulk = h.spoil(&["m1", "m2", "m3"]).await;
    // The user handled m2 by hand afterwards.
    h.ledger()
        .append(
            Entry::new(Actor::User, "archive_messages", "You archived 1 message")
                .affected(vec![EntityRef::new(EntityKind::Message, "m2")]),
        )
        .unwrap();
    h.clock.advance(chrono::Duration::seconds(1));
    h.events.take();

    let preview = h.engine.preview(&h.stores, &bulk).await.unwrap();
    assert_eq!(preview.will_restore.len(), 2);
    assert_eq!(preview.will_skip[0].entity.id, "m2");

    let report = h
        .engine
        .undo(&h.runtime, &bulk, Principal::User)
        .await
        .unwrap();
    assert_eq!(
        report
            .restored
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["m1", "m3"]
    );
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].entity.id, "m2");
    assert!(
        report.skipped[0].reason.starts_with(HANDLED_SINCE),
        "{}",
        report.skipped[0].reason
    );
    assert!(report.conflicts.is_empty());
    assert_eq!(h.mailbox.restored.lock().clone(), vec!["m1", "m3"]);
    assert_eq!(h.mailbox.dispositions.lock()["m2"], "spoil");
    assert_eq!(h.mailbox.dispositions.lock()["m1"], "unresolved");

    let undo_id = report.undo_activity_id.clone().unwrap();
    let original = h.activity(&bulk);
    assert!(
        matches!(&original.undo, UndoInfo::Undone { by: Actor::User, undo_activity_id, .. } if *undo_activity_id == undo_id)
    );
    let undo_activity = h.activity(&undo_id);
    assert_eq!(undo_activity.undoes, Some(bulk.clone()));
    assert_eq!(undo_activity.action, "undo_activity");
    assert_eq!(
        undo_activity.summary,
        "You undid: You moved 3 messages to Spoil"
    );
    assert!(
        matches!(&undo_activity.result, ActivityResult::Partial { skipped } if skipped.len() == 1)
    );
    assert!(matches!(undo_activity.undo, UndoInfo::NotUndoable { .. }));
    let appended = h
        .events
        .take()
        .iter()
        .filter(|e| matches!(e, RiddleEvent::ActivityAppended { .. }))
        .count();
    assert_eq!(
        appended, 2,
        "the undone original and the undo row are both published"
    );
}

#[tokio::test]
async fn an_already_undone_activity_is_refused() {
    let h = harness();
    let id = h.spoil(&["m1"]).await;
    h.engine
        .undo(&h.runtime, &id, Principal::User)
        .await
        .unwrap();
    let err = h
        .engine
        .undo(&h.runtime, &id, Principal::User)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Invalid);
    assert!(err.message.contains("already undone"));
    assert!(err.detail.unwrap()["undoActivityId"].is_string());
    assert_eq!(h.mailbox.restored.lock().len(), 1, "no second restore");
}

#[tokio::test]
async fn a_not_undoable_activity_is_refused_and_a_missing_one_is_not_found() {
    let h = harness();
    let a = h
        .ledger()
        .append(
            Entry::new(Actor::User, "send_message", "You sent 1 message")
                .not_undoable("Sent mail cannot be unsent"),
        )
        .unwrap();
    let err = h
        .engine
        .undo(&h.runtime, &a.id, Principal::User)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Invalid);
    assert!(err.message.contains("Sent mail cannot be unsent"));
    let err = h
        .engine
        .undo(&h.runtime, &ActivityId::new(), Principal::User)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn undo_retracts_the_correction_the_activity_generated() {
    let h = harness();
    let dig = DigId::from("dig-1");
    let out = h
        .runtime
        .invoke(ActionInvocation {
            action: "move_to_spoil".into(),
            args: json!({ "ids": ["m1"] }),
            principal: Principal::User,
            dig_id: Some(dig.clone()),
            idempotency_key: None,
            confirmed: true,
            pending_id: None,
        })
        .await
        .unwrap();
    let id = out.activity_id.unwrap();
    let dig_store = h.stores.dig(&dig).unwrap();
    let message = MessageId::for_provider(&dig, "m1");
    let correction = Correction {
        id: CorrectionId::new(),
        kind: CorrectionKind::Move,
        dig_id: dig.clone(),
        message_id: message.clone(),
        from_disposition: Some(Disposition::Unresolved),
        to_disposition: Some(Disposition::Spoil),
        from_find_type: None,
        to_find_type: None,
        mesh_id: None,
        mesh_version: None,
        activity_id: Some(id.clone()),
        created_at: h.clock.now(),
        retracted_at: None,
    };
    repo::corrections::record(&dig_store, &correction).unwrap();
    let unrelated = Correction {
        id: CorrectionId::new(),
        activity_id: Some(ActivityId::new()),
        ..correction.clone()
    };
    repo::corrections::record(&dig_store, &unrelated).unwrap();
    h.clock.advance(chrono::Duration::seconds(1));

    h.engine
        .undo(&h.runtime, &id, Principal::User)
        .await
        .unwrap();
    let corrections = repo::corrections::for_message(&dig_store, &message).unwrap();
    let ours = corrections.iter().find(|c| c.id == correction.id).unwrap();
    assert!(ours.retracted_at.is_some(), "the correction stops teaching");
    let theirs = corrections.iter().find(|c| c.id == unrelated.id).unwrap();
    assert!(theirs.retracted_at.is_none());
    assert_eq!(repo::corrections::active(&dig_store, 10).unwrap().len(), 1);
}

struct ChangedIn(&'static [&'static str]);
#[async_trait]
impl ProviderStateCheck for ChangedIn {
    async fn changed_since(
        &self,
        entity: &EntityRef,
        _at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool> {
        if entity.id == "boom" {
            return Err(RiddleError::provider("offline"));
        }
        Ok(self.0.contains(&entity.id.as_str()))
    }
}

#[tokio::test]
async fn provider_changes_are_reported_as_conflicts_not_guessed() {
    let h = harness_with(Arc::new(ChangedIn(&["m2"])), false, false);
    let id = h.spoil(&["m1", "m2", "boom"]).await;
    let report = h
        .engine
        .undo(&h.runtime, &id, Principal::User)
        .await
        .unwrap();
    assert_eq!(
        report
            .restored
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["m1"]
    );
    assert_eq!(report.conflicts.len(), 2);
    assert_eq!(report.conflicts[0].entity.id, "m2");
    assert!(report.conflicts[1].detail.contains("could not verify"));
    assert_eq!(h.mailbox.restored.lock().clone(), vec!["m1"]);
}

#[tokio::test]
async fn when_everything_was_handled_since_nothing_is_restored_and_the_original_stays_undoable() {
    let h = harness();
    let id = h.spoil(&["m1"]).await;
    h.ledger()
        .append(
            Entry::new(Actor::User, "restore_messages", "You restored 1 message")
                .affected(vec![EntityRef::new(EntityKind::Message, "m1")]),
        )
        .unwrap();
    let report = h
        .engine
        .undo(&h.runtime, &id, Principal::User)
        .await
        .unwrap();
    assert!(report.restored.is_empty());
    assert!(report.undo_activity_id.is_none());
    assert!(matches!(h.activity(&id).undo, UndoInfo::Undoable { .. }));
    assert!(h.mailbox.restored.lock().is_empty());
}

#[tokio::test]
async fn opaque_restore_data_is_all_or_nothing() {
    let h = harness();
    let id = h.spoil_with(&["m1", "m2"], false).await;
    h.ledger()
        .append(
            Entry::new(Actor::User, "archive_messages", "You archived 1 message")
                .affected(vec![EntityRef::new(EntityKind::Message, "m2")]),
        )
        .unwrap();
    let report = h
        .engine
        .undo(&h.runtime, &id, Principal::User)
        .await
        .unwrap();
    assert!(report.restored.is_empty());
    assert_eq!(report.skipped.len(), 2);
    assert_eq!(report.skipped[1].reason, ALL_OR_NOTHING);
    assert!(report.undo_activity_id.is_none());

    // Without a later touch, opaque data restores everything.
    let whole = h.spoil_with(&["m3", "m4"], false).await;
    let report = h
        .engine
        .undo(&h.runtime, &whole, Principal::User)
        .await
        .unwrap();
    assert_eq!(report.restored.len(), 2);
}

#[tokio::test]
async fn a_failed_later_activity_does_not_count_as_handling() {
    let h = harness();
    let id = h.spoil(&["m1"]).await;
    h.ledger()
        .append(
            Entry::new(Actor::User, "archive_messages", "archive failed")
                .affected(vec![EntityRef::new(EntityKind::Message, "m1")])
                .result(ActivityResult::Failed {
                    error: "nope".into(),
                }),
        )
        .unwrap();
    let report = h
        .engine
        .undo(&h.runtime, &id, Principal::User)
        .await
        .unwrap();
    assert_eq!(report.restored.len(), 1);
}

#[tokio::test]
async fn the_undo_action_confirms_in_place_and_records_one_activity_with_undoes() {
    let h = harness();
    let bulk = h.spoil(&["m1", "m2"]).await;
    h.ledger()
        .append(
            Entry::new(Actor::User, "archive_messages", "You archived 1 message")
                .affected(vec![EntityRef::new(EntityKind::Message, "m2")]),
        )
        .unwrap();
    h.clock.advance(chrono::Duration::seconds(1));
    let inv = ActionInvocation {
        action: "undo_activity".into(),
        args: json!({ "activityId": bulk }),
        principal: Principal::User,
        dig_id: None,
        idempotency_key: None,
        confirmed: false,
        pending_id: None,
    };
    let err = h.runtime.invoke(inv.clone()).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::NeedsConfirmation);
    let pending: PendingAction = serde_json::from_value(err.detail.unwrap()).unwrap();
    assert_eq!(pending.confirmation, ConfirmationPolicy::InPlaceUndo);
    assert!(
        pending.summary.contains("restores 1, skips 1"),
        "{}",
        pending.summary
    );
    let preview: UndoPreview = serde_json::from_value(pending.real_arguments).unwrap();
    assert_eq!(preview.will_restore[0].id, "m1");
    assert_eq!(preview.will_skip[0].entity.id, "m2");

    let before = repo::activities::list(
        &h.stores.index(),
        &Default::default(),
        &PageRequest::default(),
    )
    .unwrap()
    .total
    .unwrap();
    let out = h
        .runtime
        .invoke(ActionInvocation {
            pending_id: Some(pending.id),
            ..inv
        })
        .await
        .unwrap();
    let report: UndoReport = serde_json::from_value(out.result).unwrap();
    assert_eq!(report.restored.len(), 1);
    let undo_id = out.activity_id.unwrap();
    assert_eq!(report.undo_activity_id, Some(undo_id.clone()));
    let after = repo::activities::list(
        &h.stores.index(),
        &Default::default(),
        &PageRequest::default(),
    )
    .unwrap()
    .total
    .unwrap();
    assert_eq!(after, before + 1, "exactly one Activity for the undo");
    let row = h.activity(&undo_id);
    assert_eq!(row.undoes, Some(bulk.clone()));
    assert!(matches!(&row.result, ActivityResult::Partial { skipped } if skipped.len() == 1));
    assert!(
        matches!(&h.activity(&bulk).undo, UndoInfo::Undone { undo_activity_id, .. } if *undo_activity_id == undo_id)
    );

    // The undo row itself is not undoable, and the original is refused now.
    let err = h
        .runtime
        .invoke_as_user("undo_activity", json!({ "activityId": undo_id }))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Invalid);
    let err = h
        .runtime
        .invoke_as_user("undo_activity", json!({ "activityId": bulk }))
        .await
        .unwrap_err();
    assert!(err.message.contains("already undone"));
}

#[tokio::test]
async fn an_unarmed_dev_build_refuses_to_undo_a_provider_mutation() {
    let h = harness_with(Arc::new(NoProviderCheck), true, true);
    h.runtime.arm(true);
    let id = h.spoil(&["m1"]).await;
    h.runtime.arm(false);
    let err = h
        .engine
        .undo(&h.runtime, &id, Principal::User)
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::PolicyDenied);
    assert!(matches!(h.activity(&id).undo, UndoInfo::Undoable { .. }));
}

#[tokio::test]
async fn activities_can_be_listed_by_affected_entity_mesh_and_undoable() {
    let h = harness();
    let a = h.spoil(&["m1", "m2"]).await;
    let mesh = MeshId::new();
    h.ledger()
        .append(
            Entry::new(
                Actor::Sift,
                "move_to_spoil",
                "Sift moved 1 message to Spoil",
            )
            .affected(vec![EntityRef::new(EntityKind::Message, "m9")])
            .provenance(Provenance {
                mechanism: DecisionMechanism::Mesh,
                mesh_id: Some(mesh.clone()),
                mesh_version: Some(3),
                mesh_name: Some("Routine".into()),
                trace_id: None,
                note: None,
            }),
        )
        .unwrap();
    let index = h.stores.index();
    let by_entity = repo::activities::list_where(
        &index,
        &Default::default(),
        &repo::activities::ActivityQuery {
            affected_id: Some("m2".into()),
            ..Default::default()
        },
        &PageRequest::default(),
    )
    .unwrap();
    assert_eq!(by_entity.items.len(), 1);
    assert_eq!(by_entity.items[0].id, a);
    let by_mesh = repo::activities::list_where(
        &index,
        &Default::default(),
        &repo::activities::ActivityQuery {
            mesh_id: Some(mesh.0.clone()),
            ..Default::default()
        },
        &PageRequest::default(),
    )
    .unwrap();
    assert_eq!(by_mesh.items.len(), 1);
    assert_eq!(by_mesh.items[0].actor, Actor::Sift);
    let undoable = repo::activities::list_where(
        &index,
        &Default::default(),
        &repo::activities::ActivityQuery {
            undoable_only: true,
            ..Default::default()
        },
        &PageRequest::default(),
    )
    .unwrap();
    assert_eq!(undoable.items.len(), 1);
}
