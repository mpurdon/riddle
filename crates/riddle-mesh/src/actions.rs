//! Mesh and Strata Actions (§8, §51). Creating or updating a Mesh is egress
//! (§58 control 3): durable policy that a Director task which has read mail
//! must get approval for. `update_mesh` returns the simulation of the new
//! version so the UI can preview before activating; the version itself is
//! Shadow until the user says otherwise.

use crate::service::MeshService;
use crate::simulate::{simulate, SimulationInput};
use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn all() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(CreateMesh),
        Arc::new(UpdateMesh),
        Arc::new(SetMeshStatus),
        Arc::new(SetMeshFallback),
        Arc::new(SaveList),
        Arc::new(DeleteList),
        Arc::new(DeleteMesh),
        Arc::new(RestoreMesh),
        Arc::new(RenameStratum),
        Arc::new(MergeStrata),
        Arc::new(CreateStratum),
    ]
}

/// Who a Mesh version is recorded as created by, from the principal of the Action.
pub fn created_by(p: &Principal) -> CreatedBy {
    match p {
        Principal::User | Principal::Cli => CreatedBy::User,
        Principal::Director { .. } | Principal::Expedition { .. } => CreatedBy::Director,
        _ => CreatedBy::System,
    }
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| RiddleError::invalid(format!("missing argument {key}")))
}

fn mesh_ref(m: &Mesh) -> EntityRef {
    EntityRef::new(EntityKind::Mesh, &m.id)
}

fn parse_status(v: Option<&Value>) -> Result<Option<MeshStatus>> {
    match v.and_then(|s| s.as_str()) {
        None => Ok(None),
        Some(s) => Ok(Some(match s.to_ascii_lowercase().as_str() {
            "draft" => MeshStatus::Draft,
            "shadow" => MeshStatus::Shadow,
            "active" => MeshStatus::Active,
            "disabled" => MeshStatus::Disabled,
            other => return Err(RiddleError::invalid(format!("unknown status {other}"))),
        })),
    }
}

fn digs(ctx: &ActionContext) -> Result<Vec<Dig>> {
    repo::digs::list_enabled(&ctx.services.stores.index())
}

pub struct CreateMesh;

