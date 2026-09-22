//! The runner: plan, execute step by step with checkpoints, conclude.

use parking_lot::Mutex;
use riddle_actions::Services;
use riddle_agent::{
    tools, LoopBudget, LoopEvent, LoopPrincipal, LoopRequest, LoopStop, ReadScope, ToolLoop,
};
use riddle_ai::{refiner::extract_json, LlmMessage, LlmRequest, PromptName, PromptStore};
use riddle_core::*;
use riddle_errors::{ErrorKind, Result, RiddleError};
use riddle_observability::{rinfo, rwarn, Category, Timed};
use riddle_store::{repo, Stores};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ExpeditionConfig {
    pub model_id: Option<String>,
    pub max_iterations_per_step: u32,
    pub max_tokens_per_call: u32,
    pub max_steps: usize,
    /// Findings turned into Finds per Expedition.
    pub max_finds: usize,
}

impl Default for ExpeditionConfig {
    fn default() -> Self {
        Self {
            model_id: None,
            max_iterations_per_step: 8,
            max_tokens_per_call: 4096,
            max_steps: 8,
            max_finds: 10,
        }
    }
}

/// What `Expedition.checkpoint` holds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    /// Steps completed, in plan order.
    pub completed: u32,
    /// One summary per completed step.
    pub notes: Vec<String>,
    /// Message ids cited so far.
    pub evidence: Vec<String>,
    pub read_untrusted: bool,
}

impl Checkpoint {
    pub fn from_value(v: &Value) -> Self {
        serde_json::from_value(v.clone()).unwrap_or_default()
    }
}

/// Sized handle registered in `Services` so Actions reach the runner without
/// a reference cycle (the integrator keeps the `Arc` alive).
pub struct ExpeditionHandle(pub Weak<ExpeditionRunner>);

impl ExpeditionHandle {
    pub fn runner(&self) -> Result<Arc<ExpeditionRunner>> {
        self.0
            .upgrade()
            .ok_or_else(|| RiddleError::internal("the Expedition runner is gone"))
    }
}

pub struct ExpeditionRunner {
    tool_loop: Arc<ToolLoop>,
    stores: Arc<Stores>,
    events: Arc<dyn EventSink>,
    prompts: Arc<PromptStore>,
    clock: Arc<dyn Clock>,
    config: ExpeditionConfig,
    cancels: Mutex<HashMap<ExpeditionId, Arc<AtomicBool>>>,
}

impl ExpeditionRunner {
    pub fn new(
        tool_loop: Arc<ToolLoop>,
        stores: Arc<Stores>,
        events: Arc<dyn EventSink>,
        prompts: Arc<PromptStore>,
        clock: Arc<dyn Clock>,
        config: ExpeditionConfig,
    ) -> Arc<Self> {
        Arc::new(Self {
            tool_loop,
            stores,
            events,
            prompts,
            clock,
            config,
            cancels: Mutex::new(HashMap::new()),
        })
    }

    /// Make the runner reachable from Actions.
    pub fn register(self: &Arc<Self>, services: &Services) {
        services.insert(Arc::new(ExpeditionHandle(Arc::downgrade(self))));
    }

    pub fn stores(&self) -> Arc<Stores> {
        self.stores.clone()
    }

    /// Validate and persist a new Expedition in `Planned`.
    pub fn create(
        &self,
        goal: &str,
        scope: ExpeditionScope,
        budget: ExpeditionBudget,
    ) -> Result<Expedition> {
        let goal = goal.trim();
        if goal.is_empty() {
            return Err(RiddleError::invalid("an Expedition needs a goal"));
        }
        if goal.chars().count() > 2000 {
            return Err(RiddleError::invalid(
                "the goal is too long (2000 characters)",
            ));
        }
        if budget.cost_usd <= 0.0 || budget.seconds == 0 || budget.tool_calls == 0 {
            return Err(RiddleError::invalid(
                "the budget needs a positive cost, time and tool-call limit",
            ));
        }
        if let (Some(s), Some(u)) = (&scope.since, &scope.until) {
            if s > u {
                return Err(RiddleError::invalid("scope.since is after scope.until"));
            }
        }
        let index = self.stores.index();
        let digs = repo::digs::list(&index).unwrap_or_default();
        for id in &scope.dig_ids {
            let d = digs.iter().find(|d| &d.id == id).ok_or_else(|| {
                RiddleError::not_found(format!(
                    "no Dig {}",
                    riddle_observability::redact(id.as_str())
                ))
            })?;
            if !riddle_privacy::allowed(d.privacy, riddle_privacy::Purpose::Llm) {
                return Err(RiddleError::policy_denied(format!(
                    "Dig {} does not allow cloud reasoning",
                    d.name
                )));
            }
        }
        let now = self.clock.now();
        let e = Expedition {
            id: ExpeditionId::new(),
            goal: goal.to_string(),
            scope,
            budget,
            spent: ExpeditionBudget::default(),
            status: ExpeditionStatus::Planned,
            plan: vec![],
            checkpoint: json!(Checkpoint::default()),
            evidence: vec![],
            findings: vec![],
            error: None,
            created_at: now,
            updated_at: now,
            finished_at: None,
        };
        repo::expeditions::upsert(&index, &e)?;
        self.events.emit(RiddleEvent::ExpeditionChanged {
            expedition_id: e.id.clone(),
        });
        rinfo!(
            Category::Expedition,
            "created expedition {} with budget ${:.2}/{}s/{} calls",
            riddle_observability::redact(e.id.as_str()),
            e.budget.cost_usd,
            e.budget.seconds,
            e.budget.tool_calls
        );
        Ok(e)
    }

