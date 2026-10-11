//! Actions that belong to the assembled application rather than one
//! domain crate.

use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview, Services};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, rwarn, Category};
use riddle_store::repo;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::sync::Arc;

fn pack_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| RiddleError::invalid(format!("missing argument {key}")))
}

/// Install a rule pack (or update it to the version Riddle now has): its
/// lists are created from the answers given (or the pack's defaults) and
/// its Meshes placed by band, in Shadow unless `status` says Active. A Mesh
/// of the same name that is not from the pack is refused unless `replace`.
/// Undo removes what was created and puts replaced Meshes and lists back.
pub struct InstallPack;

#[async_trait]
impl Action for InstallPack {
    fn meta(&self) -> ActionMeta {
        meta(
            "install_pack",
            "Install or update a rule pack, with answers for its lists",
        )
        .mutating()
        .reversible()
        .risk(Risk::Medium)
        .confirm(ConfirmationPolicy::TwoStep)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let index = ctx.services.stores.index();
        let id = pack_arg(args, "packId")?;
        let info = riddle_mesh::packs::catalogue(&index)?
            .into_iter()
            .find(|i| i.pack.id == id)
            .ok_or_else(|| RiddleError::not_found(format!("no pack {id}")))?;
        let replace = args
            .get("replace")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let verb = match &info.installed {
            Some(i) if i.version < info.pack.version => format!(
                "Update {} from v{} to v{}",
                info.pack.name, i.version, info.pack.version
            ),
            Some(_) => format!("Reinstall {}", info.pack.name),
            None => format!("Install {}", info.pack.name),
        };
        Ok(Preview {
            summary: format!(
                "{verb}: {} Meshes ({}){}",
                info.mesh_names.len(),
                info.mesh_names.join(", "),
                if info.conflicts.is_empty() {
                    String::new()
                } else if replace {
                    format!("; replaces your {}", info.conflicts.join(", "))
                } else {
                    format!(
                        "; {} already exist and would be refused",
                        info.conflicts.join(", ")
                    )
                }
            ),
            real_arguments: json!({ "packId": id, "version": info.pack.version, "meshes": info.mesh_names, "conflicts": info.conflicts, "replace": replace, "lists": info.pack.lists.iter().map(|l| &l.name).collect::<Vec<_>>() }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let index = ctx.services.stores.index();
        let id = pack_arg(&args, "packId")?;
        let (pack, _) = riddle_mesh::packs::find_pack(&index, id)?;
        let answers: std::collections::BTreeMap<String, Vec<String>> =
            serde_json::from_value(args.get("lists").cloned().unwrap_or(json!({})))
                .map_err(|e| RiddleError::invalid(format!("lists: {e}")))?;
        let status = match args.get("status").and_then(|v| v.as_str()) {
            Some("active") => MeshStatus::Active,
            Some("shadow") | None => MeshStatus::Shadow,
            Some(other) => {
                return Err(RiddleError::invalid(format!(
                    "status is shadow or active, not {other}"
                )))
            }
        };
        let replace = args
            .get("replace")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let report = riddle_mesh::packs::install(
            &index,
            &pack,
            &answers,
            status,
            replace,
            riddle_mesh::actions::created_by(&ctx.principal),
        )?;
        ctx.services.emit(RiddleEvent::SettingsChanged);
        rinfo!(
            Category::Actions,
            "installed pack {} v{}",
            pack.id,
            pack.version
        );
        Ok(ActionResult::ok(
            json!({ "packId": pack.id, "created": report.created, "replaced": report.replaced.iter().map(|r| &r.id).collect::<Vec<_>>(), "listsCreated": report.lists_created }),
            format!(
                "Installed {} v{}: {} new, {} replaced",
                pack.name,
                pack.version,
                report.created.len(),
                report.replaced.len()
            ),
        )
        .with_inputs(json!({ "packId": pack.id, "version": pack.version }))
        .with_undo(json!({ "packId": pack.id, "report": report }), "Undo the install"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let id = pack_arg(&restore, "packId")?.to_string();
        let report: riddle_mesh::packs::InstallReport =
            serde_json::from_value(restore["report"].clone())
                .map_err(|e| RiddleError::invalid(format!("report: {e}")))?;
        riddle_mesh::packs::undo_install(&ctx.services.stores.index(), &id, &report)?;
        Ok(ActionResult::ok(
            json!({ "packId": id }),
            format!("Undid the install of {id}"),
        ))
    }
}

/// Remove an installed pack: its Meshes go to Recently Deleted; the lists
/// stay, since they hold your answers.
pub struct RemovePack;

#[async_trait]
impl Action for RemovePack {
    fn meta(&self) -> ActionMeta {
        meta(
            "remove_pack",
            "Remove an installed rule pack (its Meshes go to Recently Deleted)",
        )
        .mutating()
        .risk(Risk::Medium)
        .confirm(ConfirmationPolicy::TwoStep)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = pack_arg(&args, "packId")?;
        let inst = riddle_mesh::packs::remove(&ctx.services.stores.index(), id)?;
        ctx.services.emit(RiddleEvent::SettingsChanged);
        let mut r = ActionResult::ok(
            json!({ "packId": id, "meshes": inst.meshes }),
            format!(
                "Removed pack {id}; its {} Meshes are in Recently Deleted",
                inst.meshes.len()
            ),
        );
        r.undo = UndoInfo::NotUndoable {
            reason: "restore the Meshes from Recently Deleted, or install the pack again".into(),
        };
        Ok(r)
    }
}

/// Add a pack from a file someone shared (TOML). It joins the catalogue;
/// installing it is a separate step.
pub struct ImportPack;

#[async_trait]
impl Action for ImportPack {
    fn meta(&self) -> ActionMeta {
        meta(
            "import_pack",
            "Add a rule pack from a pack file to the catalogue",
        )
        .mutating()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let text = pack_arg(&args, "text")?;
        let pack = riddle_mesh::packs::save_imported(&ctx.services.stores.index(), text)?;
        Ok(ActionResult::ok(
            json!({ "packId": pack.id, "name": pack.name, "version": pack.version }),
            format!(
                "Added pack {} v{} to the catalogue",
                pack.name, pack.version
            ),
        ))
    }
}

/// The pack file for a pack, to save and share.
pub struct ExportPack;

#[async_trait]
impl Action for ExportPack {
    fn meta(&self) -> ActionMeta {
        meta("export_pack", "Write a rule pack as a pack file").build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = pack_arg(&args, "packId")?;
        let (pack, _) = riddle_mesh::packs::find_pack(&ctx.services.stores.index(), id)?;
        let text = riddle_mesh::packs::to_toml(&pack)?;
        Ok(ActionResult::ok(
            json!({ "packId": id, "fileName": format!("{id}.riddlepack.toml"), "text": text }),
            format!("Pack file for {}", pack.name),
        ))
    }
}

/// Write a new pack from Meshes and lists on this store, to share. List
/// entries are left out unless `includeEntries`: they are your senders.
pub struct CreatePack;

#[async_trait]
impl Action for CreatePack {
    fn meta(&self) -> ActionMeta {
        meta(
            "create_pack",
            "Make a rule pack from your Meshes and lists, to share",
        )
        .mutating()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let index = ctx.services.stores.index();
        let mesh_ids: Vec<MeshId> =
            serde_json::from_value(args.get("meshIds").cloned().unwrap_or(json!([])))
                .map_err(|e| RiddleError::invalid(format!("meshIds: {e}")))?;
        if mesh_ids.is_empty() {
            return Err(RiddleError::invalid(
                "choose at least one Mesh for the pack",
            ));
        }
        let pack = riddle_mesh::packs::pack_from(
            &index,
            pack_arg(&args, "packId")?,
            pack_arg(&args, "name")?,
            args.get("area").and_then(|v| v.as_str()).unwrap_or("Mine"),
            args.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
            &mesh_ids,
            args.get("includeEntries")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        )?;
        let text = riddle_mesh::packs::to_toml(&pack)?;
        riddle_mesh::packs::save_imported(&index, &text)?;
        Ok(ActionResult::ok(
            json!({ "packId": pack.id, "fileName": format!("{}.riddlepack.toml", pack.id), "text": text }),
            format!("Made pack {} from {} Meshes", pack.name, mesh_ids.len()),
        ))
    }
}

/// Grade one decision a Mesh made in the spot check: `right` or `wrong`.
/// A wrong one is corrected separately (set_disposition), which is what
/// teaches; the grade is the tally the check shows per Mesh.
pub struct GradeDecision;

#[async_trait]
impl Action for GradeDecision {
    fn meta(&self) -> ActionMeta {
        meta(
            "grade_decision",
            "Mark a rule's decision on a message right or wrong in the spot check",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let mesh = pack_arg(&args, "meshId")?;
        let message = pack_arg(&args, "messageId")?;
        let verdict = pack_arg(&args, "verdict")?;
        if verdict != "right" && verdict != "wrong" {
            return Err(RiddleError::invalid("verdict is right or wrong"));
        }
        let index = ctx.services.stores.index();
        let key = format!("{}{mesh}:{message}", crate::reads::GRADE_PREFIX);
        let prior = repo::kv::get(&index, &key)?;
        repo::kv::set(&index, &key, verdict)?;
        Ok(
            ActionResult::ok(json!({ "verdict": verdict }), format!("Marked {verdict}"))
                .with_undo(json!({ "key": key, "prior": prior }), "Undo the grade"),
        )
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let key = pack_arg(&restore, "key")?;
        let index = ctx.services.stores.index();
        match restore["prior"].as_str() {
            Some(p) => repo::kv::set(&index, key, p)?,
            None => index.write(|c| {
                c.execute("DELETE FROM kv WHERE key = ?1", [key])?;
                Ok(())
            })?,
        }
        Ok(ActionResult::ok(json!({}), "Grade removed".to_string()))
    }
}

/// The Mesh that holds the user's own sender rules. Its name is the
/// convention the client already uses for "never Spoil from this sender".
pub const YOUR_CORRECTIONS: &str = "Your corrections";
/// Where the user's own rules run: after the Never Hidden guards, before
/// every policy, since the user named the sender.
const YOUR_CORRECTIONS_PRIORITY: i32 = -90;

/// "Always do this for this sender": the way the user decided one message
/// (disposition, Find type, Strata) becomes a route for its sender or
/// domain in Your corrections, so future mail from them is decided the same
/// way. Corrections used to teach only "never Spoil": a message the user
/// moved to Charity, or made a Pay Find, taught nothing, and the next one
/// from that sender was decided as before. A second rule for the same
/// sender or domain replaces the first. Rules like these stay Active when
/// added (`riddle_mesh::service::sender_rules_only`).
pub struct RememberForSender;

#[async_trait]
impl Action for RememberForSender {
    fn meta(&self) -> ActionMeta {
        meta(
            "remember_for_sender",
            "Always decide a sender's (or domain's) mail the way you decided this message",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .build()
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let dig_id: DigId =
            serde_json::from_value(args.get("digId").cloned().unwrap_or(Value::Null))
                .map_err(|_| RiddleError::invalid("missing argument digId"))?;
        let message_id = MessageId(
            args.get("messageId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RiddleError::invalid("missing argument messageId"))?
                .to_string(),
        );
        let scope = args
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("sender");
        let store = ctx.services.stores.dig(&dig_id)?;
        let message = repo::messages::get(&store, &dig_id, &message_id)?
            .ok_or_else(|| RiddleError::not_found(format!("message {message_id}")))?;
        let decided = repo::classifications::current(&store, &message_id)?
            .ok_or_else(|| RiddleError::invalid("the message has no decision to remember yet"))?;
        if decided.disposition == Disposition::Unresolved {
            return Err(RiddleError::invalid(
                "decide the message first (Find, Filed, Spoil…); an Unresolved message has nothing to remember",
            ));
        }
        let email = message.from.email.trim().to_ascii_lowercase();
        let (fact, value) = match scope {
            "sender" => ("sender", email.clone()),
            "domain" => ("sender_domain", message.from.domain().to_ascii_lowercase()),
            other => {
                return Err(RiddleError::invalid(format!(
                    "scope is sender or domain, not {other}"
                )))
            }
        };
        if value.is_empty() || value.contains('"') {
            return Err(RiddleError::invalid(
                "this sender has no address to write a rule for",
            ));
        }
        // A level you set on the message is part of what is remembered.
        let output = MeshOutput {
            disposition: Some(decided.disposition.clone()),
            strata: decided.strata.clone(),
            importance: repo::importance::get(&store, &message_id)?,
            ..Default::default()
        };
        let line = format!(
            "IF {fact} = \"{value}\" → {}",
            riddle_mesh::format::fmt_output(&output)
        );
        let route = riddle_mesh::parse::parse(&format!("Mesh: r\nROUTES\n{line}"))?
            .routes
            .remove(0);
        let index = ctx.services.stores.index();
        let svc = riddle_mesh::service::MeshService::new(index.clone());
        let same_rule = |r: &Route| matches!(&r.condition, Expr::Fact { fact: f, value: FactValue::Text(v), .. } if f == fact && v.eq_ignore_ascii_case(&value));
        let (mesh, prior_text) = match repo::meshes::find_by_name(&index, YOUR_CORRECTIONS)? {
            None => {
                let text = format!(
                    "Mesh: {YOUR_CORRECTIONS}\nScope: All Digs\nPriority: {YOUR_CORRECTIONS_PRIORITY}\n\nROUTES\n\n{line}\n"
                );
                (
                    svc.create(
                        &text,
                        None,
                        MeshStatus::Active,
                        riddle_mesh::actions::created_by(&ctx.principal),
                    )?,
                    None,
                )
            }
            Some(existing) => {
                let prior = riddle_mesh::format::format(&existing);
                let mut next = existing.clone();
                next.routes.retain(|r| !same_rule(r));
                // A sender's rule before a domain's, so the narrower one wins.
                let at = if fact == "sender" {
                    0
                } else {
                    next.routes.len()
                };
                next.routes.insert(at, route);
                if next.routes.len() == 1 && next.priority > YOUR_CORRECTIONS_PRIORITY {
                    next.priority = YOUR_CORRECTIONS_PRIORITY;
                }
                let text = riddle_mesh::format::format(&next);
                let m = svc.update(
                    &existing.id,
                    &text,
                    Some(format!(
                        "Remember {value}: {}",
                        riddle_mesh::format::fmt_output(&output)
                    )),
                    riddle_mesh::actions::created_by(&ctx.principal),
                )?;
                (m, Some(prior))
            }
        };
        let who = if fact == "sender" {
            value.clone()
        } else {
            format!("everything from {value}")
        };
        rinfo!(
            Category::Actions,
            "remembered a rule for {} in mesh {}",
            fact,
            mesh.id
        );
        Ok(ActionResult::ok(
            json!({ "meshId": mesh.id, "version": mesh.version, "rule": line }),
            format!(
                "From now on, {who} → {}",
                riddle_mesh::format::fmt_output(&output)
            ),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Mesh, &mesh.id)])
        .with_inputs(json!({ "scope": scope, "meshId": mesh.id }))
        .with_undo(
            json!({ "meshId": mesh.id, "priorText": prior_text }),
            "Forget the rule",
        ))
    }

    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let mesh_id = MeshId(
            restore["meshId"]
                .as_str()
                .ok_or_else(|| RiddleError::invalid("meshId"))?
                .to_string(),
        );
        let svc = riddle_mesh::service::MeshService::new(ctx.services.stores.index());
        let m = match restore["priorText"].as_str() {
            Some(text) => svc.update(
                &mesh_id,
                text,
                Some("Forget a remembered rule".into()),
                CreatedBy::User,
            )?,
            None => svc.delete(&mesh_id)?,
        };
        Ok(
            ActionResult::ok(json!({ "meshId": m.id }), "Forgot the rule".to_string())
                .with_affected(vec![EntityRef::new(EntityKind::Mesh, &m.id)]),
        )
    }
}

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
        Arc::new(SupersedeMessages),
        Arc::new(ConnectGmail),
        Arc::new(DismissPending),
        Arc::new(Resift),
        Arc::new(DecideAgain),
        Arc::new(SetSecret),
        Arc::new(ClearSecret),
        Arc::new(ImportGmailClient),
        Arc::new(ApplyMeshToHistory),
        Arc::new(RememberForSender),
        Arc::new(InstallPack),
        Arc::new(RemovePack),
        Arc::new(ImportPack),
        Arc::new(ExportPack),
        Arc::new(CreatePack),
        Arc::new(GradeDecision),
        Arc::new(SendHeldChanges),
        Arc::new(DiscardHeldChanges),
    ]
}

/// The accounts whose queued mailbox writes count: enabled, and set to
/// write to their mailbox at all.
fn writing_digs(services: &Services) -> Result<Vec<Dig>> {
    Ok(repo::digs::list_enabled(&services.stores.index())?
        .into_iter()
        .filter(|d| d.projection != ProjectionMode::None)
        .collect())
}

/// What waits in the outboxes, across the accounts that write.
pub(crate) fn held_changes(services: &Services) -> Result<u64> {
    let mut n = 0;
    for d in writing_digs(services)? {
        n += repo::outbox::pending_count(&*services.stores.dig(&d.id)?)?;
    }
    Ok(n)
}

/// `send_held_changes`: write what waits in the outbox now, the "send
/// these" of a dev build on Ask. It calls the provider, so Hold refuses it
/// and Ask asks once for the whole batch: seven corrections are one prompt.
pub struct SendHeldChanges;

#[async_trait]
impl Action for SendHeldChanges {
    fn meta(&self) -> ActionMeta {
        meta(
            "send_held_changes",
            "Write the mailbox changes waiting in the outbox now",
        )
        .mutating()
        .external()
        .risk(Risk::Medium)
        .confirm(ConfirmationPolicy::Approval)
        .autonomy(Autonomy::Trusted)
        .build()
    }

    async fn preview(&self, ctx: &ActionContext, _args: &Value) -> Result<Preview> {
        let n = held_changes(&ctx.services)?;
        Ok(Preview {
            summary: format!(
                "Write {} to the mailbox",
                riddle_core::text::plural(n as usize, "waiting change")
            ),
            real_arguments: json!({ "held": n }),
        })
    }

    async fn execute(&self, ctx: &ActionContext, _args: Value) -> Result<ActionResult> {
        let domains = ctx.services.require::<crate::domains::Domains>()?;
        let (mut applied, mut failed, mut conflicts) = (0u64, 0u64, 0usize);
        for d in writing_digs(&ctx.services)? {
            let r = domains.outbox.drain(&d, 50).await?;
            applied += r.applied;
            failed += r.failed;
            conflicts += r.conflicts.len();
        }
        rinfo!(
            Category::Projection,
            "held changes sent: {applied} applied, {failed} failed, {conflicts} conflicts"
        );
        let mut summary = format!(
            "Wrote {} to the mailbox",
            riddle_core::text::plural(applied as usize, "change")
        );
        if failed + conflicts as u64 > 0 {
            summary.push_str(&format!(
                "; {} did not go and will be retried",
                failed + conflicts as u64
            ));
        }
        Ok(ActionResult::ok(
            json!({ "applied": applied, "failed": failed, "conflicts": conflicts }),
            summary,
        ))
    }
}

/// `discard_held_changes`: drop what waits in the outbox. Riddle keeps its
/// decisions; the mailbox keeps what it had. Not undoable: the rows are
/// gone, and a later correction queues its own.
pub struct DiscardHeldChanges;

#[async_trait]
impl Action for DiscardHeldChanges {
    fn meta(&self) -> ActionMeta {
        meta(
            "discard_held_changes",
            "Drop the mailbox changes waiting in the outbox; Riddle keeps its decisions",
        )
        .mutating()
        .risk(Risk::Medium)
        .confirm(ConfirmationPolicy::Approval)
        .autonomy(Autonomy::Trusted)
        .build()
    }

    async fn preview(&self, ctx: &ActionContext, _args: &Value) -> Result<Preview> {
        let n = held_changes(&ctx.services)?;
        Ok(Preview {
            summary: format!(
                "Drop {}; the mailbox stays as it is",
                riddle_core::text::plural(n as usize, "waiting change")
            ),
            real_arguments: json!({ "held": n }),
        })
    }

    async fn execute(&self, ctx: &ActionContext, _args: Value) -> Result<ActionResult> {
        let mut n = 0;
        for d in writing_digs(&ctx.services)? {
            n += repo::outbox::discard_pending(&*ctx.services.stores.dig(&d.id)?)?;
        }
        Ok(ActionResult::ok(
            json!({ "discarded": n }),
            format!(
                "Dropped {}",
                riddle_core::text::plural(n as usize, "waiting mailbox change")
            ),
        ))
    }
}

/// Most messages one apply may re-decide; a wider change asks for a
/// narrower window so its undo record stays a sensible size.
const APPLY_LIMIT: usize = 2_000;

/// What applying a Mesh to past mail would touch, computed the same way for
/// the preview and the run: the run simulates again rather than trusting a
/// plan shown minutes earlier, because mail moves on between the two.
struct ApplyPlan {
    mesh: Mesh,
    redecide: Vec<(DigId, MessageId)>,
    kept_yours: u64,
    transitions: Vec<String>,
    window_days: u32,
    /// Of `redecide`, those never scored on a Signal the Mesh reads, and
    /// the estimated cost of the judge calls that scores them.
    rescore: u64,
    rescore_cost_usd: Option<f64>,
}

fn apply_plan(ctx: &ActionContext, args: &Value) -> Result<ApplyPlan> {
    let mesh_id = MeshId(
        args.get("meshId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RiddleError::invalid("which Mesh? pass meshId"))?
            .to_string(),
    );
    let window_days = window_days(args, 30);
    let index = ctx.services.stores.index();
    let mesh = repo::meshes::latest(&index, &mesh_id)?
        .ok_or_else(|| RiddleError::not_found(format!("no Mesh {mesh_id}")))?;
    if mesh.status != MeshStatus::Active {
        return Err(RiddleError::invalid(format!(
            "Mesh {} v{} is {:?}; only an active Mesh decides mail, so activate it before applying it to past mail",
            mesh.name, mesh.version, mesh.status
        )));
    }
    let digs = repo::digs::list_enabled(&index)?;
    let mut input = riddle_mesh::simulate::SimulationInput::new(&ctx.services.stores, digs, &mesh);
    input.window_days = window_days;
    input.now = ctx.services.clock.now();
    let (report, changed) = riddle_mesh::simulate::simulate_detailed(input)?;
    let mut redecide = Vec::new();
    let mut kept_yours = 0;
    for c in changed {
        // Your own decisions are never re-decided by a rule change.
        if c.user_decided {
            kept_yours += 1;
        } else {
            redecide.push((c.dig_id, c.message_id));
        }
    }
    let mut transitions: Vec<String> = report
        .transitions
        .iter()
        .map(|t| format!("{} {} → {}", t.count, t.from.as_str(), t.to.as_str()))
        .collect();
    // Re-decided without changing disposition; said so the count adds up.
    if report.recorded_changes > 0 {
        transitions.push(format!(
            "{} keep their disposition with another Find type, Strata or series",
            report.recorded_changes
        ));
    }
    if report.joins_series > 0 {
        transitions.push(format!("{} join a + Replaces series", report.joins_series));
    }
    let per_call = report
        .estimated_cost_usd
        .filter(|_| report.new_judge_calls_required > 0)
        .map(|c| c / report.new_judge_calls_required as f64);
    let plan = ApplyPlan {
        mesh,
        redecide,
        kept_yours,
        transitions,
        window_days,
        rescore: report.rescore,
        rescore_cost_usd: per_call.map(|c| c * report.rescore as f64),
    };
    // Said when the plan is made, so the two-step dialog refuses before the
    // user confirms rather than after.
    if plan.redecide.len() > APPLY_LIMIT {
        return Err(RiddleError::invalid(format!(
            "{} messages would be re-decided; the limit is {APPLY_LIMIT}. Apply a shorter windowDays first; once Sift has re-decided those, the same wider window picks up only what is still decided differently.",
            plan.redecide.len()
        )));
    }
    Ok(plan)
}

/// Re-decide past mail after a Mesh change (§38). A new Mesh version only
/// decides new mail; the messages it would have decided differently keep
/// their old disposition until the user chooses this. It returns them to
/// the Sift backlog rather than writing the simulated answer, so the Never
/// Hidden gate, the current Signals and projection's autonomy rules all
/// apply as for new mail. Messages the user classified by hand are left
/// alone, and so are Finds on them. Undo puts every prior decision back.
pub struct ApplyMeshToHistory;

#[async_trait]
impl Action for ApplyMeshToHistory {
    fn meta(&self) -> ActionMeta {
        meta(
            "apply_mesh_to_history",
            "Re-decide recent mail under a Mesh's current version",
        )
        .mutating()
        .reversible()
        .risk(Risk::Medium)
        .confirm(ConfirmationPolicy::TwoStep)
        .autonomy(Autonomy::Act)
        .build()
    }

    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let plan = apply_plan(ctx, args)?;
        Ok(Preview {
            summary: format!(
                "Re-decide {} messages from the last {} days under Mesh {} v{}{}{}",
                plan.redecide.len(),
                plan.window_days,
                plan.mesh.name,
                plan.mesh.version,
                match (plan.rescore, plan.rescore_cost_usd) {
                    (0, _) => String::new(),
                    (n, Some(cost)) => format!(
                        "; {n} of them need a fresh judge call for a Signal it reads (about ${cost:.2})"
                    ),
                    (n, None) => format!("; {n} of them need a fresh judge call for a Signal it reads"),
                },
                if plan.kept_yours > 0 {
                    format!(
                        "; {} you decided yourself stay as they are",
                        plan.kept_yours
                    )
                } else {
                    String::new()
                }
            ),
            real_arguments: json!({
                "meshId": plan.mesh.id,
                "meshName": plan.mesh.name,
                "version": plan.mesh.version,
                "windowDays": plan.window_days,
                "count": plan.redecide.len(),
                "keptYours": plan.kept_yours,
                "transitions": plan.transitions,
                "rescore": plan.rescore,
                "rescoreCostUsd": plan.rescore_cost_usd,
            }),
        })
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let plan = apply_plan(ctx, &args)?;
        let sent = send_back_to_sift(&ctx.services, plan.redecide.iter().cloned())?;
        rinfo!(
            Category::Sift,
            "apply_mesh_to_history {} v{}: {} returned to Sift",
            plan.mesh.id,
            plan.mesh.version,
            sent.restored.len()
        );
        Ok(sent
            .result(
                plan.kept_yours,
                &format!(" under Mesh {} v{}", plan.mesh.name, plan.mesh.version),
            )
            .with_inputs(json!({
                "meshId": plan.mesh.id,
                "version": plan.mesh.version,
                "windowDays": plan.window_days,
                "count": sent.restored.len(),
            })))
    }

    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        put_back_result(ctx, &restore)
    }
}

/// What "send back to Sift" retired, for the Activity and for undo.
pub(crate) struct SentBack {
    /// `{ digId, messageId, prior }` per message, the prior classification whole.
    pub(crate) restored: Vec<Value>,
    /// Open Finds closed as Superseded because their message was a Find.
    finds: Vec<FindId>,
}

impl SentBack {
    fn undo(&self) -> UndoInfo {
        UndoInfo::Undoable {
            restore: json!({ "messages": self.restored, "finds": self.finds }),
            label: "Put the previous decisions back".into(),
        }
    }

    /// The Activity of a send-back: the counts, the sentence ("Sent 3
    /// messages back to Sift under Mesh X v2; left 1 you decided yourself")
    /// and the undo. `under` names the rule, or is empty.
    fn result(&self, kept_yours: u64, under: &str) -> ActionResult {
        let n = self.restored.len();
        let mut r = ActionResult::ok(
            json!({ "count": n, "keptYours": kept_yours, "findsSuperseded": self.finds.len() }),
            format!(
                "Sent {} back to Sift{under}{}",
                plural(n, "message"),
                if kept_yours > 0 {
                    format!("; left {kept_yours} you decided yourself")
                } else {
                    String::new()
                }
            ),
        );
        r.undo = self.undo();
        r
    }
}

/// The undo of a send-back, for both Actions that do one.
fn put_back_result(ctx: &ActionContext, restore: &Value) -> Result<ActionResult> {
    let n = put_back(ctx, restore)?;
    Ok(ActionResult::ok(
        json!({ "count": n }),
        format!(
            "Put back the previous decision for {}",
            plural(n, "message")
        ),
    ))
}

/// Retire the current decision on each message so the next Sift pass decides
/// it again under the current rules, close the open Finds of a message that
/// was a Find (a fresh one appears if it still is), and queue that pass. The
/// one mechanism behind applying a Mesh to history and deciding one message
/// again; a decision the user made is never retired. The queued Sift job is
/// the whole mechanism: no refresh trigger, which would sync the mailbox
/// this needs nothing from. Sync calls it too, when Gmail's spam verdict on
/// a message changes (`App::sync_pass`), so it takes the services rather
/// than an Action's context.
pub(crate) fn send_back_to_sift(
    services: &Services,
    items: impl Iterator<Item = (DigId, MessageId)>,
) -> Result<SentBack> {
    let finds = services.get::<riddle_finds::service::FindService>();
    let index = services.stores.index();
    let mut sent = SentBack {
        restored: Vec::new(),
        finds: Vec::new(),
    };
    let mut digs_touched: BTreeSet<DigId> = BTreeSet::new();
    for (dig_id, message_id) in items {
        let store = services.stores.dig(&dig_id)?;
        if repo::classifications::current(&store, &message_id)?
            .is_some_and(|k| k.mechanism == DecisionMechanism::User)
        {
            continue;
        }
        let Some(prior) = repo::classifications::supersede_current(&store, &message_id)? else {
            continue;
        };
        if prior.disposition.kind() == DispositionKind::Find {
            if let Some(svc) = &finds {
                for f in repo::finds::for_message(&index, &message_id)? {
                    if matches!(f.status, FindStatus::Open | FindStatus::Snoozed)
                        && svc.close_superseded(&f.id).is_ok()
                    {
                        sent.finds.push(f.id.clone());
                    }
                }
            }
        }
        sent.restored
            .push(json!({ "digId": dig_id, "messageId": message_id, "prior": prior }));
        digs_touched.insert(dig_id);
    }
    let ops = services.get::<crate::ops::Ops>();
    for d in &digs_touched {
        if let Some(ops) = &ops {
            let _ = ops.queue.enqueue(
                riddle_jobs::JobSpec::new(JobKind::Sift, "Decide again".to_string()).dig(d.clone()),
            );
        }
        services.emit(RiddleEvent::MessagesChanged {
            dig_id: d.clone(),
            count: 0,
        });
    }
    services.emit(RiddleEvent::SummaryChanged);
    Ok(sent)
}

/// One entry of a send-back's restore data.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PutBackEntry {
    dig_id: DigId,
    message_id: MessageId,
    prior: Classification,
}

/// The undo of `send_back_to_sift`: the prior decisions return as fresh
/// rows, projected again, and the Finds closed for them reopen.
fn put_back(ctx: &ActionContext, restore: &Value) -> Result<usize> {
    let hook = ctx.services.get::<ProjectionHookHandle>();
    let entries: Vec<PutBackEntry> = serde_json::from_value(restore["messages"].clone())?;
    let mut put_back = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut prior = entry.prior;
        let store = ctx.services.stores.dig(&entry.dig_id)?;
        repo::classifications::supersede_current(&store, &entry.message_id)?;
        if let Some(h) = &hook {
            let _ = h.0.unproject(&entry.dig_id, &entry.message_id);
        }
        prior.id = ClassificationId::new();
        prior.superseded_at = None;
        repo::classifications::record(&store, &prior)?;
        if let Some(h) = &hook {
            let _ = h.0.project(&entry.dig_id, &prior);
        }
        put_back.push(entry.message_id);
    }
    if let Some(svc) = ctx.services.get::<riddle_finds::service::FindService>() {
        let reopen: Vec<FindId> = restore["finds"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| serde_json::from_value::<FindId>(id).ok())
            .collect();
        // A Find the re-decision raised gives way to the one it replaced.
        let index = ctx.services.stores.index();
        for message_id in &put_back {
            for f in repo::finds::for_message(&index, message_id)? {
                if matches!(f.status, FindStatus::Open | FindStatus::Snoozed)
                    && !reopen.contains(&f.id)
                {
                    let _ = svc.close_superseded(&f.id);
                }
            }
        }
        for id in &reopen {
            let _ = svc.reopen(id);
        }
    }
    ctx.services.emit(RiddleEvent::SummaryChanged);
    Ok(put_back.len())
}

