//! The `Llm` trait and its request and response shapes. Provider-neutral by
//! construction: nothing here names Bedrock or Anthropic.

use async_trait::async_trait;
use riddle_core::{DigId, PrivacyMode, UsageTask};
use riddle_errors::{Result, RiddleError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// One block of a message. Tool results carry an `untrusted` flag that the
/// adapter turns into delimiters and a notice (§58 control 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        id: String,
        content: String,
        /// True when the content came from mail or search results.
        untrusted: bool,
        /// True when the tool failed; the content then describes the error.
        is_error: bool,
    },
}

impl ContentBlock {
    pub fn text(s: impl Into<String>) -> Self {
        ContentBlock::Text { text: s.into() }
    }
    pub fn tool_result(id: impl Into<String>, content: impl Into<String>, untrusted: bool) -> Self {
        ContentBlock::ToolResult {
            id: id.into(),
            content: content.into(),
            untrusted,
            is_error: false,
        }
    }
    pub fn tool_error(id: impl Into<String>, content: impl Into<String>) -> Self {
        ContentBlock::ToolResult {
            id: id.into(),
            content: content.into(),
            untrusted: false,
            is_error: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl LlmMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::text(text)],
        }
    }
    pub fn assistant(content: Vec<ContentBlock>) -> Self {
        Self {
            role: Role::Assistant,
            content,
        }
    }
    pub fn user_blocks(content: Vec<ContentBlock>) -> Self {
        Self {
            role: Role::User,
            content,
        }
    }
    /// The concatenated text blocks.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

/// A tool the model may call: name, description and a JSON schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl ToolSpec {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmRequest {
    pub system: String,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
    pub temperature: f32,
    /// Ask the provider to cache the stable prefix (system prompt and tools).
    pub cache_hint: bool,
    /// The privacy mode of the Dig (or the most restrictive of the Digs) the
    /// call serves. Checked before any network activity.
    pub privacy: PrivacyMode,
    /// Ledger attribution (§56).
    pub task: UsageTask,
    pub dig_id: Option<DigId>,
    /// Override the implementation's default model, e.g. a tier from the catalog.
    pub model_id: Option<String>,
}

impl LlmRequest {
    pub fn new(system: impl Into<String>, messages: Vec<LlmMessage>) -> Self {
        Self {
            system: system.into(),
            messages,
            tools: vec![],
            max_tokens: 4096,
            temperature: 0.0,
            cache_hint: false,
            privacy: PrivacyMode::CloudReasoning,
            task: UsageTask::Director,
            dig_id: None,
            model_id: None,
        }
    }
    pub fn with_tools(mut self, tools: Vec<ToolSpec>) -> Self {
        self.tools = tools;
        self
    }
    pub fn with_privacy(mut self, privacy: PrivacyMode) -> Self {
        self.privacy = privacy;
        self
    }
    pub fn with_task(mut self, task: UsageTask) -> Self {
        self.task = task;
        self
    }
    pub fn with_model(mut self, model_id: impl Into<String>) -> Self {
        self.model_id = Some(model_id.into());
        self
    }
    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = n;
        self
    }
    pub fn with_temperature(mut self, t: f32) -> Self {
        self.temperature = t;
        self
    }
    pub fn with_cache_hint(mut self, hint: bool) -> Self {
        self.cache_hint = hint;
        self
    }
    pub fn with_dig(mut self, dig_id: Option<DigId>) -> Self {
        self.dig_id = dig_id;
        self
    }

    /// The privacy check every implementation runs first (§57). Local-only
    /// and cloud-judgment Digs never reach an LLM.
    pub fn check_privacy(&self) -> Result<()> {
        if riddle_privacy::allowed(self.privacy, riddle_privacy::Purpose::Llm) {
            Ok(())
        } else {
            Err(RiddleError::policy_denied(format!(
                "privacy mode {:?} does not permit cloud reasoning",
                self.privacy
            )))
        }
    }

    /// Rough token estimate for budget accounting before a call.
    pub fn approx_input_tokens(&self) -> u64 {
        let mut chars = self.system.len();
        for m in &self.messages {
            for b in &m.content {
                chars += match b {
                    ContentBlock::Text { text } => text.len(),
                    ContentBlock::ToolUse { input, .. } => input.to_string().len() + 32,
                    ContentBlock::ToolResult { content, .. } => content.len() + 32,
                };
            }
        }
        for t in &self.tools {
            chars += t.description.len() + t.input_schema.to_string().len() + t.name.len();
        }
        (chars / 4) as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    /// The provider filtered or refused the content.
    ContentFiltered,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub model_id: String,
    pub latency_ms: u64,
    /// `Some` only when the catalog prices the model (§56).
    pub cost_usd: Option<f64>,
}

impl LlmResponse {
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
    pub fn tool_uses(&self) -> Vec<(&str, &str, &Value)> {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.as_str(), name.as_str(), input))
                }
                _ => None,
            })
            .collect()
    }
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmIdentity {
    /// `bedrock`, `scripted`, `null`.
    pub provider: String,
    pub model_id: String,
}

impl LlmIdentity {
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider, self.model_id)
    }
}

/// The reasoning model behind escalation, the Director and Expeditions.
#[async_trait]
pub trait Llm: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse>;

    /// Streaming variant. The default shares the non-streaming path and
    /// emits the whole text as one delta; Bedrock overrides it with
    /// `ConverseStream`.
    async fn complete_stream(
        &self,
        req: LlmRequest,
        mut on_delta: Box<dyn FnMut(String) + Send>,
    ) -> Result<LlmResponse> {
        let resp = self.complete(req).await?;
        let text = resp.text();
        if !text.is_empty() {
            on_delta(text);
        }
        Ok(resp)
    }

    fn identity(&self) -> LlmIdentity;

    /// The breaker and the last calls, without making one. The doubles keep
    /// neither and answer `Unknown`.
    fn health(&self) -> riddle_core::ServiceHealth {
        riddle_core::ServiceHealth::default()
    }

    /// A cheap liveness probe for the Developer pane.
    async fn probe(&self) -> Result<u64> {
        let req = LlmRequest::new(
            "Reply with the single word OK.",
            vec![LlmMessage::user("ping")],
        )
        .with_max_tokens(8)
        .with_task(UsageTask::Probe);
        let started = std::time::Instant::now();
        self.complete(req).await?;
        Ok(started.elapsed().as_millis() as u64)
    }
}