    /// Run on a spawned task.
    pub fn spawn(self: &Arc<Self>, id: ExpeditionId) {
        let me = self.clone();
        tokio::spawn(async move {
            if let Err(e) = me.run(&id).await {
                rwarn!(
                    Category::Expedition,
                    "expedition {} ended with {}",
                    riddle_observability::redact(id.as_str()),
                    e.kind.as_str()
                );
            }
        });
    }

    /// Request cancellation. A running Expedition stops after its current
    /// tool call; one that is not running is marked cancelled at once.
    pub fn cancel(&self, id: &ExpeditionId) -> Result<Expedition> {
        let index = self.stores.index();
        let mut e = repo::expeditions::get(&index, id)?
            .ok_or_else(|| RiddleError::not_found("no such Expedition"))?;
        if matches!(
            e.status,
            ExpeditionStatus::Completed
                | ExpeditionStatus::Partial
                | ExpeditionStatus::Cancelled
                | ExpeditionStatus::Failed
        ) {
            return Err(RiddleError::conflict(
                format!("the Expedition is already {:?}", e.status).to_lowercase(),
            ));
        }
        let running = self.cancels.lock().get(id).cloned();
        match running {
            Some(flag) => {
                flag.store(true, Ordering::SeqCst);
                rinfo!(
                    Category::Expedition,
                    "cancellation requested for running expedition {}",
                    riddle_observability::redact(id.as_str())
                );
            }
            None => {
                e.status = ExpeditionStatus::Cancelled;
                e.finished_at = Some(self.clock.now());
                e.updated_at = self.clock.now();
                repo::expeditions::upsert(&index, &e)?;
                self.events.emit(RiddleEvent::ExpeditionChanged {
                    expedition_id: id.clone(),
                });
            }
        }
        Ok(e)
    }

    /// Resume everything that was running or planned when the app stopped.
    pub fn resume_all(self: &Arc<Self>) -> Result<Vec<ExpeditionId>> {
        let list = repo::expeditions::resumable(&self.stores.index())?;
        let ids: Vec<ExpeditionId> = list.iter().map(|e| e.id.clone()).collect();
        for id in &ids {
            self.spawn(id.clone());
        }
        if !ids.is_empty() {
            rinfo!(Category::Expedition, "resuming {} expeditions", ids.len());
        }
        Ok(ids)
    }

    /// Continue from the checkpoint; the same as `run`.
    pub async fn resume(&self, id: &ExpeditionId) -> Result<Expedition> {
        self.run(id).await
    }

    /// Plan (if not yet planned), execute the remaining steps, conclude.
    pub async fn run(&self, id: &ExpeditionId) -> Result<Expedition> {
        let _t = Timed::start(
            Category::Expedition,
            format!("expedition {}", riddle_observability::redact(id.as_str())),
            Duration::from_secs(600),
        );
        let index = self.stores.index();
        let mut e = repo::expeditions::get(&index, id)?
            .ok_or_else(|| RiddleError::not_found("no such Expedition"))?;
        if matches!(
            e.status,
            ExpeditionStatus::Completed
                | ExpeditionStatus::Partial
                | ExpeditionStatus::Cancelled
                | ExpeditionStatus::Failed
        ) {
            return Ok(e);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut c = self.cancels.lock();
            if c.contains_key(id) {
                return Err(RiddleError::conflict("that Expedition is already running"));
            }
            c.insert(id.clone(), cancel.clone());
        }
        let result = self.execute(&mut e, cancel).await;
        self.cancels.lock().remove(id);
        match result {
            Ok(()) => {}
            Err(err) => {
                e.status = ExpeditionStatus::Failed;
                e.error = Some(err.message.clone());
                e.finished_at = Some(self.clock.now());
                self.persist(&mut e)?;
                return Err(err);
            }
        }
        Ok(e)
    }

