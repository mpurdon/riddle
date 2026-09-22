//! The invocation path: authorize, preview or execute, record Activity,
//! honour idempotency, and publish events. One function, so every client
//! gets the same behaviour.

use crate::policy::{decide, AutonomyPolicy, Decision};
use crate::{Action, ActionContext, Registry};
use parking_lot::RwLock;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rerror, rinfo, rwarn, Category, Timed};
use riddle_store::{repo, Stores};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Services domain crates register so Actions can reach them without the
/// Action layer depending on every crate at compile time. Typed access via
/// `Services::get::<T>()`.
pub struct Services {
    pub stores: Arc<Stores>,
    pub clock: Arc<dyn Clock>,
    pub events: Arc<dyn EventSink>,
    pub judge: RwLock<Option<Arc<dyn SemanticJudge>>>,
    pub refiner: RwLock<Option<Arc<dyn SignalRefiner>>>,
    pub build: BuildStamp,
    ext: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

impl Services {
    pub fn new(stores: Arc<Stores>, events: Arc<dyn EventSink>, build: BuildStamp) -> Self {
        Self {
            stores,
            clock: Arc::new(SystemClock),
            events,
            judge: RwLock::new(None),
            refiner: RwLock::new(None),
            build,
            ext: RwLock::new(HashMap::new()),
        }
    }
    pub fn insert<T: Any + Send + Sync>(&self, value: Arc<T>) {
        self.ext.write().insert(TypeId::of::<T>(), value);
    }
    pub fn get<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.ext
            .read()
            .get(&TypeId::of::<T>())
            .and_then(|a| a.clone().downcast::<T>().ok())
    }
    pub fn require<T: Any + Send + Sync>(&self) -> Result<Arc<T>> {
        self.get::<T>().ok_or_else(|| {
            RiddleError::internal(format!(
                "service {} not registered",
                std::any::type_name::<T>()
            ))
        })
    }
    pub fn judge(&self) -> Option<Arc<dyn SemanticJudge>> {
        self.judge.read().clone()
    }
    pub fn refiner(&self) -> Option<Arc<dyn SignalRefiner>> {
        self.refiner.read().clone()
    }
    pub fn emit(&self, e: RiddleEvent) {
        self.events.emit(e);
    }
}

pub struct Runtime {
    pub registry: RwLock<Registry>,
    pub services: Arc<Services>,
    pub policy: RwLock<AutonomyPolicy>,
}

pub struct RuntimeBuilder {
    services: Arc<Services>,
    registry: Registry,
    policy: AutonomyPolicy,
}

impl RuntimeBuilder {
    pub fn new(services: Arc<Services>) -> Self {
        Self {
            services,
            registry: Registry::new(),
            policy: AutonomyPolicy::default(),
        }
    }
    pub fn register(mut self, a: Arc<dyn Action>) -> Self {
        self.registry.register(a);
        self
    }
    pub fn registry(mut self, f: impl FnOnce(&mut Registry)) -> Self {
        f(&mut self.registry);
        self
    }
    pub fn policy(mut self, p: AutonomyPolicy) -> Self {
        self.policy = p;
        self
    }
    pub fn build(self) -> Arc<Runtime> {
        Arc::new(Runtime {
            registry: RwLock::new(self.registry),
            services: self.services,
            policy: RwLock::new(self.policy),
        })
    }
}

const PENDING_TTL_MS: i64 = 30 * 60 * 1000;

impl Runtime {
    pub fn action(&self, name: &str) -> Result<Arc<dyn Action>> {
        self.registry
            .read()
            .get(name)
            .ok_or_else(|| RiddleError::not_found(format!("no action named {name}")))
    }

    pub fn list_actions(&self) -> Vec<ActionMeta> {
        self.registry.read().list()
    }

