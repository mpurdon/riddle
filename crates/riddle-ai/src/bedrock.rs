//! Amazon Bedrock through the AWS SDK's `Converse` and `ConverseStream`
//! APIs, authenticated by a named profile (§56). The model provider
//! credential is a separate principal from the mailbox credential: this
//! adapter never sees a Gmail token and the Director never sees an AWS one.
//!
//! What this adapter guarantees, in order, for every call:
//! 1. the privacy check (`LlmRequest::check_privacy`) before any network;
//! 2. a circuit-breaker permit per model, so an outage fails fast;
//! 3. up to three attempts with backoff on throttling and transient errors;
//! 4. untrusted tool results wrapped in delimiters at the wire (§58);
//! 5. a usage row for the call, success or failure, that never fails it;
//! 6. `cost_usd` only when the catalog prices the model.
//!
//! The client reads the profile's credentials once, so credentials that a
//! tool such as sleipnir rewrites in `~/.aws/credentials` were never seen:
//! a Director call failed with ExpiredTokenException for as long as the app
//! ran, while the same profile worked from the shell (14 Oct 2026). An
//! expired or unrecognised credential now drops the client, and the retry
//! builds a new one from what is on disk.
//!
//! Streaming uses `ConverseStream` and accumulates text and tool-use blocks
//! by content-block index; the non-streaming path uses `Converse`. Both map
//! to the same `LlmResponse`.

use crate::breaker::{Breaker, BreakerConfig};
use crate::catalog::ModelCatalog;
use crate::llm::{
    ContentBlock, Llm, LlmIdentity, LlmMessage, LlmRequest, LlmResponse, Role, StopReason,
};
use crate::retry::{with_backoff, RetryPolicy};
use crate::untrusted;
use crate::usage::{record_usage, UsageRecorder};
use async_trait::async_trait;
use aws_sdk_bedrockruntime::types as bt;
use aws_smithy_types::Document;
use riddle_errors::{ErrorKind, Result, RiddleError};
use riddle_observability::{rdebug, rinfo, rwarn, Category};
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct BedrockConfig {
    /// The AWS profile name; `None` uses the default credential chain.
    pub profile: Option<String>,
    pub region: String,
    pub default_model: String,
    pub retry: RetryPolicy,
    pub breaker: BreakerConfig,
}

impl Default for BedrockConfig {
    fn default() -> Self {
        Self {
            profile: Some("dev".into()),
            region: "us-east-1".into(),
            default_model: crate::catalog::HAIKU_4_5.into(),
            retry: RetryPolicy::default(),
            breaker: BreakerConfig::default(),
        }
    }
}

pub struct BedrockLlm {
    config: BedrockConfig,
    client: std::sync::Mutex<Option<aws_sdk_bedrockruntime::Client>>,
    catalog: Arc<ModelCatalog>,
    breaker: Arc<Breaker>,
    recorder: Option<UsageRecorder>,
    /// When a model last answered and last failed, for `health()`.
    calls: riddle_core::CallLog,
}

impl BedrockLlm {
    pub fn new(config: BedrockConfig, catalog: Arc<ModelCatalog>) -> Self {
        let breaker = Arc::new(Breaker::new(config.breaker));
        Self {
            config,
            client: std::sync::Mutex::new(None),
            catalog,
            breaker,
            recorder: None,
            calls: riddle_core::CallLog::default(),
        }
    }

    pub fn with_recorder(mut self, recorder: UsageRecorder) -> Self {
        self.recorder = Some(recorder);
        self
    }

    pub fn breaker(&self) -> Arc<Breaker> {
        self.breaker.clone()
    }

    pub fn catalog(&self) -> Arc<ModelCatalog> {
        self.catalog.clone()
    }