#[async_trait]
impl Action for CreateMesh {
    fn meta(&self) -> ActionMeta {
        meta("create_mesh", "Create a Mesh from its text")
            .mutating()
            .reversible()
            .egress()
            .risk(Risk::Medium)
            .autonomy(Autonomy::Suggest)
            .build()
    }
    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let text = arg_str(args, "text")?;
        let (m, _) = crate::parse::parse_full(text)?;
        Ok(Preview {
            summary: format!(
                "Create Mesh {:?} with {} routes and {} guards",
                m.name,
                m.routes.len(),
                m.guards.len()
            ),
            real_arguments: json!({ "name": m.name, "text": crate::format::format(&m), "scope": m.scope }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let text = arg_str(&args, "text")?;
        let scope: Option<MeshScope> = args
            .get("scope")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|e| RiddleError::invalid(format!("scope: {e}")))?;
        let status = parse_status(args.get("status"))?.unwrap_or(MeshStatus::Draft);
        let svc = MeshService::new(ctx.services.stores.index());
        let m = svc.create(text, scope, status, created_by(&ctx.principal))?;
        Ok(ActionResult::ok(
            json!({ "mesh": m, "simulation": Value::Null, "report": Value::Null }),
            format!("Created Mesh {} ({:?})", m.name, m.status),
        )
        .with_affected(vec![mesh_ref(&m)])
        .with_inputs(json!({ "meshId": m.id, "status": m.status }))
        .with_undo(json!({ "meshId": m.id }), "Delete the Mesh"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&restore, "meshId")?.to_string());
        let m = MeshService::new(ctx.services.stores.index()).delete(&id)?;
        Ok(
            ActionResult::ok(Value::Null, format!("Deleted Mesh {}", m.name))
                .with_affected(vec![mesh_ref(&m)]),
        )
    }
}

pub struct UpdateMesh;

#[async_trait]
impl Action for UpdateMesh {
    fn meta(&self) -> ActionMeta {
        meta(
            "update_mesh",
            "Create a new version of a Mesh from its text",
        )
        .mutating()
        .reversible()
        .egress()
        .risk(Risk::Medium)
        .autonomy(Autonomy::Suggest)
        .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let id = MeshId(arg_str(args, "meshId")?.to_string());
        let text = arg_str(args, "text")?;
        let (m, _) = crate::parse::parse_full(text)?;
        let prior = MeshService::new(ctx.services.stores.index()).latest(&id)?;
        Ok(Preview {
            summary: format!(
                "Change Mesh {} from v{} to v{}",
                prior.name,
                prior.version,
                prior.version + 1
            ),
            real_arguments: json!({ "meshId": id, "name": m.name, "text": crate::format::format(&m) }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&args, "meshId")?.to_string());
        let text = arg_str(&args, "text")?;
        let note = args
            .get("changeNote")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let svc = MeshService::new(ctx.services.stores.index());
        let prior = svc.latest(&id)?;
        let m = svc.update(&id, text, note, created_by(&ctx.principal))?;
        let window = riddle_core::window_days(&args, 30);
        let mut input = SimulationInput::new(&ctx.services.stores, digs(ctx)?, &m);
        input.window_days = window;
        input.now = ctx.services.clock.now();
        let simulation = simulate(input).unwrap_or_else(|e| {
            riddle_observability::rwarn!(
                riddle_observability::Category::Mesh,
                "simulation of {} v{} failed: {}",
                m.id,
                m.version,
                e.message
            );
            SimulationReport {
                mesh_id: Some(m.id.clone()),
                mesh_name: m.name.clone(),
                from_version: Some(prior.version),
                to_version: m.version,
                historical_messages: 0,
                unchanged: 0,
                changed: 0,
                transitions: vec![],
                new_judge_calls_required: 0,
                estimated_cost_usd: None,
                sample_changed: vec![],
                window_days: window,
                joins_series: 0,
                recorded_changes: 0,
                taken_first_by: vec![],
                takes_from: vec![],
                rescore: 0,
            }
        });
        Ok(ActionResult::ok(json!({ "mesh": m, "simulation": simulation, "report": simulation }), format!("Changed Mesh {} to v{} ({:?}); {} of {} messages would change", m.name, m.version, m.status, simulation.changed, simulation.historical_messages))
            .with_affected(vec![mesh_ref(&m)])
            .with_inputs(json!({ "meshId": m.id, "version": m.version }))
            .with_undo(json!({ "meshId": m.id, "version": m.version, "priorVersion": prior.version, "priorStatus": prior.status }), format!("Revert to v{}", prior.version)))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&restore, "meshId")?.to_string());
        let version = restore["version"].as_u64().unwrap_or(0) as u32;
        let index = ctx.services.stores.index();
        let mut affected = vec![EntityRef::new(EntityKind::Mesh, &id)];
        let hook = ctx.services.get::<ProjectionHookHandle>();
        for dig in digs(ctx)? {
            let store = ctx.services.stores.dig(&dig.id)?;
            for mid in repo::classifications::supersede_by_mesh(&store, id.as_str(), version)? {
                if let Some(h) = &hook {
                    if let Err(e) = h.0.unproject(&dig.id, &mid) {
                        riddle_observability::rwarn!(
                            riddle_observability::Category::Projection,
                            "unproject failed for {}: {}",
                            mid,
                            e.message
                        );
                    }
                }
                affected.push(EntityRef::new(EntityKind::Message, &mid));
            }
        }
        repo::meshes::remove_version(&index, &id, version)?;
        let prior = MeshService::new(index).latest(&id)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&prior)?,
            format!(
                "Reverted Mesh {} to v{}; {} messages returned to the backlog",
                prior.name,
                prior.version,
                affected.len() - 1
            ),
        )
        .with_affected(affected))
    }
}