    /// The one entry point (§8). Returns `needs_confirmation` with a
    /// `PendingAction` in the detail when policy asks for it.
    pub async fn invoke(&self, inv: ActionInvocation) -> Result<ActionOutcome> {
        let _t = Timed::start(
            Category::Actions,
            format!("action {}", inv.action),
            Duration::from_secs(5),
        );
        let action = self.action(&inv.action)?;
        let meta = action.meta();
        let index = self.services.stores.index();

        // Idempotent replay.
        if let Some(key) = &inv.idempotency_key {
            if let Some(prior) = repo::pending::get_idempotent(&index, key)? {
                rinfo!(
                    Category::Actions,
                    "replayed {} for idempotency key {}",
                    meta.name,
                    riddle_observability::redact(key)
                );
                return Ok(ActionOutcome {
                    result: prior,
                    activity_id: None,
                    replayed: true,
                });
            }
        }

        // A confirmation must match a pending action we issued.
        let confirmed = if let Some(pid) = &inv.pending_id {
            let _ = repo::pending::expire_pending(&index, PENDING_TTL_MS);
            let pending = repo::pending::get_pending(&index, pid)?
                .ok_or_else(|| RiddleError::invalid("that confirmation has expired; try again"))?;
            if pending.action != inv.action {
                return Err(RiddleError::invalid(
                    "confirmation does not match the action",
                ));
            }
            repo::pending::remove_pending(&index, pid)?;
            true
        } else {
            inv.confirmed && inv.principal.is_user()
        };

        let activity_id = ActivityId::new();
        let ctx = ActionContext {
            services: self.services.clone(),
            principal: inv.principal.clone(),
            dig_id: inv.dig_id.clone(),
            idempotency_key: inv.idempotency_key.clone(),
            activity_id: activity_id.clone(),
        };
        // The tier may depend on the arguments (`Action::confirmation_for`).
        let mut meta = meta;
        meta.confirmation = action.confirmation_for(&ctx, &inv.args);
        let decision = decide(
            &meta,
            &inv.principal,
            inv.dig_id.as_ref(),
            &self.policy.read(),
            confirmed,
        );
        match decision {
            Decision::Deny(reason) => {
                rwarn!(
                    Category::Actions,
                    "denied {} for {:?}: {reason}",
                    meta.name,
                    inv.principal
                );
                return Err(RiddleError::policy_denied(reason));
            }
            Decision::Confirm(tier, reason) => {
                let preview = action.preview(&ctx, &inv.args).await?;
                let pending = PendingAction {
                    id: uuid::Uuid::now_v7().to_string(),
                    action: meta.name.clone(),
                    args: inv.args.clone(),
                    real_arguments: preview.real_arguments,
                    summary: preview.summary,
                    confirmation: tier,
                    reason,
                    principal: inv.principal.clone(),
                    created_at: self.services.clock.now(),
                };
                repo::pending::put_pending(&index, &pending)?;
                return Err(
                    RiddleError::needs_confirmation(pending.summary.clone()).with_detail(&pending)
                );
            }
            Decision::Proceed => {}
        }

        let started = self.services.clock.now();
        let outcome = action.execute(&ctx, inv.args.clone()).await;
        let (result_value, activity) = match outcome {
            Ok(r) => {
                let activity = Activity {
                    id: activity_id.clone(),
                    actor: inv.principal.actor(),
                    action: meta.name.clone(),
                    summary: r.summary.clone(),
                    why: r.why.clone(),
                    inputs: r.inputs.clone(),
                    affected: r.affected.clone(),
                    result: r.result.clone(),
                    provenance: r.provenance.clone(),
                    undo: r.undo.clone(),
                    dig_id: inv.dig_id.clone(),
                    trace_id: r.provenance.as_ref().and_then(|p| p.trace_id.clone()),
                    undoes: r.undoes.clone(),
                    created_at: started,
                };
                (r.value, activity)
            }
            Err(e) => {
                if meta.mutating {
                    // A failed mutation is still Activity: the user asked for
                    // something and it did not happen.
                    let activity = Activity {
                        id: activity_id.clone(),
                        actor: inv.principal.actor(),
                        action: meta.name.clone(),
                        summary: format!("{} failed", meta.description),
                        why: None,
                        inputs: serde_json::Value::Null,
                        affected: vec![],
                        result: ActivityResult::Failed {
                            error: e.message.clone(),
                        },
                        provenance: None,
                        undo: UndoInfo::NotUndoable {
                            reason: "nothing happened".into(),
                        },
                        dig_id: inv.dig_id.clone(),
                        trace_id: None,
                        undoes: None,
                        created_at: started,
                    };
                    if let Err(le) = repo::activities::append(&index, &activity) {
                        rerror!(Category::Actions, "could not record failed activity: {le}");
                    }
                }
                return Err(e);
            }
        };

        let mut activity_id_out = None;
        if meta.mutating {
            // Bookkeeping never fails the work (invariant 39).
            match repo::activities::append(&index, &activity) {
                Ok(()) => {
                    activity_id_out = Some(activity.id.clone());
                    self.services.emit(RiddleEvent::ActivityAppended {
                        activity: activity.clone(),
                    });
                }
                Err(e) => rerror!(
                    Category::Actions,
                    "could not record activity for {}: {e}",
                    meta.name
                ),
            }
        }
        if let Some(key) = &inv.idempotency_key {
            if let Err(e) = repo::pending::put_idempotent(&index, key, &meta.name, &result_value) {
                rerror!(Category::Actions, "could not record idempotency key: {e}");
            }
        }
        Ok(ActionOutcome {
            result: result_value,
            activity_id: activity_id_out,
            replayed: false,
        })
    }

    /// Convenience for in-process callers that are the user.
    pub async fn invoke_as_user(
        &self,
        action: &str,
        args: serde_json::Value,
    ) -> Result<ActionOutcome> {
        self.invoke(ActionInvocation {
            action: action.into(),
            args,
            principal: Principal::User,
            dig_id: None,
            idempotency_key: None,
            confirmed: true,
            pending_id: None,
        })
        .await
    }