    fn cached_client(&self) -> std::sync::MutexGuard<'_, Option<aws_sdk_bedrockruntime::Client>> {
        self.client.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The client, built from the credentials on disk the first time and
    /// again after `forget_stale_client` dropped it. Two calls racing to
    /// build both load the same files; the last one is kept.
    async fn client(&self) -> Result<aws_sdk_bedrockruntime::Client> {
        if let Some(c) = self.cached_client().as_ref() {
            return Ok(c.clone());
        }
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(self.config.region.clone()));
        if let Some(p) = &self.config.profile {
            loader = loader.profile_name(p);
        }
        let cfg = loader.load().await;
        rinfo!(
            Category::Llm,
            "bedrock client ready in {} (profile {})",
            self.config.region,
            self.config
                .profile
                .as_deref()
                .map(|_| "named")
                .unwrap_or("default chain")
        );
        let client = aws_sdk_bedrockruntime::Client::new(&cfg);
        *self.cached_client() = Some(client.clone());
        Ok(client)
    }

    /// Drops the client when AWS refused its credentials, so the next
    /// attempt reads them from disk again.
    fn forget_stale_client(&self, e: RiddleError) -> RiddleError {
        if is_stale_credential(&e) {
            rwarn!(
                Category::Llm,
                "bedrock credentials were refused; reloading them for the next attempt"
            );
            *self.cached_client() = None;
        }
        e
    }

    fn model_for(&self, req: &LlmRequest) -> String {
        req.model_id
            .clone()
            .unwrap_or_else(|| self.config.default_model.clone())
    }

    /// The shared entry: privacy, breaker, retry, ledger.
    async fn run<F, Fut>(&self, req: &LlmRequest, mut attempt: F) -> Result<LlmResponse>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<LlmResponse>>,
    {
        req.check_privacy()?;
        let model = self.model_for(req);
        let permit = self.breaker.acquire(&model)?;
        let started = Instant::now();
        let outcome = with_backoff(self.config.retry, |n| {
            if n > 1 {
                rwarn!(Category::Llm, "bedrock retry {n} for model {model}");
            }
            attempt(model.clone())
        })
        .await;
        let latency = started.elapsed().as_millis() as u64;
        match &outcome {
            Ok(resp) => {
                permit.succeed();
                self.calls.succeeded(chrono::Utc::now());
                record_usage(
                    self.recorder.as_ref(),
                    "bedrock",
                    &model,
                    req.task,
                    req.dig_id.clone(),
                    resp.input_tokens,
                    resp.output_tokens,
                    resp.cost_usd,
                    resp.latency_ms,
                    true,
                );
                rinfo!(
                    Category::Llm,
                    "bedrock {} ok: {} in, {} out, {} cached, {}ms, priced={}",
                    req.task.as_str(),
                    resp.input_tokens,
                    resp.output_tokens,
                    resp.cache_read_tokens,
                    resp.latency_ms,
                    resp.cost_usd.is_some()
                );
            }
            Err(e) => {
                permit.fail();
                self.calls.failed(chrono::Utc::now(), e.kind.as_str());
                record_usage(
                    self.recorder.as_ref(),
                    "bedrock",
                    &model,
                    req.task,
                    req.dig_id.clone(),
                    req.approx_input_tokens(),
                    0,
                    None,
                    latency,
                    false,
                );
                rwarn!(
                    Category::Llm,
                    "bedrock {} failed after {latency}ms: {}",
                    req.task.as_str(),
                    e.kind.as_str()
                );
            }
        }
        outcome
    }

    fn finish(
        &self,
        model: &str,
        content: Vec<ContentBlock>,
        stop: StopReason,
        usage: Option<&bt::TokenUsage>,
        latency_ms: u64,
    ) -> LlmResponse {
        let input_tokens = usage.map(|u| u.input_tokens().max(0) as u64).unwrap_or(0);
        let output_tokens = usage.map(|u| u.output_tokens().max(0) as u64).unwrap_or(0);
        let cache_read = usage
            .and_then(|u| u.cache_read_input_tokens())
            .unwrap_or(0)
            .max(0) as u64;
        let cache_write = usage
            .and_then(|u| u.cache_write_input_tokens())
            .unwrap_or(0)
            .max(0) as u64;
        let cost_usd =
            self.catalog
                .cost(model, input_tokens, output_tokens, cache_read, cache_write);
        LlmResponse {
            content,
            stop_reason: stop,
            input_tokens,
            output_tokens,
            cache_read_tokens: cache_read,
            cache_write_tokens: cache_write,
            model_id: model.to_string(),
            latency_ms,
            cost_usd,
        }
    }

    async fn converse_once(&self, model: String, req: &LlmRequest) -> Result<LlmResponse> {
        let client = self.client().await?;
        let (system, messages, tool_config) = build_wire(req)?;
        let started = Instant::now();
        let mut call = client
            .converse()
            .model_id(&model)
            .set_system(Some(system))
            .set_messages(Some(messages))
            .inference_config(inference(req));
        if let Some(tc) = tool_config {
            call = call.tool_config(tc);
        }
        let out = call
            .send()
            .await
            .map_err(|e| self.forget_stale_client(map_sdk_error(e)))?;
        let latency = out
            .metrics()
            .map(|m| m.latency_ms().max(0) as u64)
            .unwrap_or(started.elapsed().as_millis() as u64);
        let content = out
            .output()
            .and_then(|o| o.as_message().ok())
            .map(|m| m.content().iter().filter_map(from_wire_block).collect())
            .unwrap_or_default();
        let stop = map_stop(out.stop_reason());
        Ok(self.finish(&model, content, stop, out.usage(), latency))
    }

    async fn converse_stream_once(
        &self,
        model: String,
        req: &LlmRequest,
        on_delta: &mut (dyn FnMut(String) + Send),
    ) -> Result<LlmResponse> {
        let client = self.client().await?;
        let (system, messages, tool_config) = build_wire(req)?;
        let started = Instant::now();
        let mut call = client
            .converse_stream()
            .model_id(&model)
            .set_system(Some(system))
            .set_messages(Some(messages))
            .inference_config(inference(req));
        if let Some(tc) = tool_config {
            call = call.tool_config(tc);
        }
        let mut out = call
            .send()
            .await
            .map_err(|e| self.forget_stale_client(map_sdk_error(e)))?;

        // Blocks by index: text accumulates; tool use accumulates its JSON.
        let mut blocks: Vec<(i32, Partial)> = Vec::new();
        let mut stop = StopReason::EndTurn;
        let mut usage: Option<bt::TokenUsage> = None;
        let mut latency: Option<u64> = None;
        loop {
            let event = out.stream.recv().await.map_err(|e| {
                RiddleError::llm_unavailable(format!("bedrock stream failed: {}", describe(&e)))
                    .with_detail(serde_json::json!({"retryable": true}))
            })?;
            let Some(event) = event else { break };
            match event {
                bt::ConverseStreamOutput::ContentBlockStart(s) => {
                    let idx = s.content_block_index();
                    if let Some(bt::ContentBlockStart::ToolUse(t)) = s.start() {
                        blocks.push((
                            idx,
                            Partial::Tool {
                                id: t.tool_use_id().to_string(),
                                name: t.name().to_string(),
                                json: String::new(),
                            },
                        ));
                    }
                }
                bt::ConverseStreamOutput::ContentBlockDelta(d) => {
                    let idx = d.content_block_index();
                    match d.delta() {
                        Some(bt::ContentBlockDelta::Text(t)) => {
                            match blocks.iter_mut().find(|(i, _)| *i == idx) {
                                Some((_, Partial::Text(s))) => s.push_str(t),
                                Some(_) => {}
                                None => blocks.push((idx, Partial::Text(t.clone()))),
                            }
                            on_delta(t.clone());
                        }
                        Some(bt::ContentBlockDelta::ToolUse(tu)) => {
                            if let Some((_, Partial::Tool { json, .. })) =
                                blocks.iter_mut().find(|(i, _)| *i == idx)
                            {
                                json.push_str(tu.input());
                            }
                        }
                        _ => {}
                    }
                }
                bt::ConverseStreamOutput::MessageStop(m) => {
                    stop = map_stop(m.stop_reason());
                }
                bt::ConverseStreamOutput::Metadata(m) => {
                    usage = m.usage().cloned();
                    latency = m.metrics().map(|x| x.latency_ms().max(0) as u64);
                }
                _ => {}
            }
        }
        let content: Vec<ContentBlock> = blocks
            .into_iter()
            .map(|(_, p)| match p {
                Partial::Text(t) => ContentBlock::Text { text: t },
                Partial::Tool { id, name, json } => {
                    let input = if json.trim().is_empty() {
                        Value::Object(Default::default())
                    } else {
                        serde_json::from_str(&json).unwrap_or(Value::Object(Default::default()))
                    };
                    ContentBlock::ToolUse { id, name, input }
                }
            })
            .collect();
        let latency = latency.unwrap_or(started.elapsed().as_millis() as u64);
        Ok(self.finish(&model, content, stop, usage.as_ref(), latency))
    }
}