pub struct SetMeshStatus;

#[async_trait]
impl Action for SetMeshStatus {
    fn meta(&self) -> ActionMeta {
        meta(
            "set_mesh_status",
            "Set a Mesh to draft, shadow, active or disabled",
        )
        .mutating()
        .reversible()
        .risk(Risk::Medium)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&args, "meshId")?.to_string());
        let status = parse_status(args.get("status"))?
            .ok_or_else(|| RiddleError::invalid("missing argument status"))?;
        let svc = MeshService::new(ctx.services.stores.index());
        let prior = svc.latest(&id)?;
        let m = svc.set_status(&id, status)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&m)?,
            format!("Set Mesh {} to {:?}", m.name, m.status),
        )
        .with_affected(vec![mesh_ref(&m)])
        .with_inputs(json!({ "meshId": m.id, "status": m.status }))
        .with_undo(
            json!({ "meshId": m.id, "priorStatus": prior.status }),
            format!("Back to {:?}", prior.status),
        ))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&restore, "meshId")?.to_string());
        let status: MeshStatus = serde_json::from_value(restore["priorStatus"].clone())
            .map_err(|e| RiddleError::invalid(format!("priorStatus: {e}")))?;
        let m = MeshService::new(ctx.services.stores.index()).set_status(&id, status)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&m)?,
            format!("Restored Mesh {} to {:?}", m.name, m.status),
        )
        .with_affected(vec![mesh_ref(&m)]))
    }
}

/// Mark a Mesh a fallback (a catch-all expected to lose mail to earlier
/// Meshes) or a specific rule. Changes no decision.
pub struct SetMeshFallback;

#[async_trait]
impl Action for SetMeshFallback {
    fn meta(&self) -> ActionMeta {
        meta(
            "set_mesh_fallback",
            "Mark a Mesh a fallback, a catch-all for mail no specific Mesh decides",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&args, "meshId")?.to_string());
        let fallback = args
            .get("fallback")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| RiddleError::invalid("missing argument fallback (true or false)"))?;
        let svc = MeshService::new(ctx.services.stores.index());
        let prior = svc.latest(&id)?.fallback;
        let m = svc.set_fallback(&id, fallback)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&m)?,
            if fallback {
                format!("Mesh {} is a fallback", m.name)
            } else {
                format!("Mesh {} is a specific rule", m.name)
            },
        )
        .with_affected(vec![mesh_ref(&m)])
        .with_inputs(json!({ "meshId": m.id, "fallback": fallback }))
        .with_undo(
            json!({ "meshId": m.id, "fallback": prior }),
            if prior {
                "Make it a fallback again"
            } else {
                "Make it a specific rule again"
            },
        ))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        self.execute(ctx, restore).await
    }
}

/// Create or replace a named list (`sender IN list:banks`). Entries are
/// addresses or domains; they are normalised (lowercase, no leading `@`,
/// no duplicates). Undo puts the previous entries back.
pub struct SaveList;