/// Decide one or a few messages again under the current rules: the button
/// in a Find's Why drawer, for checking a new Mesh against the very mail
/// that prompted it without re-deciding a whole window of history.
pub struct DecideAgain;

#[async_trait]
impl Action for DecideAgain {
    fn meta(&self) -> ActionMeta {
        meta(
            "decide_again",
            "Decide messages again under the current rules",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Act)
        .build()
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let dig_id: DigId =
            serde_json::from_value(args.get("digId").cloned().unwrap_or(Value::Null))
                .map_err(|_| RiddleError::invalid("digId is required"))?;
        if dig_id.as_str().is_empty() {
            return Err(RiddleError::invalid("digId is required"));
        }
        let ids: Vec<MessageId> =
            serde_json::from_value(args.get("messageIds").cloned().unwrap_or(json!([])))?;
        if ids.is_empty() {
            return Err(RiddleError::invalid("messageIds is required"));
        }
        let asked = ids.len();
        let sent = send_back_to_sift(
            &ctx.services,
            ids.iter().map(|m| (dig_id.clone(), m.clone())),
        )?;
        let n = sent.restored.len();
        rinfo!(
            Category::Sift,
            "decide_again on {}: {n} returned to Sift",
            dig_id
        );
        let affected = sent
            .restored
            .iter()
            .filter_map(|e| {
                e["messageId"]
                    .as_str()
                    .map(|m| EntityRef::new(EntityKind::Message, m))
            })
            .collect();
        Ok(sent
            .result((asked - n) as u64, "")
            .with_affected(affected)
            .with_inputs(json!({ "digId": dig_id, "count": n })))
    }

    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        put_back_result(ctx, &restore)
    }
}

