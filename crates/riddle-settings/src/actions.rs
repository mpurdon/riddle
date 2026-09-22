//! Settings Actions. Everything that changes settings, the User Profile or
//! the Never Hidden list goes through here so the Director, the CLI and the
//! webview get the same validation, confirmation and undo (§8).
//!
//! Decisions: raising any Dig (or the default) to Trusted is press-and-hold
//! (§51), decided per invocation through `Action::confirmation_for`, so an
//! ordinary settings save stays frictionless. Never Hidden edits are egress
//! Actions (§58 control 3) and, when made from a message, record a
//! `NeverHide` correction so Sift learns from them (§46). Arming a dev build
//! is press-and-hold; disarming is not.

use crate::model::Settings;
use crate::store::SettingsStore;
use async_trait::async_trait;
use riddle_actions::policy::ensure_args;
use riddle_actions::runtime::Runtime;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview};
use riddle_core::{
    Autonomy, ConfirmationPolicy, Correction, CorrectionId, CorrectionKind, DigId, EntityKind,
    EntityRef, MessageId, Risk, UserProfile,
};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rerror, rinfo, Category};
use riddle_store::repo;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{Arc, Weak};

/// How `arm_dev` reaches the runtime's policy without a reference cycle:
/// the integrator registers `ArmControl::new(&runtime)` in `Services`.
pub struct ArmControl {
    runtime: Weak<Runtime>,
}

impl ArmControl {
    pub fn new(runtime: &Arc<Runtime>) -> Self {
        Self {
            runtime: Arc::downgrade(runtime),
        }
    }
    fn runtime(&self) -> Result<Arc<Runtime>> {
        self.runtime
            .upgrade()
            .ok_or_else(|| RiddleError::internal("runtime has gone away"))
    }
}

pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(SaveSettings),
        Arc::new(ArmDev),
        Arc::new(AddNeverHidden),
        Arc::new(RemoveNeverHidden),
        Arc::new(SaveUserProfile),
    ]
}

fn store(ctx: &ActionContext) -> Result<Arc<SettingsStore>> {
    ctx.services.require::<SettingsStore>()
}

// ---------------------------------------------------------------- save_settings

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveSettingsArgs {
    settings: Settings,
}

/// Top-level sections whose value differs, for the preview and the inputs.
fn changed_sections(a: &Settings, b: &Settings) -> Vec<String> {
    let (Value::Object(a), Value::Object(b)) = (
        serde_json::to_value(a).unwrap_or(Value::Null),
        serde_json::to_value(b).unwrap_or(Value::Null),
    ) else {
        return vec![];
    };
    a.iter()
        .filter(|(k, v)| b.get(*k) != Some(v))
        .map(|(k, _)| k.clone())
        .collect()
}

pub struct SaveSettings;

#[async_trait]
impl Action for SaveSettings {
    fn meta(&self) -> riddle_core::ActionMeta {
        meta("save_settings", "Save settings")
            .mutating()
            .reversible()
            .risk(Risk::Medium)
            .autonomy(Autonomy::Trusted)
            .build()
    }

    fn confirmation_for(&self, ctx: &ActionContext, args: &Value) -> ConfirmationPolicy {
        let raises = (|| -> Option<bool> {
            let args: SaveSettingsArgs = serde_json::from_value(args.clone()).ok()?;
            let current = store(ctx).ok()?.settings();
            Some(!current.trusted_raises(&args.settings).is_empty())
        })();
        if raises == Some(true) {
            ConfirmationPolicy::PressHold
        } else {
            ConfirmationPolicy::None
        }
    }

    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let args: SaveSettingsArgs = ensure_args(args)?;
        args.settings.validate()?;
        let current = store(ctx)?.settings();
        let raises = current.trusted_raises(&args.settings);
        let summary = if raises.is_empty() {
            "Save settings".to_string()
        } else {
            format!(
                "Raise {} to Trusted: Riddle will act on these Digs without asking",
                raises.join(", ")
            )
        };
        Ok(Preview {
            summary,
            real_arguments: json!({ "raisesToTrusted": raises, "changedSections": changed_sections(&current, &args.settings) }),
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let args: SaveSettingsArgs = ensure_args(&args)?;
        let store = store(ctx)?;
        let previous = store.settings();
        let changed = changed_sections(&previous, &args.settings);
        let saved = store.save(args.settings)?;
        rinfo!(
            Category::App,
            "settings saved by {:?}: {} sections changed",
            ctx.principal.actor(),
            changed.len()
        );
        Ok(
            ActionResult::ok(serde_json::to_value(&saved)?, "You changed settings")
                .with_inputs(json!({ "changedSections": changed }))
                .with_undo(
                    serde_json::to_value(&previous)?,
                    "Restore previous settings",
                ),
        )
    }

    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let previous: Settings = serde_json::from_value(restore)
            .map_err(|e| RiddleError::invalid(format!("restore data: {e}")))?;
        let store = store(ctx)?;
        let before_undo = store.settings();
        let saved = store.save(previous)?;
        Ok(
            ActionResult::ok(serde_json::to_value(&saved)?, "Restored previous settings")
                .with_undo(serde_json::to_value(&before_undo)?, "Restore settings"),
        )
    }
}