#[async_trait]
impl Action for SaveList {
    fn meta(&self) -> ActionMeta {
        meta(
            "save_list",
            "Create or replace a named list of senders and domains",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let name = arg_str(&args, "name")?.trim().to_ascii_lowercase();
        if !NamedListDef::valid_name(&name) {
            return Err(RiddleError::invalid(
                "a list name is lowercase letters, digits and _, like banks or keep_promotions",
            ));
        }
        let entries: Vec<String> =
            serde_json::from_value(args.get("entries").cloned().unwrap_or(json!([])))
                .map_err(|e| RiddleError::invalid(format!("entries: {e}")))?;
        let index = ctx.services.stores.index();
        let prior = repo::lists::get(&index, &name)?;
        let list = NamedListDef {
            description: args
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| prior.as_ref().map(|p| p.description.clone()))
                .unwrap_or_default(),
            entries: NamedListDef::normalise(&entries),
            pack: args
                .get("pack")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| prior.as_ref().and_then(|p| p.pack.clone())),
            name: name.clone(),
        };
        repo::lists::put(&index, &list)?;
        ctx.services.emit(RiddleEvent::SettingsChanged);
        Ok(ActionResult::ok(
            serde_json::to_value(&list)?,
            format!("Saved list:{name} ({} entries)", list.entries.len()),
        )
        .with_inputs(json!({ "name": name, "entries": list.entries.len() }))
        .with_undo(json!({ "name": name, "prior": prior }), "Put the list back"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let name = arg_str(&restore, "name")?.to_string();
        let index = ctx.services.stores.index();
        match serde_json::from_value::<Option<NamedListDef>>(restore["prior"].clone())
            .ok()
            .flatten()
        {
            Some(prior) => repo::lists::put(&index, &prior)?,
            None => repo::lists::delete(&index, &name)?,
        }
        Ok(ActionResult::ok(
            json!({ "name": name }),
            format!("Restored list:{name}"),
        ))
    }
}

/// Delete a named list no Mesh uses.
pub struct DeleteList;

#[async_trait]
impl Action for DeleteList {
    fn meta(&self) -> ActionMeta {
        meta("delete_list", "Delete a named list that no Mesh uses")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let name = arg_str(&args, "name")?.trim().to_ascii_lowercase();
        let index = ctx.services.stores.index();
        let prior = repo::lists::get(&index, &name)?
            .ok_or_else(|| RiddleError::not_found(format!("no list:{name}")))?;
        let users: Vec<String> = repo::meshes::list_latest(&index, false)?
            .into_iter()
            .filter(|m| {
                let mut names = Vec::new();
                for c in m
                    .routes
                    .iter()
                    .map(|r| &r.condition)
                    .chain(m.guards.iter().map(|g| &g.condition))
                {
                    crate::service::named_lists_in(c, &mut names);
                }
                names.contains(&name)
            })
            .map(|m| m.name)
            .collect();
        if !users.is_empty() {
            return Err(RiddleError::invalid(format!(
                "list:{name} is used by {}; change those Meshes first",
                users.join(", ")
            )));
        }
        repo::lists::delete(&index, &name)?;
        ctx.services.emit(RiddleEvent::SettingsChanged);
        Ok(
            ActionResult::ok(json!({ "name": name }), format!("Deleted list:{name}"))
                .with_undo(json!({ "list": prior }), "Restore the list"),
        )
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let list: NamedListDef = serde_json::from_value(restore["list"].clone())
            .map_err(|e| RiddleError::invalid(format!("list: {e}")))?;
        repo::lists::put(&ctx.services.stores.index(), &list)?;
        Ok(ActionResult::ok(
            json!({ "name": list.name }),
            format!("Restored list:{}", list.name),
        ))
    }
}

pub struct DeleteMesh;

#[async_trait]
impl Action for DeleteMesh {
    fn meta(&self) -> ActionMeta {
        meta("delete_mesh", "Move a Mesh to Recently Deleted")
            .mutating()
            .reversible()
            .risk(Risk::Medium)
            .confirm(ConfirmationPolicy::InPlaceUndo)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&args, "meshId")?.to_string());
        let m = MeshService::new(ctx.services.stores.index()).delete(&id)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&m)?,
            format!("Deleted Mesh {}", m.name),
        )
        .with_affected(vec![mesh_ref(&m)])
        .with_inputs(json!({ "meshId": m.id }))
        .with_undo(json!({ "meshId": m.id }), "Restore the Mesh"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&restore, "meshId")?.to_string());
        let m = MeshService::new(ctx.services.stores.index()).restore(&id)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&m)?,
            format!("Restored Mesh {}", m.name),
        )
        .with_affected(vec![mesh_ref(&m)]))
    }
}

pub struct RestoreMesh;