/// Import the OAuth client JSON Google Auth Platform downloads: the client
/// id goes to settings and the secret to the keychain, the same places the
/// two fields on the Accounts tab write. A file without a secret removes any
/// stored one, because a secret left over from another client would break
/// the token exchange. `user_only` for the same reason as `set_secret`: the
/// file text carries the secret.
pub struct ImportGmailClient;

#[async_trait]
impl Action for ImportGmailClient {
    fn meta(&self) -> ActionMeta {
        meta(
            "import_gmail_client",
            "Import the Gmail OAuth client file from Google",
        )
        .mutating()
        .user_only()
        .risk(Risk::Medium)
        .autonomy(Autonomy::Trusted)
        .build()
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RiddleError::invalid("choose the JSON file Google downloaded"))?;
        let file = riddle_gmail::parse_client_file(text)?;
        let credentials = ctx.services.require::<riddle_credentials::Credentials>()?;
        match &file.client_secret {
            Some(secret) => credentials.set(riddle_credentials::GMAIL_CLIENT_SECRET, secret)?,
            None => credentials.delete(riddle_credentials::GMAIL_CLIENT_SECRET)?,
        }
        let settings = ctx.services.require::<riddle_settings::SettingsStore>()?;
        settings.update(|s| s.gmail.client_id = file.client_id.clone())?;
        secrets_changed(ctx);
        let with_secret = file.client_secret.is_some();
        let mut r = ActionResult::ok(
            json!({
                "clientId": file.client_id,
                "projectId": file.project_id,
                "clientSecretStored": with_secret,
                "keychainService": credentials.service(),
            }),
            if with_secret {
                "You imported the Gmail client: id in settings, secret in the keychain"
            } else {
                "You imported the Gmail client id"
            },
        )
        .with_inputs(json!({ "clientId": file.client_id, "clientSecret": with_secret }));
        r.undo = UndoInfo::NotUndoable {
            reason: "the previous client is not kept; import or enter another".into(),
        };
        Ok(r)
    }
}

