//! The undo engine (ARCHITECTURE.md §50). Undo is non-linear: an Activity
//! restores only the entities no later Activity touched, and lists the rest
//! as skipped. Undo All on a bulk Activity is the same path over the
//! remainder. Provider state is checked first through `ProviderStateCheck`
//! and a message moved elsewhere since is reported as a conflict, never
//! guessed at. The Action's own `undo` does the restoring (through
//! `Runtime::undo_via`), so Mesh changes revert their version and their
//! reclassifications the way the Mesh Action defines. Corrections the
//! Activity generated are retracted so a mistaken correction stops teaching
//! (§46). The original row is marked `Undone` and a new row with `undoes`
//! set records what happened.
//!
//! Partial restoration needs restore data keyed by entity (`RestoreMap`);
//! opaque restore data is all-or-nothing, and the engine says so rather
//! than restoring entities the user has since handled.
//!
//! "Handled since" means a later Activity by someone other than Sift. Sift
//! only classifies unclassified messages, so a later row with the Sift
//! actor is either the undone Activity's own consequence (it sent the
//! message back to Sift) or the message's first classification, never a
//! hand-edit; the first undo of "decide again" was skipped as "handled
//! since by sift_classify" every time until that row stopped counting.

use crate::format::undo_sentence;
use crate::ledger::{Entry, Ledger};
use async_trait::async_trait;
use riddle_actions::runtime::Runtime;
use riddle_actions::{ActionContext, ActionResult};
use riddle_core::{
    Activity, ActivityId, ActivityResult, Actor, EntityKind, EntityRef, Principal, RestoreMap,
    SkippedEntity, UndoConflict, UndoInfo, UndoPreview, UndoReport,
};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, rwarn, Category};
use riddle_store::{repo, Stores};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;

/// Whether the provider's copy of an entity changed since `at`. The mail
/// track implements this from the message's current labels and history id;
/// `NoProviderCheck` answers no (fixture Digs, local-only entities).
#[async_trait]
pub trait ProviderStateCheck: Send + Sync {
    async fn changed_since(
        &self,
        entity: &EntityRef,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool>;
}

pub struct NoProviderCheck;
#[async_trait]
impl ProviderStateCheck for NoProviderCheck {
    async fn changed_since(
        &self,
        _entity: &EntityRef,
        _at: chrono::DateTime<chrono::Utc>,
    ) -> Result<bool> {
        Ok(false)
    }
}

pub const HANDLED_SINCE: &str = "handled since";
pub const ALL_OR_NOTHING: &str = "cannot be restored separately from entities handled since";

struct Plan {
    activity: Activity,
    label: String,
    restore: Value,
    restorable: Vec<EntityRef>,
    skipped: Vec<SkippedEntity>,
    conflicts: Vec<UndoConflict>,
    /// False when there is nothing left to restore.
    should_run: bool,
}

pub struct UndoEngine {
    provider: Arc<dyn ProviderStateCheck>,
}

impl UndoEngine {
    pub fn new(provider: Arc<dyn ProviderStateCheck>) -> Self {
        Self { provider }
    }