enum Partial {
    Text(String),
    Tool {
        id: String,
        name: String,
        json: String,
    },
}

#[async_trait]
impl Llm for BedrockLlm {
    async fn complete(&self, req: LlmRequest) -> Result<LlmResponse> {
        let r = &req;
        self.run(r, |model| self.converse_once(model, r)).await
    }

    async fn complete_stream(
        &self,
        req: LlmRequest,
        on_delta: Box<dyn FnMut(String) + Send>,
    ) -> Result<LlmResponse> {
        let r = &req;
        // A tokio mutex because its guard is `Send`; the retry closure
        // re-borrows the sink for each attempt.
        let on_delta = tokio::sync::Mutex::new(on_delta);
        self.run(r, |model| {
            let on_delta = &on_delta;
            async move {
                let mut guard = on_delta.lock().await;
                self.converse_stream_once(model, r, &mut **guard).await
            }
        })
        .await
    }

    fn identity(&self) -> LlmIdentity {
        LlmIdentity {
            provider: "bedrock".into(),
            model_id: self.config.default_model.clone(),
        }
    }

    /// The default model's circuit; the breaker is per model, and the
    /// default is the one Sift's escalation and the Director call.
    fn health(&self) -> riddle_core::ServiceHealth {
        self.calls
            .health(self.breaker.health(&self.config.default_model))
    }
}

