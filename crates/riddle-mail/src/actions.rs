//! Mail Actions: drafts and sending, Dig management, and Demo Mode's
//! injection (`docs/COMMANDS.md`). Sending is TwoStep and its preview shows
//! the real recipients from the stored draft, not whatever the caller typed
//! (§58 control 4); it is not undoable and says so. Archive, restore, label
//! and snooze live in `riddle-projection` because they enqueue provider
//! writes; `refresh_now` lives in `riddle-sync`.

use crate::fixture::{FixtureProvider, FixtureSpec};
use crate::provider::OutgoingMessage;
use crate::registry::ProviderRegistry;
use crate::triggers::RefreshTriggers;
use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, rwarn, Category};
use riddle_store::{repo, Store};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::Arc;

/// Shared argument helpers for every mail-track Action.
pub mod util {
    use super::*;

    pub fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T> {
        let v = args
            .get(key)
            .ok_or_else(|| RiddleError::invalid(format!("missing argument '{key}'")))?;
        serde_json::from_value(v.clone())
            .map_err(|e| RiddleError::invalid(format!("argument '{key}': {e}")))
    }

    pub fn opt<T: DeserializeOwned>(args: &Value, key: &str) -> Result<Option<T>> {
        match args.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|e| RiddleError::invalid(format!("argument '{key}': {e}"))),
        }
    }

    /// The Dig named in the args (or the invocation) and its store.
    pub fn dig_and_store(ctx: &ActionContext, args: &Value) -> Result<(Dig, Arc<Store>)> {
        let dig_id: DigId = match opt::<DigId>(args, "digId")? {
            Some(d) => d,
            None => ctx
                .dig_id
                .clone()
                .ok_or_else(|| RiddleError::invalid("missing argument 'digId'"))?,
        };
        let dig = repo::digs::get(&ctx.services.stores.index(), &dig_id)?
            .ok_or_else(|| RiddleError::not_found(format!("no Dig {dig_id}")))?;
        let store = ctx.services.stores.dig(&dig.id)?;
        Ok((dig, store))
    }

    pub fn message_ids(args: &Value) -> Result<Vec<MessageId>> {
        let ids: Vec<MessageId> = arg(args, "messageIds")?;
        if ids.is_empty() {
            return Err(RiddleError::invalid("messageIds is empty"));
        }
        Ok(ids)
    }

    pub fn message_refs(ids: &[MessageId]) -> Vec<EntityRef> {
        ids.iter()
            .map(|m| EntityRef::new(EntityKind::Message, m))
            .collect()
    }
}

use util::*;

fn drafts_meta(name: &str, description: &str) -> ActionMeta {
    meta(name, description)
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Act)
        .build()
}

/// Turn a stored Draft into what the provider sends, resolving the reply
/// headers from the store.
pub fn outgoing_from_draft(
    index: &Store,
    dig: &Dig,
    store: &Store,
    d: &Draft,
) -> Result<OutgoingMessage> {
    let profile = repo::user_profile::get(index)?;
    let from = Address::new(dig.email.clone(), profile.name.as_deref());
    let mut out = OutgoingMessage {
        from,
        to: d.to.clone(),
        cc: d.cc.clone(),
        bcc: d.bcc.clone(),
        subject: d.subject.clone(),
        body_text: d.body_text.clone(),
        body_html: None,
        thread_provider_id: d.thread_id.as_ref().map(|t| t.provider_id().to_string()),
        in_reply_to: None,
        references: vec![],
    };
    if let Some(parent_id) = &d.in_reply_to {
        if let Some(parent) = repo::messages::get(store, &dig.id, parent_id)? {
            out.in_reply_to = parent.headers.message_id.clone();
            out.references = parent
                .headers
                .references
                .iter()
                .cloned()
                .chain(parent.headers.message_id.clone())
                .collect();
            out.thread_provider_id
                .get_or_insert_with(|| parent.thread_id.provider_id().to_string());
        }
    }
    Ok(out)
}

pub struct CreateDraft;
#[async_trait]
impl Action for CreateDraft {
    fn meta(&self) -> ActionMeta {
        drafts_meta("create_draft", "Create a draft")
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, store) = dig_and_store(ctx, &args)?;
        let mut draft: Draft = arg(&args, "draft")?;
        if draft.id.is_empty() {
            draft.id = uuid::Uuid::now_v7().to_string();
        }
        draft.dig_id = dig.id.clone();
        draft.updated_at = ctx.services.clock.now();
        let registry = ctx.services.require::<ProviderRegistry>()?;
        let provider = registry.provider_for(&dig)?;
        let outgoing = outgoing_from_draft(&ctx.services.stores.index(), &dig, &store, &draft)?;
        match provider.create_draft(&outgoing).await {
            Ok(pid) => draft.provider_id = Some(pid),
            Err(e) if e.kind.is_transient() || e.kind == riddle_errors::ErrorKind::ProviderAuth => {
                // The draft still exists locally; the provider copy follows on the next save.
                rwarn!(
                    Category::Actions,
                    "draft {} kept locally; provider draft failed: {}",
                    draft.id,
                    e.kind.as_str()
                );
            }
            Err(e) => return Err(e),
        }
        repo::drafts::upsert(&store, &draft)?;
        Ok(
            ActionResult::ok(serde_json::to_value(&draft)?, "Created a draft")
                .with_affected(vec![EntityRef::new(EntityKind::Draft, &draft.id)])
                .with_inputs(json!({"digId": dig.id, "draftId": draft.id}))
                .with_undo(
                    json!({"digId": dig.id, "draftId": draft.id}),
                    "Delete draft",
                ),
        )
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        DeleteDraft.execute(ctx, restore).await
    }
}