#[async_trait]
impl Action for RestoreMesh {
    fn meta(&self) -> ActionMeta {
        meta("restore_mesh", "Restore a Mesh from Recently Deleted")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&args, "meshId")?.to_string());
        let m = MeshService::new(ctx.services.stores.index()).restore(&id)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&m)?,
            format!("Restored Mesh {} as {:?}", m.name, m.status),
        )
        .with_affected(vec![mesh_ref(&m)])
        .with_inputs(json!({ "meshId": m.id }))
        .with_undo(json!({ "meshId": m.id }), "Delete again"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let id = MeshId(arg_str(&restore, "meshId")?.to_string());
        let m = MeshService::new(ctx.services.stores.index()).delete(&id)?;
        Ok(
            ActionResult::ok(Value::Null, format!("Deleted Mesh {}", m.name))
                .with_affected(vec![mesh_ref(&m)]),
        )
    }
}

// ---- Strata ----

fn normalize_name(s: &str) -> String {
    let mut n: String = s
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if n.ends_with('s') && n.len() > 3 {
        n.pop();
    }
    n
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Strata whose names are near duplicates of `name` (§2 "near-duplicate
/// names are flagged before a new Stratum is created").
pub fn near_duplicates(existing: &[Stratum], name: &str) -> Vec<Stratum> {
    let n = normalize_name(name);
    existing
        .iter()
        .filter(|s| {
            let e = normalize_name(&s.name);
            e == n
                || (n.len() >= 4 && edit_distance(&e, &n) <= 1)
                || (n.len() >= 8 && edit_distance(&e, &n) <= 2)
        })
        .cloned()
        .collect()
}

fn relabel_everywhere(ctx: &ActionContext, from: &str, to: &str) -> Result<u64> {
    let mut n = 0;
    for dig in digs(ctx)? {
        let store = ctx.services.stores.dig(&dig.id)?;
        n += repo::classifications::rename_stratum(&store, from, to)?;
    }
    Ok(n)
}

fn stratum(ctx: &ActionContext, id: &str) -> Result<Stratum> {
    repo::strata::get(&ctx.services.stores.index(), &StratumId(id.to_string()))?
        .ok_or_else(|| RiddleError::not_found(format!("stratum {id}")))
}

pub struct RenameStratum;

#[async_trait]
impl Action for RenameStratum {
    fn meta(&self) -> ActionMeta {
        meta("rename_stratum", "Rename a Stratum")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let id = arg_str(&args, "stratumId")?.to_string();
        let name = arg_str(&args, "name")?.trim().to_string();
        if name.is_empty() {
            return Err(RiddleError::invalid("a Stratum needs a name"));
        }
        let index = ctx.services.stores.index();
        let mut s = stratum(ctx, &id)?;
        if let Some(clash) = repo::strata::by_name(&index, &name)? {
            if clash.id != s.id {
                return Err(RiddleError::invalid(format!(
                    "a Stratum named {name:?} already exists"
                )));
            }
        }
        let old = s.name.clone();
        s.name = name.clone();
        s.version += 1;
        repo::strata::upsert(&index, &s)?;
        let relabelled = relabel_everywhere(ctx, &old, &name)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&s)?,
            format!("Renamed Stratum {old} to {name} on {relabelled} messages"),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Stratum, &s.id)])
        .with_inputs(json!({ "stratumId": s.id, "relabelled": relabelled }))
        .with_undo(json!({ "stratumId": s.id, "name": old }), "Rename back"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        self.execute(ctx, restore).await
    }
}

pub struct MergeStrata;

