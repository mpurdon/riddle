//! The Director's turn: load the conversation, work out which Digs and which
//! privacy mode apply, render the versioned system prompt, run the tool loop
//! with streaming events, persist the turn, and say when it is done.
//!
//! A turn runs on a spawned task so the `director_send` command returns the
//! conversation id at once; the UI follows `DirectorDelta`, `DirectorToolCall`,
//! `ProposalCreated` and `DirectorDone` (§7). Errors are never swallowed: a
//! failed turn is a Director turn with the error text and a `DirectorDone`
//! carrying it, so the panel never waits on a promise that will not resolve.
//!
//! Privacy (§57): the Director only reasons over Digs whose mode permits
//! cloud reasoning. With no Digs named, every permitted Dig is in scope and
//! the excluded ones are named in the prompt; with a Dig named that does not
//! permit it, the turn is refused before any model call.

use crate::reads::ReadScope;
use crate::tool_loop::{LoopBudget, LoopEvent, LoopOutcome, LoopPrincipal, LoopRequest, ToolLoop};
use crate::tools;
use parking_lot::Mutex;
use riddle_ai::{LlmMessage, PromptName, PromptStore};
use riddle_core::{
    Clock, Conversation, ConversationId, Dig, DigId, EventSink, PrivacyMode, RiddleEvent,
    ToolAllowance, Turn, TurnFailure, TurnRole, UserProfile,
};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rerror, rinfo, rwarn, Category, Timed};
use riddle_store::{repo, Stores};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

/// The round limit under "keep going": a ceiling against a turn that loops
/// forever, far past any turn doing useful work.
const KEEP_GOING_ROUNDS: u32 = 200;

#[derive(Debug, Clone)]
pub struct DirectorConfig {
    /// The Director tier model; `None` uses the `Llm`'s default.
    pub model_id: Option<String>,
    /// Tool rounds per turn. A backstop against a runaway turn, not the
    /// budget: cost and tokens are. At 12 it ended ordinary work (find a
    /// message, explain it, simulate four drafts) mid-proposal.
    pub max_iterations: u32,
    pub max_turn_cost_usd: Option<f64>,
    pub max_turn_tokens: Option<u64>,
    pub max_tokens_per_call: u32,
    /// Prior turns replayed as context (§43: history is context, not state).
    pub history_turns: usize,
}