// ---------------------------------------------------------------------- arm_dev

#[derive(Deserialize)]
struct ArmArgs {
    armed: bool,
}

pub struct ArmDev;

#[async_trait]
impl Action for ArmDev {
    fn meta(&self) -> riddle_core::ActionMeta {
        meta("arm_dev", "Arm this dev build for provider mutations")
            .mutating()
            .reversible()
            .risk(Risk::High)
            .confirm(ConfirmationPolicy::PressHold)
            .autonomy(Autonomy::Trusted)
            .build()
    }

    fn confirmation_for(&self, _ctx: &ActionContext, args: &Value) -> ConfirmationPolicy {
        match serde_json::from_value::<ArmArgs>(args.clone()) {
            Ok(a) if a.armed => ConfirmationPolicy::PressHold,
            _ => ConfirmationPolicy::None,
        }
    }

    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let a: ArmArgs = ensure_args(args)?;
        let summary = if a.armed {
            "Arm the dev build: Actions may change the connected mailbox".to_string()
        } else {
            "Disarm the dev build".to_string()
        };
        Ok(Preview {
            summary,
            real_arguments: json!({ "armed": a.armed }),
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: ArmArgs = ensure_args(&args)?;
        let control = ctx.services.require::<ArmControl>()?;
        let runtime = control.runtime()?;
        if !runtime.policy.read().dev {
            return Err(RiddleError::policy_denied(
                "arming applies to dev builds only",
            ));
        }
        let previous = runtime.is_armed();
        runtime.arm(a.armed);
        rinfo!(
            Category::App,
            "dev build {}",
            if a.armed { "armed" } else { "disarmed" }
        );
        let summary = if a.armed {
            "You armed the dev build"
        } else {
            "You disarmed the dev build"
        };
        Ok(ActionResult::ok(json!({ "armed": a.armed }), summary)
            .with_inputs(json!({ "armed": a.armed }))
            .with_undo(
                json!({ "armed": previous }),
                if previous { "Re-arm" } else { "Disarm" },
            ))
    }

    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let a: ArmArgs = ensure_args(&restore)?;
        let control = ctx.services.require::<ArmControl>()?;
        let runtime = control.runtime()?;
        let previous = runtime.is_armed();
        runtime.arm(a.armed);
        Ok(ActionResult::ok(
            json!({ "armed": a.armed }),
            if a.armed {
                "Re-armed the dev build"
            } else {
                "Disarmed the dev build"
            },
        )
        .with_undo(json!({ "armed": previous }), "Undo"))
    }
}

// ------------------------------------------------------------- never hidden

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NeverHiddenArgs {
    entry: String,
    /// When the entry was added from a message, the correction it records.
    #[serde(default)]
    dig_id: Option<DigId>,
    #[serde(default)]
    message_id: Option<MessageId>,
}

/// Lower-case, trimmed; either `user@domain` or `@domain`.
pub fn normalize_entry(raw: &str) -> Result<String> {
    let e = raw.trim().to_lowercase();
    let valid = match e.strip_prefix('@') {
        Some(domain) => !domain.is_empty() && domain.contains('.') && !domain.contains('@'),
        None => {
            matches!(e.split_once('@'), Some((local, domain)) if !local.is_empty() && !domain.is_empty() && !domain.contains('@'))
        }
    };
    if valid {
        Ok(e)
    } else {
        Err(RiddleError::invalid("a Never Hidden entry is an address like name@example.com or a domain like @example.com"))
    }
}

