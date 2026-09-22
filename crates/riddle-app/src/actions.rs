//! Actions that belong to the assembled application rather than one
//! domain crate.

use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use serde_json::{json, Value};
use std::sync::Arc;

/// Permanent delete: the provider trashes the messages and Riddle forgets
/// them. Typing the exact count is the confirmation (§51); there is no
/// undo, and the Action says so before it runs.
pub struct PermanentDelete;

#[async_trait]
impl Action for PermanentDelete {
    fn meta(&self) -> ActionMeta {
        meta(
            "permanent_delete",
            "Delete messages permanently (trash at the provider, forget locally)",
        )
        .mutating()
        .external()
        .risk(Risk::Critical)
        .confirm(ConfirmationPolicy::TypeCount)
        .autonomy(Autonomy::Trusted)
        .build()
    }

    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let ids: Vec<MessageId> =
            serde_json::from_value(args.get("messageIds").cloned().unwrap_or(json!([])))?;
        Ok(Preview {
            summary: format!(
                "Permanently delete {} messages. This cannot be undone.",
                ids.len()
            ),
            real_arguments: json!({ "count": ids.len(), "messageIds": ids }),
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let dig_id: DigId =
            serde_json::from_value(args.get("digId").cloned().unwrap_or(Value::Null))
                .map_err(|_| RiddleError::invalid("digId required"))?;
        let ids: Vec<MessageId> =
            serde_json::from_value(args.get("messageIds").cloned().unwrap_or(json!([])))?;
        if ids.is_empty() {
            return Err(RiddleError::invalid("no messages given"));
        }
        let dig = repo::digs::get(&ctx.services.stores.index(), &dig_id)?
            .ok_or_else(|| RiddleError::not_found("dig"))?;
        let domains = ctx.services.require::<crate::domains::Domains>()?;
        let provider = domains.providers.provider_for(&dig)?;
        let provider_ids: Vec<String> = ids.iter().map(|i| i.provider_id().to_string()).collect();
        provider.trash(&provider_ids).await?;
        let store = ctx.services.stores.dig(&dig_id)?;
        for id in &ids {
            repo::messages::mark_deleted(&store, id)?;
        }
        let affected = ids
            .iter()
            .map(|i| EntityRef::new(EntityKind::Message, i))
            .collect();
        let mut r = ActionResult::ok(
            json!({ "deleted": ids.len() }),
            format!("Permanently deleted {} messages", ids.len()),
        )
        .with_affected(affected)
        .with_inputs(json!({ "digId": dig_id, "count": ids.len() }));
        r.undo = UndoInfo::NotUndoable {
            reason: "permanent deletion has no undo".into(),
        };
        Ok(r)
    }
}

/// Export every running Mesh as one text file in the §2 DSL (policy travels
/// as files; §12). Read-only: the file content is the result.
pub struct ExportMeshes;

#[async_trait]
impl Action for ExportMeshes {
    fn meta(&self) -> ActionMeta {
        meta("export_meshes", "Export Meshes as text in the Mesh DSL").build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let include_disabled = args
            .get("includeDisabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let index = ctx.services.stores.index();
        let meshes = repo::meshes::list_latest(&index, false)?;
        let mut out = String::new();
        let mut n = 0;
        for m in meshes
            .iter()
            .filter(|m| include_disabled || m.status != MeshStatus::Disabled)
        {
            out.push_str(&riddle_mesh::format::format(m));
            out.push_str("\n\n---\n\n");
            n += 1;
        }
        Ok(ActionResult::ok(
            json!({ "text": out, "count": n }),
            format!("Exported {n} Meshes"),
        ))
    }
}

/// Import Meshes from exported text; each becomes a new Draft Mesh (or a
/// new version when the name exists), never Active without the user.
pub struct ImportMeshes;

#[async_trait]
impl Action for ImportMeshes {
    fn meta(&self) -> ActionMeta {
        meta("import_meshes", "Import Meshes from text as Drafts")
            .mutating()
            .reversible()
            .risk(Risk::Medium)
            .confirm(ConfirmationPolicy::InPlaceUndo)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let blocks: Vec<&str> = text
            .split("\n---\n")
            .filter(|b| !b.trim().is_empty())
            .collect();
        let names: Vec<String> = blocks
            .iter()
            .filter_map(|b| riddle_mesh::parse::parse(b).ok().map(|m| m.name))
            .collect();
        Ok(Preview {
            summary: format!("Import {} Meshes as Drafts", names.len()),
            real_arguments: json!({ "names": names }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RiddleError::invalid("text required"))?;
        let svc = riddle_mesh::service::MeshService::new(ctx.services.stores.index());
        let mut created = Vec::new();
        let mut affected = Vec::new();
        for block in text.split("\n---\n").filter(|b| !b.trim().is_empty()) {
            let parsed = riddle_mesh::parse::parse(block)?;
            let m = match repo::meshes::find_by_name(&ctx.services.stores.index(), &parsed.name)? {
                Some(existing) => svc.update(
                    &existing.id,
                    block,
                    Some("imported".into()),
                    CreatedBy::User,
                )?,
                None => svc.create(
                    block,
                    Some(parsed.scope.clone()),
                    MeshStatus::Draft,
                    CreatedBy::User,
                )?,
            };
            affected.push(EntityRef::new(EntityKind::Mesh, &m.id));
            created.push(json!({ "meshId": m.id, "version": m.version, "name": m.name }));
        }
        let n = created.len();
        Ok(ActionResult::ok(
            json!({ "imported": created.clone() }),
            format!("Imported {n} Meshes as Drafts"),
        )
        .with_affected(affected)
        .with_inputs(json!({ "count": n }))
        .with_undo(
            json!({ "imported": created }),
            "Remove the imported versions",
        ))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let items = restore
            .get("imported")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let index = ctx.services.stores.index();
        let mut n = 0;
        for it in items {
            let (Some(id), Some(v)) = (
                it.get("meshId").and_then(|v| v.as_str()),
                it.get("version").and_then(|v| v.as_u64()),
            ) else {
                continue;
            };
            repo::meshes::remove_version(&index, &MeshId::from(id), v as u32)?;
            n += 1;
        }
        Ok(ActionResult::ok(
            json!({ "removed": n }),
            format!("Removed {n} imported Mesh versions"),
        ))
    }
}

/// Unsubscribe through the message's List-Unsubscribe header (RFC 8058
/// one-click when it offers an https URL). Gmail's API has no unsubscribe
/// (§16), so this is Riddle's own, and it is an external side effect.
pub struct Unsubscribe;

#[async_trait]
impl Action for Unsubscribe {
    fn meta(&self) -> ActionMeta {
        meta(
            "unsubscribe",
            "Unsubscribe via the sender's List-Unsubscribe link",
        )
        .mutating()
        .external()
        .risk(Risk::Medium)
        .confirm(ConfirmationPolicy::InPlaceUndo)
        .autonomy(Autonomy::Trusted)
        .egress()
        .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let (dig_id, message_id) = ids(args)?;
        let store = ctx.services.stores.dig(&dig_id)?;
        let m = repo::messages::get(&store, &dig_id, &message_id)?
            .ok_or_else(|| RiddleError::not_found("message"))?;
        let target = unsubscribe_target(m.headers.list_unsubscribe.as_deref());
        Ok(Preview {
            summary: format!("Unsubscribe from {}", m.from.email),
            real_arguments: json!({ "sender": m.from.email, "target": target }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (dig_id, message_id) = ids(&args)?;
        let store = ctx.services.stores.dig(&dig_id)?;
        let m = repo::messages::get(&store, &dig_id, &message_id)?
            .ok_or_else(|| RiddleError::not_found("message"))?;
        let Some(target) = unsubscribe_target(m.headers.list_unsubscribe.as_deref()) else {
            return Err(RiddleError::invalid(
                "this message offers no List-Unsubscribe link",
            ));
        };
        let outcome = if target.starts_with("https://") && !m.from.domain().ends_with(".example") {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(riddle_errors::internal)?;
            match client
                .post(&target)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body("List-Unsubscribe=One-Click")
                .send()
                .await
            {
                Ok(r) if r.status().is_success() => "one-click accepted".to_string(),
                Ok(r) => format!("provider answered {}", r.status()),
                Err(e) => {
                    return Err(RiddleError::provider(format!(
                        "unsubscribe request failed: {e}"
                    )))
                }
            }
        } else {
            "recorded (no reachable one-click endpoint)".to_string()
        };
        let mut r = ActionResult::ok(
            json!({ "sender": m.from.email, "outcome": outcome }),
            format!("Unsubscribed from {}", m.from.email),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Message, &m.id)])
        .with_inputs(json!({ "digId": dig_id, "messageId": message_id }));
        r.undo = UndoInfo::NotUndoable {
            reason: "an unsubscribe request cannot be recalled".into(),
        };
        Ok(r)
    }
}

fn ids(args: &Value) -> Result<(DigId, MessageId)> {
    let dig: DigId = serde_json::from_value(args.get("digId").cloned().unwrap_or(Value::Null))
        .map_err(|_| RiddleError::invalid("digId required"))?;
    let message: MessageId =
        serde_json::from_value(args.get("messageId").cloned().unwrap_or(Value::Null))
            .map_err(|_| RiddleError::invalid("messageId required"))?;
    Ok((dig, message))
}

/// Prefer the https URL, then mailto, from a `<a>, <b>` header value.
pub fn unsubscribe_target(header: Option<&str>) -> Option<String> {
    let h = header?;
    let parts: Vec<String> = h
        .split(',')
        .map(|p| p.trim().trim_matches(|c| c == '<' || c == '>').to_string())
        .collect();
    parts
        .iter()
        .find(|p| p.starts_with("https://"))
        .or_else(|| parts.iter().find(|p| p.starts_with("mailto:")))
        .cloned()
}

pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(PermanentDelete),
        Arc::new(ExportMeshes),
        Arc::new(ImportMeshes),
        Arc::new(Unsubscribe),
        Arc::new(SiftClassify),
    ]
}

/// The Activity behind a Sift pass (§47): "Sift moved 14 messages to Spoil,
/// triggered by Mesh X v7; 13 matched on cached Signals, 1 needed a fresh
/// evaluation". Never invoked by a client; it exists so the ledger row has
/// an Action to undo through: undoing returns the messages to Unresolved
/// (recorded as a User classification) and withdraws their projection.
pub struct SiftClassify;

#[async_trait]
impl Action for SiftClassify {
    fn meta(&self) -> ActionMeta {
        meta("sift_classify", "Sift classified messages")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn execute(&self, _ctx: &ActionContext, _args: Value) -> Result<ActionResult> {
        Err(RiddleError::policy_denied(
            "sift_classify is recorded by the engine, not invoked",
        ))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let dig_id: DigId =
            serde_json::from_value(restore.get("digId").cloned().unwrap_or(Value::Null))
                .map_err(|_| RiddleError::invalid("restore lacks digId"))?;
        let ids: Vec<MessageId> =
            serde_json::from_value(restore.get("messageIds").cloned().unwrap_or(json!([])))?;
        let store = ctx.services.stores.dig(&dig_id)?;
        let hook = ctx.services.get::<ProjectionHookHandle>();
        let mut restored = Vec::new();
        for id in &ids {
            let Some(prior) = repo::classifications::current(&store, id)? else {
                continue;
            };
            let k = Classification {
                id: ClassificationId::new(),
                message_id: id.clone(),
                dig_id: dig_id.clone(),
                disposition: Disposition::Unresolved,
                strata: vec![],
                notify: false,
                mechanism: DecisionMechanism::User,
                mesh_id: None,
                mesh_version: None,
                matched_rule: Some("undo of a Sift decision".into()),
                confidence: None,
                reason: format!(
                    "Undo returned this message from {} to Unresolved",
                    prior.disposition.kind().as_str()
                ),
                trace_id: prior.trace_id.clone(),
                shadow: false,
                created_at: ctx.services.clock.now(),
                superseded_at: None,
            };
            repo::classifications::record(&store, &k)?;
            if let Some(h) = &hook {
                let _ = h.0.unproject(&dig_id, id);
            }
            restored.push(EntityRef::new(EntityKind::Message, id));
            ctx.services.emit(RiddleEvent::MessageClassified {
                message_id: id.clone(),
                dig_id: dig_id.clone(),
                disposition: Disposition::Unresolved,
                shadow: false,
            });
        }
        let n = restored.len();
        Ok(ActionResult::ok(
            json!({ "restored": n }),
            format!("Returned {n} messages to Unresolved"),
        )
        .with_affected(restored))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_https_one_click_target_is_preferred_over_mailto() {
        let t = unsubscribe_target(Some(
            "<mailto:unsub@list.example>, <https://list.example/u/1>",
        ));
        assert_eq!(t.as_deref(), Some("https://list.example/u/1"));
        assert_eq!(
            unsubscribe_target(Some("<mailto:unsub@list.example>")).as_deref(),
            Some("mailto:unsub@list.example")
        );
        assert!(unsubscribe_target(None).is_none());
    }
}