    fn persist(&self, e: &mut Expedition) -> Result<()> {
        e.updated_at = self.clock.now();
        repo::expeditions::upsert(&self.stores.index(), e)?;
        self.events.emit(RiddleEvent::ExpeditionChanged {
            expedition_id: e.id.clone(),
        });
        Ok(())
    }

    fn privacy_and_scope(&self, e: &Expedition) -> Result<(PrivacyMode, ReadScope)> {
        let digs = repo::digs::list(&self.stores.index()).unwrap_or_default();
        let chosen: Vec<&Dig> = if e.scope.dig_ids.is_empty() {
            digs.iter()
                .filter(|d| riddle_privacy::allowed(d.privacy, riddle_privacy::Purpose::Llm))
                .collect()
        } else {
            digs.iter()
                .filter(|d| e.scope.dig_ids.contains(&d.id))
                .collect()
        };
        let privacy = riddle_privacy::most_restrictive(chosen.iter().map(|d| d.privacy));
        if !riddle_privacy::allowed(privacy, riddle_privacy::Purpose::Llm) {
            return Err(RiddleError::policy_denied(
                "a Dig in scope does not allow cloud reasoning",
            ));
        }
        Ok((
            privacy,
            ReadScope {
                dig_ids: chosen.iter().map(|d| d.id.clone()).collect(),
                allow_contamination: false,
            },
        ))
    }