fn profile(ctx: &ActionContext) -> Result<UserProfile> {
    repo::user_profile::get(&ctx.services.stores.index())
}
fn put_profile(ctx: &ActionContext, p: &UserProfile) -> Result<()> {
    repo::user_profile::put(&ctx.services.stores.index(), p)
}

pub struct AddNeverHidden;

#[async_trait]
impl Action for AddNeverHidden {
    fn meta(&self) -> riddle_core::ActionMeta {
        meta("add_never_hidden", "Add a sender or domain to Never Hidden")
            .mutating()
            .reversible()
            .risk(Risk::Medium)
            .autonomy(Autonomy::Trusted)
            .egress()
            .build()
    }

    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let a: NeverHiddenArgs = ensure_args(args)?;
        let entry = normalize_entry(&a.entry)?;
        Ok(Preview {
            summary: format!("Never hide mail from {entry}"),
            real_arguments: json!({ "entry": entry }),
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: NeverHiddenArgs = ensure_args(&args)?;
        let entry = normalize_entry(&a.entry)?;
        let mut p = profile(ctx)?;
        let affected = vec![EntityRef::new(EntityKind::Person, &entry)];
        if p.never_hidden.iter().any(|e| e == &entry) {
            return Ok(ActionResult::ok(
                json!({ "entry": entry, "added": false }),
                format!("{entry} is already in Never Hidden"),
            )
            .with_affected(affected));
        }
        p.never_hidden.push(entry.clone());
        put_profile(ctx, &p)?;
        rinfo!(
            Category::App,
            "never hidden entry added; list size {}",
            p.never_hidden.len()
        );
        // The learning signal (§46): recorded only when a message prompted it.
        if let (Some(dig), Some(message)) = (&a.dig_id, &a.message_id) {
            let correction = Correction {
                id: CorrectionId::new(),
                kind: CorrectionKind::NeverHide,
                dig_id: dig.clone(),
                message_id: message.clone(),
                from_disposition: None,
                to_disposition: None,
                from_find_type: None,
                to_find_type: None,
                mesh_id: None,
                mesh_version: None,
                activity_id: Some(ctx.activity_id.clone()),
                created_at: ctx.services.clock.now(),
                retracted_at: None,
            };
            match ctx
                .services
                .stores
                .dig(dig)
                .and_then(|s| repo::corrections::record(&s, &correction))
            {
                Ok(()) => {}
                Err(e) => rerror!(
                    Category::App,
                    "could not record never-hide correction for {}: {}",
                    message,
                    e.message
                ),
            }
        }
        Ok(ActionResult::ok(
            json!({ "entry": entry, "added": true }),
            format!("You added {entry} to Never Hidden"),
        )
        .with_inputs(json!({ "entry": entry, "digId": a.dig_id, "messageId": a.message_id }))
        .with_affected(affected)
        .with_undo(json!({ "entry": entry }), "Remove from Never Hidden"))
    }

    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let entry = restore
            .get("entry")
            .and_then(Value::as_str)
            .ok_or_else(|| RiddleError::invalid("restore data lacks entry"))?
            .to_string();
        let mut p = profile(ctx)?;
        p.never_hidden.retain(|e| e != &entry);
        put_profile(ctx, &p)?;
        Ok(ActionResult::ok(
            json!({ "entry": entry }),
            format!("Removed {entry} from Never Hidden"),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Person, &entry)])
        .with_undo(json!({ "entry": entry }), "Add back"))
    }
}

pub struct RemoveNeverHidden;