    pub fn arm(&self, armed: bool) {
        self.policy.write().armed = armed;
    }
    pub fn is_armed(&self) -> bool {
        self.policy.read().armed
    }

    /// Run an Action's `undo` from recorded restore data on behalf of the undo
    /// engine (§50). Authorization and the Activity row belong to the
    /// `undo_activity` invocation that calls this; here we only build the
    /// context and dispatch.
    pub async fn undo_via(
        &self,
        action_name: &str,
        restore: serde_json::Value,
        principal: Principal,
        dig_id: Option<DigId>,
        activity_id: ActivityId,
    ) -> Result<crate::ActionResult> {
        let action = self.action(action_name)?;
        let ctx = ActionContext {
            services: self.services.clone(),
            principal,
            dig_id,
            idempotency_key: None,
            activity_id,
        };
        action.undo(&ctx, restore).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionResult, Preview};
    use async_trait::async_trait;
    use serde_json::json;

    struct Echo;
    #[async_trait]
    impl Action for Echo {
        fn meta(&self) -> ActionMeta {
            crate::meta("echo", "echo the args").build()
        }
        async fn execute(
            &self,
            _ctx: &ActionContext,
            args: serde_json::Value,
        ) -> Result<ActionResult> {
            Ok(ActionResult::ok(args, "echoed"))
        }
    }

    struct Spoil;
    #[async_trait]
    impl Action for Spoil {
        fn meta(&self) -> ActionMeta {
            crate::meta("move_to_spoil", "move messages to Spoil")
                .mutating()
                .reversible()
                .confirm(ConfirmationPolicy::InPlaceUndo)
                .autonomy(Autonomy::Act)
                .build()
        }
        async fn preview(&self, _ctx: &ActionContext, args: &serde_json::Value) -> Result<Preview> {
            Ok(Preview {
                summary: "move 2 messages".into(),
                real_arguments: args.clone(),
            })
        }
        async fn execute(
            &self,
            _ctx: &ActionContext,
            args: serde_json::Value,
        ) -> Result<ActionResult> {
            Ok(
                ActionResult::ok(json!({"moved": 2}), "Moved 2 messages to Spoil")
                    .with_inputs(args)
                    .with_undo(json!({"ids": ["a", "b"]}), "Undo"),
            )
        }
    }

    fn runtime() -> Arc<Runtime> {
        let stores = Arc::new(Stores::in_memory().unwrap());
        let services = Arc::new(Services::new(
            stores,
            Arc::new(RecordingSink::default()),
            BuildStamp::from_env_vars("0", None, None, None, "test"),
        ));
        RuntimeBuilder::new(services)
            .register(Arc::new(Echo))
            .register(Arc::new(Spoil))
            .build()
    }

    #[tokio::test]
    async fn a_read_only_action_runs_without_activity() {
        let rt = runtime();
        let out = rt.invoke_as_user("echo", json!({"a": 1})).await.unwrap();
        assert_eq!(out.result["a"], 1);
        assert!(out.activity_id.is_none());
    }

    #[tokio::test]
    async fn a_mutating_action_needs_confirmation_then_records_activity_and_is_idempotent() {
        let rt = runtime();
        let inv = ActionInvocation {
            action: "move_to_spoil".into(),
            args: json!({"ids": ["a", "b"]}),
            principal: Principal::User,
            dig_id: None,
            idempotency_key: Some("k1".into()),
            confirmed: false,
            pending_id: None,
        };
        let err = rt.invoke(inv.clone()).await.unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::NeedsConfirmation);
        let pending: PendingAction = serde_json::from_value(err.detail.unwrap()).unwrap();
        assert_eq!(pending.summary, "move 2 messages");

        let confirmed = ActionInvocation {
            pending_id: Some(pending.id),
            ..inv.clone()
        };
        let out = rt.invoke(confirmed).await.unwrap();
        assert!(out.activity_id.is_some());
        let a = repo::activities::get(
            &rt.services.stores.index(),
            out.activity_id.as_ref().unwrap(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(a.summary, "Moved 2 messages to Spoil");
        assert!(matches!(a.undo, UndoInfo::Undoable { .. }));

        let replay = rt
            .invoke(ActionInvocation {
                confirmed: true,
                ..inv
            })
            .await
            .unwrap();
        assert!(replay.replayed);
    }

    #[tokio::test]
    async fn sift_cannot_run_a_mutating_action_below_act() {
        let rt = runtime();
        let inv = ActionInvocation {
            action: "move_to_spoil".into(),
            args: json!({}),
            principal: Principal::Sift,
            dig_id: None,
            idempotency_key: None,
            confirmed: false,
            pending_id: None,
        };
        let err = rt.invoke(inv).await.unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::PolicyDenied);
    }
}