fn inference(req: &LlmRequest) -> bt::InferenceConfiguration {
    bt::InferenceConfiguration::builder()
        .max_tokens(req.max_tokens.min(i32::MAX as u32) as i32)
        .temperature(req.temperature)
        .build()
}

type Wire = (
    Vec<bt::SystemContentBlock>,
    Vec<bt::Message>,
    Option<bt::ToolConfiguration>,
);

/// Build the Converse request. Untrusted tool results are wrapped here so no
/// caller can forget; consecutive same-role messages are merged because the
/// API requires alternation; cache points mark the stable prefix.
///
/// Converse refuses a request whose messages carry `toolUse` or `toolResult`
/// blocks unless it also carries a `toolConfig`, and the tool loop offers no
/// tools on its closing call after a proposal or an exhausted budget. The
/// Director's "go ahead and propose the change" ended in a ValidationException
/// on exactly that call, after the proposal had been made. With no tools
/// offered, the history's tool blocks go over as text: the model still reads
/// what it called and what came back, and there is nothing it could call.
pub fn build_wire(req: &LlmRequest) -> Result<Wire> {
    let mut system = vec![bt::SystemContentBlock::Text(req.system.clone())];
    if req.cache_hint {
        system.push(bt::SystemContentBlock::CachePoint(cache_point()?));
    }

    let merged = merge_roles(&req.messages);
    let last_user = merged.iter().rposition(|m| m.role == Role::User);
    let to_block = if req.tools.is_empty() {
        to_text_block
    } else {
        to_wire_block
    };
    let mut messages = Vec::with_capacity(merged.len());
    for (i, m) in merged.iter().enumerate() {
        let mut content: Vec<bt::ContentBlock> =
            m.content.iter().map(to_block).collect::<Result<_>>()?;
        if content.is_empty() {
            content.push(bt::ContentBlock::Text("(empty)".into()));
        }
        if req.cache_hint && Some(i) == last_user && !req.tools.is_empty() {
            content.push(bt::ContentBlock::CachePoint(cache_point()?));
        }
        let role = match m.role {
            Role::User => bt::ConversationRole::User,
            Role::Assistant => bt::ConversationRole::Assistant,
        };
        messages.push(
            bt::Message::builder()
                .role(role)
                .set_content(Some(content))
                .build()
                .map_err(|e| RiddleError::internal(format!("bedrock message: {e}")))?,
        );
    }

    let tool_config = if req.tools.is_empty() {
        None
    } else {
        let mut tools = Vec::with_capacity(req.tools.len() + 1);
        for t in &req.tools {
            let spec = bt::ToolSpecification::builder()
                .name(&t.name)
                .description(&t.description)
                .input_schema(bt::ToolInputSchema::Json(json_to_document(&t.input_schema)))
                .build()
                .map_err(|e| RiddleError::internal(format!("bedrock tool spec {}: {e}", t.name)))?;
            tools.push(bt::Tool::ToolSpec(spec));
        }
        if req.cache_hint {
            tools.push(bt::Tool::CachePoint(cache_point()?));
        }
        Some(
            bt::ToolConfiguration::builder()
                .set_tools(Some(tools))
                .build()
                .map_err(|e| RiddleError::internal(format!("bedrock tool config: {e}")))?,
        )
    };
    Ok((system, messages, tool_config))
}

