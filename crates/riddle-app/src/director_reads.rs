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
        Ok(repo::classifications::current(&store, id)?.map(|k| k.disposition.kind() == DispositionKind::Contamination).unwrap_or(false))
    }

    fn scrub_page(&self, scope: &ReadScope, mut page: Value) -> Value {
        if scope.allow_contamination {
            return page;
        }
        if let Some(items) = page.get_mut("items").and_then(|i| i.as_array_mut()) {
            items.retain(|m| m.get("disposition").and_then(|d| d.get("kind")).and_then(|k| k.as_str()) != Some("contamination"));
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
    async fn get_message(&self, scope: &ReadScope, dig_id: &DigId, message_id: &MessageId) -> Result<Value> {
        if !scope.allow_contamination && self.is_contaminated(dig_id, message_id)? {
            return Err(RiddleError::policy_denied("that message is Contamination; open it yourself first"));
        }
        let mut v = serde_json::to_value(self.0.get_message(dig_id, message_id)?)?;
        // The Director reads text; HTML is for the sandboxed viewer only.
        if let Some(o) = v.as_object_mut() {
            o.remove("bodyHtml");
        }
        Ok(v)
    }
    async fn get_thread(&self, scope: &ReadScope, dig_id: &DigId, thread_id: &ThreadId) -> Result<Value> {
        let thread = self.0.get_thread(dig_id, thread_id)?;
        if !scope.allow_contamination {
            for m in &thread.messages {
                if self.is_contaminated(dig_id, &m.id)? {
                    return Err(RiddleError::policy_denied("that thread contains Contamination; open it yourself first"));
                }
            }
        }
        let mut v = serde_json::to_value(thread)?;
        if let Some(ms) = v.get_mut("messages").and_then(|m| m.as_array_mut()) {
            for m in ms {
                if let Some(o) = m.as_object_mut() { o.remove("bodyHtml"); }
            }
        }
        Ok(v)
    }
    async fn search(&self, scope: &ReadScope, args: Value) -> Result<Value> {
        let page = dispatch::call(&self.0, "search", Self::with_scope(scope, args)).await?;
        Ok(self.scrub_page(scope, page))
    }
    async fn list_finds(&self, scope: &ReadScope, args: Value) -> Result<Value> { dispatch::call(&self.0, "list_finds", Self::with_scope(scope, args)).await }
    async fn list_meshes(&self, _scope: &ReadScope, args: Value) -> Result<Value> { dispatch::call(&self.0, "list_meshes", args).await }
    async fn list_signals(&self, _scope: &ReadScope) -> Result<Value> { dispatch::call(&self.0, "list_signals", json!({})).await }
    async fn explain(&self, _scope: &ReadScope, dig_id: &DigId, message_id: &MessageId) -> Result<Value> {
        serde_json::to_value(self.0.explain(dig_id, message_id)?).map_err(Into::into)
    }
    async fn list_activity(&self, scope: &ReadScope, args: Value) -> Result<Value> { dispatch::call(&self.0, "list_activity", Self::with_scope(scope, args)).await }
}