pub struct UpdateDraft;
#[async_trait]
impl Action for UpdateDraft {
    fn meta(&self) -> ActionMeta {
        drafts_meta("update_draft", "Update a draft")
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, store) = dig_and_store(ctx, &args)?;
        let mut draft: Draft = arg(&args, "draft")?;
        let previous = repo::drafts::get(&store, &draft.id)?
            .ok_or_else(|| RiddleError::not_found(format!("no draft {}", draft.id)))?;
        draft.dig_id = dig.id.clone();
        draft.provider_id = draft.provider_id.or(previous.provider_id.clone());
        draft.updated_at = ctx.services.clock.now();
        let registry = ctx.services.require::<ProviderRegistry>()?;
        let provider = registry.provider_for(&dig)?;
        let outgoing = outgoing_from_draft(&ctx.services.stores.index(), &dig, &store, &draft)?;
        let result = match &draft.provider_id {
            Some(pid) => provider.update_draft(pid, &outgoing).await,
            None => provider.create_draft(&outgoing).await,
        };
        match result {
            Ok(pid) => draft.provider_id = Some(pid),
            Err(e)
                if e.kind.is_transient()
                    || e.kind == riddle_errors::ErrorKind::ProviderAuth
                    || e.kind == riddle_errors::ErrorKind::NotFound =>
            {
                rwarn!(
                    Category::Actions,
                    "draft {} updated locally; provider update failed: {}",
                    draft.id,
                    e.kind.as_str()
                );
            }
            Err(e) => return Err(e),
        }
        repo::drafts::upsert(&store, &draft)?;
        Ok(
            ActionResult::ok(serde_json::to_value(&draft)?, "Updated a draft")
                .with_affected(vec![EntityRef::new(EntityKind::Draft, &draft.id)])
                .with_inputs(json!({"digId": dig.id, "draftId": draft.id}))
                .with_undo(
                    json!({"digId": dig.id, "draft": previous}),
                    "Restore previous draft",
                ),
        )
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        self.execute(ctx, restore).await
    }
}

pub struct DeleteDraft;
#[async_trait]
impl Action for DeleteDraft {
    fn meta(&self) -> ActionMeta {
        drafts_meta("delete_draft", "Delete a draft")
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, store) = dig_and_store(ctx, &args)?;
        let id: String = arg(&args, "draftId")?;
        let draft = repo::drafts::get(&store, &id)?
            .ok_or_else(|| RiddleError::not_found(format!("no draft {id}")))?;
        if let Some(pid) = &draft.provider_id {
            let registry = ctx.services.require::<ProviderRegistry>()?;
            if let Err(e) = registry.provider_for(&dig)?.delete_draft(pid).await {
                riddle_observability::swallowed!(
                    Category::Actions,
                    "provider_draft_delete",
                    e,
                    "provider draft for {} not deleted",
                    id
                );
            }
        }
        repo::drafts::remove(&store, &id)?;
        Ok(
            ActionResult::ok(json!({"deleted": true}), "Deleted a draft")
                .with_affected(vec![EntityRef::new(EntityKind::Draft, &id)])
                .with_inputs(json!({"digId": dig.id, "draftId": id}))
                .with_undo(
                    json!({"digId": dig.id, "draft": Draft { provider_id: None, ..draft }}),
                    "Restore draft",
                ),
        )
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        CreateDraft.execute(ctx, restore).await
    }
}