fn cache_point() -> Result<bt::CachePointBlock> {
    bt::CachePointBlock::builder()
        .r#type(bt::CachePointType::Default)
        .build()
        .map_err(|e| RiddleError::internal(format!("cache point: {e}")))
}

/// Merge adjacent messages with the same role.
pub fn merge_roles(messages: &[LlmMessage]) -> Vec<LlmMessage> {
    let mut out: Vec<LlmMessage> = Vec::with_capacity(messages.len());
    for m in messages {
        match out.last_mut() {
            Some(last) if last.role == m.role => last.content.extend(m.content.iter().cloned()),
            _ => out.push(m.clone()),
        }
    }
    out
}

/// A tool result's text as the model sees it: an untrusted result wrapped in
/// the §58 delimiters, once.
fn result_text(content: &str, untrusted: bool) -> String {
    if untrusted && !untrusted::is_wrapped(content) {
        untrusted::wrap(content)
    } else {
        content.to_string()
    }
}

fn to_wire_block(b: &ContentBlock) -> Result<bt::ContentBlock> {
    Ok(match b {
        ContentBlock::Text { text } => bt::ContentBlock::Text(text.clone()),
        ContentBlock::ToolUse { id, name, input } => bt::ContentBlock::ToolUse(
            bt::ToolUseBlock::builder()
                .tool_use_id(id)
                .name(name)
                .input(json_to_document(input))
                .build()
                .map_err(|e| RiddleError::internal(format!("tool use: {e}")))?,
        ),
        ContentBlock::ToolResult {
            id,
            content,
            untrusted,
            is_error,
        } => {
            let mut builder = bt::ToolResultBlock::builder().tool_use_id(id).content(
                bt::ToolResultContentBlock::Text(result_text(content, *untrusted)),
            );
            if *is_error {
                builder = builder.status(bt::ToolResultStatus::Error);
            }
            bt::ContentBlock::ToolResult(
                builder
                    .build()
                    .map_err(|e| RiddleError::internal(format!("tool result: {e}")))?,
            )
        }
    })
}

/// A block as text only, for a request that offers no tools: what was
/// called and what came back, with untrusted results still wrapped.
fn to_text_block(b: &ContentBlock) -> Result<bt::ContentBlock> {
    Ok(bt::ContentBlock::Text(match b {
        ContentBlock::Text { text } => text.clone(),
        ContentBlock::ToolUse { id, name, input } => {
            format!("[called {name} ({id}) with {input}]")
        }
        ContentBlock::ToolResult {
            id,
            content,
            untrusted,
            is_error,
        } => {
            let status = if *is_error { " failed" } else { "" };
            format!("[{id}{status}: {}]", result_text(content, *untrusted))
        }
    }))
}

fn from_wire_block(b: &bt::ContentBlock) -> Option<ContentBlock> {
    match b {
        bt::ContentBlock::Text(t) => Some(ContentBlock::Text { text: t.clone() }),
        bt::ContentBlock::ToolUse(t) => Some(ContentBlock::ToolUse {
            id: t.tool_use_id().to_string(),
            name: t.name().to_string(),
            input: document_to_json(t.input()),
        }),
        _ => None,
    }
}

pub fn map_stop(s: &bt::StopReason) -> StopReason {
    match s {
        bt::StopReason::EndTurn => StopReason::EndTurn,
        bt::StopReason::ToolUse => StopReason::ToolUse,
        bt::StopReason::MaxTokens => StopReason::MaxTokens,
        bt::StopReason::StopSequence => StopReason::StopSequence,
        bt::StopReason::GuardrailIntervened => StopReason::ContentFiltered,
        other => StopReason::Other(other.as_str().to_string()),
    }
}