#[async_trait]
impl Action for RemoveNeverHidden {
    fn meta(&self) -> riddle_core::ActionMeta {
        meta(
            "remove_never_hidden",
            "Remove a sender or domain from Never Hidden",
        )
        .mutating()
        .reversible()
        .risk(Risk::High)
        .autonomy(Autonomy::Trusted)
        .build()
    }

    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let a: NeverHiddenArgs = ensure_args(args)?;
        let entry = normalize_entry(&a.entry)?;
        Ok(Preview {
            summary: format!("Allow Riddle to hide mail from {entry} again"),
            real_arguments: json!({ "entry": entry }),
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: NeverHiddenArgs = ensure_args(&args)?;
        let entry = normalize_entry(&a.entry)?;
        let mut p = profile(ctx)?;
        let before = p.never_hidden.len();
        p.never_hidden.retain(|e| e != &entry);
        if p.never_hidden.len() == before {
            return Err(RiddleError::not_found(format!(
                "{entry} is not in Never Hidden"
            )));
        }
        put_profile(ctx, &p)?;
        rinfo!(
            Category::App,
            "never hidden entry removed; list size {}",
            p.never_hidden.len()
        );
        Ok(ActionResult::ok(
            json!({ "entry": entry }),
            format!("You removed {entry} from Never Hidden"),
        )
        .with_inputs(json!({ "entry": entry }))
        .with_affected(vec![EntityRef::new(EntityKind::Person, &entry)])
        .with_undo(json!({ "entry": entry }), "Add back to Never Hidden"))
    }

    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let entry = restore
            .get("entry")
            .and_then(Value::as_str)
            .ok_or_else(|| RiddleError::invalid("restore data lacks entry"))?
            .to_string();
        let mut p = profile(ctx)?;
        if !p.never_hidden.iter().any(|e| e == &entry) {
            p.never_hidden.push(entry.clone());
            put_profile(ctx, &p)?;
        }
        Ok(ActionResult::ok(
            json!({ "entry": entry }),
            format!("Added {entry} back to Never Hidden"),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Person, &entry)])
        .with_undo(json!({ "entry": entry }), "Remove"))
    }
}

// --------------------------------------------------------- save_user_profile

#[derive(Deserialize)]
struct ProfileArgs {
    profile: UserProfile,
}

pub struct SaveUserProfile;