pub struct SendMessage;
#[async_trait]
impl Action for SendMessage {
    fn meta(&self) -> ActionMeta {
        meta("send_message", "Send a draft")
            .mutating()
            .external()
            .risk(Risk::High)
            .confirm(ConfirmationPolicy::TwoStep)
            .autonomy(Autonomy::Trusted)
            .egress()
            .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let (dig, store) = dig_and_store(ctx, args)?;
        let id: String = arg(args, "draftId")?;
        let draft = repo::drafts::get(&store, &id)?
            .ok_or_else(|| RiddleError::not_found(format!("no draft {id}")))?;
        let outgoing = outgoing_from_draft(&ctx.services.stores.index(), &dig, &store, &draft)?;
        if outgoing.recipients().is_empty() {
            return Err(RiddleError::invalid("the draft has no recipients"));
        }
        let names = |v: &[Address]| v.iter().map(|a| a.email.clone()).collect::<Vec<_>>();
        Ok(Preview {
            summary: format!(
                "Send \"{}\" to {}",
                draft.subject,
                outgoing
                    .recipients()
                    .iter()
                    .map(|a| a.email.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            real_arguments: json!({
                "digId": dig.id, "draftId": id, "from": outgoing.from.email,
                "to": names(&outgoing.to), "cc": names(&outgoing.cc), "bcc": names(&outgoing.bcc),
                "subject": outgoing.subject, "replyTo": outgoing.in_reply_to,
            }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, store) = dig_and_store(ctx, &args)?;
        let id: String = arg(&args, "draftId")?;
        let draft = repo::drafts::get(&store, &id)?
            .ok_or_else(|| RiddleError::not_found(format!("no draft {id}")))?;
        let outgoing = outgoing_from_draft(&ctx.services.stores.index(), &dig, &store, &draft)?;
        if outgoing.recipients().is_empty() {
            return Err(RiddleError::invalid("the draft has no recipients"));
        }
        let registry = ctx.services.require::<ProviderRegistry>()?;
        let provider = registry.provider_for(&dig)?;
        // Only through the provider: the provider's copy is the truth of what
        // went out, so prefer sending the provider draft when one exists.
        let sent = match &draft.provider_id {
            Some(pid) => match provider.send_draft(pid).await {
                Ok(s) => s,
                Err(e) if e.kind == riddle_errors::ErrorKind::NotFound => {
                    provider.send(&outgoing).await?
                }
                Err(e) => return Err(e),
            },
            None => provider.send(&outgoing).await?,
        };
        rinfo!(
            Category::Actions,
            "sent message {} in dig {} to {} recipients",
            sent.provider_id,
            dig.id,
            outgoing.recipients().len()
        );
        repo::drafts::remove(&store, &id)?;
        // Best effort: bring the sent copy in now so Thread State updates
        // before the next poll. Sync will reconcile it anyway.
        let message_id = MessageId::for_provider(&dig.id, &sent.provider_id);
        if let Ok(m) = provider
            .get_message(&sent.provider_id, crate::provider::Detail::Full)
            .await
        {
            if let Err(e) = repo::messages::upsert(&store, &m) {
                riddle_observability::swallowed!(
                    Category::Actions,
                    "sent_message_cache",
                    e,
                    "sent message {} not cached",
                    message_id
                );
            } else {
                ctx.services.emit(RiddleEvent::MessagesChanged {
                    dig_id: dig.id.clone(),
                    count: 1,
                });
            }
        }
        let mut r = ActionResult::ok(
            json!({"messageId": message_id, "threadProviderId": sent.thread_provider_id}),
            format!("Sent \"{}\"", draft.subject),
        )
        .with_affected(vec![
            EntityRef::new(EntityKind::Message, &message_id),
            EntityRef::new(EntityKind::Draft, &id),
        ])
        .with_inputs(
            json!({"digId": dig.id, "draftId": id, "recipients": outgoing.recipients().len()}),
        );
        r.undo = UndoInfo::NotUndoable {
            reason: "a sent message cannot be recalled".into(),
        };
        Ok(r)
    }
}

pub struct AddFixtureDig;
#[async_trait]
impl Action for AddFixtureDig {
    fn meta(&self) -> ActionMeta {
        meta("add_fixture_dig", "Add a synthetic (fixture) Dig")
            .mutating()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let name: String = arg(&args, "name")?;
        let email: String = arg(&args, "email")?;
        let seed: Option<u64> = opt(&args, "seed")?;
        let count: Option<u32> = opt(&args, "count")?;
        if !email.contains('@') {
            return Err(RiddleError::invalid("email must contain '@'"));
        }
        let index = ctx.services.stores.index();
        let now = ctx.services.clock.now();
        let dig = Dig {
            id: DigId::new(),
            name: name.clone(),
            email: email.to_lowercase(),
            provider: ProviderKind::Fixture,
            group: opt(&args, "group")?,
            privacy: opt(&args, "privacy")?.unwrap_or(PrivacyMode::CloudJudgment),
            projection: opt(&args, "projection")?.unwrap_or_default(),
            status: DigStatus::NotConnected,
            shadow_started_at: None,
            created_at: now,
            disabled: false,
            history_days: None,
            mailbox: None,
        };
        repo::digs::upsert(&index, &dig)?;
        let store = ctx.services.stores.dig(&dig.id)?;
        let spec = FixtureSpec {
            seed: seed.unwrap_or_else(|| {
                u64::from_str_radix(&sha256_hex(dig.id.as_str())[..12], 16).unwrap_or(1)
            }),
            count: count.unwrap_or(400).clamp(1, 5000),
            email: dig.email.clone(),
            name: name.split(' ').next().unwrap_or("Matt").to_string(),
            days: 60,
        };
        let registry = ctx.services.require::<ProviderRegistry>()?;
        registry.save_fixture_spec(&dig.id, &spec)?;
        let provider = Arc::new(FixtureProvider::new(dig.id.clone(), spec.clone()));
        registry.insert_fixture(dig.id.clone(), provider);
        // Seed the profile so `directly_addressed` is defined (§2 User Profile).
        let mut profile = repo::user_profile::get(&index)?;
        if !profile.is_me(&dig.email) {
            profile.addresses.push(dig.email.clone());
            if profile.name.is_none() {
                profile.name = Some(spec.name.clone());
            }
            let domain = dig
                .email
                .rsplit_once('@')
                .map(|(_, d)| d.to_string())
                .unwrap_or_default();
            // A free mailbox is not an organisation: `gmail.com` there would
            // put every Gmail sender behind the Never Hidden gate.
            if !riddle_core::free_mail_domain(&domain)
                && !profile
                    .organisation_domains
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case(&domain))
            {
                profile.organisation_domains.push(domain);
            }
            repo::user_profile::put(&index, &profile)?;
        }
        let _ = store; // opened so the file exists before the first sync
        ctx.services.emit(RiddleEvent::DigStatusChanged {
            dig_id: dig.id.clone(),
            status: dig.status.clone(),
        });
        ctx.services.emit(RiddleEvent::SettingsChanged);
        rinfo!(
            Category::Actions,
            "added fixture dig {} (seed {}, count {})",
            dig.id,
            spec.seed,
            spec.count
        );
        Ok(ActionResult::ok(
            serde_json::to_value(&dig)?,
            format!("Added fixture Dig \"{name}\""),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Dig, &dig.id)])
        .with_inputs(json!({"digId": dig.id, "seed": spec.seed, "count": spec.count})))
    }
}

pub struct RemoveDig;
#[async_trait]
impl Action for RemoveDig {
    fn meta(&self) -> ActionMeta {
        meta(
            "remove_dig",
            "Remove a Dig and purge its store and credentials",
        )
        .mutating()
        .risk(Risk::Critical)
        .confirm(ConfirmationPolicy::TypeCount)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let (dig, store) = dig_and_store(ctx, args)?;
        let count = repo::messages::count(&store)?;
        Ok(Preview {
            summary: format!(
                "Remove Dig \"{}\" and its {count} cached messages",
                dig.name
            ),
            real_arguments: json!({"digId": dig.id, "name": dig.name, "count": count}),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, _store) = dig_and_store(ctx, &args)?;
        let index = ctx.services.stores.index();
        if let Some(registry) = ctx.services.get::<ProviderRegistry>() {
            registry.remove(&dig.id);
        }
        if let Some(creds) = ctx.services.get::<riddle_credentials::Credentials>() {
            if let Err(e) = creds.delete(&riddle_credentials::gmail_refresh_token_key(&dig.id)) {
                riddle_observability::swallowed!(
                    Category::Actions,
                    "credential_purge",
                    e,
                    "credential purge for dig {} failed",
                    dig.id
                );
            }
        }
        ctx.services.stores.remove_dig(&dig.id)?;
        repo::digs::remove(&index, &dig.id)?;
        // Drop it from its group.
        for mut g in repo::digs::list_groups(&index)? {
            if g.digs.contains(&dig.id) {
                g.digs.retain(|d| d != &dig.id);
                repo::digs::upsert_group(&index, &g)?;
            }
        }
        ctx.services.emit(RiddleEvent::SettingsChanged);
        rinfo!(Category::Actions, "removed dig {}", dig.id);
        let mut r = ActionResult::ok(
            json!({"removed": true}),
            format!("Removed Dig \"{}\"", dig.name),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Dig, &dig.id)])
        .with_inputs(json!({"digId": dig.id}));
        r.undo = UndoInfo::NotUndoable {
            reason: "the store was purged; add the Dig again to resync".into(),
        };
        Ok(r)
    }
}

