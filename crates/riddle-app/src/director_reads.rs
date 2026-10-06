//! `DirectorReads` over the shared read services. Contamination never
//! reaches the Director unless the user opened it (§58 control 2): rows are
//! scrubbed from lists and refused on direct reads.

use crate::{dispatch, App};
use async_trait::async_trait;
use riddle_agent::reads::{DirectorReads, ReadScope};
use riddle_core::{DigId, DispositionKind, MessageId, ThreadId};
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct AppReads(pub Arc<App>);

impl AppReads {
    fn is_contaminated(&self, dig: &DigId, id: &MessageId) -> Result<bool> {
        let store = self.0.stores.dig(dig)?;
        Ok(repo::classifications::current(&store, id)?
            .map(|k| k.disposition.kind() == DispositionKind::Contamination)
            .unwrap_or(false))
    }

    fn scrub_page(&self, scope: &ReadScope, mut page: Value) -> Value {
        if scope.allow_contamination {
            return page;
        }
        if let Some(items) = page.get_mut("items").and_then(|i| i.as_array_mut()) {
            items.retain(|m| {
                m.get("disposition")
                    .and_then(|d| d.get("kind"))
                    .and_then(|k| k.as_str())
                    != Some("contamination")
            });
        }
        page
    }

    fn with_scope(scope: &ReadScope, mut args: Value) -> Value {
        if args.get("digId").is_none() && scope.dig_ids.len() == 1 {
            args["digId"] = json!(scope.dig_ids[0]);
        }
        args
    }
}

#[async_trait]
impl DirectorReads for AppReads {
    async fn list_messages(&self, scope: &ReadScope, args: Value) -> Result<Value> {
        let page = dispatch::call(&self.0, "list_messages", Self::with_scope(scope, args)).await?;
        Ok(self.scrub_page(scope, page))
    }
    async fn get_message(
        &self,
        scope: &ReadScope,
        dig_id: &DigId,
        message_id: &MessageId,
    ) -> Result<Value> {
        if !scope.allow_contamination && self.is_contaminated(dig_id, message_id)? {
            return Err(RiddleError::policy_denied(
                "that message is Contamination; open it yourself first",
            ));
        }
        let mut v = serde_json::to_value(self.0.get_message(dig_id, message_id)?)?;
        // The Director reads text; HTML is for the sandboxed viewer only.
        if let Some(o) = v.as_object_mut() {
            o.remove("bodyHtml");
        }
        Ok(v)
    }
    async fn get_thread(
        &self,
        scope: &ReadScope,
        dig_id: &DigId,
        thread_id: &ThreadId,
    ) -> Result<Value> {
        let thread = self.0.get_thread(dig_id, thread_id)?;
        if !scope.allow_contamination {
            for m in &thread.messages {
                if self.is_contaminated(dig_id, &m.id)? {
                    return Err(RiddleError::policy_denied(
                        "that thread contains Contamination; open it yourself first",
                    ));
                }
            }
        }
        let mut v = serde_json::to_value(thread)?;
        if let Some(ms) = v.get_mut("messages").and_then(|m| m.as_array_mut()) {
            for m in ms {
                if let Some(o) = m.as_object_mut() {
                    o.remove("bodyHtml");
                }
            }
        }
        Ok(v)
    }
    async fn search(&self, scope: &ReadScope, args: Value) -> Result<Value> {
        let page = dispatch::call(&self.0, "search", Self::with_scope(scope, args)).await?;
        Ok(self.scrub_page(scope, page))
    }
    async fn list_finds(&self, scope: &ReadScope, args: Value) -> Result<Value> {
        dispatch::call(&self.0, "list_finds", Self::with_scope(scope, args)).await
    }
    async fn list_meshes(&self, _scope: &ReadScope, args: Value) -> Result<Value> {
        dispatch::call(&self.0, "list_meshes", args).await
    }
    async fn list_signals(&self, _scope: &ReadScope) -> Result<Value> {
        dispatch::call(&self.0, "list_signals", json!({})).await
    }
    async fn explain(
        &self,
        _scope: &ReadScope,
        dig_id: &DigId,
        message_id: &MessageId,
    ) -> Result<Value> {
        serde_json::to_value(self.0.explain(dig_id, message_id)?).map_err(Into::into)
    }
    async fn list_activity(&self, scope: &ReadScope, args: Value) -> Result<Value> {
        dispatch::call(&self.0, "list_activity", Self::with_scope(scope, args)).await
    }
    async fn simulate_mesh(&self, _scope: &ReadScope, args: Value) -> Result<Value> {
        dispatch::call(&self.0, "simulate_mesh", args).await
    }
    async fn mesh_corrections(&self, scope: &ReadScope, args: Value) -> Result<Value> {
        let only = args
            .get("meshId")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let index = self.0.stores.index();
        let mut corrections = Vec::new();
        for dig in self.0.list_digs()? {
            if !scope.dig_ids.is_empty() && !scope.dig_ids.contains(&dig.id) {
                continue;
            }
            let store = self.0.stores.dig(&dig.id)?;
            corrections.extend(
                repo::corrections::active(&store, 500)?
                    .into_iter()
                    .filter(|c| {
                        only.is_none() || c.mesh_id.as_ref().map(|m| m.as_str()) == only.as_deref()
                    }),
            );
        }
        let suggestions = riddle_agent::suggest_mesh_updates(&corrections, |id| {
            repo::meshes::latest(&index, id)
                .ok()
                .flatten()
                .map(|m| m.name)
        });
        corrections.sort_by_key(|c| std::cmp::Reverse(c.created_at));
        Ok(json!({
            "suggestions": suggestions.iter().map(|s| json!({
                "meshId": s.mesh_id,
                "meshVersion": s.mesh_version,
                "meshName": s.mesh_name,
                "count": s.count,
                "pattern": s.pattern,
                "text": s.text,
            })).collect::<Vec<_>>(),
            // Identifiers and dispositions only; the Director reads a message
            // with get_message when its content matters.
            "corrections": corrections.iter().take(50).map(|c| json!({
                "kind": c.kind,
                "digId": c.dig_id,
                "messageId": c.message_id,
                "from": c.from_disposition,
                "to": c.to_disposition,
                "meshId": c.mesh_id,
                "meshVersion": c.mesh_version,
                "at": c.created_at,
            })).collect::<Vec<_>>(),
        }))
    }
}