/// The secrets a person may store from Settings, by keychain key.
const STORABLE_SECRETS: &[(&str, &str)] = &[
    (riddle_credentials::JEV_API_KEY, "the Jev API key"),
    (
        riddle_credentials::GMAIL_CLIENT_SECRET,
        "the Gmail client secret",
    ),
];

fn storable(args: &Value) -> Result<(&'static str, &'static str)> {
    let name = args.get("secret").and_then(|v| v.as_str()).unwrap_or("");
    STORABLE_SECRETS
        .iter()
        .find(|(k, _)| *k == name)
        .copied()
        .ok_or_else(|| RiddleError::invalid(format!("{name:?} is not a secret Settings can store")))
}

/// Store a key in the OS keychain (§57). Before this the app reported
/// whether a key was present but offered no way to put one there. The value
/// arrives in the arguments, so the Action is `user_only`: an agent is
/// refused before a pending confirmation could write it to the store. The
/// Activity row names the secret, never the value.
pub struct SetSecret;

#[async_trait]
impl Action for SetSecret {
    fn meta(&self) -> ActionMeta {
        meta("set_secret", "Store a key in the OS keychain")
            .mutating()
            .user_only()
            .risk(Risk::Medium)
            .autonomy(Autonomy::Trusted)
            .build()
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (key, label) = storable(&args)?;
        let value = args
            .get("value")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| RiddleError::invalid("paste the key first"))?;
        let credentials = ctx.services.require::<riddle_credentials::Credentials>()?;
        credentials.set(key, value)?;
        secrets_changed(ctx);
        let mut r = ActionResult::ok(
            json!({ "secret": key }),
            format!("You stored {label} in the keychain"),
        )
        .with_inputs(json!({ "secret": key }));
        r.undo = UndoInfo::NotUndoable {
            reason: "the previous value is not kept; clear the key or store another".into(),
        };
        Ok(r)
    }
}

