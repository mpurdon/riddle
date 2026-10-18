//! `Ledger`: append Activity rows outside the Action runtime. The runtime
//! records every Action invocation itself; this is for the producers that
//! are not invocations (Sift's bulk decisions, Mesh applications, the undo
//! engine, system events) so they land in the same table with the same
//! shape and the same `ActivityAppended` event. Appending is bookkeeping:
//! `try_append` reports a failure and returns `None` rather than failing
//! the work that produced the Activity.

use riddle_core::{
    Activity, ActivityId, ActivityResult, Actor, Clock, DigId, EntityRef, EventSink, Provenance,
    RiddleEvent, TraceId, UndoInfo,
};
use riddle_errors::Result;
use riddle_observability::{rdebug, Category};
use riddle_store::{repo, Store};
use serde_json::Value;
use std::sync::Arc;

/// A row before it has an id and a time.
#[derive(Debug, Clone)]
pub struct Entry {
    pub actor: Actor,
    pub action: String,
    pub summary: String,
    pub why: Option<String>,
    pub inputs: Value,
    pub affected: Vec<EntityRef>,
    pub result: ActivityResult,
    pub provenance: Option<Provenance>,
    pub undo: UndoInfo,
    pub dig_id: Option<DigId>,
    pub trace_id: Option<TraceId>,
    pub undoes: Option<ActivityId>,
}

impl Entry {
    pub fn new(actor: Actor, action: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            actor,
            action: action.into(),
            summary: summary.into(),
            why: None,
            inputs: Value::Null,
            affected: vec![],
            result: ActivityResult::Ok,
            provenance: None,
            undo: UndoInfo::NotUndoable {
                reason: "not reversible".into(),
            },
            dig_id: None,
            trace_id: None,
            undoes: None,
        }
    }
    pub fn why(mut self, why: impl Into<String>) -> Self {
        self.why = Some(why.into());
        self
    }
    /// Identifiers and counts only.
    pub fn inputs(mut self, inputs: Value) -> Self {
        self.inputs = inputs;
        self
    }
    pub fn affected(mut self, affected: Vec<EntityRef>) -> Self {
        self.affected = affected;
        self
    }
    pub fn result(mut self, result: ActivityResult) -> Self {
        self.result = result;
        self
    }
    pub fn provenance(mut self, p: Provenance) -> Self {
        if self.trace_id.is_none() {
            self.trace_id = p.trace_id.clone();
        }
        self.provenance = Some(p);
        self
    }
    pub fn undoable(mut self, restore: Value, label: impl Into<String>) -> Self {
        self.undo = UndoInfo::Undoable {
            restore,
            label: label.into(),
        };
        self
    }
    pub fn not_undoable(mut self, reason: impl Into<String>) -> Self {
        self.undo = UndoInfo::NotUndoable {
            reason: reason.into(),
        };
        self
    }
    pub fn dig(mut self, dig: DigId) -> Self {
        self.dig_id = Some(dig);
        self
    }
    pub fn trace(mut self, trace: TraceId) -> Self {
        self.trace_id = Some(trace);
        self
    }
    pub fn undoes(mut self, id: ActivityId) -> Self {
        self.undoes = Some(id);
        self
    }

    pub fn into_activity(self, id: ActivityId, at: chrono::DateTime<chrono::Utc>) -> Activity {
        Activity {
            id,
            actor: self.actor,
            action: self.action,
            summary: self.summary,
            why: self.why,
            inputs: self.inputs,
            affected: self.affected,
            result: self.result,
            provenance: self.provenance,
            undo: self.undo,
            dig_id: self.dig_id,
            trace_id: self.trace_id,
            undoes: self.undoes,
            conversation_id: None,
            created_at: at,
        }
    }
}

pub struct Ledger {
    store: Arc<Store>,
    events: Arc<dyn EventSink>,
    clock: Arc<dyn Clock>,
}

impl Ledger {
    /// `store` is the index store, where Activity lives.
    pub fn new(store: Arc<Store>, events: Arc<dyn EventSink>, clock: Arc<dyn Clock>) -> Self {
        Self {
            store,
            events,
            clock,
        }
    }

    pub fn append(&self, entry: Entry) -> Result<Activity> {
        self.append_with_id(ActivityId::new(), entry)
    }

    pub fn append_with_id(&self, id: ActivityId, entry: Entry) -> Result<Activity> {
        let activity = entry.into_activity(id, self.clock.now());
        repo::activities::append(&self.store, &activity)?;
        rdebug!(
            Category::Actions,
            "activity {} appended: {} by {} affecting {}",
            activity.id,
            activity.action,
            activity.actor.as_str(),
            activity.affected.len()
        );
        self.events.emit(RiddleEvent::ActivityAppended {
            activity: activity.clone(),
        });
        Ok(activity)
    }

    /// Bookkeeping never fails the work.
    pub fn try_append(&self, entry: Entry) -> Option<Activity> {
        match self.append(entry) {
            Ok(a) => Some(a),
            Err(e) => {
                riddle_observability::swallowed!(
                    Category::Actions,
                    "activity_append",
                    e,
                    "could not record activity"
                );
                None
            }
        }
    }

    pub fn get(&self, id: &ActivityId) -> Result<Option<Activity>> {
        repo::activities::get(&self.store, id)
    }

    /// Mark an Activity undone (§50) and publish the change.
    pub fn mark_undone(
        &self,
        id: &ActivityId,
        by: Actor,
        undo_activity_id: ActivityId,
    ) -> Result<Activity> {
        let mut a = repo::activities::get(&self.store, id)?.ok_or_else(|| {
            riddle_errors::RiddleError::not_found(format!("activity {id} not found"))
        })?;
        a.undo = UndoInfo::Undone {
            at: self.clock.now(),
            by,
            undo_activity_id,
        };
        repo::activities::update(&self.store, &a)?;
        self.events.emit(RiddleEvent::ActivityAppended {
            activity: a.clone(),
        });
        Ok(a)
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::{EntityKind, FixedClock, RecordingSink};
    use riddle_store::StoreKind;

    #[test]
    fn an_entry_becomes_a_row_and_an_event() {
        let store = Arc::new(Store::in_memory(StoreKind::Index).unwrap());
        let events = Arc::new(RecordingSink::default());
        let clock = Arc::new(FixedClock::at(chrono::Utc::now()));
        let ledger = Ledger::new(store.clone(), events.clone(), clock);
        let a = ledger
            .append(
                Entry::new(
                    Actor::Sift,
                    "move_to_spoil",
                    "Sift moved 2 messages to Spoil",
                )
                .affected(vec![
                    EntityRef::new(EntityKind::Message, "m1"),
                    EntityRef::new(EntityKind::Message, "m2"),
                ])
                .undoable(serde_json::json!({}), "Undo")
                .dig(DigId::from("d")),
            )
            .unwrap();
        let stored = repo::activities::get(&store, &a.id).unwrap().unwrap();
        assert_eq!(stored, a);
        assert!(
            matches!(events.take().as_slice(), [RiddleEvent::ActivityAppended { activity }] if activity.id == a.id)
        );
        let undone = ledger
            .mark_undone(&a.id, Actor::User, ActivityId::new())
            .unwrap();
        assert!(matches!(
            undone.undo,
            UndoInfo::Undone {
                by: Actor::User,
                ..
            }
        ));
    }
}