pub fn json_to_document(v: &Value) -> Document {
    match v {
        Value::Null => Document::Null,
        Value::Bool(b) => Document::Bool(*b),
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                Document::Number(aws_smithy_types::Number::PosInt(u))
            } else if let Some(i) = n.as_i64() {
                Document::Number(aws_smithy_types::Number::NegInt(i))
            } else {
                Document::Number(aws_smithy_types::Number::Float(n.as_f64().unwrap_or(0.0)))
            }
        }
        Value::String(s) => Document::String(s.clone()),
        Value::Array(a) => Document::Array(a.iter().map(json_to_document).collect()),
        Value::Object(o) => Document::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), json_to_document(v)))
                .collect(),
        ),
    }
}

pub fn document_to_json(d: &Document) -> Value {
    match d {
        Document::Null => Value::Null,
        Document::Bool(b) => Value::Bool(*b),
        Document::Number(n) => match n {
            aws_smithy_types::Number::PosInt(u) => Value::from(*u),
            aws_smithy_types::Number::NegInt(i) => Value::from(*i),
            aws_smithy_types::Number::Float(f) => serde_json::Number::from_f64(*f)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        },
        Document::String(s) => Value::String(s.clone()),
        Document::Array(a) => Value::Array(a.iter().map(document_to_json).collect()),
        Document::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), document_to_json(&o[k])))
                    .collect(),
            )
        }
    }
}

fn describe<E: std::error::Error + 'static>(e: &E) -> String {
    let mut parts = vec![e.to_string()];
    let mut src = e.source();
    while let Some(s) = src {
        let m = s.to_string();
        if !parts.contains(&m) {
            parts.push(m);
        }
        src = s.source();
    }
    parts.join(": ")
}

/// Codes for credentials AWS no longer accepts: a session token past its
/// expiry, or keys it does not recognise. A fresh read of the profile may
/// hold new ones.
const STALE_CREDENTIAL_CODES: [&str; 3] = [
    "ExpiredTokenException",
    "ExpiredToken",
    "UnrecognizedClientException",
];

fn is_stale_credential(e: &RiddleError) -> bool {
    e.detail
        .as_ref()
        .and_then(|d| d.get("staleCredential"))
        .and_then(|v| v.as_bool())
        == Some(true)
}