#[async_trait]
impl Action for SaveUserProfile {
    fn meta(&self) -> riddle_core::ActionMeta {
        meta("save_user_profile", "Save the User Profile")
            .mutating()
            .reversible()
            .risk(Risk::Medium)
            .autonomy(Autonomy::Trusted)
            .egress()
            .build()
    }

    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let a: ProfileArgs = ensure_args(args)?;
        let current = profile(ctx)?;
        Ok(Preview {
            summary: "Save the User Profile".into(),
            real_arguments: json!({ "neverHiddenBefore": current.never_hidden.len(), "neverHiddenAfter": a.profile.never_hidden.len(), "addresses": a.profile.addresses.len() }),
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: ProfileArgs = ensure_args(&args)?;
        let mut next = a.profile;
        next.never_hidden = next
            .never_hidden
            .iter()
            .map(|e| normalize_entry(e))
            .collect::<Result<Vec<_>>>()?;
        next.never_hidden.dedup();
        let previous = profile(ctx)?;
        put_profile(ctx, &next)?;
        rinfo!(
            Category::App,
            "user profile saved: {} addresses, {} never hidden",
            next.addresses.len(),
            next.never_hidden.len()
        );
        Ok(ActionResult::ok(serde_json::to_value(&next)?, "You updated your profile")
            .with_inputs(json!({ "addresses": next.addresses.len(), "neverHidden": next.never_hidden.len() }))
            .with_undo(serde_json::to_value(&previous)?, "Restore previous profile"))
    }

    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let previous: UserProfile = serde_json::from_value(restore)
            .map_err(|e| RiddleError::invalid(format!("restore data: {e}")))?;
        let before_undo = profile(ctx)?;
        put_profile(ctx, &previous)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&previous)?,
            "Restored previous profile",
        )
        .with_undo(serde_json::to_value(&before_undo)?, "Restore"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::NoSecrets;
    use riddle_actions::{RuntimeBuilder, Services};
    use riddle_core::{ActionInvocation, BuildStamp, PendingAction, Principal, RecordingSink};
    use riddle_store::Stores;

    fn runtime(dir: &std::path::Path, dev: bool) -> (Arc<Runtime>, Arc<SettingsStore>) {
        let stores = Arc::new(Stores::in_memory().unwrap());
        let services = Arc::new(Services::new(
            stores,
            Arc::new(RecordingSink::default()),
            BuildStamp::from_env_vars("0", None, None, None, "test"),
        ));
        let settings = Arc::new(
            SettingsStore::open(
                &dir.join("c"),
                &dir.join("d"),
                dev,
                Arc::new(NoSecrets),
                Arc::new(RecordingSink::default()),
            )
            .unwrap(),
        );
        services.insert(settings.clone());
        let mut policy = settings.autonomy_policy();
        policy.dev = dev;
        let mut builder = RuntimeBuilder::new(services.clone()).policy(policy);
        for a in actions() {
            builder = builder.register(a);
        }
        let rt = builder.build();
        services.insert(Arc::new(ArmControl::new(&rt)));
        (rt, settings)
    }

    fn user(action: &str, args: Value) -> ActionInvocation {
        ActionInvocation {
            action: action.into(),
            args,
            principal: Principal::User,
            dig_id: None,
            idempotency_key: None,
            confirmed: false,
            pending_id: None,
        }
    }

    #[tokio::test]
    async fn an_ordinary_save_needs_no_confirmation_and_is_undoable() {
        let dir = tempfile::tempdir().unwrap();
        let (rt, store) = runtime(dir.path(), false);
        let mut s = store.settings();
        s.shadow_period_days = 14;
        let out = rt
            .invoke(user("save_settings", json!({ "settings": s })))
            .await
            .unwrap();
        assert_eq!(store.settings().shadow_period_days, 14);
        let a = repo::activities::get(
            &rt.services.stores.index(),
            out.activity_id.as_ref().unwrap(),
        )
        .unwrap()
        .unwrap();
        let riddle_core::UndoInfo::Undoable { restore, .. } = a.undo else {
            panic!("undoable")
        };
        let back = rt
            .undo_via(
                "save_settings",
                restore,
                Principal::User,
                None,
                riddle_core::ActivityId::new(),
            )
            .await
            .unwrap();
        assert_eq!(back.summary, "Restored previous settings");
        assert_eq!(store.settings().shadow_period_days, 7);
    }

    #[tokio::test]
    async fn raising_a_dig_to_trusted_is_press_and_hold() {
        let dir = tempfile::tempdir().unwrap();
        let (rt, store) = runtime(dir.path(), false);
        let mut s = store.settings();
        s.autonomy.per_dig.insert("dig-1".into(), Autonomy::Trusted);
        let err = rt
            .invoke(user("save_settings", json!({ "settings": s })))
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::NeedsConfirmation);
        let pending: PendingAction = serde_json::from_value(err.detail.unwrap()).unwrap();
        assert_eq!(pending.confirmation, ConfirmationPolicy::PressHold);
        assert_eq!(pending.real_arguments["raisesToTrusted"][0], "dig-1");
        let confirmed = ActionInvocation {
            pending_id: Some(pending.id),
            ..user("save_settings", json!({ "settings": s }))
        };
        rt.invoke(confirmed).await.unwrap();
        assert_eq!(
            store.settings().autonomy.per_dig["dig-1"],
            Autonomy::Trusted
        );
        // Saving again with the Dig already Trusted is not a raise.
        let again = store.settings();
        rt.invoke(user("save_settings", json!({ "settings": again })))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn an_invalid_settings_save_is_refused_before_any_write() {
        let dir = tempfile::tempdir().unwrap();
        let (rt, store) = runtime(dir.path(), false);
        let mut s = store.settings();
        s.budgets.daily_llm_usd = -1.0;
        let err = rt
            .invoke(user("save_settings", json!({ "settings": s })))
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
        assert!(!store.config_path().exists());
    }

    #[tokio::test]
    async fn arming_needs_press_and_hold_in_a_dev_build_and_is_denied_in_prod() {
        let dir = tempfile::tempdir().unwrap();
        let (rt, _) = runtime(dir.path(), true);
        let err = rt
            .invoke(user("arm_dev", json!({ "armed": true })))
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::NeedsConfirmation);
        let pending: PendingAction = serde_json::from_value(err.detail.unwrap()).unwrap();
        assert_eq!(pending.confirmation, ConfirmationPolicy::PressHold);
        rt.invoke(ActionInvocation {
            pending_id: Some(pending.id),
            ..user("arm_dev", json!({ "armed": true }))
        })
        .await
        .unwrap();
        assert!(rt.is_armed());
        rt.invoke(user("arm_dev", json!({ "armed": false })))
            .await
            .unwrap();
        assert!(!rt.is_armed());

        let dir = tempfile::tempdir().unwrap();
        let (prod, _) = runtime(dir.path(), false);
        let err = prod
            .invoke(ActionInvocation {
                confirmed: true,
                ..user("arm_dev", json!({ "armed": false }))
            })
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::PolicyDenied);
    }

    #[tokio::test]
    async fn never_hidden_add_records_a_correction_and_undo_removes_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        let (rt, _) = runtime(dir.path(), false);
        let dig = DigId::from("dig-1");
        let message = MessageId::for_provider(&dig, "m1");
        let out = rt
            .invoke(user(
                "add_never_hidden",
                json!({ "entry": " Boss@Example.com ", "digId": dig, "messageId": message }),
            ))
            .await
            .unwrap();
        let profile = repo::user_profile::get(&rt.services.stores.index()).unwrap();
        assert_eq!(profile.never_hidden, vec!["boss@example.com".to_string()]);
        let corrections =
            repo::corrections::for_message(&rt.services.stores.dig(&dig).unwrap(), &message)
                .unwrap();
        assert_eq!(corrections.len(), 1);
        assert_eq!(corrections[0].kind, CorrectionKind::NeverHide);
        assert_eq!(corrections[0].activity_id, out.activity_id);

        let again = rt
            .invoke(user(
                "add_never_hidden",
                json!({ "entry": "boss@example.com" }),
            ))
            .await
            .unwrap();
        assert_eq!(again.result["added"], false);

        rt.undo_via(
            "add_never_hidden",
            json!({ "entry": "boss@example.com" }),
            Principal::User,
            None,
            riddle_core::ActivityId::new(),
        )
        .await
        .unwrap();
        assert!(repo::user_profile::get(&rt.services.stores.index())
            .unwrap()
            .never_hidden
            .is_empty());
    }

    #[tokio::test]
    async fn removing_an_absent_entry_is_not_found_and_a_bad_entry_is_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let (rt, _) = runtime(dir.path(), false);
        let err = rt
            .invoke(user(
                "remove_never_hidden",
                json!({ "entry": "nobody@example.com" }),
            ))
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::NotFound);
        let err = rt
            .invoke(user(
                "add_never_hidden",
                json!({ "entry": "not an address" }),
            ))
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
        assert_eq!(normalize_entry("@Example.COM").unwrap(), "@example.com");
    }

    #[tokio::test]
    async fn the_director_cannot_touch_never_hidden_below_trusted() {
        let dir = tempfile::tempdir().unwrap();
        let (rt, _) = runtime(dir.path(), false);
        let principal = Principal::Director {
            conversation_id: riddle_core::ConversationId::new(),
            read_untrusted: true,
        };
        let inv = ActionInvocation {
            principal,
            ..user("add_never_hidden", json!({ "entry": "x@example.com" }))
        };
        let err = rt.invoke(inv).await.unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::NeedsConfirmation);
        let pending: PendingAction = serde_json::from_value(err.detail.unwrap()).unwrap();
        assert_eq!(pending.confirmation, ConfirmationPolicy::Approval);
    }

    #[tokio::test]
    async fn saving_the_profile_normalizes_never_hidden_and_restores_on_undo() {
        let dir = tempfile::tempdir().unwrap();
        let (rt, _) = runtime(dir.path(), false);
        let profile = UserProfile {
            addresses: vec!["me@example.com".into()],
            never_hidden: vec!["Mum@Example.com".into()],
            ..Default::default()
        };
        let out = rt
            .invoke(user("save_user_profile", json!({ "profile": profile })))
            .await
            .unwrap();
        let saved = repo::user_profile::get(&rt.services.stores.index()).unwrap();
        assert_eq!(saved.never_hidden, vec!["mum@example.com".to_string()]);
        let a = repo::activities::get(
            &rt.services.stores.index(),
            out.activity_id.as_ref().unwrap(),
        )
        .unwrap()
        .unwrap();
        let riddle_core::UndoInfo::Undoable { restore, .. } = a.undo else {
            panic!("undoable")
        };
        rt.undo_via(
            "save_user_profile",
            restore,
            Principal::User,
            None,
            riddle_core::ActivityId::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            repo::user_profile::get(&rt.services.stores.index()).unwrap(),
            UserProfile::default()
        );
    }
}