    async fn plan(&self, stores: &Stores, id: &ActivityId) -> Result<Plan> {
        let index = stores.index();
        let activity = repo::activities::get(&index, id)?
            .ok_or_else(|| RiddleError::not_found(format!("activity {id} not found")))?;
        let (restore, label) = match &activity.undo {
            UndoInfo::Undoable { restore, label } => (restore.clone(), label.clone()),
            UndoInfo::NotUndoable { reason } => {
                return Err(RiddleError::invalid(format!(
                    "this cannot be undone: {reason}"
                )))
            }
            UndoInfo::Undone {
                at,
                undo_activity_id,
                ..
            } => {
                return Err(
                    RiddleError::invalid(format!("already undone at {}", at.to_rfc3339()))
                        .with_detail(json!({ "undoActivityId": undo_activity_id })),
                );
            }
        };

        // Later Activities that touched the same entities (non-linear undo).
        let candidates = repo::activities::since(
            &index,
            &(activity.created_at - chrono::Duration::milliseconds(1)),
        )?;
        let mut touched: HashSet<EntityRef> = HashSet::new();
        let mut touched_by: std::collections::HashMap<EntityRef, String> =
            std::collections::HashMap::new();
        for later in candidates.iter().filter(|a| {
            a.id != activity.id && (a.created_at > activity.created_at || a.id > activity.id)
        }) {
            if matches!(later.result, ActivityResult::Failed { .. }) {
                continue;
            }
            // Sift's rows are consequences or first classifications, never
            // hand-edits (module doc).
            if later.actor == Actor::Sift {
                continue;
            }
            for e in &later.affected {
                if touched.insert(e.clone()) {
                    touched_by.insert(e.clone(), later.action.clone());
                }
            }
        }

        let mut restorable = Vec::new();
        let mut skipped = Vec::new();
        let mut conflicts = Vec::new();
        for entity in &activity.affected {
            if touched.contains(entity) {
                skipped.push(SkippedEntity {
                    entity: entity.clone(),
                    reason: format!(
                        "{HANDLED_SINCE} by {}",
                        touched_by.get(entity).cloned().unwrap_or_default()
                    ),
                });
                continue;
            }
            if matches!(entity.kind, EntityKind::Message | EntityKind::Thread) {
                match self
                    .provider
                    .changed_since(entity, activity.created_at)
                    .await
                {
                    Ok(false) => {}
                    Ok(true) => {
                        conflicts.push(UndoConflict {
                            entity: entity.clone(),
                            detail: "changed in the mailbox since".into(),
                        });
                        continue;
                    }
                    Err(e) => {
                        conflicts.push(UndoConflict {
                            entity: entity.clone(),
                            detail: format!("could not verify the mailbox: {}", e.message),
                        });
                        continue;
                    }
                }
            }
            restorable.push(entity.clone());
        }

        let partial = !skipped.is_empty() || !conflicts.is_empty();
        let restore = match RestoreMap::from_value(&restore) {
            Some(mut map) => {
                let keep: HashSet<String> = restorable.iter().map(|e| e.id.clone()).collect();
                map.retain(&keep);
                map.to_value()
            }
            None if partial && !restorable.is_empty() => {
                // Opaque restore data cannot be split; be honest.
                for e in restorable.drain(..) {
                    skipped.push(SkippedEntity {
                        entity: e,
                        reason: ALL_OR_NOTHING.into(),
                    });
                }
                Value::Null
            }
            None => restore,
        };
        let should_run = !restorable.is_empty() || activity.affected.is_empty();
        Ok(Plan {
            activity,
            label,
            restore,
            restorable,
            skipped,
            conflicts,
            should_run,
        })
    }

    /// What an undo would do, for the in-place confirmation.
    pub async fn preview(&self, stores: &Stores, id: &ActivityId) -> Result<UndoPreview> {
        let p = self.plan(stores, id).await?;
        Ok(UndoPreview {
            activity_id: p.activity.id.clone(),
            label: p.label,
            summary: p.activity.summary.clone(),
            will_restore: p.restorable,
            will_skip: p.skipped,
            conflicts: p.conflicts,
        })
    }

    /// Undo and record the undo Activity here (CLI, tests, direct callers).
    pub async fn undo(
        &self,
        runtime: &Runtime,
        id: &ActivityId,
        principal: Principal,
    ) -> Result<UndoReport> {
        let undo_id = ActivityId::new();
        let (report, result) = self
            .perform(runtime, id, principal.clone(), undo_id.clone())
            .await?;
        if let Some(result) = result {
            let ledger = Ledger::new(
                runtime.services.stores.index(),
                runtime.services.events.clone(),
                runtime.services.clock.clone(),
            );
            let mut entry = Entry::new(principal.actor(), "undo_activity", result.summary)
                .affected(result.affected)
                .result(result.result)
                .inputs(result.inputs)
                .undoes(id.clone());
            entry.undo = result.undo;
            if let Some(dig) = report_dig(runtime, id) {
                entry = entry.dig(dig);
            }
            if let Err(e) = ledger.append_with_id(undo_id, entry) {
                riddle_observability::swallowed!(
                    Category::Actions,
                    "undo_activity",
                    e,
                    "could not record undo activity for {id}"
                );
            }
        }
        Ok(report)
    }

    /// Undo from inside the `undo_activity` Action: the runtime appends the
    /// Activity from the returned `ActionResult`, under `ctx.activity_id`.
    pub async fn undo_within(
        &self,
        runtime: &Runtime,
        id: &ActivityId,
        ctx: &ActionContext,
    ) -> Result<(UndoReport, ActionResult)> {
        let (report, result) = self
            .perform(runtime, id, ctx.principal.clone(), ctx.activity_id.clone())
            .await?;
        let result = result.unwrap_or_else(|| nothing_restored_result(id, &report));
        Ok((report, result))
    }