#[async_trait]
impl Action for MergeStrata {
    fn meta(&self) -> ActionMeta {
        meta("merge_strata", "Merge one Stratum into another")
            .mutating()
            .risk(Risk::Medium)
            .confirm(ConfirmationPolicy::InPlaceUndo)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let from = stratum(ctx, arg_str(args, "fromId")?)?;
        let into = stratum(ctx, arg_str(args, "intoId")?)?;
        Ok(Preview {
            summary: format!("Merge Stratum {} into {}", from.name, into.name),
            real_arguments: json!({ "from": from.name, "into": into.name, "fromMessages": from.message_count }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let mut from = stratum(ctx, arg_str(&args, "fromId")?)?;
        let into = stratum(ctx, arg_str(&args, "intoId")?)?;
        if from.id == into.id {
            return Err(RiddleError::invalid(
                "a Stratum cannot be merged into itself",
            ));
        }
        let relabelled = relabel_everywhere(ctx, &from.name, &into.name)?;
        from.deleted_at = Some(ctx.services.clock.now());
        repo::strata::upsert(&ctx.services.stores.index(), &from)?;
        let mut r = ActionResult::ok(
            json!({ "relabelled": relabelled, "into": into }),
            format!(
                "Merged Stratum {} into {} ({relabelled} messages)",
                from.name, into.name
            ),
        )
        .with_affected(vec![
            EntityRef::new(EntityKind::Stratum, &from.id),
            EntityRef::new(EntityKind::Stratum, &into.id),
        ])
        .with_inputs(json!({ "fromId": from.id, "intoId": into.id, "relabelled": relabelled }));
        r.undo = UndoInfo::NotUndoable {
            reason: "merged labels cannot be told apart afterwards".into(),
        };
        Ok(r)
    }
}

pub struct CreateStratum;

#[async_trait]
impl Action for CreateStratum {
    fn meta(&self) -> ActionMeta {
        meta("create_stratum", "Create a Stratum")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Suggest)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let name = arg_str(&args, "name")?.trim().to_string();
        if name.is_empty() {
            return Err(RiddleError::invalid("a Stratum needs a name"));
        }
        let index = ctx.services.stores.index();
        let existing = repo::strata::list(&index, false)?;
        let dupes = near_duplicates(&existing, &name);
        let force =
            args.get("force").and_then(|v| v.as_bool()).unwrap_or(false) && ctx.principal.is_user();
        if !dupes.is_empty() && !force {
            return Err(RiddleError::invalid(format!(
                "{name:?} is close to an existing Stratum: {}",
                dupes
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .with_detail(json!({ "nearDuplicates": dupes })));
        }
        let s = Stratum {
            id: StratumId::new(),
            name: name.clone(),
            version: 1,
            scope_group: args
                .get("scopeGroup")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            description: args
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            proposed_by_riddle: !ctx.principal.is_user(),
            message_count: 0,
            created_at: ctx.services.clock.now(),
            deleted_at: None,
        };
        repo::strata::upsert(&index, &s)?;
        Ok(
            ActionResult::ok(serde_json::to_value(&s)?, format!("Created Stratum {name}"))
                .with_affected(vec![EntityRef::new(EntityKind::Stratum, &s.id)])
                .with_inputs(json!({ "stratumId": s.id }))
                .with_undo(json!({ "stratumId": s.id }), "Delete the Stratum"),
        )
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let mut s = stratum(ctx, arg_str(&restore, "stratumId")?)?;
        s.deleted_at = Some(ctx.services.clock.now());
        repo::strata::upsert(&ctx.services.stores.index(), &s)?;
        Ok(
            ActionResult::ok(Value::Null, format!("Deleted Stratum {}", s.name))
                .with_affected(vec![EntityRef::new(EntityKind::Stratum, &s.id)]),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_duplicate_strata_names_are_flagged() {
        let mk = |n: &str| Stratum {
            id: StratumId::new(),
            name: n.into(),
            version: 1,
            scope_group: None,
            description: String::new(),
            proposed_by_riddle: false,
            message_count: 0,
            created_at: chrono::Utc::now(),
            deleted_at: None,
        };
        let existing = vec![mk("Receipts"), mk("Newsletters"), mk("People")];
        assert_eq!(near_duplicates(&existing, "receipt").len(), 1);
        assert_eq!(near_duplicates(&existing, "News letters").len(), 1);
        assert_eq!(near_duplicates(&existing, "Projects").len(), 0);
    }
}