/// Remove a stored key from the OS keychain.
pub struct ClearSecret;

#[async_trait]
impl Action for ClearSecret {
    fn meta(&self) -> ActionMeta {
        meta("clear_secret", "Remove a key from the OS keychain")
            .mutating()
            .user_only()
            .risk(Risk::Medium)
            .autonomy(Autonomy::Trusted)
            .build()
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let (key, label) = storable(&args)?;
        ctx.services
            .require::<riddle_credentials::Credentials>()?
            .delete(key)?;
        secrets_changed(ctx);
        let mut r = ActionResult::ok(
            json!({ "secret": key }),
            format!("You removed {label} from the keychain"),
        )
        .with_inputs(json!({ "secret": key }));
        r.undo = UndoInfo::NotUndoable {
            reason: "the value is gone; store it again to restore it".into(),
        };
        Ok(r)
    }
}

/// Tell the settings watcher to rebuild the judge with the new key, and the
/// webview to refetch presence.
fn secrets_changed(ctx: &ActionContext) {
    if let Some(s) = ctx.services.get::<crate::live::SecretsChanged>() {
        s.0.notify_one();
    }
    ctx.services.emit(RiddleEvent::SettingsChanged);
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
                importance: Importance::default_for(&Disposition::Unresolved),
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
                series: None,
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

/// Messages a newer one of their `+ Replaces` series superseded (a store's
/// earlier sales once the next arrives). Never invoked by a client; the
/// engine records it so the Activity row has an Action to undo through.
/// Undo keeps the messages: they stand again and leave their series, as an
/// undone Find replacement keeps its Find.
pub struct SupersedeMessages;

#[async_trait]
impl Action for SupersedeMessages {
    fn meta(&self) -> ActionMeta {
        meta("supersede_messages", "Supersede mail a newer one replaces")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn execute(&self, _ctx: &ActionContext, _args: Value) -> Result<ActionResult> {
        Err(RiddleError::policy_denied(
            "supersede_messages is recorded by the engine, not invoked",
        ))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        use riddle_mail::actions::util::{arg, message_refs};
        let dig_id: DigId = arg(&restore, "digId")?;
        let ids: Vec<MessageId> = arg(&restore, "messageIds")?;
        let store = ctx.services.stores.dig(&dig_id)?;
        let n = repo::message_series::keep(&store, &ids, &ctx.services.clock.now())?;
        ctx.services.emit(RiddleEvent::MessagesChanged {
            dig_id: dig_id.clone(),
            count: n as u64,
        });
        Ok(ActionResult::ok(
            json!({ "kept": n }),
            format!("Kept {}", plural(n, "message")),
        )
        .with_affected(message_refs(&ids)))
    }
}

/// Connect a Gmail account (§16), or sign in again to one that exists.
/// Returns as soon as the consent URL exists so no IPC call is left hanging
/// while the user is in the browser; the rest — the loopback callback, the
/// token exchange, reading the address, creating or updating the Dig and
/// starting its sync — runs on a task and reports through events. The client
/// id comes from settings; it is not a secret (desktop clients are public and
/// use PKCE), which is why only an id is asked for.
///
/// With `digId` the flow signs an existing Dig in again: the refresh token is
/// replaced under the same key, the Dig keeps its name, settings, corrections
/// and history, and a catch-up runs instead of a first sync. Before this, a
/// refresh token Google had expired (an External client in Testing status
/// does that a week after consent) could only be recovered by removing the
/// Dig, which wiped everything Riddle had learned about that mailbox.
pub struct ConnectGmail;

#[async_trait]
impl Action for ConnectGmail {
    fn meta(&self) -> ActionMeta {
        // Not `.external()`: signing in changes nothing in the mailbox, and
        // syncing reads it on Hold anyway. Gated with the mailbox writes, an
        // expired sign-in on a dev build could not be renewed from its own
        // "Sign in again" banner.
        meta("connect_gmail", "Connect a Gmail account, or sign in again")
            .mutating()
            .risk(Risk::High)
            .autonomy(Autonomy::Trusted)
            .build()
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let settings = ctx
            .services
            .require::<riddle_settings::SettingsStore>()?
            .settings();
        let client_id = settings.gmail.client_id.trim().to_string();
        if client_id.is_empty() {
            return Err(RiddleError::invalid(
                "no Gmail OAuth client id yet — see docs/GMAIL_SETUP.md, then set it in Settings → Accounts → Google sign-in",
            ));
        }
        let existing: Option<Dig> = match args.get("digId").and_then(|v| v.as_str()) {
            Some(id) => {
                let dig = repo::digs::get(&ctx.services.stores.index(), &DigId::from(id))?
                    .ok_or_else(|| RiddleError::not_found(format!("dig {id}")))?;
                if dig.provider != ProviderKind::Gmail {
                    return Err(RiddleError::invalid(
                        "only a Gmail account can sign in again; a test mailbox has no sign-in",
                    ));
                }
                Some(dig)
            }
            None => None,
        };
        let credentials = ctx.services.require::<riddle_credentials::Credentials>()?;
        let config = riddle_gmail::GmailOAuthConfig::new(client_id, None);
        // Google pre-selects the account for a hint, which is what a user
        // signing in again wants: the same address, not a chooser.
        let login_hint: Option<String> = args
            .get("loginHint")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| existing.as_ref().map(|d| d.email.clone()))
            .filter(|s| !s.is_empty());
        let flow = riddle_gmail::AuthFlow::begin(config, login_hint.as_deref()).await?;
        let auth_url = flow.auth_url().to_string();

        let target = match existing {
            Some(dig) => ConnectTarget::Existing(dig),
            None => ConnectTarget::New {
                id: DigId::new(),
                name: args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Gmail")
                    .to_string(),
                group: args
                    .get("group")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                privacy: args
                    .get("privacy")
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or(settings.privacy_default),
                projection: args
                    .get("projection")
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or(settings.projection_default),
            },
        };
        let dig_id = target.id().clone();
        let reconnect = matches!(target, ConnectTarget::Existing(_));

        let services = ctx.services.clone();
        let id = dig_id.clone();
        tokio::spawn(async move {
            let http = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default();
            let outcome = finish_gmail_connect(&services, flow, &http, &credentials, target).await;
            match outcome {
                Ok(email) => {
                    rinfo!(
                        Category::Credentials,
                        "gmail dig {} {}",
                        id,
                        if reconnect {
                            "signed in again"
                        } else {
                            "connected"
                        }
                    );
                    // Signed in again: the expired sign-in is history, so Sync
                    // now and the banner stop saying so before the next sync.
                    if let Ok(store) = services.stores.dig(&id) {
                        for key in [
                            repo::sync_state::SYNC_FAILING_SINCE,
                            repo::sync_state::SYNC_FAILURE,
                        ] {
                            if let Err(e) = repo::sync_state::clear(&store, key) {
                                rwarn!(
                                    Category::Sync,
                                    "sync failure of {} not cleared: {}",
                                    id,
                                    e.kind.as_str()
                                );
                            }
                        }
                    }
                    services.emit(RiddleEvent::Notify {
                        title: "Gmail connected".into(),
                        body: if reconnect {
                            format!("{email} is signed in again and catching up.")
                        } else {
                            format!("{email} is syncing now.")
                        },
                        find_id: None,
                    });
                    services.emit(RiddleEvent::GmailSignIn {
                        dig_id: id.clone(),
                        connected: true,
                        message: None,
                    });
                    services.emit(RiddleEvent::SummaryChanged);
                }
                Err(e) => {
                    services.emit(RiddleEvent::GmailSignIn {
                        dig_id: id.clone(),
                        connected: false,
                        message: Some(e.message.clone()),
                    });
                    rwarn!(
                        Category::Credentials,
                        "gmail sign-in failed for {}: {}",
                        id,
                        e.kind.as_str()
                    );
                    if !reconnect {
                        // Leave nothing half-created behind. An existing Dig
                        // keeps its history; it is simply still signed out.
                        let index = services.stores.index();
                        let _ = repo::digs::remove(&index, &id);
                        let _ =
                            credentials.delete(&riddle_credentials::gmail_refresh_token_key(&id));
                    }
                    services.emit(RiddleEvent::Notify {
                        title: "Gmail sign-in failed".into(),
                        body: e.message.clone(),
                        find_id: None,
                    });
                }
            }
        });

        let mut r = ActionResult::ok(
            json!({ "digId": dig_id, "authUrl": auth_url, "reconnect": reconnect }),
            if reconnect {
                "Started signing in to Gmail again"
            } else {
                "Started a Gmail sign-in"
            },
        )
        .with_inputs(json!({ "digId": dig_id, "reconnect": reconnect }));
        r.undo = UndoInfo::NotUndoable {
            reason: if reconnect {
                "the sign-in completes in the browser; nothing changes until Google returns".into()
            } else {
                "nothing is connected until Google returns; remove the Dig in Settings to disconnect"
                    .into()
            },
        };
        Ok(r)
    }
}

/// What a Gmail sign-in is for: a Dig to create, or one to sign in again.
enum ConnectTarget {
    New {
        id: DigId,
        name: String,
        group: Option<String>,
        privacy: PrivacyMode,
        projection: ProjectionMode,
    },
    Existing(Dig),
}

impl ConnectTarget {
    fn id(&self) -> &DigId {
        match self {
            ConnectTarget::New { id, .. } => id,
            ConnectTarget::Existing(dig) => &dig.id,
        }
    }
}

async fn finish_gmail_connect(
    services: &std::sync::Arc<riddle_actions::Services>,
    flow: riddle_gmail::AuthFlow,
    http: &reqwest::Client,
    credentials: &std::sync::Arc<riddle_credentials::Credentials>,
    target: ConnectTarget,
) -> Result<String> {
    let dig_id = target.id().clone();
    let domains = services.require::<crate::domains::Domains>()?;
    let reconnect = matches!(target, ConnectTarget::Existing(_));
    if let ConnectTarget::Existing(dig) = &target {
        // The new token lands under a holding key and is checked against the
        // account's address before it replaces anything: a sign-in with the
        // wrong Google account must leave a working token alone.
        let holding = DigId::from(format!("{}~signing-in", dig_id.as_str()));
        flow.complete(http, credentials, &holding).await?;
        let probe = Dig {
            id: holding.clone(),
            ..dig.clone()
        };
        let profile = match domains.providers.provider_for(&probe) {
            Ok(p) => p.profile().await,
            Err(e) => Err(e),
        };
        domains.providers.remove(&holding);
        let holding_key = riddle_credentials::gmail_refresh_token_key(&holding);
        let profile = match profile {
            Ok(p) => p,
            Err(e) => {
                let _ = credentials.delete(&holding_key);
                return Err(e);
            }
        };
        if !dig.email.is_empty() && !dig.email.eq_ignore_ascii_case(&profile.email) {
            let _ = credentials.delete(&holding_key);
            return Err(RiddleError::invalid(
                "Google signed you in with a different address than this account uses. Sign in with the same address, or add it as a new account.",
            ));
        }
        let token = credentials
            .get(&holding_key)?
            .ok_or_else(|| RiddleError::internal("the sign-in left no token to keep"))?;
        credentials.set(
            &riddle_credentials::gmail_refresh_token_key(&dig_id),
            &token,
        )?;
        let _ = credentials.delete(&holding_key);
    } else {
        flow.complete(http, credentials, &dig_id).await?;
    }
    let mut dig = match target {
        ConnectTarget::New {
            id,
            name,
            group,
            privacy,
            projection,
        } => Dig {
            id,
            name,
            email: String::new(),
            provider: ProviderKind::Gmail,
            group,
            privacy,
            projection,
            status: DigStatus::NotConnected,
            shadow_started_at: None,
            created_at: chrono::Utc::now(),
            disabled: false,
            mailbox: None,
        },
        ConnectTarget::Existing(dig) => dig,
    };
    if reconnect {
        // The registry caches one client per Dig; the cached one holds the
        // credentials that just expired.
        domains.providers.remove(&dig_id);
    }
    let provider = domains.providers.provider_for(&dig)?;
    // The address comes from Google, not from the user typing it.
    let profile = provider.profile().await?;
    dig.email = profile.email.clone();
    if reconnect {
        dig.status = DigStatus::CatchingUp {
            remaining: 0,
            total: 0,
        };
    }
    repo::digs::upsert(&services.stores.index(), &dig)?;
    services.emit(RiddleEvent::DigStatusChanged {
        dig_id: dig_id.clone(),
        status: DigStatus::CatchingUp {
            remaining: 0,
            total: 0,
        },
    });
    if let Some(ops) = services.get::<crate::ops::Ops>() {
        let spec = if reconnect {
            riddle_jobs::JobSpec::new(JobKind::CatchUp, format!("Catch-up of {}", profile.email))
        } else {
            riddle_jobs::JobSpec::new(
                JobKind::InitialSync,
                format!("First sync of {}", profile.email),
            )
        };
        let _ = ops.queue.enqueue(spec.dig(dig_id.clone()));
    }
    Ok(profile.email)
}

/// Decline a Director proposal. Dismissing used to be a client-side gesture,
/// so the PendingAction stayed in the store and the proposal came back on the
/// next launch; declining is an Action now, recorded in Activity with the
/// reason if one was given, so the backend forgets it too.
pub struct DismissPending;

#[async_trait]
impl Action for DismissPending {
    fn meta(&self) -> ActionMeta {
        meta("dismiss_pending", "Decline a pending proposal")
            .mutating()
            .user_only()
            .build()
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = args
            .get("pendingId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| RiddleError::invalid("pendingId is required"))?;
        let index = ctx.services.stores.index();
        let pending = repo::pending::get_pending(&index, id)?.ok_or_else(|| {
            RiddleError::not_found("that proposal was already applied, declined or expired")
        })?;
        repo::pending::remove_pending(&index, id)?;
        rinfo!(Category::Actions, "declined pending action {}", id);
        let mut r = ActionResult::ok(
            json!({ "pendingId": id }),
            format!("Declined: {}", pending.summary),
        )
        .with_inputs(json!({ "pendingId": id, "action": pending.action }));
        r.why = args
            .get("reason")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        r.undo = UndoInfo::NotUndoable {
            reason: "the Director can propose it again".into(),
        };
        Ok(r)
    }
}

/// Sift every message Riddle decided again. A decision's reason is written
/// when the decision is made, so a change to the rules, or to how a reason
/// is written, reaches old mail only through a new decision. The user's own
/// decisions are kept; the earlier ones are superseded, not deleted, so the
/// inspector still shows them.
///
/// This takes the bulk SQL path (`supersede_sift_decisions`), not
/// `send_back_to_sift`: it retires thousands of rows per account, has no
/// undo, and keeps no per-message restore data, so reading, checking and
/// recording each decision one at a time would buy nothing.
pub struct Resift;

#[async_trait]
impl Action for Resift {
    fn meta(&self) -> ActionMeta {
        meta(
            "resift",
            "Sift every message Riddle decided again, with the current rules",
        )
        .mutating()
        .user_only()
        .build()
    }

    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let index = ctx.services.stores.index();
        let only = args
            .get("digId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(DigId::from);
        let digs: Vec<Dig> = repo::digs::list_enabled(&index)?
            .into_iter()
            .filter(|d| only.as_ref().is_none_or(|id| &d.id == id))
            .collect();
        if digs.is_empty() {
            return Err(RiddleError::not_found("no account to sift"));
        }
        let ops = ctx.services.get::<crate::ops::Ops>();
        let mut retired = 0usize;
        let mut affected = Vec::new();
        for dig in &digs {
            let store = ctx.services.stores.dig(&dig.id)?;
            let n = repo::classifications::supersede_sift_decisions(&store)?;
            retired += n;
            if let Some(ops) = &ops {
                let _ = ops.queue.enqueue(
                    riddle_jobs::JobSpec::new(JobKind::Sift, format!("Sift {} again", dig.name))
                        .dig(dig.id.clone()),
                );
            }
            rinfo!(
                Category::Sift,
                "resift of {}: {} decisions retired",
                dig.id,
                n
            );
            affected.push(EntityRef::new(EntityKind::Dig, &dig.id));
        }
        let mut r = ActionResult::ok(
            json!({ "digs": digs.len(), "messages": retired }),
            format!(
                "Sifting {retired} messages again across {}",
                plural(digs.len(), "account")
            ),
        )
        .with_affected(affected)
        .with_inputs(json!({ "digId": only, "digs": digs.len(), "messages": retired }));
        r.undo = UndoInfo::NotUndoable {
            reason: "the earlier decisions stay as history; correct any message from its row"
                .into(),
        };
        Ok(r)
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