/// Map an SDK error to the taxonomy. Throttling and transient service
/// errors are retryable; access, validation and unknown-model errors are not.
pub fn map_sdk_error<E, R>(
    err: aws_smithy_runtime_api::client::result::SdkError<E, R>,
) -> RiddleError
where
    E: std::error::Error + aws_smithy_types::error::metadata::ProvideErrorMetadata + 'static,
    R: std::fmt::Debug + 'static,
{
    let code = err
        .as_service_error()
        .and_then(|s| s.code())
        .map(str::to_string);
    let message = describe(&err);
    rdebug!(
        Category::Llm,
        "bedrock error code {}",
        code.as_deref().unwrap_or("none")
    );
    let stale = code
        .as_deref()
        .is_some_and(|c| STALE_CREDENTIAL_CODES.contains(&c));
    if stale {
        let advice = "Bedrock refused the AWS credentials as expired or unknown. Riddle reloads them from ~/.aws on the next try; if this persists, refresh the profile's credentials.";
        return RiddleError::new(
            ErrorKind::LlmUnavailable,
            format!("bedrock: {advice} ({message})"),
        )
        .with_detail(
            serde_json::json!({"retryable": true, "code": code, "staleCredential": true}),
        );
    }
    let (kind, retryable) = match code.as_deref() {
        Some("ThrottlingException") => (ErrorKind::ProviderRateLimited, true),
        Some("ServiceUnavailableException")
        | Some("ModelNotReadyException")
        | Some("ModelTimeoutException")
        | Some("InternalServerException") => (ErrorKind::LlmUnavailable, true),
        Some("AccessDeniedException")
        | Some("ResourceNotFoundException")
        | Some("ModelErrorException") => (ErrorKind::LlmUnavailable, false),
        Some("ValidationException") => (ErrorKind::Internal, false),
        Some(_) => (ErrorKind::LlmUnavailable, false),
        // Dispatch, timeout, construction: the network or the credential chain.
        None => (ErrorKind::LlmUnavailable, true),
    };
    RiddleError::new(kind, format!("bedrock: {message}"))
        .with_detail(serde_json::json!({"retryable": retryable, "code": code}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ToolSpec;
    use riddle_core::PrivacyMode;
    use serde_json::json;

    #[test]
    fn the_wire_form_wraps_untrusted_results_merges_roles_and_places_cache_points() {
        let req = LlmRequest::new(
            "sys",
            vec![
                LlmMessage::user("q"),
                LlmMessage::assistant(vec![ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "search".into(),
                    input: json!({"q": "x"}),
                }]),
                LlmMessage::user_blocks(vec![ContentBlock::tool_result(
                    "t1",
                    "subject: hello",
                    true,
                )]),
                LlmMessage::user_blocks(vec![ContentBlock::tool_result(
                    "t2",
                    "{\"ok\":true}",
                    false,
                )]),
            ],
        )
        .with_tools(vec![ToolSpec::new(
            "search",
            "search mail",
            json!({"type":"object"}),
        )])
        .with_cache_hint(true);
        let (system, messages, tools) = build_wire(&req).unwrap();
        assert_eq!(system.len(), 2, "system text plus cache point");
        assert_eq!(messages.len(), 3, "the two user messages merged");
        let last = &messages[2];
        assert_eq!(last.role(), &bt::ConversationRole::User);
        let wrapped = last.content().iter().any(|c| match c {
            bt::ContentBlock::ToolResult(r) => r.content().iter().any(
                |x| matches!(x, bt::ToolResultContentBlock::Text(t) if t.contains(untrusted::OPEN)),
            ),
            _ => false,
        });
        assert!(wrapped);
        let plain = last.content().iter().any(|c| match c {
            bt::ContentBlock::ToolResult(r) => r.tool_use_id() == "t2" && r.content().iter().any(|x| matches!(x, bt::ToolResultContentBlock::Text(t) if !t.contains(untrusted::OPEN))),
            _ => false,
        });
        assert!(plain);
        assert!(matches!(
            last.content().last(),
            Some(bt::ContentBlock::CachePoint(_))
        ));
        let tools = tools.unwrap();
        assert_eq!(tools.tools().len(), 2, "tool spec plus cache point");
    }

    #[test]
    fn a_request_without_tools_sends_its_tool_history_as_text() {
        let req = LlmRequest::new(
            "sys",
            vec![
                LlmMessage::user("q"),
                LlmMessage::assistant(vec![ContentBlock::ToolUse {
                    id: "t1".into(),
                    name: "propose_mesh".into(),
                    input: json!({"text": "Mesh: Charity"}),
                }]),
                LlmMessage::user_blocks(vec![
                    ContentBlock::tool_result("t1", "subject: hello", true),
                    ContentBlock::text("Tell the user what you proposed."),
                ]),
            ],
        );
        let (_, messages, tools) = build_wire(&req).unwrap();
        assert!(tools.is_none(), "no tools offered, so no tool config");
        let blocks: Vec<&bt::ContentBlock> = messages.iter().flat_map(|m| m.content()).collect();
        assert!(
            blocks
                .iter()
                .all(|b| matches!(b, bt::ContentBlock::Text(_))),
            "every block is text: Converse refuses tool blocks without a tool config"
        );
        let texts: Vec<&str> = blocks
            .iter()
            .filter_map(|b| match b {
                bt::ContentBlock::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t.contains("called propose_mesh (t1)")));
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("[t1: ") && t.contains(untrusted::OPEN)),
            "the result keeps its untrusted wrapper: {texts:?}"
        );
    }

    #[test]
    fn json_round_trips_through_smithy_documents() {
        let v = json!({"a": 1, "b": -2, "c": 1.5, "d": [true, null, "s"], "e": {"nested": {}}});
        assert_eq!(document_to_json(&json_to_document(&v)), v);
    }

    #[tokio::test]
    async fn a_local_only_request_never_builds_a_client() {
        let llm = BedrockLlm::new(BedrockConfig::default(), Arc::new(ModelCatalog::default()));
        let req =
            LlmRequest::new("s", vec![LlmMessage::user("u")]).with_privacy(PrivacyMode::LocalOnly);
        let e = llm.complete(req).await.unwrap_err();
        assert_eq!(e.kind, ErrorKind::PolicyDenied);
        assert!(llm.cached_client().is_none());
    }

    #[test]
    fn refused_credentials_drop_the_client_so_the_retry_reads_them_again() {
        let llm = BedrockLlm::new(BedrockConfig::default(), Arc::new(ModelCatalog::default()));
        let offline = aws_sdk_bedrockruntime::Config::builder()
            .behavior_version(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new("us-east-1"))
            .build();
        let keep = |llm: &BedrockLlm| {
            *llm.cached_client() = Some(aws_sdk_bedrockruntime::Client::from_conf(offline.clone()))
        };
        keep(&llm);
        let throttled = RiddleError::new(ErrorKind::ProviderRateLimited, "slow down")
            .with_detail(json!({"retryable": true, "code": "ThrottlingException"}));
        llm.forget_stale_client(throttled);
        assert!(
            llm.cached_client().is_some(),
            "other failures keep the client"
        );

        let expired = RiddleError::new(ErrorKind::LlmUnavailable, "expired").with_detail(
            json!({"retryable": true, "code": "ExpiredTokenException", "staleCredential": true}),
        );
        let e = llm.forget_stale_client(expired);
        assert!(llm.cached_client().is_none());
        assert!(crate::retry::is_retryable(&e), "the next attempt runs");
    }

    #[test]
    fn stop_reasons_map() {
        assert_eq!(map_stop(&bt::StopReason::ToolUse), StopReason::ToolUse);
        assert_eq!(map_stop(&bt::StopReason::MaxTokens), StopReason::MaxTokens);
        assert_eq!(
            map_stop(&bt::StopReason::GuardrailIntervened),
            StopReason::ContentFiltered
        );
    }

    /// Live: proves the `dev` profile can call Haiku 4.5 through Converse
    /// with a tool, and that the response has the documented shape (text or
    /// tool use, token counts, a priced cost). Run with
    /// `cargo test -p riddle-ai --release -- --ignored live_bedrock`.
    #[tokio::test]
    #[ignore]
    async fn live_bedrock_haiku_answers_with_the_documented_shape() {
        let usage = Arc::new(crate::usage::RecordingUsage::default());
        let llm = BedrockLlm::new(BedrockConfig::default(), Arc::new(ModelCatalog::default()))
            .with_recorder(usage.recorder());
        let req = LlmRequest::new(
            "You are a terse assistant. Use the tool when asked to look something up; otherwise answer in one short sentence.",
            vec![LlmMessage::user("Look up the subject of message m-1.")],
        )
        .with_tools(vec![ToolSpec::new("get_message", "Read one message by id.", json!({"type":"object","properties":{"message_id":{"type":"string"}},"required":["message_id"]}))])
        .with_max_tokens(200)
        .with_task(riddle_core::UsageTask::Probe);
        let resp = llm.complete(req).await.expect("bedrock call");
        assert!(resp.input_tokens > 0 && resp.output_tokens > 0);
        assert!(resp.cost_usd.is_some(), "Haiku 4.5 is priced");
        assert!(!resp.content.is_empty());
        eprintln!(
            "live bedrock: model={} stop={:?} in={} out={} latency={}ms cost=${:.6} tool_uses={}",
            resp.model_id,
            resp.stop_reason,
            resp.input_tokens,
            resp.output_tokens,
            resp.latency_ms,
            resp.cost_usd.unwrap_or(0.0),
            resp.tool_uses().len()
        );
        assert_eq!(usage.rows().len(), 1);
        assert!(usage.rows()[0].ok);

        // Streaming path, plain text.
        let deltas = Arc::new(parking_lot::Mutex::new(String::new()));
        let d2 = deltas.clone();
        let req = LlmRequest::new(
            "Reply with exactly the word OK.",
            vec![LlmMessage::user("ping")],
        )
        .with_max_tokens(20)
        .with_task(riddle_core::UsageTask::Probe);
        let resp = llm
            .complete_stream(req, Box::new(move |t| d2.lock().push_str(&t)))
            .await
            .expect("stream");
        assert!(!deltas.lock().is_empty());
        assert_eq!(resp.text(), *deltas.lock());
        eprintln!(
            "live bedrock stream: text={:?} latency={}ms cost=${:.6}",
            resp.text(),
            resp.latency_ms,
            resp.cost_usd.unwrap_or(0.0)
        );
    }
}