pub struct UpdateDig;
#[async_trait]
impl Action for UpdateDig {
    fn meta(&self) -> ActionMeta {
        meta("update_dig", "Update a Dig's settings")
            .mutating()
            .reversible()
            .risk(Risk::Medium)
            .autonomy(Autonomy::Trusted)
            .changes_rules()
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let dig: Dig = arg(&args, "dig")?;
        let index = ctx.services.stores.index();
        let previous = repo::digs::get(&index, &dig.id)?
            .ok_or_else(|| RiddleError::not_found(format!("no Dig {}", dig.id)))?;
        // Provider kind and identity are not editable; status is runtime.
        let updated = Dig {
            provider: previous.provider,
            created_at: previous.created_at,
            status: previous.status.clone(),
            // Switched only by `set_dig_enabled`, so saving other settings
            // from a stale copy cannot turn an account back on.
            disabled: previous.disabled,
            ..dig
        };
        repo::digs::upsert(&index, &updated)?;
        ctx.services.emit(RiddleEvent::SettingsChanged);
        Ok(ActionResult::ok(
            serde_json::to_value(&updated)?,
            format!("Updated Dig \"{}\"", updated.name),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Dig, &updated.id)])
        .with_inputs(json!({"digId": updated.id}))
        .with_undo(json!({"dig": previous}), "Restore previous settings"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        self.execute(ctx, restore).await
    }
}