impl Default for DirectorConfig {
    fn default() -> Self {
        Self {
            model_id: None,
            max_iterations: 40,
            max_turn_cost_usd: Some(0.50),
            max_turn_tokens: Some(150_000),
            max_tokens_per_call: 4096,
            history_turns: 24,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SendRequest {
    pub conversation_id: Option<ConversationId>,
    pub message: String,
    pub dig_ids: Vec<DigId>,
    /// Set by the UI only when the user has opened a Contamination message.
    pub allow_contamination: bool,
}

pub struct Director {
    tool_loop: Arc<ToolLoop>,
    stores: Arc<Stores>,
    events: Arc<dyn EventSink>,
    prompts: Arc<PromptStore>,
    clock: Arc<dyn Clock>,
    config: DirectorConfig,
    in_flight: Mutex<HashSet<ConversationId>>,
}

impl Director {
    pub fn new(
        tool_loop: Arc<ToolLoop>,
        stores: Arc<Stores>,
        events: Arc<dyn EventSink>,
        prompts: Arc<PromptStore>,
        clock: Arc<dyn Clock>,
        config: DirectorConfig,
    ) -> Self {
        Self {
            tool_loop,
            stores,
            events,
            prompts,
            clock,
            config,
            in_flight: Mutex::new(HashSet::new()),
        }
    }

    pub fn config(&self) -> &DirectorConfig {
        &self.config
    }

    pub fn tool_loop(&self) -> Arc<ToolLoop> {
        self.tool_loop.clone()
    }

    /// Record the user's message and start the turn on a spawned task.
    pub fn send(self: &Arc<Self>, req: SendRequest) -> Result<ConversationId> {
        let (id, allow) = self.begin(req)?;
        let me = self.clone();
        let cid = id.clone();
        tokio::spawn(async move {
            me.run_turn(&cid, allow).await;
        });
        Ok(id)
    }

    /// The same as `send`, awaiting the turn; for tests and the CLI.
    pub async fn send_and_wait(self: &Arc<Self>, req: SendRequest) -> Result<ConversationId> {
        let (id, allow) = self.begin(req)?;
        self.run_turn(&id, allow).await;
        Ok(id)
    }

    /// Run the last user message again after a failed turn: the failed
    /// Director turns come off the conversation and the turn starts afresh.
    /// The client calls this when the wait a provider named has passed, or
    /// when the person presses Retry.
    pub fn retry(self: &Arc<Self>, conversation_id: &ConversationId) -> Result<()> {
        self.begin_retry(conversation_id)?;
        let me = self.clone();
        let cid = conversation_id.clone();
        tokio::spawn(async move {
            me.run_turn(&cid, false).await;
        });
        Ok(())
    }

    /// `retry`, awaiting the turn; for tests and the CLI.
    pub async fn retry_and_wait(self: &Arc<Self>, conversation_id: &ConversationId) -> Result<()> {
        self.begin_retry(conversation_id)?;
        self.run_turn(conversation_id, false).await;
        Ok(())
    }

    /// Validate first, then take the in-flight slot only around the write
    /// that commits to the turn, so no error path has a slot to give back.
    fn begin_retry(&self, id: &ConversationId) -> Result<()> {
        let index = self.stores.index();
        let mut conv = repo::conversations::get(&index, id)?
            .ok_or_else(|| RiddleError::not_found("no such conversation"))?;
        while conv
            .turns
            .last()
            .is_some_and(|t| t.role == TurnRole::Director && t.failure.is_some())
        {
            conv.turns.pop();
        }
        if !conv.turns.last().is_some_and(|t| t.role == TurnRole::User) {
            return Err(RiddleError::invalid(
                "nothing to retry: the last turn is not a message of yours that failed",
            ));
        }
        conv.updated_at = self.clock.now();
        let mut busy = self.in_flight.lock();
        if busy.contains(id) {
            return Err(RiddleError::conflict(
                "the Director is still working on that conversation",
            ));
        }
        repo::conversations::upsert(&index, &conv)?;
        busy.insert(id.clone());
        Ok(())
    }

    fn begin(&self, req: SendRequest) -> Result<(ConversationId, bool)> {
        let message = req.message.trim().to_string();
        if message.is_empty() {
            return Err(RiddleError::invalid("say something for the Director to do"));
        }
        let index = self.stores.index();
        let now = self.clock.now();
        let mut conv = match &req.conversation_id {
            Some(id) => repo::conversations::get(&index, id)?
                .ok_or_else(|| RiddleError::not_found("no such conversation"))?,
            None => Conversation {
                id: ConversationId::new(),
                title: title_of(&message),
                turns: vec![],
                read_untrusted: false,
                dig_ids: req.dig_ids.clone(),
                model_id: None,
                prompt_version: None,
                created_at: now,
                updated_at: now,
                allowance: None,
            },
        };
        {
            let mut busy = self.in_flight.lock();
            if busy.contains(&conv.id) {
                return Err(RiddleError::conflict(
                    "the Director is still working on that conversation",
                ));
            }
            busy.insert(conv.id.clone());
        }
        if !req.dig_ids.is_empty() {
            conv.dig_ids = req.dig_ids;
        }
        if let Some(asked) = crate::allowance::parse(&message) {
            conv.allowance = Some(crate::allowance::fold(conv.allowance.take(), asked));
        }
        conv.turns.push(Turn {
            role: TurnRole::User,
            content: message,
            tool_calls: vec![],
            created_at: now,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: None,
            failure: None,
            stopped_early: None,
        });
        conv.updated_at = now;
        if let Err(e) = repo::conversations::upsert(&index, &conv) {
            self.in_flight.lock().remove(&conv.id);
            return Err(e);
        }
        Ok((conv.id, req.allow_contamination))
    }

    /// Run one turn to completion, emitting events and persisting the result.
    pub async fn run_turn(&self, conversation_id: &ConversationId, allow_contamination: bool) {
        let _t = Timed::start(
            Category::Director,
            format!(
                "turn {}",
                riddle_observability::redact(conversation_id.as_str())
            ),
            Duration::from_secs(120),
        );
        let result = self.turn(conversation_id, allow_contamination).await;
        let error = match &result {
            Ok(()) => None,
            Err(e) => {
                rwarn!(Category::Director, "turn failed: {}", e.kind.as_str());
                self.record_failure(conversation_id, e);
                Some(e.message.clone())
            }
        };
        self.in_flight.lock().remove(conversation_id);
        self.events.emit(RiddleEvent::DirectorDone {
            conversation_id: conversation_id.clone(),
            error,
        });
    }

    async fn turn(
        &self,
        conversation_id: &ConversationId,
        allow_contamination: bool,
    ) -> Result<()> {
        let index = self.stores.index();
        let conv = repo::conversations::get(&index, conversation_id)?
            .ok_or_else(|| RiddleError::not_found("no such conversation"))?;
        let all_digs = repo::digs::list_enabled(&index).unwrap_or_default();
        let (digs, excluded) = scope_digs(&all_digs, &conv.dig_ids)?;
        let privacy = riddle_privacy::most_restrictive(digs.iter().map(|d| d.privacy));
        if !riddle_privacy::allowed(privacy, riddle_privacy::Purpose::Llm) {
            let names: Vec<&str> = digs
                .iter()
                .filter(|d| !riddle_privacy::allowed(d.privacy, riddle_privacy::Purpose::Llm))
                .map(|d| d.name.as_str())
                .collect();
            return Err(RiddleError::policy_denied(format!(
                "the Director cannot reason over {}: the Dig's privacy setting does not allow cloud reasoning",
                names.join(", ")
            )));
        }

        let metas = self.tool_loop.runtime.list_actions();
        let profile = repo::user_profile::get(&index).unwrap_or_default();
        let prompt = self.prompts.render(
            PromptName::Director,
            &[
                ("today", &self.clock.now().format("%A %Y-%m-%d").to_string()),
                ("user_profile", &profile_summary(&profile)),
                ("digs", &dig_list(&digs)),
                ("excluded_digs", &excluded_line(&excluded)),
                ("tools", &tools::describe_tools(&metas)),
            ],
        );
        let messages = history(&conv, self.config.history_turns);
        let scope = ReadScope {
            dig_ids: digs.iter().map(|d| d.id.clone()).collect(),
            allow_contamination,
        };
        let req = LoopRequest {
            task: None,
            system: prompt.text,
            messages,
            tools: tools::director_tools(&metas),
            principal: LoopPrincipal::Director(conv.id.clone()),
            scope,
            privacy,
            dig_id: if digs.len() == 1 {
                Some(digs[0].id.clone())
            } else {
                None
            },
            model_id: self.config.model_id.clone(),
            budget: self.budget(conv.allowance.as_ref()),
            read_untrusted: conv.read_untrusted,
            cancel: None,
            max_tokens_per_call: self.config.max_tokens_per_call,
        };

        let events = self.events.clone();
        let cid = conv.id.clone();
        let mut on_event = move |e: LoopEvent<'_>| match e {
            LoopEvent::Delta(text) => events.emit(RiddleEvent::DirectorDelta {
                conversation_id: cid.clone(),
                text: text.to_string(),
            }),
            LoopEvent::ToolCall(call) => events.emit(RiddleEvent::DirectorToolCall {
                conversation_id: cid.clone(),
                call: call.clone(),
            }),
            LoopEvent::Proposal {
                pending_id,
                summary,
            } => events.emit(RiddleEvent::ProposalCreated {
                pending_id: pending_id.to_string(),
                summary: summary.to_string(),
            }),
        };
        let outcome = self.tool_loop.run(req, &mut on_event).await?;
        rinfo!(
            Category::Director,
            "turn done: {} tool calls, {} model calls, {} in, {} out, priced={}, untrusted={}, stop={:?}",
            outcome.tool_calls.len(),
            outcome.llm_calls,
            outcome.input_tokens,
            outcome.output_tokens,
            outcome.cost_usd.is_some(),
            outcome.read_untrusted,
            outcome.stop
        );
        self.record_outcome(&conv.id, outcome, &prompt.version)
    }

    /// The turn's limits, widened by what the person allowed. "Keep going"
    /// lifts the round limit to a ceiling no useful turn reaches; the cost
    /// and token limits hold unless a dollar figure was given.
    /// Answer one question afresh with `model_id`, for a model comparison:
    /// the Director's own prompt over every enabled account, read-only tools
    /// only (a replay can look things up, never propose or change anything),
    /// no conversation recorded, and the spend attributed to `Comparison`.
    pub async fn replay(&self, question: &str, model_id: &str) -> Result<LoopOutcome> {
        let index = self.stores.index();
        let all_digs = repo::digs::list_enabled(&index).unwrap_or_default();
        let (digs, excluded) = scope_digs(&all_digs, &[])?;
        let privacy = riddle_privacy::most_restrictive(digs.iter().map(|d| d.privacy));
        if !riddle_privacy::allowed(privacy, riddle_privacy::Purpose::Llm) {
            return Err(RiddleError::policy_denied(
                "an account's privacy setting does not allow cloud reasoning",
            ));
        }
        let metas: Vec<_> = self
            .tool_loop
            .runtime
            .list_actions()
            .into_iter()
            .filter(|m| m.read_only)
            .collect();
        let profile = repo::user_profile::get(&index).unwrap_or_default();
        let prompt = self.prompts.render(
            PromptName::Director,
            &[
                ("today", &self.clock.now().format("%A %Y-%m-%d").to_string()),
                ("user_profile", &profile_summary(&profile)),
                ("digs", &dig_list(&digs)),
                ("excluded_digs", &excluded_line(&excluded)),
                ("tools", &tools::describe_tools(&metas)),
            ],
        );
        let req = LoopRequest {
            system: prompt.text,
            messages: vec![LlmMessage::user(question.to_string())],
            tools: tools::director_tools(&metas),
            principal: LoopPrincipal::Director(ConversationId::new()),
            scope: ReadScope {
                dig_ids: digs.iter().map(|d| d.id.clone()).collect(),
                allow_contamination: false,
            },
            privacy,
            dig_id: None,
            model_id: Some(model_id.to_string()),
            budget: self.budget(None),
            read_untrusted: false,
            cancel: None,
            max_tokens_per_call: self.config.max_tokens_per_call,
            task: Some(riddle_core::UsageTask::Comparison),
        };
        self.tool_loop.run(req, &mut |_| {}).await
    }

    fn budget(&self, allowance: Option<&ToolAllowance>) -> LoopBudget {
        let a = allowance.cloned().unwrap_or_default();
        let calls = a.next_turn_tool_calls;
        LoopBudget {
            max_iterations: if a.keep_going {
                KEEP_GOING_ROUNDS
            } else {
                // A tool-call allowance needs the rounds to spend it in.
                self.config.max_iterations.max(calls.map_or(0, |n| n + 1))
            },
            max_tool_calls: calls,
            max_cost_usd: a.next_turn_cost_usd.or(self.config.max_turn_cost_usd),
            max_tokens: self.config.max_turn_tokens,
            deadline: None,
        }
    }

    fn record_outcome(
        &self,
        id: &ConversationId,
        outcome: LoopOutcome,
        prompt_version: &str,
    ) -> Result<()> {
        let index = self.stores.index();
        let mut conv = repo::conversations::get(&index, id)?
            .ok_or_else(|| RiddleError::not_found("no such conversation"))?;
        let now = self.clock.now();
        let mut content = outcome.text;
        let mut stopped_early = None;
        match &outcome.stop {
            crate::tool_loop::LoopStop::BudgetExhausted(r) => stopped_early = Some(r.clone()),
            crate::tool_loop::LoopStop::IterationCap => {
                stopped_early = Some("the turn's tool limit is reached".into())
            }
            crate::tool_loop::LoopStop::MaxTokens => {
                content.push_str("\n\n(The answer was cut off at the length limit.)")
            }
            crate::tool_loop::LoopStop::ContentFiltered => {
                content.push_str("\n\n(The model declined to continue.)")
            }
            _ => {}
        }
        conv.turns.push(Turn {
            role: TurnRole::Director,
            content,
            tool_calls: outcome.tool_calls,
            created_at: now,
            input_tokens: outcome.input_tokens,
            output_tokens: outcome.output_tokens,
            cost_usd: outcome.cost_usd,
            failure: None,
            stopped_early,
        });
        // The next-turn part of an allowance is spent by this turn; "keep
        // going" lasts the conversation.
        if let Some(a) = conv.allowance.as_mut() {
            a.next_turn_tool_calls = None;
            a.next_turn_cost_usd = None;
        }
        conv.read_untrusted |= outcome.read_untrusted;
        conv.model_id = Some(outcome.model_id);
        conv.prompt_version = Some(prompt_version.to_string());
        conv.updated_at = now;
        repo::conversations::upsert(&index, &conv)
    }

    fn record_failure(&self, id: &ConversationId, e: &RiddleError) {
        let index = self.stores.index();
        let Ok(Some(mut conv)) = repo::conversations::get(&index, id) else {
            return;
        };
        let now = self.clock.now();
        let retry_after_secs = e
            .detail
            .as_ref()
            .and_then(|d| d.get("retryAfterSecs"))
            .and_then(|v| v.as_u64())
            .map(|n| n.min(u32::MAX as u64) as u32);
        conv.turns.push(Turn {
            role: TurnRole::Director,
            content: format!("I could not complete that: {}", e.message),
            tool_calls: vec![],
            created_at: now,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: None,
            failure: Some(TurnFailure {
                kind: e.kind,
                retry_after_secs,
            }),
            stopped_early: None,
        });
        conv.updated_at = now;
        if let Err(pe) = repo::conversations::upsert(&index, &conv) {
            rerror!(
                Category::Director,
                "could not record failed turn: {}",
                pe.kind.as_str()
            );
        }
    }
}

/// The Digs in scope and the ones excluded by privacy.
fn scope_digs<'a>(all: &'a [Dig], requested: &[DigId]) -> Result<(Vec<&'a Dig>, Vec<&'a Dig>)> {
    if requested.is_empty() {
        let (ok, excluded): (Vec<&Dig>, Vec<&Dig>) = all
            .iter()
            .partition(|d| riddle_privacy::allowed(d.privacy, riddle_privacy::Purpose::Llm));
        return Ok((ok, excluded));
    }
    let mut chosen = Vec::new();
    for id in requested {
        let d = all.iter().find(|d| &d.id == id).ok_or_else(|| {
            RiddleError::not_found(format!(
                "no Dig {}",
                riddle_observability::redact(id.as_str())
            ))
        })?;
        chosen.push(d);
    }
    Ok((chosen, vec![]))
}

fn title_of(message: &str) -> String {
    let first = message.lines().next().unwrap_or("").trim();
    let mut t: String = first.chars().take(60).collect();
    if first.chars().count() > 60 {
        t.push('…');
    }
    if t.is_empty() {
        "Conversation".into()
    } else {
        t
    }
}

fn profile_summary(p: &UserProfile) -> String {
    let mut parts = Vec::new();
    if let Some(n) = &p.name {
        parts.push(format!("name {n}"));
    }
    if let Some(r) = &p.role {
        parts.push(format!("role {r}"));
    }
    if !p.addresses.is_empty() {
        parts.push(format!("addresses {}", p.addresses.join(", ")));
    }
    if !p.organisation_domains.is_empty() {
        parts.push(format!(
            "organisation domains {}",
            p.organisation_domains.join(", ")
        ));
    }
    if !p.active_projects.is_empty() {
        parts.push(format!("active projects {}", p.active_projects.join(", ")));
    }
    if !p.vips.is_empty() {
        parts.push(format!("{} VIP senders", p.vips.len()));
    }
    if let Some(tz) = &p.timezone {
        parts.push(format!("timezone {tz}"));
    }
    if parts.is_empty() {
        "no profile yet".into()
    } else {
        parts.join("; ")
    }
}

fn dig_list(digs: &[&Dig]) -> String {
    if digs.is_empty() {
        return "none (you can still discuss Meshes, Signals and settings)".into();
    }
    digs.iter()
        .map(|d| {
            format!(
                "{} (id {}, {:?}, privacy {:?})",
                d.name, d.id, d.provider, d.privacy
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn excluded_line(excluded: &[&Dig]) -> String {
    if excluded.is_empty() {
        return String::new();
    }
    let names: Vec<String> = excluded
        .iter()
        .map(|d| {
            format!(
                "{} ({})",
                d.name,
                if d.privacy == PrivacyMode::LocalOnly {
                    "local only"
                } else {
                    "cloud judgment only"
                }
            )
        })
        .collect();
    format!("Digs excluded from this conversation by their privacy setting, which you cannot read or change: {}.", names.join(", "))
}

/// Prior turns as model messages: user and Director text only; tool
/// results are not replayed (§44), only a note of which tools were used.
fn history(conv: &Conversation, limit: usize) -> Vec<LlmMessage> {
    let turns: Vec<&Turn> = conv
        .turns
        .iter()
        .filter(|t| t.role != TurnRole::Tool)
        .collect();
    let start = turns.len().saturating_sub(limit.max(1));
    let mut out = Vec::new();
    for t in &turns[start..] {
        // A failed turn is an error the person saw, not something the model said.
        if t.failure.is_some() {
            continue;
        }
        match t.role {
            TurnRole::User => out.push(LlmMessage::user(t.content.clone())),
            TurnRole::Director => {
                let mut text = t.content.clone();
                if !t.tool_calls.is_empty() {
                    let names: Vec<&str> = t.tool_calls.iter().map(|c| c.action.as_str()).collect();
                    text.push_str(&format!("\n(tools used: {})", names.join(", ")));
                }
                if text.trim().is_empty() {
                    text = "(no answer)".into();
                }
                out.push(LlmMessage::assistant(vec![riddle_ai::ContentBlock::text(
                    text,
                )]));
            }
            TurnRole::Tool => {}
        }
    }
    // The transcript must start with the user.
    while out
        .first()
        .is_some_and(|m| m.role == riddle_ai::Role::Assistant)
    {
        out.remove(0);
    }
    if out.is_empty() {
        out.push(LlmMessage::user("(continue)"));
    }
    out
}
