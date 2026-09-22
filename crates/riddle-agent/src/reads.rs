//! Read-only data the Director needs that is not an Action: messages,
//! threads, search, Finds, Meshes, Signals, explanations and Activity. The
//! integrator implements this over the real services and registers a
//! `DirectorReadsHandle` in `Services`; tests use `ScriptedReads`.
//!
//! Every method takes a `ReadScope`: the Digs the conversation may see and
//! whether the user has deliberately opened Contamination. Implementations
//! must filter Contamination-classified messages out of results and refuse
//! `get_message`/`get_thread` on them with `PolicyDenied` unless
//! `allow_contamination` is set (§58 control 2). The Director additionally
//! scrubs list results it can see the disposition of, so the control does
//! not depend on one implementation alone.

use async_trait::async_trait;
use parking_lot::Mutex;
use riddle_core::{DigId, MessageId, ThreadId};
use riddle_errors::{Result, RiddleError};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadScope {
    /// Empty means every Dig the privacy policy permits.
    pub dig_ids: Vec<DigId>,
    /// True only when the user opened a Contamination message in the UI.
    pub allow_contamination: bool,
}

#[async_trait]
pub trait DirectorReads: Send + Sync {
    /// `{ digId?, filter: MessageFilter, page }` -> `Page<MessageSummary>`.
    async fn list_messages(&self, scope: &ReadScope, args: Value) -> Result<Value>;
    /// -> `Message` with sanitized body text.
    async fn get_message(
        &self,
        scope: &ReadScope,
        dig_id: &DigId,
        message_id: &MessageId,
    ) -> Result<Value>;
    /// -> `Thread`.
    async fn get_thread(
        &self,
        scope: &ReadScope,
        dig_id: &DigId,
        thread_id: &ThreadId,
    ) -> Result<Value>;
    /// `{ query, digId?, page }` -> `Page<MessageSummary>`.
    async fn search(&self, scope: &ReadScope, args: Value) -> Result<Value>;
    /// `{ status?, findType?, digId?, page }` -> `Page<Find>`.
    async fn list_finds(&self, scope: &ReadScope, args: Value) -> Result<Value>;
    /// `{ includeDeleted? }` -> `Mesh[]`.
    async fn list_meshes(&self, scope: &ReadScope, args: Value) -> Result<Value>;
    /// -> `Signal[]`.
    async fn list_signals(&self, scope: &ReadScope) -> Result<Value>;
    /// -> `Explanation`.
    async fn explain(
        &self,
        scope: &ReadScope,
        dig_id: &DigId,
        message_id: &MessageId,
    ) -> Result<Value>;
    /// `{ digId?, actor?, action?, since?, until?, page }` -> `Page<Activity>`.
    async fn list_activity(&self, scope: &ReadScope, args: Value) -> Result<Value>;
}

/// Sized handle for `Services::insert`, like `ProjectionHookHandle`.
pub struct DirectorReadsHandle(pub Arc<dyn DirectorReads>);

/// A scripted implementation for tests: canned values per tool name, in
/// order, with the calls recorded. A message or thread id starting with
/// `contam` is treated as Contamination and refused unless allowed.
#[derive(Default)]
pub struct ScriptedReads {
    responses: Mutex<HashMap<String, VecDeque<Value>>>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl ScriptedReads {
    pub fn new() -> Self {
        Self::default()
    }
    /// Queue a response for `tool`.
    pub fn on(self, tool: &str, value: Value) -> Self {
        self.responses
            .lock()
            .entry(tool.to_string())
            .or_default()
            .push_back(value);
        self
    }
    pub fn push(&self, tool: &str, value: Value) {
        self.responses
            .lock()
            .entry(tool.to_string())
            .or_default()
            .push_back(value);
    }
    pub fn calls(&self) -> Vec<(String, Value)> {
        self.calls.lock().clone()
    }
    fn next(&self, tool: &str, args: Value) -> Result<Value> {
        self.calls.lock().push((tool.to_string(), args));
        let mut map = self.responses.lock();
        match map.get_mut(tool).and_then(|q| q.pop_front()) {
            Some(v) => Ok(v),
            None => Err(RiddleError::not_found(format!(
                "scripted reads: nothing queued for {tool}"
            ))),
        }
    }
    fn contamination(scope: &ReadScope, id: &str) -> Result<()> {
        if id.starts_with("contam") && !scope.allow_contamination {
            return Err(RiddleError::policy_denied(
                "that message is Contamination; open it in Riddle to let the Director read it",
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl DirectorReads for ScriptedReads {
    async fn list_messages(&self, _scope: &ReadScope, args: Value) -> Result<Value> {
        self.next("list_messages", args)
    }
    async fn get_message(
        &self,
        scope: &ReadScope,
        dig_id: &DigId,
        message_id: &MessageId,
    ) -> Result<Value> {
        Self::contamination(scope, message_id.as_str())?;
        self.next(
            "get_message",
            serde_json::json!({"digId": dig_id, "messageId": message_id}),
        )
    }
    async fn get_thread(
        &self,
        scope: &ReadScope,
        dig_id: &DigId,
        thread_id: &ThreadId,
    ) -> Result<Value> {
        Self::contamination(scope, thread_id.as_str())?;
        self.next(
            "get_thread",
            serde_json::json!({"digId": dig_id, "threadId": thread_id}),
        )
    }
    async fn search(&self, _scope: &ReadScope, args: Value) -> Result<Value> {
        self.next("search", args)
    }
    async fn list_finds(&self, _scope: &ReadScope, args: Value) -> Result<Value> {
        self.next("list_finds", args)
    }
    async fn list_meshes(&self, _scope: &ReadScope, args: Value) -> Result<Value> {
        self.next("list_meshes", args)
    }
    async fn list_signals(&self, _scope: &ReadScope) -> Result<Value> {
        self.next("list_signals", Value::Null)
    }
    async fn explain(
        &self,
        _scope: &ReadScope,
        dig_id: &DigId,
        message_id: &MessageId,
    ) -> Result<Value> {
        self.next(
            "explain",
            serde_json::json!({"digId": dig_id, "messageId": message_id}),
        )
    }
    async fn list_activity(&self, _scope: &ReadScope, args: Value) -> Result<Value> {
        self.next("list_activity", args)
    }
}