/// Switch an account off or on without removing it. Off, Riddle stops
/// syncing and sifting it and leaves it out of everything that spans
/// accounts; its store, decisions and credentials stay, so switching it on
/// again catches up from where it stopped.
pub struct SetDigEnabled;
#[async_trait]
impl Action for SetDigEnabled {
    fn meta(&self) -> ActionMeta {
        meta(
            "set_dig_enabled",
            "Disable or re-enable an account, keeping its mail",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .changes_rules()
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (mut dig, _store) = dig_and_store(ctx, &args)?;
        let enabled: bool = arg(&args, "enabled")?;
        let was_enabled = !dig.disabled;
        dig.disabled = !enabled;
        let index = ctx.services.stores.index();
        repo::digs::upsert(&index, &dig)?;
        ctx.services.emit(RiddleEvent::SettingsChanged);
        ctx.services.emit(RiddleEvent::SummaryChanged);
        rinfo!(
            Category::Actions,
            "dig {} {}",
            dig.id,
            if enabled { "enabled" } else { "disabled" }
        );
        let summary = if enabled {
            format!("Enabled \"{}\"; it syncs and sifts again", dig.name)
        } else {
            format!("Disabled \"{}\"; its mail is kept", dig.name)
        };
        Ok(
            ActionResult::ok(json!({"digId": dig.id, "enabled": enabled}), summary)
                .with_affected(vec![EntityRef::new(EntityKind::Dig, &dig.id)])
                .with_inputs(json!({"digId": dig.id, "enabled": enabled}))
                .with_undo(
                    json!({"digId": dig.id, "enabled": was_enabled}),
                    if was_enabled {
                        "Enable it again"
                    } else {
                        "Disable it again"
                    },
                ),
        )
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        self.execute(ctx, restore).await
    }
}

pub struct SaveDigGroup;
#[async_trait]
impl Action for SaveDigGroup {
    fn meta(&self) -> ActionMeta {
        meta("save_dig_group", "Create or update a Dig Group")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let group: DigGroup = arg(&args, "group")?;
        if group.name.trim().is_empty() {
            return Err(RiddleError::invalid("a group needs a name"));
        }
        let index = ctx.services.stores.index();
        let previous = repo::digs::list_groups(&index)?
            .into_iter()
            .find(|g| g.name == group.name);
        repo::digs::upsert_group(&index, &group)?;
        // A Dig belongs to at most one group: move members, clear leavers.
        for mut dig in repo::digs::list(&index)? {
            let member = group.digs.contains(&dig.id);
            let was = dig.group.as_deref() == Some(&group.name);
            if member && !was {
                dig.group = Some(group.name.clone());
                repo::digs::upsert(&index, &dig)?;
            } else if !member && was {
                dig.group = None;
                repo::digs::upsert(&index, &dig)?;
            }
        }
        for mut other in repo::digs::list_groups(&index)? {
            if other.name != group.name && other.digs.iter().any(|d| group.digs.contains(d)) {
                other.digs.retain(|d| !group.digs.contains(d));
                repo::digs::upsert_group(&index, &other)?;
            }
        }
        ctx.services.emit(RiddleEvent::SettingsChanged);
        let restore = match previous {
            Some(p) => json!({"group": p}),
            None => json!({"removeGroup": group.name}),
        };
        Ok(ActionResult::ok(
            serde_json::to_value(&group)?,
            format!("Saved Dig Group \"{}\"", group.name),
        )
        .with_inputs(json!({"group": group.name, "digs": group.digs.len()}))
        .with_undo(restore, "Restore previous group"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        if let Some(name) = restore.get("removeGroup").and_then(|v| v.as_str()) {
            let index = ctx.services.stores.index();
            for mut dig in repo::digs::list(&index)? {
                if dig.group.as_deref() == Some(name) {
                    dig.group = None;
                    repo::digs::upsert(&index, &dig)?;
                }
            }
            repo::digs::remove_group(&index, name)?;
            ctx.services.emit(RiddleEvent::SettingsChanged);
            return Ok(ActionResult::ok(
                json!({"removed": name}),
                format!("Removed Dig Group \"{name}\""),
            ));
        }
        self.execute(ctx, restore).await
    }
}

/// What refers to a group by name: Meshes scoped to it and Strata scoped to
/// it. Renaming or deleting the group would leave them scoped to nothing and
/// quietly stop them applying, so both refuse while this is not empty.
pub fn group_users(store: &Store, name: &str) -> Result<Vec<String>> {
    let mut users: Vec<String> = repo::meshes::list_latest(store, false)?
        .into_iter()
        .filter(|m| matches!(&m.scope, MeshScope::DigGroup { group } if group == name))
        .map(|m| format!("Mesh \"{}\"", m.name))
        .collect();
    users.extend(
        repo::strata::list(store, false)?
            .into_iter()
            .filter(|s| s.scope_group.as_deref() == Some(name))
            .map(|s| format!("Stratum \"{}\"", s.name)),
    );
    Ok(users)
}

fn refuse_if_used(store: &Store, name: &str, verb: &str) -> Result<()> {
    let users = group_users(store, name)?;
    if users.is_empty() {
        return Ok(());
    }
    Err(RiddleError::invalid(format!(
        "{} {} scoped to \"{name}\"; change {} scope before you {verb} the group",
        users.join(", "),
        if users.len() == 1 { "is" } else { "are" },
        if users.len() == 1 { "its" } else { "their" },
    )))
}

fn find_group(store: &Store, name: &str) -> Result<DigGroup> {
    repo::digs::list_groups(store)?
        .into_iter()
        .find(|g| g.name == name)
        .ok_or_else(|| RiddleError::not_found(format!("no group \"{name}\"")))
}

/// Rename a group, keeping its accounts. Account membership is stored on
/// both sides (the group's list and each Dig's `group`), so both move.
pub struct RenameDigGroup;
#[async_trait]
impl Action for RenameDigGroup {
    fn meta(&self) -> ActionMeta {
        meta("rename_dig_group", "Rename a Dig Group")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let from: String = arg(&args, "from")?;
        let to: String = arg::<String>(&args, "to")?.trim().to_string();
        if to.is_empty() {
            return Err(RiddleError::invalid("a group needs a name"));
        }
        let index = ctx.services.stores.index();
        let group = find_group(&index, &from)?;
        if to == from {
            return Ok(ActionResult::ok(json!({"name": to}), "Nothing to rename"));
        }
        if repo::digs::list_groups(&index)?
            .iter()
            .any(|g| g.name != from && g.name.eq_ignore_ascii_case(&to))
        {
            return Err(RiddleError::invalid(format!(
                "a group called \"{to}\" exists"
            )));
        }
        refuse_if_used(&index, &from, "rename")?;
        repo::digs::upsert_group(
            &index,
            &DigGroup {
                used_by: vec![],
                name: to.clone(),
                digs: group.digs.clone(),
            },
        )?;
        repo::digs::remove_group(&index, &from)?;
        for mut dig in repo::digs::list(&index)? {
            if dig.group.as_deref() == Some(from.as_str()) {
                dig.group = Some(to.clone());
                repo::digs::upsert(&index, &dig)?;
            }
        }
        ctx.services.emit(RiddleEvent::SettingsChanged);
        Ok(ActionResult::ok(
            json!({"name": to}),
            format!("Renamed Dig Group \"{from}\" to \"{to}\""),
        )
        .with_inputs(json!({"from": from, "to": to}))
        .with_undo(json!({"from": to, "to": from}), "Rename it back"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        self.execute(ctx, restore).await
    }
}

/// Delete a group. Its accounts stay; they just belong to no group.
pub struct DeleteDigGroup;
#[async_trait]
impl Action for DeleteDigGroup {
    fn meta(&self) -> ActionMeta {
        meta("delete_dig_group", "Delete a Dig Group (its accounts stay)")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let name: String = arg(&args, "name")?;
        let index = ctx.services.stores.index();
        let group = find_group(&index, &name)?;
        refuse_if_used(&index, &name, "delete")?;
        for mut dig in repo::digs::list(&index)? {
            if dig.group.as_deref() == Some(name.as_str()) {
                dig.group = None;
                repo::digs::upsert(&index, &dig)?;
            }
        }
        repo::digs::remove_group(&index, &name)?;
        ctx.services.emit(RiddleEvent::SettingsChanged);
        Ok(ActionResult::ok(
            json!({"removed": name}),
            format!("Deleted Dig Group \"{name}\""),
        )
        .with_inputs(json!({"group": name, "digs": group.digs.len()}))
        .with_undo(
            json!({"group": group}),
            "Restore the group and its accounts",
        ))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        SaveDigGroup.execute(ctx, restore).await
    }
}

pub struct InjectFixtureMessage;
#[async_trait]
impl Action for InjectFixtureMessage {
    fn meta(&self) -> ActionMeta {
        meta(
            "inject_fixture_message",
            "Demo Mode: deliver a synthetic message to a fixture Dig",
        )
        .mutating()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig, _store) = dig_and_store(ctx, &args)?;
        if dig.provider != ProviderKind::Fixture {
            return Err(RiddleError::invalid(
                "messages can only be injected into a fixture Dig",
            ));
        }
        let registry = ctx.services.require::<ProviderRegistry>()?;
        registry.provider_for(&dig)?;
        let fixture = registry
            .fixture(&dig.id)
            .ok_or_else(|| RiddleError::internal("fixture provider missing"))?;
        let template: String = opt(&args, "template")?.unwrap_or_else(|| "custom".into());
        let from: Option<String> = opt(&args, "from")?;
        let from = match from {
            Some(f) => Some(
                crate::mime::parse_address(&f)
                    .ok_or_else(|| RiddleError::invalid("from is not an address"))?,
            ),
            None => None,
        };
        let message =
            fixture.inject(&template, from, opt(&args, "subject")?, opt(&args, "body")?)?;
        if let Some(triggers) = ctx.services.get::<RefreshTriggers>() {
            triggers.trigger(Some(&dig.id));
        }
        let mut value = serde_json::to_value(&message)?;
        if let Some(o) = value.as_object_mut() {
            o.insert("messageId".into(), json!(message.id));
            o.insert("digId".into(), json!(dig.id));
        }
        let mut r = ActionResult::ok(value, format!("Injected a {template} message"))
            .with_affected(vec![EntityRef::new(EntityKind::Message, &message.id)])
            .with_inputs(json!({"digId": dig.id, "template": template}));
        r.undo = UndoInfo::NotUndoable {
            reason: "a delivered message stays delivered; archive it instead".into(),
        };
        Ok(r)
    }
}

/// The Actions this crate contributes to the registry.
pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(CreateDraft),
        Arc::new(UpdateDraft),
        Arc::new(DeleteDraft),
        Arc::new(SendMessage),
        Arc::new(AddFixtureDig),
        Arc::new(RemoveDig),
        Arc::new(UpdateDig),
        Arc::new(SetDigEnabled),
        Arc::new(SaveDigGroup),
        Arc::new(RenameDigGroup),
        Arc::new(DeleteDigGroup),
        Arc::new(InjectFixtureMessage),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_actions::{Runtime, RuntimeBuilder, Services};
    use riddle_store::Stores;

    fn runtime() -> Arc<Runtime> {
        let stores = Arc::new(Stores::in_memory().unwrap());
        let services = Arc::new(Services::new(
            stores.clone(),
            Arc::new(RecordingSink::default()),
            BuildStamp::from_env_vars("0", None, None, None, "test"),
        ));
        services.insert(Arc::new(ProviderRegistry::new(stores)));
        services.insert(Arc::new(RefreshTriggers::new()));
        services.insert(Arc::new(riddle_credentials::Credentials::in_memory()));
        let mut b = RuntimeBuilder::new(services);
        for a in actions() {
            b = b.register(a);
        }
        b.build()
    }

    async fn add_dig(rt: &Runtime) -> Dig {
        let out = rt
            .invoke_as_user(
                "add_fixture_dig",
                json!({"name": "Work", "email": "matt@example.com", "seed": 5, "count": 60}),
            )
            .await
            .unwrap();
        serde_json::from_value(out.result).unwrap()
    }

    #[tokio::test]
    async fn adding_a_fixture_dig_creates_the_row_the_store_the_provider_and_the_profile() {
        let rt = runtime();
        let dig = add_dig(&rt).await;
        assert_eq!(dig.provider, ProviderKind::Fixture);
        let index = rt.services.stores.index();
        assert!(repo::digs::get(&index, &dig.id).unwrap().is_some());
        let registry = rt.services.require::<ProviderRegistry>().unwrap();
        assert!(registry.fixture(&dig.id).is_some());
        assert_eq!(registry.fixture(&dig.id).unwrap().spec().seed, 5);
        assert!(repo::user_profile::get(&index)
            .unwrap()
            .is_me("matt@example.com"));
        // The spec is persisted, so a fresh registry rebuilds the same mailbox.
        let fresh = ProviderRegistry::new(rt.services.stores.clone());
        let p = fresh.provider_for(&dig).unwrap();
        assert_eq!(p.profile().await.unwrap().email, "matt@example.com");
        assert_eq!(fresh.fixture(&dig.id).unwrap().spec().seed, 5);
    }

    #[tokio::test]
    async fn drafts_round_trip_and_send_previews_the_real_recipients_and_is_not_undoable() {
        let rt = runtime();
        let dig = add_dig(&rt).await;
        let draft = Draft {
            id: String::new(),
            dig_id: dig.id.clone(),
            provider_id: None,
            thread_id: None,
            in_reply_to: None,
            to: vec![Address::new("priya@example.com", Some("Priya"))],
            cc: vec![],
            bcc: vec![],
            subject: "Numbers".into(),
            body_text: "Here they are.".into(),
            updated_at: chrono::Utc::now(),
        };
        let created: Draft = serde_json::from_value(
            rt.invoke_as_user("create_draft", json!({"digId": dig.id, "draft": draft}))
                .await
                .unwrap()
                .result,
        )
        .unwrap();
        assert!(created.provider_id.is_some());
        let store = rt.services.stores.dig(&dig.id).unwrap();
        assert_eq!(repo::drafts::list(&store).unwrap().len(), 1);

        let mut edited = created.clone();
        edited.subject = "Q3 numbers".into();
        rt.invoke_as_user("update_draft", json!({"digId": dig.id, "draft": edited}))
            .await
            .unwrap();
        assert_eq!(
            repo::drafts::get(&store, &created.id)
                .unwrap()
                .unwrap()
                .subject,
            "Q3 numbers"
        );

        let inv = ActionInvocation {
            action: "send_message".into(),
            args: json!({"digId": dig.id, "draftId": created.id}),
            principal: Principal::User,
            dig_id: Some(dig.id.clone()),
            idempotency_key: None,
            confirmed: false,
            pending_id: None,
        };
        let err = rt.invoke(inv.clone()).await.unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::NeedsConfirmation);
        let pending: PendingAction = serde_json::from_value(err.detail.unwrap()).unwrap();
        assert_eq!(pending.confirmation, ConfirmationPolicy::TwoStep);
        assert_eq!(pending.real_arguments["to"][0], "priya@example.com");
        assert_eq!(pending.real_arguments["from"], "matt@example.com");

        let out = rt
            .invoke(ActionInvocation {
                pending_id: Some(pending.id),
                ..inv
            })
            .await
            .unwrap();
        let activity = repo::activities::get(
            &rt.services.stores.index(),
            out.activity_id.as_ref().unwrap(),
        )
        .unwrap()
        .unwrap();
        assert!(matches!(activity.undo, UndoInfo::NotUndoable { .. }));
        assert!(repo::drafts::list(&store).unwrap().is_empty());
        let registry = rt.services.require::<ProviderRegistry>().unwrap();
        assert_eq!(registry.fixture(&dig.id).unwrap().call_count("send"), 1);
        let sent_id: MessageId = serde_json::from_value(out.result["messageId"].clone()).unwrap();
        assert!(
            repo::messages::get(&store, &dig.id, &sent_id)
                .unwrap()
                .unwrap()
                .is_sent
        );
    }

    #[tokio::test]
    async fn injecting_a_message_reaches_the_fixture_and_triggers_a_refresh() {
        let rt = runtime();
        let dig = add_dig(&rt).await;
        let triggers = rt.services.require::<RefreshTriggers>().unwrap();
        let mut refreshes = triggers.subscribe();
        let out = rt
            .invoke_as_user(
                "inject_fixture_message",
                json!({"digId": dig.id, "template": "phishing", "subject": "Verify now"}),
            )
            .await
            .unwrap();
        let m: Message = serde_json::from_value(out.result).unwrap();
        assert_eq!(m.subject, "Verify now");
        assert_eq!(m.headers.authentication.spf, AuthVerdict::Fail);
        assert_eq!(
            refreshes.try_recv().unwrap(),
            Some(dig.id.clone()),
            "the injection asked for this Dig's refresh"
        );
        let err = rt
            .invoke_as_user(
                "inject_fixture_message",
                json!({"digId": dig.id, "template": "nope"}),
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
    }

    #[tokio::test]
    async fn groups_keep_a_dig_in_at_most_one_group_and_removal_purges_everything() {
        let rt = runtime();
        let a = add_dig(&rt).await;
        let b = rt
            .invoke_as_user(
                "add_fixture_dig",
                json!({"name": "Home", "email": "matt@home.example", "count": 20}),
            )
            .await
            .unwrap();
        let b: Dig = serde_json::from_value(b.result).unwrap();
        rt.invoke_as_user(
            "save_dig_group",
            json!({"group": {"name": "Work", "digs": [a.id, b.id]}}),
        )
        .await
        .unwrap();
        rt.invoke_as_user(
            "save_dig_group",
            json!({"group": {"name": "Personal", "digs": [b.id]}}),
        )
        .await
        .unwrap();
        let index = rt.services.stores.index();
        let groups = repo::digs::list_groups(&index).unwrap();
        assert_eq!(
            groups.iter().find(|g| g.name == "Work").unwrap().digs,
            vec![a.id.clone()]
        );
        assert_eq!(
            repo::digs::get(&index, &b.id)
                .unwrap()
                .unwrap()
                .group
                .as_deref(),
            Some("Personal")
        );

        let inv = ActionInvocation {
            action: "remove_dig".into(),
            args: json!({"digId": b.id}),
            principal: Principal::User,
            dig_id: None,
            idempotency_key: None,
            confirmed: true,
            pending_id: None,
        };
        rt.invoke(inv).await.unwrap();
        assert!(repo::digs::get(&index, &b.id).unwrap().is_none());
        assert!(repo::digs::list_groups(&index)
            .unwrap()
            .iter()
            .find(|g| g.name == "Personal")
            .unwrap()
            .digs
            .is_empty());
        assert!(rt
            .services
            .require::<ProviderRegistry>()
            .unwrap()
            .get(&b.id)
            .is_none());
    }

    async fn grouped(rt: &Runtime) -> (Dig, Dig) {
        let a = add_dig(rt).await;
        let b = rt
            .invoke_as_user(
                "add_fixture_dig",
                json!({"name": "Home", "email": "matt@home.example", "count": 20}),
            )
            .await
            .unwrap();
        let b: Dig = serde_json::from_value(b.result).unwrap();
        rt.invoke_as_user(
            "save_dig_group",
            json!({"group": {"name": "Work", "digs": [a.id, b.id]}}),
        )
        .await
        .unwrap();
        (a, b)
    }

    fn group_of(rt: &Runtime, dig: &Dig) -> Option<String> {
        repo::digs::get(&rt.services.stores.index(), &dig.id)
            .unwrap()
            .unwrap()
            .group
    }

    #[tokio::test]
    async fn renaming_a_group_moves_its_accounts_and_undo_renames_it_back() {
        let rt = runtime();
        let (a, b) = grouped(&rt).await;
        rt.invoke_as_user("rename_dig_group", json!({"from": "Work", "to": "Clients"}))
            .await
            .unwrap();
        let index = rt.services.stores.index();
        let groups = repo::digs::list_groups(&index).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Clients");
        assert_eq!(groups[0].digs, vec![a.id.clone(), b.id.clone()]);
        assert_eq!(group_of(&rt, &a).as_deref(), Some("Clients"));

        rt.undo_via(
            "rename_dig_group",
            json!({"from": "Clients", "to": "Work"}),
            Principal::User,
            None,
            ActivityId::new(),
        )
        .await
        .unwrap();
        assert_eq!(group_of(&rt, &b).as_deref(), Some("Work"));
    }

    #[tokio::test]
    async fn a_rename_onto_an_existing_group_is_refused() {
        let rt = runtime();
        grouped(&rt).await;
        rt.invoke_as_user(
            "save_dig_group",
            json!({"group": {"name": "Home", "digs": []}}),
        )
        .await
        .unwrap();
        let err = rt
            .invoke_as_user("rename_dig_group", json!({"from": "Work", "to": "home"}))
            .await
            .unwrap_err();
        assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
    }

    #[tokio::test]
    async fn deleting_a_group_keeps_its_accounts_and_undo_restores_membership() {
        let rt = runtime();
        let (a, b) = grouped(&rt).await;
        rt.invoke_as_user("delete_dig_group", json!({"name": "Work"}))
            .await
            .unwrap();
        let index = rt.services.stores.index();
        assert!(repo::digs::list_groups(&index).unwrap().is_empty());
        assert_eq!(repo::digs::list(&index).unwrap().len(), 2);
        assert_eq!(group_of(&rt, &a), None);

        rt.undo_via(
            "delete_dig_group",
            json!({"group": {"name": "Work", "digs": [a.id, b.id]}}),
            Principal::User,
            None,
            ActivityId::new(),
        )
        .await
        .unwrap();
        assert_eq!(group_of(&rt, &b).as_deref(), Some("Work"));
    }

    #[tokio::test]
    async fn a_group_a_stratum_is_scoped_to_can_be_neither_renamed_nor_deleted() {
        let rt = runtime();
        grouped(&rt).await;
        let index = rt.services.stores.index();
        repo::strata::upsert(
            &index,
            &Stratum {
                id: StratumId::new(),
                name: "Invoices".into(),
                version: 1,
                scope_group: Some("Work".into()),
                description: String::new(),
                proposed_by_riddle: false,
                message_count: 0,
                created_at: chrono::Utc::now(),
                deleted_at: None,
            },
        )
        .unwrap();
        for (action, args) in [
            ("rename_dig_group", json!({"from": "Work", "to": "Clients"})),
            ("delete_dig_group", json!({"name": "Work"})),
        ] {
            let err = rt.invoke_as_user(action, args).await.unwrap_err();
            assert_eq!(err.kind, riddle_errors::ErrorKind::Invalid);
            assert!(
                err.message.contains("Stratum \"Invoices\""),
                "{}",
                err.message
            );
        }
        assert_eq!(repo::digs::list_groups(&index).unwrap()[0].name, "Work");
    }
}
