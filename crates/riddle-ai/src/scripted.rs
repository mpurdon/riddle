//! `ScriptedLlm`: canned responses in order, for tests. It enforces the same
//! privacy check as the real adapter and records every request it saw (with
//! untrusted tool results wrapped exactly as the wire adapter wraps them), so
//! a test can prove both that a local-only Dig never reached the model and
//! that mail entered context delimited.

use crate::llm::{ContentBlock, Llm, LlmIdentity, LlmRequest, LlmResponse, StopReason};
use crate::untrusted;
use async_trait::async_trait;
use parking_lot::Mutex;
use riddle_errors::{Result, RiddleError};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone)]
pub enum ScriptedResponse {
    Text(String),
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    /// Several tool calls in one assistant turn (parallel tool use).
    ToolCalls(Vec<(String, String, Value)>),
    Blocks {
        content: Vec<ContentBlock>,
        stop_reason: StopReason,
    },
    Error(RiddleError),
}

#[derive(Default)]
pub struct ScriptedLlm {
    script: Mutex<VecDeque<ScriptedResponse>>,
    requests: Mutex<Vec<LlmRequest>>,
    calls: AtomicUsize,
    model_id: String,
    /// Tokens reported per call, so budget tests are deterministic.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: Option<f64>,
}

impl ScriptedLlm {
    pub fn new(script: Vec<ScriptedResponse>) -> Self {
        Self {
            script: Mutex::new(script.into()),
            requests: Mutex::new(vec![]),
            calls: AtomicUsize::new(0),
            model_id: "scripted-1".into(),
            input_tokens: 100,
            output_tokens: 20,
            cost_usd: Some(0.001),
        }
    }
    pub fn with_cost(
        mut self,
        input_tokens: u64,
        output_tokens: u64,
        cost_usd: Option<f64>,
    ) -> Self {
        self.input_tokens = input_tokens;
        self.output_tokens = output_tokens;
        self.cost_usd = cost_usd;
        self
    }
    pub fn text(s: impl Into<String>) -> ScriptedResponse {
        ScriptedResponse::Text(s.into())
    }
    pub fn tool_call(name: impl Into<String>, input: Value) -> ScriptedResponse {
        let name = name.into();
        ScriptedResponse::ToolCall {
            id: format!("call-{}", uuid::Uuid::now_v7().simple()),
            name,
            input,
        }
    }
    pub fn error(e: RiddleError) -> ScriptedResponse {
        ScriptedResponse::Error(e)
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    pub fn remaining(&self) -> usize {
        self.script.lock().len()
    }
    /// Requests as the wire adapter would have sent them.
    pub fn requests(&self) -> Vec<LlmRequest> {
        self.requests.lock().clone()
    }
    pub fn push(&self, r: ScriptedResponse) {
        self.script.lock().push_back(r);
    }

    fn build(&self, r: ScriptedResponse) -> Result<LlmResponse> {
        let (content, stop_reason) = match r {
            ScriptedResponse::Text(t) => (vec![ContentBlock::text(t)], StopReason::EndTurn),
            ScriptedResponse::ToolCall { id, name, input } => (
                vec![ContentBlock::ToolUse { id, name, input }],
                StopReason::ToolUse,
            ),
            ScriptedResponse::ToolCalls(calls) => (
                calls
                    .into_iter()
                    .map(|(id, name, input)| ContentBlock::ToolUse { id, name, input })
                    .collect(),
                StopReason::ToolUse,
            ),
            ScriptedResponse::Blocks {
                content,
                stop_reason,
            } => (content, stop_reason),
            ScriptedResponse::Error(e) => return Err(e),
        };
        Ok(LlmResponse {
            content,
            stop_reason,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            model_id: self.model_id.clone(),
            latency_ms: 1,
            cost_usd: self.cost_usd,
        })
    }
}

/// Apply the boundary marking the wire adapter applies, for recording.
pub fn as_wire(req: &LlmRequest) -> LlmRequest {
    let mut r = req.clone();
    for m in &mut r.messages {
        for b in &mut m.content {
            if let ContentBlock::ToolResult {
                content,
                untrusted: true,
                ..
            } = b
            {
                if !untrusted::is_wrapped(content) {
                    *content = untrusted::wrap(content);
                }
            }
        }
    }
    r
}

#[async_trait]
impl Llm for ScriptedLlm {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse> {
        req.check_privacy()?;
        self.requests.lock().push(as_wire(&req));
        self.calls.fetch_add(1, Ordering::SeqCst);
        let next = self.script.lock().pop_front();
        match next {
            Some(r) => self.build(r),
            None => Err(RiddleError::llm_unavailable(
                "scripted LLM: script exhausted",
            )),
        }
    }

    fn identity(&self) -> LlmIdentity {
        LlmIdentity {
            provider: "scripted".into(),
            model_id: self.model_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmMessage;
    use riddle_core::PrivacyMode;
    use riddle_errors::ErrorKind;

    #[tokio::test]
    async fn a_local_only_request_is_denied_before_the_script_is_consumed() {
        let llm = ScriptedLlm::new(vec![ScriptedLlm::text("hi")]);
        let req =
            LlmRequest::new("s", vec![LlmMessage::user("u")]).with_privacy(PrivacyMode::LocalOnly);
        let e = llm.complete(req).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::PolicyDenied);
        assert_eq!(llm.calls(), 0);
        assert_eq!(llm.remaining(), 1);
        let judgment_only = LlmRequest::new("s", vec![LlmMessage::user("u")])
            .with_privacy(PrivacyMode::CloudJudgment);
        assert_eq!(
            llm.complete(judgment_only).await.unwrap_err().kind,
            ErrorKind::PolicyDenied
        );
    }

    #[tokio::test]
    async fn responses_come_in_order_and_untrusted_results_are_recorded_wrapped() {
        let llm = ScriptedLlm::new(vec![
            ScriptedLlm::tool_call("search", serde_json::json!({"q": "x"})),
            ScriptedLlm::text("done"),
        ]);
        let first = llm
            .complete(LlmRequest::new("s", vec![LlmMessage::user("u")]))
            .await
            .unwrap();
        assert_eq!(first.stop_reason, StopReason::ToolUse);
        let req = LlmRequest::new(
            "s",
            vec![LlmMessage::user_blocks(vec![ContentBlock::tool_result(
                "call-1",
                "subject: hi",
                true,
            )])],
        );
        let second = llm.complete(req).await.unwrap();
        assert_eq!(second.text(), "done");
        let seen = llm.requests();
        match &seen[1].messages[0].content[0] {
            ContentBlock::ToolResult { content, .. } => assert!(content.contains(untrusted::OPEN)),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(
            llm.complete(LlmRequest::new("s", vec![]))
                .await
                .unwrap_err()
                .kind,
            ErrorKind::LlmUnavailable
        );
    }
}