    fn system_prompt(&self, e: &Expedition, metas: &[ActionMeta]) -> riddle_ai::Prompt {
        let plan = if e.plan.is_empty() {
            "(not yet planned)".to_string()
        } else {
            e.plan
                .iter()
                .map(|s| {
                    format!(
                        "{}. [{}] {}",
                        s.n,
                        if s.done { "done" } else { "todo" },
                        s.text
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let scope = format!(
            "Digs {}; since {}; until {}; tools allowed beyond reads: {}",
            if e.scope.dig_ids.is_empty() {
                "all permitted".to_string()
            } else {
                e.scope
                    .dig_ids
                    .iter()
                    .map(|d| d.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            e.scope
                .since
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "any".into()),
            e.scope
                .until
                .map(|t| t.to_rfc3339())
                .unwrap_or_else(|| "any".into()),
            if e.scope.tools_allowed.is_empty() {
                "none".to_string()
            } else {
                e.scope.tools_allowed.join(", ")
            }
        );
        let budget = format!(
            "cost ${:.2} (spent ${:.2}), {} seconds (spent {}), {} tool calls (spent {})",
            e.budget.cost_usd,
            e.spent.cost_usd,
            e.budget.seconds,
            e.spent.seconds,
            e.budget.tool_calls,
            e.spent.tool_calls
        );
        let allowed: Vec<ActionMeta> = metas
            .iter()
            .filter(|m| m.read_only || e.scope.tools_allowed.contains(&m.name))
            .cloned()
            .collect();
        self.prompts.render(
            PromptName::Expedition,
            &[
                ("today", &self.clock.now().format("%A %Y-%m-%d").to_string()),
                ("goal", &e.goal),
                ("scope", &scope),
                ("budget", &budget),
                ("plan", &plan),
                ("tools", &tools::describe_tools(&allowed)),
            ],
        )
    }

    fn tool_list(&self, e: &Expedition, metas: &[ActionMeta]) -> Vec<riddle_ai::ToolSpec> {
        let mut v: Vec<riddle_ai::ToolSpec> =
            tools::ReadTool::ALL.iter().map(|t| t.spec()).collect();
        let read_only: Vec<ActionMeta> = metas.iter().filter(|m| m.read_only).cloned().collect();
        v.extend(tools::action_tools(&read_only, None));
        if !e.scope.tools_allowed.is_empty() {
            let extra: Vec<ActionMeta> = metas.iter().filter(|m| !m.read_only).cloned().collect();
            v.extend(tools::action_tools(&extra, Some(&e.scope.tools_allowed)));
        }
        v
    }

    /// One model call producing a numbered plan.
    pub async fn plan(&self, e: &Expedition) -> Result<Vec<PlanStep>> {
        let (privacy, _) = self.privacy_and_scope(e)?;
        let metas = self.tool_loop.runtime.list_actions();
        let prompt = self.system_prompt(e, &metas);
        let mut req = LlmRequest::new(
            prompt.text,
            vec![LlmMessage::user(
                "Plan this Expedition. Respond with the planning JSON only.",
            )],
        )
        .with_task(UsageTask::Expedition)
        .with_privacy(privacy)
        .with_max_tokens(1024)
        .with_temperature(0.0);
        req.model_id = self.config.model_id.clone();
        req.dig_id = e.scope.dig_ids.first().cloned();
        let resp = self.tool_loop.llm.complete(req).await?;
        let steps = parse_plan(&resp.text(), self.config.max_steps).ok_or_else(|| {
            RiddleError::llm_unavailable("the planner did not return a usable plan")
        })?;
        Ok(steps
            .into_iter()
            .enumerate()
            .map(|(i, text)| PlanStep {
                n: i as u32 + 1,
                text,
                done: false,
            })
            .collect())
    }

    async fn execute(&self, e: &mut Expedition, cancel: Arc<AtomicBool>) -> Result<()> {
        let (privacy, scope) = self.privacy_and_scope(e)?;
        let started = Instant::now();
        let seconds_before = e.spent.seconds;
        let mut cp = Checkpoint::from_value(&e.checkpoint);

        if e.plan.is_empty() {
            e.plan = self.plan(e).await?;
            e.status = ExpeditionStatus::Running;
            cp.completed = 0;
            e.checkpoint = json!(cp);
            self.persist(e)?;
        } else if e.status == ExpeditionStatus::Planned {
            e.status = ExpeditionStatus::Running;
            self.persist(e)?;
        }

        let metas = self.tool_loop.runtime.list_actions();
        let tool_specs = self.tool_list(e, &metas);
        let total = e.plan.len();
        let mut budget_hit: Option<String> = None;
        let mut cancelled = false;

        for i in (cp.completed as usize)..total {
            if cancel.load(Ordering::SeqCst) {
                cancelled = true;
                break;
            }
            e.spent.seconds = seconds_before + started.elapsed().as_secs();
            if let Some(reason) = over_budget(e) {
                budget_hit = Some(reason);
                break;
            }
            let step = e.plan[i].clone();
            let prompt = self.system_prompt(e, &metas);
            let notes = if cp.notes.is_empty() {
                "(none yet)".to_string()
            } else {
                cp.notes
                    .iter()
                    .enumerate()
                    .map(|(n, s)| format!("Step {}: {}", n + 1, s))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let user = format!(
                "Evidence established so far:\n{notes}\n\nCurrent step {} of {total}: {}\n\nDo this step with the tools, then end with a factual summary of what you established for it, citing message ids.",
                step.n, step.text
            );
            let remaining_secs = e.budget.seconds.saturating_sub(e.spent.seconds);
            let req = LoopRequest {
                system: prompt.text,
                messages: vec![LlmMessage::user(user)],
                tools: tool_specs.clone(),
                principal: LoopPrincipal::Expedition(e.id.clone()),
                scope: scope.clone(),
                privacy,
                dig_id: e.scope.dig_ids.first().cloned(),
                model_id: self.config.model_id.clone(),
                budget: LoopBudget {
                    max_iterations: self.config.max_iterations_per_step,
                    max_tool_calls: Some(e.budget.tool_calls.saturating_sub(e.spent.tool_calls)),
                    max_cost_usd: Some((e.budget.cost_usd - e.spent.cost_usd).max(0.0)),
                    max_tokens: None,
                    deadline: Some(Instant::now() + Duration::from_secs(remaining_secs.max(1))),
                },
                read_untrusted: cp.read_untrusted,
                cancel: Some(cancel.clone()),
                max_tokens_per_call: self.config.max_tokens_per_call,
            };
            let events = self.events.clone();
            let eid = e.id.clone();
            let mut on_event = move |ev: LoopEvent<'_>| {
                if let LoopEvent::ToolCall(_) = ev {
                    events.emit(RiddleEvent::ExpeditionChanged {
                        expedition_id: eid.clone(),
                    });
                }
            };
            let outcome = self.tool_loop.run(req, &mut on_event).await?;

            e.spent.tool_calls += outcome.tool_calls.len() as u32;
            e.spent.cost_usd += outcome.cost_usd.unwrap_or(0.0);
            e.spent.seconds = seconds_before + started.elapsed().as_secs();
            cp.read_untrusted |= outcome.read_untrusted;
            for c in &outcome.tool_calls {
                for id in message_ids_in(&c.args) {
                    if !cp.evidence.contains(&id) {
                        cp.evidence.push(id);
                    }
                }
            }
            cp.notes.push(if outcome.text.trim().is_empty() {
                "(no summary)".into()
            } else {
                outcome.text.trim().to_string()
            });
            cp.completed = i as u32 + 1;
            e.plan[i].done = true;
            e.checkpoint = json!(cp);
            e.evidence = cp
                .evidence
                .iter()
                .map(|m| EntityRef::new(EntityKind::Message, m))
                .collect();
            self.persist(e)?;
            rinfo!(
                Category::Expedition,
                "step {}/{total} done: {} tool calls, stop {:?}",
                i + 1,
                outcome.tool_calls.len(),
                outcome.stop
            );

            match outcome.stop {
                LoopStop::Cancelled => {
                    cancelled = true;
                    break;
                }
                LoopStop::BudgetExhausted(r) => {
                    budget_hit = Some(r);
                    break;
                }
                _ => {}
            }
        }

        if cancelled {
            e.status = ExpeditionStatus::Cancelled;
            e.finished_at = Some(self.clock.now());
            self.persist(e)?;
            return Ok(());
        }

        // Conclude from the notes when anything was established.
        if cp.completed > 0 {
            match self.conclude(e, &cp, privacy).await {
                Ok((findings, cited)) => {
                    e.findings = findings;
                    for id in cited {
                        if !cp.evidence.contains(&id) {
                            cp.evidence.push(id);
                        }
                    }
                    e.evidence = cp
                        .evidence
                        .iter()
                        .map(|m| EntityRef::new(EntityKind::Message, m))
                        .collect();
                    e.checkpoint = json!(cp);
                }
                Err(err) => {
                    rwarn!(
                        Category::Expedition,
                        "conclusion failed: {}",
                        err.kind.as_str()
                    );
                    e.findings = cp.notes.clone();
                }
            }
        }
        e.status = if budget_hit.is_some() || (cp.completed as usize) < total {
            ExpeditionStatus::Partial
        } else {
            ExpeditionStatus::Completed
        };
        if let Some(r) = &budget_hit {
            e.error = Some(format!("stopped early: {r}"));
        }
        e.finished_at = Some(self.clock.now());
        self.persist(e)?;
        self.create_finds(e).await;
        Ok(())
    }

    async fn conclude(
        &self,
        e: &Expedition,
        cp: &Checkpoint,
        privacy: PrivacyMode,
    ) -> Result<(Vec<String>, Vec<String>)> {
        let metas = self.tool_loop.runtime.list_actions();
        let prompt = self.system_prompt(e, &metas);
        let notes = cp
            .notes
            .iter()
            .enumerate()
            .map(|(n, s)| format!("Step {}: {}", n + 1, s))
            .collect::<Vec<_>>()
            .join("\n");
        let mut req = LlmRequest::new(prompt.text, vec![LlmMessage::user(format!("Evidence established:\n{notes}\n\nConclude the Expedition. Respond with the findings JSON only."))])
            .with_task(UsageTask::Expedition)
            .with_privacy(privacy)
            .with_max_tokens(2048)
            .with_temperature(0.0);
        req.model_id = self.config.model_id.clone();
        req.dig_id = e.scope.dig_ids.first().cloned();
        let resp = self.tool_loop.llm.complete(req).await?;
        let text = resp.text();
        Ok(parse_findings(&text))
    }

    /// Findings become Finds through the `create_find` Action when it is
    /// registered; otherwise they stay on the Expedition.
    async fn create_finds(&self, e: &Expedition) {
        if e.findings.is_empty() || self.tool_loop.runtime.action("create_find").is_err() {
            return;
        }
        let sources: Vec<Value> = e
            .evidence
            .iter()
            .filter(|r| r.kind == EntityKind::Message)
            .take(20)
            .map(|r| json!({"messageId": r.id}))
            .collect();
        for (i, finding) in e.findings.iter().take(self.config.max_finds).enumerate() {
            let inv = ActionInvocation {
                action: "create_find".into(),
                args: json!({
                    "findType": "know",
                    "title": title_of(finding),
                    "summary": finding,
                    "sources": sources,
                    "reason": format!("Expedition: {}", e.goal),
                    "identityKey": format!("expedition:{}:{}", e.id, i),
                    "expeditionId": e.id,
                }),
                principal: Principal::Expedition {
                    expedition_id: e.id.clone(),
                    read_untrusted: Checkpoint::from_value(&e.checkpoint).read_untrusted,
                },
                dig_id: e.scope.dig_ids.first().cloned(),
                idempotency_key: Some(format!("expedition-find:{}:{}", e.id, i)),
                confirmed: false,
                pending_id: None,
            };
            match self.tool_loop.runtime.invoke(inv).await {
                Ok(_) => {}
                Err(err) if err.kind == ErrorKind::NeedsConfirmation => {
                    rinfo!(
                        Category::Expedition,
                        "create_find needs confirmation; finding {} stays on the Expedition",
                        i + 1
                    );
                }
                Err(err) => rwarn!(
                    Category::Expedition,
                    "create_find failed for finding {}: {}",
                    i + 1,
                    err.kind.as_str()
                ),
            }
        }
    }
}

fn over_budget(e: &Expedition) -> Option<String> {
    if e.spent.tool_calls >= e.budget.tool_calls {
        return Some(format!(
            "the budget of {} tool calls is spent",
            e.budget.tool_calls
        ));
    }
    if e.spent.cost_usd >= e.budget.cost_usd {
        return Some(format!(
            "the cost budget of ${:.2} is spent",
            e.budget.cost_usd
        ));
    }
    if e.spent.seconds >= e.budget.seconds {
        return Some(format!(
            "the time budget of {} seconds is spent",
            e.budget.seconds
        ));
    }
    None
}

fn message_ids_in(args: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(s) = args.get("messageId").and_then(|v| v.as_str()) {
        out.push(s.to_string());
    }
    if let Some(a) = args.get("messageIds").and_then(|v| v.as_array()) {
        out.extend(a.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()));
    }
    out
}

fn title_of(finding: &str) -> String {
    let t: String = finding.chars().take(80).collect();
    if finding.chars().count() > 80 {
        format!("{t}…")
    } else {
        t
    }
}

/// `{"steps": [...]}` or numbered lines; two to `max` steps.
pub fn parse_plan(text: &str, max: usize) -> Option<Vec<String>> {
    let mut steps: Vec<String> =
        match extract_json(text).and_then(|v| v.get("steps").and_then(|s| s.as_array()).cloned()) {
            Some(arr) => arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect(),
            None => text
                .lines()
                .filter_map(|l| {
                    let l = l.trim();
                    let rest = l
                        .trim_start_matches(|c: char| c.is_ascii_digit())
                        .trim_start_matches(['.', ')', ':'])
                        .trim();
                    if l.chars().next().is_some_and(|c| c.is_ascii_digit()) && !rest.is_empty() {
                        Some(rest.to_string())
                    } else {
                        None
                    }
                })
                .collect(),
        };
    steps.truncate(max);
    if steps.len() < 2 {
        None
    } else {
        Some(steps)
    }
}

/// `{"findings": [...], "evidence": [...]}`; falls back to the text as one finding.
pub fn parse_findings(text: &str) -> (Vec<String>, Vec<String>) {
    if let Some(v) = extract_json(text) {
        let findings: Vec<String> = v
            .get("findings")
            .and_then(|f| f.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let evidence: Vec<String> = v
            .get("evidence")
            .and_then(|f| f.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if !findings.is_empty() {
            return (findings, evidence);
        }
    }
    let t = text.trim();
    if t.is_empty() {
        (vec![], vec![])
    } else {
        (vec![t.to_string()], vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_parse_from_json_or_numbered_lines() {
        assert_eq!(
            parse_plan(r#"{"steps":["a","b","c"]}"#, 8).unwrap(),
            vec!["a", "b", "c"]
        );
        assert_eq!(
            parse_plan("1. Find receipts\n2) Compare dates\nnot a step", 8).unwrap(),
            vec!["Find receipts", "Compare dates"]
        );
        assert!(parse_plan("just one line", 8).is_none());
        assert_eq!(
            parse_plan(r#"{"steps":["a","b","c","d"]}"#, 3)
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn findings_parse_with_a_text_fallback() {
        let (f, e) = parse_findings(r#"{"findings":["Adobe charged twice"],"evidence":["d1:9"]}"#);
        assert_eq!(f, vec!["Adobe charged twice"]);
        assert_eq!(e, vec!["d1:9"]);
        let (f, e) = parse_findings("Nothing structured here.");
        assert_eq!(f, vec!["Nothing structured here."]);
        assert!(e.is_empty());
    }
}