    async fn perform(
        &self,
        runtime: &Runtime,
        id: &ActivityId,
        principal: Principal,
        undo_id: ActivityId,
    ) -> Result<(UndoReport, Option<ActionResult>)> {
        let stores = runtime.services.stores.clone();
        let plan = self.plan(&stores, id).await?;
        if !plan.should_run {
            rinfo!(
                Category::Actions,
                "undo of {id}: nothing to restore ({} skipped, {} conflicts)",
                plan.skipped.len(),
                plan.conflicts.len()
            );
            return Ok((
                UndoReport {
                    activity_id: id.clone(),
                    restored: vec![],
                    skipped: plan.skipped,
                    conflicts: plan.conflicts,
                    undo_activity_id: None,
                },
                None,
            ));
        }

        // Hold (§59) applies to undo as to the forward action. Undo is the
        // user's own click, so Ask lets it through; a projected Action's
        // undo only queues writes, which wait in the outbox.
        let meta = runtime.action(&plan.activity.action)?.meta();
        {
            let policy = runtime.policy.read();
            if policy.dev
                && meta.external_side_effect
                && policy.writes == riddle_core::ProviderWrites::Hold
            {
                return Err(RiddleError::policy_denied(
                    riddle_actions::policy::HOLD_DENIAL,
                ));
            }
        }

        let outcome = runtime
            .undo_via(
                &plan.activity.action,
                plan.restore.clone(),
                principal.clone(),
                plan.activity.dig_id.clone(),
                undo_id.clone(),
            )
            .await?;

        let retracted = retract_corrections(&stores, &plan.activity);
        let ledger = Ledger::new(
            stores.index(),
            runtime.services.events.clone(),
            runtime.services.clock.clone(),
        );
        if let Err(e) = ledger.mark_undone(id, principal.actor(), undo_id.clone()) {
            riddle_observability::swallowed!(
                Category::Actions,
                "mark_undone",
                e,
                "could not mark activity {id} undone"
            );
        }
        rinfo!(
            Category::Actions,
            "undid {id} ({}): {} restored, {} skipped, {} conflicts, {} corrections retracted",
            plan.activity.action,
            plan.restorable.len(),
            plan.skipped.len(),
            plan.conflicts.len(),
            retracted
        );

        let report = UndoReport {
            activity_id: id.clone(),
            restored: plan.restorable.clone(),
            skipped: plan.skipped.clone(),
            conflicts: plan.conflicts.clone(),
            undo_activity_id: Some(undo_id),
        };
        let mut all_skipped = plan.skipped.clone();
        all_skipped.extend(plan.conflicts.iter().map(|c| SkippedEntity {
            entity: c.entity.clone(),
            reason: c.detail.clone(),
        }));
        let mut result = ActionResult::ok(serde_json::to_value(&report)?, undo_sentence(principal.actor(), &plan.activity.summary))
            .with_affected(plan.restorable.clone())
            .with_inputs(json!({ "activityId": id, "restored": plan.restorable.len(), "skipped": plan.skipped.len(), "conflicts": plan.conflicts.len(), "correctionsRetracted": retracted }))
            .with_undoes(id.clone())
            .partial(all_skipped);
        result.why = outcome
            .why
            .or(Some(format!("Undid {}", plan.activity.action)));
        result.undo = UndoInfo::NotUndoable {
            reason: "an undo is not undone; repeat the action instead".into(),
        };
        Ok((report, Some(result)))
    }
}

fn nothing_restored_result(id: &ActivityId, report: &UndoReport) -> ActionResult {
    let mut skipped = report.skipped.clone();
    skipped.extend(report.conflicts.iter().map(|c| SkippedEntity {
        entity: c.entity.clone(),
        reason: c.detail.clone(),
    }));
    let mut r = ActionResult::ok(serde_json::to_value(report).unwrap_or(Value::Null), "Nothing to undo: every entity was handled since")
        .with_inputs(json!({ "activityId": id, "restored": 0, "skipped": report.skipped.len(), "conflicts": report.conflicts.len() }))
        .partial(skipped);
    r.undo = UndoInfo::NotUndoable {
        reason: "nothing was restored".into(),
    };
    r
}

fn report_dig(runtime: &Runtime, id: &ActivityId) -> Option<riddle_core::DigId> {
    repo::activities::get(&runtime.services.stores.index(), id)
        .ok()
        .flatten()
        .and_then(|a| a.dig_id)
}

/// Corrections live in Dig stores; retract in the Activity's Dig or, for a
/// cross-Dig Activity, in every open one. Bookkeeping: failures are logged.
fn retract_corrections(stores: &Stores, activity: &Activity) -> u64 {
    let targets: Vec<Arc<riddle_store::Store>> = match &activity.dig_id {
        Some(d) => stores.dig(d).map(|s| vec![s]).unwrap_or_default(),
        None => stores.open_digs().into_iter().map(|(_, s)| s).collect(),
    };
    let mut n = 0;
    for store in targets {
        match repo::corrections::retract_by_activity(&store, &activity.id) {
            Ok(k) => n += k,
            Err(e) => rwarn!(
                Category::Actions,
                "could not retract corrections for {}: {}",
                activity.id,
                e.message
            ),
        }
    }
    n
}
