//! The tool loop shared by the Director and Expeditions: call the model,
//! run the tools it asks for, feed the results back, stop when it answers,
//! runs out of budget, is cancelled, or makes a proposal.
//!
//! Security properties enforced here, not in the prompt (§58, invariant 5):
//! - a read tool that returns mail content marks the result untrusted and
//!   sets `read_untrusted` for the rest of the task; every later Action
//!   invocation carries `Principal::{Director,Expedition}{read_untrusted: true}`
//!   so the Action layer approval-gates egress (control 3);
//! - `needs_confirmation` from the runtime becomes a proposal event with the
//!   pending action's own summary and real arguments (control 4), the tools
//!   are withdrawn, and the model gets one more call to tell the user what it
//!   proposed, so it cannot retry or work around the gate;
//! - Contamination in list results is withheld unless the user opened it
//!   (control 2), in addition to whatever the reads implementation refuses;
//! - the model never sees an Action's real arguments differently from the
//!   user: the proposal summary is the Action layer's preview.
//!
//! Budget (§42): iterations, tool calls, tokens, cost and a deadline. When
//! any is exhausted the tools are withdrawn and the model is asked to finish
//! with what it has; the outcome says so.

use crate::reads::{DirectorReads, ReadScope};
use crate::tools::{classify, redact_args, ReadTool, ToolKind};
use riddle_actions::Runtime;
use riddle_ai::{ContentBlock, Llm, LlmMessage, LlmRequest, StopReason, ToolSpec};
use riddle_core::{
    ActionInvocation, Activity, ConversationId, DigId, EventSink, ExpeditionId, MessageId,
    PendingAction, Principal, PrivacyMode, SimulationReport, ThreadId, ToolCall, UsageTask,
};
use riddle_errors::{ErrorKind, Result, RiddleError};
use riddle_observability::{rdebug, rinfo, rwarn, Category};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// The largest tool result handed to the model, in characters.
pub const MAX_RESULT_CHARS: usize = 30_000;

pub struct ToolLoop {
    pub llm: Arc<dyn Llm>,
    pub runtime: Arc<Runtime>,
    pub reads: Arc<dyn DirectorReads>,
    pub events: Arc<dyn EventSink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopPrincipal {
    Director(ConversationId),
    Expedition(ExpeditionId),
}

impl LoopPrincipal {
    pub fn principal(&self, read_untrusted: bool) -> Principal {
        match self {
            LoopPrincipal::Director(id) => Principal::Director {
                conversation_id: id.clone(),
                read_untrusted,
            },
            LoopPrincipal::Expedition(id) => Principal::Expedition {
                expedition_id: id.clone(),
                read_untrusted,
            },
        }
    }
    pub fn task(&self) -> UsageTask {
        match self {
            LoopPrincipal::Director(_) => UsageTask::Director,
            LoopPrincipal::Expedition(_) => UsageTask::Expedition,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoopBudget {
    pub max_iterations: u32,
    pub max_tool_calls: Option<u32>,
    pub max_cost_usd: Option<f64>,
    pub max_tokens: Option<u64>,
    pub deadline: Option<Instant>,
}

impl Default for LoopBudget {
    fn default() -> Self {
        Self {
            max_iterations: 40,
            max_tool_calls: None,
            max_cost_usd: None,
            max_tokens: None,
            deadline: None,
        }
    }
}

pub struct LoopRequest {
    pub system: String,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<ToolSpec>,
    pub principal: LoopPrincipal,
    pub scope: ReadScope,
    pub privacy: PrivacyMode,
    pub dig_id: Option<DigId>,
    pub model_id: Option<String>,
    pub budget: LoopBudget,
    /// Carried over from earlier turns of the same task.
    pub read_untrusted: bool,
    pub cancel: Option<Arc<AtomicBool>>,
    pub max_tokens_per_call: u32,
    /// Ledger attribution when it is not the principal's own (a model
    /// comparison replaying a Director question records as `Comparison`).
    pub task: Option<UsageTask>,
}

impl LoopRequest {
    pub fn new(
        system: impl Into<String>,
        messages: Vec<LlmMessage>,
        tools: Vec<ToolSpec>,
        principal: LoopPrincipal,
    ) -> Self {
        Self {
            system: system.into(),
            messages,
            tools,
            principal,
            scope: ReadScope::default(),
            privacy: PrivacyMode::CloudReasoning,
            dig_id: None,
            model_id: None,
            budget: LoopBudget::default(),
            read_untrusted: false,
            cancel: None,
            max_tokens_per_call: 4096,
            task: None,
        }
    }
}

pub enum LoopEvent<'a> {
    Delta(&'a str),
    ToolCall(&'a ToolCall),
    Proposal {
        pending_id: &'a str,
        summary: &'a str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopStop {
    EndTurn,
    MaxTokens,
    IterationCap,
    BudgetExhausted(String),
    Cancelled,
    Proposal,
    ContentFiltered,
}

#[derive(Debug, Clone)]
pub struct LoopOutcome {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// The priced portion; `unpriced_calls` says how many calls had no price.
    pub cost_usd: Option<f64>,
    pub unpriced_calls: u32,
    pub llm_calls: u32,
    pub read_untrusted: bool,
    pub proposals: Vec<(String, String)>,
    pub stop: LoopStop,
    /// The transcript of this run (for continuation within a task).
    pub messages: Vec<LlmMessage>,
    pub model_id: String,
}

struct State {
    read_untrusted: bool,
    tool_calls: Vec<ToolCall>,
    proposals: Vec<(String, String)>,
    input_tokens: u64,
    output_tokens: u64,
    cost: f64,
    priced: bool,
    unpriced_calls: u32,
    llm_calls: u32,
    model_id: String,
    /// Failures per exact call (tool and arguments). The same call failing
    /// the same way twice is a loop, and ends the turn's tool use sooner
    /// than any round count would.
    failures: std::collections::HashSet<String>,
    repeated_failure: Option<String>,
}

impl ToolLoop {
    pub fn new(
        llm: Arc<dyn Llm>,
        runtime: Arc<Runtime>,
        reads: Arc<dyn DirectorReads>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            llm,
            runtime,
            reads,
            events,
        }
    }

    pub async fn run(
        &self,
        req: LoopRequest,
        on_event: &mut (dyn FnMut(LoopEvent<'_>) + Send),
    ) -> Result<LoopOutcome> {
        let LoopRequest {
            system,
            mut messages,
            mut tools,
            principal,
            scope,
            privacy,
            dig_id,
            model_id,
            budget,
            read_untrusted,
            cancel,
            max_tokens_per_call,
            task: task_override,
        } = req;
        let mut st = State {
            read_untrusted,
            tool_calls: vec![],
            proposals: vec![],
            input_tokens: 0,
            output_tokens: 0,
            cost: 0.0,
            priced: true,
            unpriced_calls: 0,
            llm_calls: 0,
            model_id: model_id
                .clone()
                .unwrap_or_else(|| self.llm.identity().model_id),
            failures: Default::default(),
            repeated_failure: None,
        };
        let mut text = String::new();
        let mut stop = LoopStop::EndTurn;
        let mut pending_stop: Option<LoopStop> = None;
        let task = task_override.unwrap_or_else(|| principal.task());

        for iteration in 0..budget.max_iterations.max(1) {
            if cancel.as_ref().is_some_and(|c| c.load(Ordering::SeqCst)) {
                stop = LoopStop::Cancelled;
                break;
            }
            // Budget: withdraw the tools and ask for a conclusion.
            if !tools.is_empty() {
                if let Some(reason) = exhausted(&budget, &st, iteration) {
                    rinfo!(Category::Director, "tools withdrawn: {reason}");
                    tools.clear();
                    messages.push(LlmMessage::user(format!("Budget note: {reason}. No more tools are available. Answer now with what you have established, and say plainly what remains unverified.")));
                    pending_stop = Some(if iteration + 1 >= budget.max_iterations {
                        LoopStop::IterationCap
                    } else {
                        LoopStop::BudgetExhausted(reason)
                    });
                }
            }

            let mut llm_req = LlmRequest::new(system.clone(), messages.clone())
                .with_tools(tools.clone())
                .with_privacy(privacy)
                .with_task(task)
                .with_dig(dig_id.clone())
                .with_max_tokens(max_tokens_per_call)
                .with_cache_hint(true);
            llm_req.model_id = model_id.clone();

            let resp = self.call_with_cache_fallback(llm_req, on_event).await?;
            st.llm_calls += 1;
            st.input_tokens += resp.input_tokens;
            st.output_tokens += resp.output_tokens;
            match resp.cost_usd {
                Some(c) => st.cost += c,
                None => {
                    st.priced = false;
                    st.unpriced_calls += 1;
                }
            }
            st.model_id = resp.model_id.clone();
            messages.push(LlmMessage::assistant(resp.content.clone()));
            let turn_text = resp.text();
            if !turn_text.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&turn_text);
            }

            let uses: Vec<(String, String, Value)> = resp
                .tool_uses()
                .into_iter()
                .map(|(i, n, a)| (i.to_string(), n.to_string(), a.clone()))
                .collect();
            if resp.stop_reason != StopReason::ToolUse || uses.is_empty() {
                stop = match resp.stop_reason {
                    StopReason::MaxTokens => LoopStop::MaxTokens,
                    StopReason::ContentFiltered => LoopStop::ContentFiltered,
                    _ => pending_stop.take().unwrap_or(LoopStop::EndTurn),
                };
                break;
            }

            // Run the tools, in order.
            let mut results = Vec::with_capacity(uses.len());
            let mut proposed = false;
            for (id, name, args) in uses {
                if tools.is_empty() {
                    results.push(ContentBlock::tool_error(
                        &id,
                        "no tools are available any more; answer with what you have",
                    ));
                    continue;
                }
                let (block, call) = self
                    .dispatch(&principal, &scope, &id, &name, &args, &mut st)
                    .await;
                if call.summary.starts_with("proposed: ") {
                    proposed = true;
                }
                on_event(LoopEvent::ToolCall(&call));
                if let Some((pid, summary)) = st
                    .proposals
                    .last()
                    .filter(|_| call.summary.starts_with("proposed: "))
                {
                    on_event(LoopEvent::Proposal {
                        pending_id: pid,
                        summary,
                    });
                }
                if !call.ok {
                    // A second identical failure is a loop; the first sets the trap.
                    if !st.failures.insert(format!("{name} {args}"))
                        && st.repeated_failure.is_none()
                    {
                        st.repeated_failure = Some(format!(
                            "the same {name} call failed twice ({})",
                            call.summary
                        ));
                    }
                }
                st.tool_calls.push(call);
                results.push(block);
            }
            messages.push(LlmMessage::user_blocks(results));
            if proposed {
                tools.clear();
                messages.push(LlmMessage::user("A proposal now awaits the user's approval. Tell the user in one or two sentences what you proposed and why, then stop. Do not call any tool."));
                pending_stop = Some(LoopStop::Proposal);
            }
            if iteration + 1 >= budget.max_iterations {
                stop = LoopStop::IterationCap;
            }
        }

        Ok(LoopOutcome {
            text,
            tool_calls: st.tool_calls,
            input_tokens: st.input_tokens,
            output_tokens: st.output_tokens,
            cost_usd: if st.llm_calls > 0 && st.unpriced_calls == st.llm_calls {
                None
            } else {
                Some(st.cost)
            },
            unpriced_calls: st.unpriced_calls,
            llm_calls: st.llm_calls,
            read_untrusted: st.read_untrusted,
            proposals: st.proposals,
            stop,
            messages,
            model_id: st.model_id,
        })
    }

    /// Stream the call, forwarding deltas; on a failure with the cache hint
    /// set, retry once without it (§56).
    async fn call_with_cache_fallback(
        &self,
        req: LlmRequest,
        on_event: &mut (dyn FnMut(LoopEvent<'_>) + Send),
    ) -> Result<riddle_ai::LlmResponse> {
        match self.stream(req.clone(), on_event).await {
            Ok(r) => Ok(r),
            Err(e)
                if req.cache_hint
                    && !matches!(
                        e.kind,
                        ErrorKind::PolicyDenied
                            | ErrorKind::BudgetExhausted
                            | ErrorKind::Invalid
                            | ErrorKind::NotFound
                    ) =>
            {
                rwarn!(
                    Category::Director,
                    "model call failed with cache hint ({}); retrying cache-free",
                    e.kind.as_str()
                );
                self.stream(req.with_cache_hint(false), on_event).await
            }
            Err(e) => Err(e),
        }
    }

    async fn stream(
        &self,
        req: LlmRequest,
        on_event: &mut (dyn FnMut(LoopEvent<'_>) + Send),
    ) -> Result<riddle_ai::LlmResponse> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let fut = self.llm.complete_stream(
            req,
            Box::new(move |t| {
                let _ = tx.send(t);
            }),
        );
        tokio::pin!(fut);
        let result = loop {
            tokio::select! {
                r = &mut fut => break r,
                d = rx.recv() => {
                    match d {
                        Some(d) => on_event(LoopEvent::Delta(&d)),
                        None => { let r = (&mut fut).await; break r; }
                    }
                }
            }
        };
        while let Ok(d) = rx.try_recv() {
            on_event(LoopEvent::Delta(&d));
        }
        result
    }

    async fn dispatch(
        &self,
        principal: &LoopPrincipal,
        scope: &ReadScope,
        id: &str,
        name: &str,
        args: &Value,
        st: &mut State,
    ) -> (ContentBlock, ToolCall) {
        let started = Instant::now();
        let redacted = redact_args(args);
        let mut call = ToolCall {
            id: id.to_string(),
            action: name.to_string(),
            args: redacted,
            input: args.clone(),
            output: None,
            ok: false,
            summary: String::new(),
            activity_id: None,
            latency_ms: 0,
            untrusted: false,
        };
        let block = match classify(name) {
            ToolKind::Read(tool) => match self.read(tool, scope, args).await {
                Ok(value) => {
                    let (value, withheld) = if scope.allow_contamination {
                        (value, 0)
                    } else {
                        scrub_contamination(value)
                    };
                    let count = item_count(&value);
                    call.ok = true;
                    call.untrusted = tool.carries_mail();
                    call.summary = match count {
                        Some(n) => format!(
                            "{} returned {n} items{}",
                            tool.name(),
                            if withheld > 0 {
                                format!(" ({withheld} Contamination withheld)")
                            } else {
                                String::new()
                            }
                        ),
                        None => format!("{} ok", tool.name()),
                    };
                    if call.untrusted {
                        st.read_untrusted = true;
                    }
                    call.output = Some(if call.untrusted {
                        mail_receipt(&value)
                    } else {
                        compact(&value)
                    });
                    if tool == ReadTool::SimulateMesh {
                        self.record_preview(principal, &value);
                    }
                    ContentBlock::tool_result(id, fitted(value), call.untrusted)
                }
                Err(e) => {
                    call.summary = format!("{} failed: {}", tool.name(), e.kind.as_str());
                    ContentBlock::tool_error(id, error_json(&e))
                }
            },
            ToolKind::Action(action) => {
                let dig_id = args
                    .get("digId")
                    .and_then(|v| v.as_str())
                    .map(DigId::from)
                    .or_else(|| {
                        if scope.dig_ids.len() == 1 {
                            scope.dig_ids.first().cloned()
                        } else {
                            None
                        }
                    });
                let inv = ActionInvocation {
                    action: action.clone(),
                    args: args.clone(),
                    principal: principal.principal(st.read_untrusted),
                    dig_id,
                    idempotency_key: None,
                    confirmed: false,
                    pending_id: None,
                };
                match self.runtime.invoke(inv).await {
                    Ok(outcome) => {
                        call.ok = true;
                        call.activity_id = outcome.activity_id.clone();
                        call.summary = format!("{action} ok");
                        call.output = Some(compact(&outcome.result));
                        ContentBlock::tool_result(id, fitted(outcome.result.clone()), false)
                    }
                    Err(e) if e.kind == ErrorKind::NeedsConfirmation => {
                        let pending: Option<PendingAction> = e
                            .detail
                            .clone()
                            .and_then(|d| serde_json::from_value(d).ok());
                        let (pid, summary) = match &pending {
                            Some(p) => (p.id.clone(), p.summary.clone()),
                            None => (String::new(), e.message.clone()),
                        };
                        let reason = pending
                            .as_ref()
                            .map(|p| p.reason.clone())
                            .unwrap_or_default();
                        rinfo!(
                            Category::Director,
                            "{action} became a proposal (pending {})",
                            riddle_observability::redact(&pid)
                        );
                        st.proposals.push((pid.clone(), summary.clone()));
                        call.ok = true;
                        call.summary = format!("proposed: {summary}");
                        let result = json!({"proposal": {"pendingId": pid, "summary": summary, "reason": reason, "status": "awaiting the user's approval"}, "note": "The action did not run. The user sees a proposal card with the real arguments. Do not retry."});
                        call.output = Some(result.clone());
                        ContentBlock::tool_result(id, result.to_string(), false)
                    }
                    Err(e) => {
                        call.summary = format!("{action} failed: {}", e.kind.as_str());
                        ContentBlock::tool_error(id, error_json(&e))
                    }
                }
            }
        };
        call.latency_ms = started.elapsed().as_millis() as u64;
        rdebug!(
            Category::Director,
            "tool {} ok={} untrusted={} {}ms",
            call.action,
            call.ok,
            call.untrusted,
            call.latency_ms
        );
        (block, call)
    }

    /// A Mesh preview the Director ran, in Activity under its name and
    /// conversation: the numbers it based a proposal on had been visible
    /// only to the model. Bookkeeping: a failure is logged.
    fn record_preview(&self, principal: &LoopPrincipal, report: &Value) {
        let Ok(r) = serde_json::from_value::<SimulationReport>(report.clone()) else {
            return rwarn!(
                Category::Director,
                "a preview result was not a simulation report"
            );
        };
        let versions = r
            .from_version
            .map_or(String::new(), |a| format!(" v{a} → v{}", r.to_version));
        // The edit's own changes first; mail today's rules re-decide anyway apart.
        let anyway = if r.drift > 0 {
            format!("; {} more would be re-decided anyway", r.drift)
        } else {
            String::new()
        };
        let p = principal.principal(false);
        self.runtime.record_activity(Activity::note(
            p.actor(),
            "simulate_mesh",
            format!("Previewed {}{versions}: the edit changes {} of {} recent messages{anyway}", r.mesh_name, r.edit_changed, r.historical_messages),
            json!({ "meshId": r.mesh_id, "editChanged": r.edit_changed, "drift": r.drift, "changed": r.changed, "historicalMessages": r.historical_messages, "editTransitions": r.edit_transitions }),
            "a preview changes nothing",
            p.conversation_id().cloned(),
            self.runtime.services.clock.now(),
        ));
    }

    async fn read(&self, tool: ReadTool, scope: &ReadScope, args: &Value) -> Result<Value> {
        let s = |k: &str| {
            args.get(k)
                .and_then(|v| v.as_str())
                .map(|v| v.to_string())
                .ok_or_else(|| RiddleError::invalid(format!("{} requires {k}", tool.name())))
        };
        match tool {
            ReadTool::ListMessages => self.reads.list_messages(scope, args.clone()).await,
            ReadTool::GetMessage => {
                self.reads
                    .get_message(
                        scope,
                        &DigId::from(s("digId")?),
                        &MessageId::from(s("messageId")?),
                    )
                    .await
            }
            ReadTool::GetThread => {
                self.reads
                    .get_thread(
                        scope,
                        &DigId::from(s("digId")?),
                        &ThreadId::from(s("threadId")?),
                    )
                    .await
            }
            ReadTool::Search => self.reads.search(scope, args.clone()).await,
            ReadTool::ListFinds => self.reads.list_finds(scope, args.clone()).await,
            ReadTool::ListMeshes => self.reads.list_meshes(scope, args.clone()).await,
            ReadTool::ListSignals => self.reads.list_signals(scope, args.clone()).await,
            ReadTool::ExplainDecision => {
                self.reads
                    .explain(
                        scope,
                        &DigId::from(s("digId")?),
                        &MessageId::from(s("messageId")?),
                    )
                    .await
            }
            ReadTool::PreviewDecision => {
                self.reads
                    .decide_preview(
                        scope,
                        &DigId::from(s("digId")?),
                        &MessageId::from(s("messageId")?),
                    )
                    .await
            }
            ReadTool::ListActivity => self.reads.list_activity(scope, args.clone()).await,
            ReadTool::SimulateMesh => self.reads.simulate_mesh(scope, args.clone()).await,
            ReadTool::MeshCorrections => self.reads.mesh_corrections(scope, args.clone()).await,
        }
    }
}

fn exhausted(budget: &LoopBudget, st: &State, iteration: u32) -> Option<String> {
    if let Some(r) = &st.repeated_failure {
        return Some(r.clone());
    }
    if iteration + 1 >= budget.max_iterations {
        return Some(format!(
            "the turn's limit of {} tool rounds is reached",
            budget.max_iterations
        ));
    }
    if let Some(max) = budget.max_tool_calls {
        if st.tool_calls.len() as u32 >= max {
            return Some(format!("the budget of {max} tool calls is spent"));
        }
    }
    if let Some(max) = budget.max_cost_usd {
        if st.cost >= max {
            return Some(format!("the cost budget of ${max:.2} is spent"));
        }
    }
    if let Some(max) = budget.max_tokens {
        if st.input_tokens + st.output_tokens >= max {
            return Some(format!("the token budget of {max} is spent"));
        }
    }
    if let Some(d) = budget.deadline {
        if Instant::now() >= d {
            return Some("the time budget is spent".into());
        }
    }
    None
}

fn error_json(e: &RiddleError) -> String {
    json!({"error": e.kind.as_str(), "message": e.message}).to_string()
}

/// A result as the model receives it: whole when it fits, otherwise made to
/// fit in ways that stay valid and say what was left out (`fit`).
fn fitted(v: Value) -> String {
    crate::fit::fit(v, MAX_RESULT_CHARS).to_string()
}

/// The size of a compact result kept on a tool call.
const OUTPUT_CHARS: usize = 4_000;

/// A result as kept on the tool call, fitted the way the model's copy is.
fn compact(v: &Value) -> Value {
    crate::fit::fit(v.clone(), OUTPUT_CHARS)
}

/// What a read that carried mail returned, without the mail: how many rows,
/// the total, and each row's ids.
fn mail_receipt(v: &Value) -> Value {
    const IDS: &[&str] = &["id", "digId", "messageId", "threadId", "findId"];
    match crate::fit::rows(v) {
        Some(rows) => json!({
            "count": rows.len(),
            "total": v.get("total"),
            "rows": rows.iter().map(|r| crate::fit::pick(r, IDS)).collect::<Vec<_>>(),
        }),
        None => crate::fit::pick(v, IDS),
    }
}

fn item_count(v: &Value) -> Option<usize> {
    crate::fit::rows(v).map(Vec::len)
}

/// Withhold Contamination-classified entries from any list the Director can
/// see the disposition of (§58 control 2). Returns the scrubbed value and
/// the count withheld.
pub fn scrub_contamination(v: Value) -> (Value, usize) {
    fn is_contamination(o: &serde_json::Map<String, Value>) -> bool {
        o.get("disposition")
            .and_then(|d| d.get("kind"))
            .and_then(|k| k.as_str())
            == Some("contamination")
    }
    fn go(v: Value, n: &mut usize) -> Value {
        match v {
            Value::Array(a) => Value::Array(
                a.into_iter()
                    .map(|x| match x {
                        Value::Object(o) if is_contamination(&o) => {
                            *n += 1;
                            json!({"id": o.get("id").cloned().unwrap_or(Value::Null), "withheld": "contamination: open it in Riddle to let the Director read it"})
                        }
                        other => go(other, n),
                    })
                    .collect(),
            ),
            Value::Object(o) => Value::Object(o.into_iter().map(|(k, x)| (k, go(x, n))).collect()),
            other => other,
        }
    }
    let mut n = 0;
    let out = go(v, &mut n);
    (out, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contamination_rows_are_withheld_from_lists() {
        let page = json!({"items": [
            {"id": "m1", "subject": "hi", "disposition": {"kind": "find", "findType": "respond"}},
            {"id": "m2", "subject": "phish", "disposition": {"kind": "contamination"}}
        ], "total": 2});
        let (out, n) = scrub_contamination(page);
        assert_eq!(n, 1);
        assert_eq!(out["items"][1]["id"], "m2");
        assert!(out["items"][1].get("subject").is_none());
        assert!(out["items"][1]["withheld"]
            .as_str()
            .unwrap()
            .contains("contamination"));
        assert_eq!(out["items"][0]["subject"], "hi");
    }

    #[test]
    fn a_long_result_reaches_the_model_as_valid_json_within_the_limit() {
        let t = fitted(Value::String("x".repeat(MAX_RESULT_CHARS + 10)));
        assert!(t.len() <= MAX_RESULT_CHARS);
        assert!(serde_json::from_str::<Value>(&t).is_ok());
    }
}
