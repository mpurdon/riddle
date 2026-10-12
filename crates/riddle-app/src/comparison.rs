//! Model comparison (Developer → Models): which model suits which job,
//! measured on the person's own mail (`riddle_core::bench`).
//!
//! Escalation is replayed per model through the real pipeline with
//! `force_escalation`, so every candidate is asked even where the judge was
//! sure, and the Mesh routes decide on each model's Signals exactly as Sift
//! would; nothing is persisted. The person's own decisions (corrections, and
//! decisions graded right in Rule check) are the answers it is scored
//! against. The Director is replayed with read-only tools on the person's
//! past questions and compared by the person's pick.
//!
//! Challengers run beside every real escalation when enabled, under one
//! daily cap shared with benchmarks; all comparison spend is recorded as
//! `UsageTask::Comparison` and kept off the accounts' ledgers, so it never
//! eats the Sift's escalation budget. The roster is re-checked for access on
//! a schedule: Bedrock listed Claude 5.5 and GPT-6 Luna weeks before this
//! account could call them, and a model should join the day it can.

use crate::ai::AiServices;
use crate::App;
use riddle_ai::llm::{LlmMessage, LlmRequest};
use riddle_ai::refiner::LlmRefiner;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, rwarn, Category};
use riddle_settings::ComparisonSettings;
use riddle_store::repo;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// The app, for Actions that start work on it (model comparison runs in
/// the background on the app itself). Weak: Services must not keep it alive.
pub struct AppHandle(pub std::sync::Weak<App>);

impl AppHandle {
    pub fn get(&self) -> Result<Arc<App>> {
        self.0
            .upgrade()
            .ok_or_else(|| RiddleError::internal("the app has shut down"))
    }
}

fn app_of(ctx: &riddle_actions::ActionContext) -> Result<Arc<App>> {
    ctx.services.require::<AppHandle>()?.get()
}

fn models_arg(args: &serde_json::Value) -> Vec<String> {
    args.get("models")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// `check_model_access`: ask every roster model for a five-token reply.
pub struct CheckModelAccess;

#[async_trait::async_trait]
impl riddle_actions::Action for CheckModelAccess {
    fn meta(&self) -> ActionMeta {
        riddle_actions::meta(
            "check_model_access",
            "Check which candidate models this account may call",
        )
        .mutating()
        .risk(Risk::Low)
        .autonomy(Autonomy::Act)
        .build()
    }
    async fn execute(
        &self,
        ctx: &riddle_actions::ActionContext,
        _args: serde_json::Value,
    ) -> Result<riddle_actions::ActionResult> {
        let models = app_of(ctx)?.probe_models().await?;
        let usable = models
            .iter()
            .filter(|m| m.access == ModelAccess::Usable)
            .count();
        Ok(riddle_actions::ActionResult::ok(
            serde_json::json!({ "models": models }),
            format!("{usable} of {} models are usable", models.len()),
        ))
    }
}

/// `run_model_benchmark`: every chosen model (or every usable one) on up to
/// `count` messages the person has decided, in the background.
pub struct RunModelBenchmark;

#[async_trait::async_trait]
impl riddle_actions::Action for RunModelBenchmark {
    fn meta(&self) -> ActionMeta {
        riddle_actions::meta(
            "run_model_benchmark",
            "Compare models on messages you have decided",
        )
        .mutating()
        .risk(Risk::Medium)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(
        &self,
        ctx: &riddle_actions::ActionContext,
        args: serde_json::Value,
    ) -> Result<riddle_actions::ActionResult> {
        let count = args
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(20)
            .clamp(1, 200) as usize;
        let n = app_of(ctx)?.start_escalation_benchmark(models_arg(&args), count)?;
        Ok(riddle_actions::ActionResult::ok(
            serde_json::json!({ "messages": n }),
            format!(
                "Comparing models on {}",
                riddle_core::text::plural(n, "message")
            ),
        ))
    }
}

/// `run_director_comparison`: replay past Director questions on each model.
pub struct RunDirectorComparison;

#[async_trait::async_trait]
impl riddle_actions::Action for RunDirectorComparison {
    fn meta(&self) -> ActionMeta {
        riddle_actions::meta(
            "run_director_comparison",
            "Replay past Director questions on each model",
        )
        .mutating()
        .risk(Risk::Medium)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(
        &self,
        ctx: &riddle_actions::ActionContext,
        args: serde_json::Value,
    ) -> Result<riddle_actions::ActionResult> {
        let count = args
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .clamp(1, 50) as usize;
        let n = app_of(ctx)?.start_director_comparison(models_arg(&args), count)?;
        Ok(riddle_actions::ActionResult::ok(
            serde_json::json!({ "questions": n }),
            format!(
                "Replaying {} on each model",
                riddle_core::text::plural(n, "question")
            ),
        ))
    }
}

/// `pick_director_answer`: the person's choice of the better answer.
pub struct PickDirectorAnswer;

#[async_trait::async_trait]
impl riddle_actions::Action for PickDirectorAnswer {
    fn meta(&self) -> ActionMeta {
        riddle_actions::meta(
            "pick_director_answer",
            "Pick the best model answer to a replayed question",
        )
        .mutating()
        .reversible()
        .risk(Risk::Low)
        .autonomy(Autonomy::Trusted)
        .build()
    }
    async fn execute(
        &self,
        ctx: &riddle_actions::ActionContext,
        args: serde_json::Value,
    ) -> Result<riddle_actions::ActionResult> {
        let key = args
            .get("questionKey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RiddleError::invalid("missing argument questionKey"))?;
        let model = args
            .get("modelId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RiddleError::invalid("missing argument modelId"))?;
        let index = ctx.services.stores.index();
        let kv_key = format!("{}{key}", repo::comparison::PICK_PREFIX);
        let prior = repo::kv::get(&index, &kv_key)?;
        repo::kv::set(&index, &kv_key, model)?;
        Ok(riddle_actions::ActionResult::ok(
            serde_json::json!({}),
            format!("Picked {}", model_label(model).0),
        )
        .with_undo(serde_json::json!({ "key": kv_key, "prior": prior }), "Undo"))
    }
    async fn undo(
        &self,
        ctx: &riddle_actions::ActionContext,
        data: serde_json::Value,
    ) -> Result<riddle_actions::ActionResult> {
        let index = ctx.services.stores.index();
        let key = data.get("key").and_then(|v| v.as_str()).unwrap_or_default();
        match data.get("prior").and_then(|v| v.as_str()) {
            Some(p) => repo::kv::set(&index, key, p)?,
            None => repo::kv::delete(&index, key)?,
        }
        Ok(riddle_actions::ActionResult::ok(
            serde_json::json!({}),
            "Pick undone",
        ))
    }
}

pub fn actions() -> Vec<Arc<dyn riddle_actions::Action>> {
    vec![
        Arc::new(CheckModelAccess),
        Arc::new(RunModelBenchmark),
        Arc::new(RunDirectorComparison),
        Arc::new(PickDirectorAnswer),
    ]
}

/// Comparisons running now, for the view's "running" line.
#[derive(Default)]
pub struct ComparisonRuns(AtomicUsize);

struct Running<'a>(&'a ComparisonRuns);
impl<'a> Running<'a> {
    fn start(r: &'a ComparisonRuns) -> Self {
        r.0.fetch_add(1, Ordering::SeqCst);
        Running(r)
    }
}
impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0 .0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// What the access check stores per model (`comparison_model:<id>`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AccessRecord {
    access: ModelAccess,
    detail: Option<String>,
    checked_at: chrono::DateTime<chrono::Utc>,
    latency_ms: Option<u64>,
}

/// A readable name and vendor from a Bedrock model id.
pub fn model_label(model_id: &str) -> (String, String) {
    // After the vendor: `us.anthropic.claude-…`, `global.openai.gpt-5.6-luna`.
    let base = ["anthropic.", "openai."]
        .iter()
        .find_map(|v| model_id.find(v).map(|i| &model_id[i + v.len()..]))
        .unwrap_or(model_id);
    let base = base.split(':').next().unwrap_or(base);
    let vendor = if model_id.contains("anthropic") {
        "Anthropic"
    } else if model_id.contains("openai") {
        "OpenAI"
    } else {
        "Other"
    };
    // `claude-haiku-4-5-20251001-v1` → `Claude Haiku 4.5`; `gpt-6-luna` → `GPT-6 Luna`.
    let words: Vec<&str> = base
        .split('-')
        .filter(|w| !(w.len() == 8 && w.chars().all(|c| c.is_ascii_digit())) && *w != "v1")
        .collect();
    let mut out: Vec<String> = Vec::new();
    let mut version: Vec<&str> = Vec::new();
    for w in words {
        if w.chars().all(|c| c.is_ascii_digit() || c == '.') {
            version.push(w);
        } else {
            if !version.is_empty() {
                out.push(version.join("."));
                version.clear();
            }
            out.push(match w {
                "gpt" => "GPT".to_string(),
                _ => {
                    let mut c = w.chars();
                    c.next()
                        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                        .unwrap_or_default()
                }
            });
        }
    }
    if !version.is_empty() {
        out.push(version.join("."));
    }
    let label = out.join(" ").replace("GPT ", "GPT-");
    (label, vendor.to_string())
}

impl App {
    fn ai(&self) -> Result<Arc<AiServices>> {
        self.services
            .get::<AiServices>()
            .ok_or_else(|| RiddleError::llm_unavailable("no LLM is configured"))
    }

    fn comparison_settings(&self) -> ComparisonSettings {
        self.services
            .get::<riddle_settings::SettingsStore>()
            .map(|s| s.settings().comparison)
            .unwrap_or_default()
    }

    fn comparison_runs(&self) -> Arc<ComparisonRuns> {
        match self.services.get::<ComparisonRuns>() {
            Some(r) => r,
            None => {
                let r = Arc::new(ComparisonRuns::default());
                self.services.insert(r.clone());
                r
            }
        }
    }

    /// What comparisons have spent since midnight UTC.
    pub fn comparison_spent_today(&self) -> f64 {
        let start = self
            .services
            .clock
            .now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|d| d.and_utc())
            .unwrap_or_else(|| self.services.clock.now());
        repo::comparison::spent_since(&self.stores.index(), &start).unwrap_or(0.0)
    }

    fn under_cap(&self) -> bool {
        self.comparison_spent_today() < self.comparison_settings().daily_usd
    }

    /// The roster with what the last access check found, prices from the
    /// catalog, and the jobs each model does now.
    pub fn comparison_models(&self) -> Result<Vec<BenchModel>> {
        let settings = self.comparison_settings();
        let index = self.stores.index();
        let ai = self.ai().ok();
        let llm = self
            .services
            .get::<riddle_settings::SettingsStore>()
            .map(|s| s.settings().llm);
        let mut roster = settings.roster.clone();
        // The models in use are always compared, roster or not.
        if let Some(l) = &llm {
            for m in [&l.escalation_model, &l.director_model] {
                if !roster.contains(m) {
                    roster.insert(0, m.clone());
                }
            }
        }
        let mut out = Vec::new();
        for model_id in roster {
            let rec: Option<AccessRecord> = repo::kv::get_json(
                &index,
                &format!("{}{model_id}", repo::comparison::MODEL_PREFIX),
            )?;
            let price = ai.as_ref().and_then(|a| a.catalog.price(&model_id));
            let (label, vendor) = model_label(&model_id);
            let mut used_for = Vec::new();
            if let Some(l) = &llm {
                if l.escalation_model == model_id {
                    used_for.push(BenchTask::Escalation);
                }
                if l.director_model == model_id {
                    used_for.push(BenchTask::Director);
                }
            }
            out.push(BenchModel {
                model_id,
                label,
                vendor,
                access: rec
                    .as_ref()
                    .map(|r| r.access)
                    .unwrap_or(ModelAccess::Unchecked),
                access_detail: rec.as_ref().and_then(|r| r.detail.clone()),
                checked_at: rec.as_ref().map(|r| r.checked_at),
                probe_latency_ms: rec.as_ref().and_then(|r| r.latency_ms),
                price_in_per_mtok: price.map(|p| p.input_per_mtok),
                price_out_per_mtok: price.map(|p| p.output_per_mtok),
                used_for,
            });
        }
        Ok(out)
    }

    /// Ask every roster model for a five-token reply and record whether the
    /// account may call it. A refusal from a model the provider lists but
    /// has not enabled is `NotEnabled`; the breaker is per model, so a
    /// refused candidate never trips the one Sift relies on.
    pub async fn probe_models(&self) -> Result<Vec<BenchModel>> {
        let ai = self.ai()?;
        let index = self.stores.index();
        for m in self.comparison_models()? {
            let t = Instant::now();
            let mut req = LlmRequest::new(
                "Reply with the single word OK.".to_string(),
                vec![LlmMessage::user("OK?".to_string())],
            )
            .with_task(UsageTask::Probe)
            .with_max_tokens(8)
            .with_temperature(0.0);
            req.model_id = Some(m.model_id.clone());
            let rec = match ai.llm.complete(req).await {
                Ok(_) => AccessRecord {
                    access: ModelAccess::Usable,
                    detail: None,
                    checked_at: self.services.clock.now(),
                    latency_ms: Some(t.elapsed().as_millis() as u64),
                },
                Err(e) => {
                    let not_enabled = e.message.contains("not available for this account")
                        || e.message.contains("AccessDenied")
                        || e.message.contains("don't have access");
                    // An open breaker says nothing new; keep what we knew.
                    if e.message.contains("circuit") && m.access != ModelAccess::Unchecked {
                        continue;
                    }
                    AccessRecord {
                        access: if not_enabled {
                            ModelAccess::NotEnabled
                        } else {
                            ModelAccess::Failing
                        },
                        detail: Some(e.message.chars().take(200).collect()),
                        checked_at: self.services.clock.now(),
                        latency_ms: None,
                    }
                }
            };
            if rec.access != m.access {
                rinfo!(
                    Category::Llm,
                    "model access changed: {} {:?} -> {:?}",
                    m.model_id,
                    m.access,
                    rec.access
                );
            }
            repo::kv::set_json(
                &index,
                &format!("{}{}", repo::comparison::MODEL_PREFIX, m.model_id),
                &rec,
            )?;
        }
        self.comparison_models()
    }

    fn usable_models(&self) -> Vec<String> {
        self.comparison_models()
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.access == ModelAccess::Usable)
            .map(|m| m.model_id)
            .collect()
    }

    /// One model's escalation of one message, through the real pipeline,
    /// recorded and never applied.
    pub async fn compare_escalation(
        &self,
        model_id: &str,
        dig_id: &DigId,
        message_id: &MessageId,
        source: BenchSource,
    ) -> Result<BenchOutput> {
        let ai = self.ai()?;
        let dig = self.dig(dig_id)?;
        let store = self.stores.dig(dig_id)?;
        let message = repo::messages::get(&store, dig_id, message_id)?
            .ok_or_else(|| RiddleError::not_found(format!("message {message_id}")))?;
        let input = self.sift_input(&dig, message)?;
        let refiner = LlmRefiner::new(
            ai.llm.clone(),
            ai.prompts.clone(),
            Some(model_id.to_string()),
        )
        .with_privacy(PrivacyMode::CloudReasoning)
        .with_task(UsageTask::Comparison);
        let mut sift = riddle_sift::Sift::from_services(&self.services)
            .with_refiner(Arc::new(refiner))
            .with_options(riddle_sift::SiftOptions {
                force_escalation: true,
                ..Default::default()
            });
        // Nothing leaves this run: no projection, no progress events.
        sift.projection = None;
        sift.events = Arc::new(NullSink);
        let started = Instant::now();
        let result = riddle_sift::pipeline::run(&sift, input).await;
        let mut out = BenchOutput {
            id: uuid::Uuid::now_v7().to_string(),
            task: BenchTask::Escalation,
            source,
            model_id: model_id.to_string(),
            dig_id: Some(dig_id.clone()),
            message_id: Some(message_id.clone()),
            question: None,
            question_key: None,
            disposition: None,
            text: String::new(),
            latency_ms: started.elapsed().as_millis() as u64,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: None,
            error: None,
            created_at: self.services.clock.now(),
        };
        match result {
            Ok(outcome) => {
                let t = &outcome.trace;
                out.input_tokens = t.cost.llm_input_tokens;
                out.output_tokens = t.cost.llm_output_tokens;
                out.cost_usd = t.cost.llm_cost_usd;
                match &t.escalation {
                    Some(esc)
                        if !esc.refined.is_empty()
                            || !esc.rationale.starts_with("escalation failed") =>
                    {
                        out.latency_ms = esc.latency_ms.max(t.timing.llm_ms);
                        out.disposition = Some(outcome.classification.disposition.clone());
                        out.text = esc.rationale.clone();
                    }
                    Some(esc) => out.error = Some(esc.rationale.clone()),
                    // A gate decided before any model was asked.
                    None => {
                        out.error =
                            Some(format!("not escalated: {}", outcome.classification.reason))
                    }
                }
            }
            Err(e) => out.error = Some(e.message),
        }
        repo::comparison::record(&self.stores.index(), &out)?;
        Ok(out)
    }

    /// Run the usable challengers on a message a real escalation just
    /// decided, in the background, under the daily cap.
    pub fn spawn_challengers(self: &Arc<Self>, dig_id: &DigId, message_id: &MessageId) {
        let settings = self.comparison_settings();
        if !settings.challengers || !self.under_cap() {
            return;
        }
        let current = self
            .services
            .get::<riddle_settings::SettingsStore>()
            .map(|s| s.settings().llm.escalation_model)
            .unwrap_or_default();
        for model in self.usable_models().into_iter().filter(|m| *m != current) {
            let app = self.clone();
            let (dig, msg) = (dig_id.clone(), message_id.clone());
            tokio::spawn(async move {
                if !app.under_cap() {
                    return;
                }
                let runs = app.comparison_runs();
                let _r = Running::start(&runs);
                if let Err(e) = app
                    .compare_escalation(&model, &dig, &msg, BenchSource::Shadow)
                    .await
                {
                    rwarn!(
                        Category::Llm,
                        "challenger {model} on {msg}: {}",
                        e.kind.as_str()
                    );
                }
            });
        }
    }

    /// What a benchmark runs on: messages with a right answer first (the
    /// person's own decisions, then decisions graded right in Rule check),
    /// then the hard cases (LLM-decided or Unresolved) to fill the count. A
    /// real mailbox had no decision of the person's yet; its hard cases
    /// still show where models disagree, and become scored as the person
    /// decides them.
    fn benchmark_messages(&self, limit: usize) -> Result<Vec<(DigId, MessageId)>> {
        let mut out = self.judged_messages(limit)?;
        if out.len() < limit {
            for dig in self.list_digs()? {
                if dig.privacy != PrivacyMode::CloudReasoning {
                    continue;
                }
                let store = self.stores.dig(&dig.id)?;
                for id in repo::classifications::recent_hard(&store, limit as u32)? {
                    if out.len() >= limit {
                        break;
                    }
                    if !out.iter().any(|(_, m)| *m == id) {
                        out.push((dig.id.clone(), id));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Messages with a right answer: the person's own decisions, then
    /// decisions graded right in Rule check; newest first.
    fn judged_messages(&self, limit: usize) -> Result<Vec<(DigId, MessageId)>> {
        let mut out = Vec::new();
        for dig in self.list_digs()? {
            if dig.privacy != PrivacyMode::CloudReasoning {
                continue;
            }
            let store = self.stores.dig(&dig.id)?;
            for id in repo::classifications::user_decided(&store, limit as u32)? {
                out.push((dig.id.clone(), id));
            }
        }
        let index = self.stores.index();
        for (key, verdict) in repo::kv::list_prefix(&index, crate::reads::GRADE_PREFIX)? {
            if verdict != "right" || out.len() >= limit * 2 {
                continue;
            }
            // `rule_check:<mesh>:<dig>:<provider id>`; the message id is the rest.
            let rest = key.trim_start_matches(crate::reads::GRADE_PREFIX);
            if let Some((_, message)) = rest.split_once(':') {
                let message = MessageId(message.to_string());
                if let Some(dig) = message.as_str().split(':').next() {
                    let dig = DigId(dig.to_string());
                    if !out.iter().any(|(_, m)| *m == message) {
                        out.push((dig, message));
                    }
                }
            }
        }
        out.truncate(limit);
        Ok(out)
    }

    /// The right answer for a message, if the person has given one.
    fn truth_of(&self, dig_id: &DigId, message_id: &MessageId) -> Option<Disposition> {
        let store = self.stores.dig(dig_id).ok()?;
        let k = repo::classifications::current(&store, message_id).ok()??;
        if k.mechanism == DecisionMechanism::User {
            return Some(k.disposition);
        }
        let mesh = k.mesh_id.as_ref()?;
        let key = format!("{}{}:{}", crate::reads::GRADE_PREFIX, mesh, message_id);
        (repo::kv::get(&self.stores.index(), &key).ok()?.as_deref() == Some("right"))
            .then_some(k.disposition)
    }

    /// Run `models` (or every usable one) on up to `count` judged messages,
    /// in the background, models side by side per message.
    pub fn start_escalation_benchmark(
        self: &Arc<Self>,
        models: Vec<String>,
        count: usize,
    ) -> Result<usize> {
        let models = if models.is_empty() {
            self.usable_models()
        } else {
            models
        };
        if models.is_empty() {
            return Err(RiddleError::invalid(
                "no model is usable yet; check access first",
            ));
        }
        let items = self.benchmark_messages(count)?;
        if items.is_empty() {
            return Err(RiddleError::invalid(
                "there is no decided, escalated or Unresolved mail to compare on yet",
            ));
        }
        let n = items.len();
        let app = self.clone();
        tokio::spawn(async move {
            let runs = app.comparison_runs();
            let _r = Running::start(&runs);
            for (dig, msg) in items {
                if !app.under_cap() {
                    rwarn!(
                        Category::Llm,
                        "benchmark stopped at the daily comparison cap"
                    );
                    break;
                }
                let tasks = models
                    .iter()
                    .map(|m| app.compare_escalation(m, &dig, &msg, BenchSource::Benchmark));
                for r in futures::future::join_all(tasks).await {
                    if let Err(e) = r {
                        rwarn!(Category::Llm, "benchmark item {msg}: {}", e.kind.as_str());
                    }
                }
            }
            rinfo!(Category::Llm, "escalation benchmark done: {n} messages");
        });
        Ok(n)
    }

    /// One model's answer to one past question, recorded.
    pub async fn compare_director(
        &self,
        model_id: &str,
        question: &str,
        source: BenchSource,
    ) -> Result<BenchOutput> {
        let ai = self.ai()?;
        let started = Instant::now();
        let result = ai.director.replay(question, model_id).await;
        let mut out = BenchOutput {
            id: uuid::Uuid::now_v7().to_string(),
            task: BenchTask::Director,
            source,
            model_id: model_id.to_string(),
            dig_id: None,
            message_id: None,
            question: Some(question.to_string()),
            question_key: Some(question_key(question)),
            disposition: None,
            text: String::new(),
            latency_ms: started.elapsed().as_millis() as u64,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: None,
            error: None,
            created_at: self.services.clock.now(),
        };
        match result {
            Ok(o) => {
                out.text = o.text;
                out.input_tokens = o.input_tokens;
                out.output_tokens = o.output_tokens;
                out.cost_usd = o.cost_usd;
            }
            Err(e) => out.error = Some(e.message),
        }
        repo::comparison::record(&self.stores.index(), &out)?;
        Ok(out)
    }

    /// The person's recent distinct questions to the Director, newest first.
    fn past_questions(&self, limit: usize) -> Result<Vec<String>> {
        let index = self.stores.index();
        let mut out: Vec<String> = Vec::new();
        for summary in repo::conversations::list(&index, 200)? {
            let Some(conv) = repo::conversations::get(&index, &summary.id)? else {
                continue;
            };
            for t in conv.turns.iter().rev() {
                if t.role == TurnRole::User {
                    let q = t.content.trim();
                    if q.len() >= 12 && !out.iter().any(|x| x == q) {
                        out.push(q.to_string());
                    }
                }
            }
            if out.len() >= limit {
                break;
            }
        }
        out.truncate(limit);
        Ok(out)
    }

    /// Replay up to `count` past questions on `models` (or every usable one).
    pub fn start_director_comparison(
        self: &Arc<Self>,
        models: Vec<String>,
        count: usize,
    ) -> Result<usize> {
        let models = if models.is_empty() {
            self.usable_models()
        } else {
            models
        };
        if models.is_empty() {
            return Err(RiddleError::invalid(
                "no model is usable yet; check access first",
            ));
        }
        let questions = self.past_questions(count)?;
        if questions.is_empty() {
            return Err(RiddleError::invalid(
                "there are no past Director questions to replay yet",
            ));
        }
        let n = questions.len();
        let app = self.clone();
        tokio::spawn(async move {
            let runs = app.comparison_runs();
            let _r = Running::start(&runs);
            for q in questions {
                if !app.under_cap() {
                    rwarn!(
                        Category::Llm,
                        "Director comparison stopped at the daily cap"
                    );
                    break;
                }
                let tasks = models
                    .iter()
                    .map(|m| app.compare_director(m, &q, BenchSource::Benchmark));
                for r in futures::future::join_all(tasks).await {
                    if let Err(e) = r {
                        rwarn!(Category::Llm, "Director comparison: {}", e.kind.as_str());
                    }
                }
            }
        });
        Ok(n)
    }

    /// Scores per model and job, from every recorded output.
    fn comparison_scores(&self) -> Result<Vec<BenchScore>> {
        let index = self.stores.index();
        let mut out = Vec::new();
        let picks: Vec<(String, String)> =
            repo::kv::list_prefix(&index, repo::comparison::PICK_PREFIX)?;
        for task in [BenchTask::Escalation, BenchTask::Director] {
            let outputs = repo::comparison::recent(&index, task, 5_000)?;
            let mut models: Vec<String> = outputs.iter().map(|o| o.model_id.clone()).collect();
            models.sort();
            models.dedup();
            for model in models {
                let mine: Vec<&BenchOutput> =
                    outputs.iter().filter(|o| o.model_id == model).collect();
                let ok: Vec<&&BenchOutput> = mine.iter().filter(|o| o.error.is_none()).collect();
                let (mut judged, mut correct, mut agrees) = (0u64, 0u64, 0u64);
                if task == BenchTask::Escalation {
                    for o in &ok {
                        let (Some(dig), Some(msg), Some(d)) =
                            (&o.dig_id, &o.message_id, &o.disposition)
                        else {
                            continue;
                        };
                        if let Some(truth) = self.truth_of(dig, msg) {
                            judged += 1;
                            if truth == *d {
                                correct += 1;
                            }
                        }
                        let current = self
                            .stores
                            .dig(dig)
                            .ok()
                            .and_then(|s| repo::classifications::current(&s, msg).ok().flatten())
                            .map(|k| k.disposition);
                        if current.as_ref() == Some(d) {
                            agrees += 1;
                        }
                    }
                }
                let priced = ok.iter().all(|o| o.cost_usd.is_some());
                let cost: f64 = ok.iter().filter_map(|o| o.cost_usd).sum();
                out.push(BenchScore {
                    model_id: model.clone(),
                    task,
                    runs: mine.len() as u64,
                    errors: (mine.len() - ok.len()) as u64,
                    judged,
                    correct,
                    agrees_with_current: agrees,
                    preferred: picks.iter().filter(|(_, m)| *m == model).count() as u64
                        * (task == BenchTask::Director) as u64,
                    avg_latency_ms: if ok.is_empty() {
                        0
                    } else {
                        ok.iter().map(|o| o.latency_ms).sum::<u64>() / ok.len() as u64
                    },
                    cost_per_1k: (priced && !ok.is_empty())
                        .then(|| cost / ok.len() as f64 * 1000.0),
                });
            }
        }
        Ok(out)
    }

    pub fn comparison_overview(&self) -> Result<BenchOverview> {
        let settings = self.comparison_settings();
        Ok(BenchOverview {
            models: self.comparison_models()?,
            scores: self.comparison_scores()?,
            shadow: settings.challengers,
            spent_today_usd: self.comparison_spent_today(),
            daily_cap_usd: settings.daily_usd,
            running: self.comparison_runs().0.load(Ordering::SeqCst) > 0,
        })
    }

    /// Escalation items side by side, newest first.
    pub fn comparison_rows(&self, limit: usize) -> Result<Vec<BenchRow>> {
        let outputs = repo::comparison::recent(&self.stores.index(), BenchTask::Escalation, 5_000)?;
        let mut rows: Vec<BenchRow> = Vec::new();
        for o in outputs {
            let (Some(dig), Some(msg)) = (o.dig_id.clone(), o.message_id.clone()) else {
                continue;
            };
            if let Some(row) = rows.iter_mut().find(|r| r.message_id == msg) {
                // Newest output per model only.
                if !row.outputs.iter().any(|x| x.model_id == o.model_id) {
                    row.outputs.push(o);
                }
                continue;
            }
            if rows.len() >= limit {
                continue;
            }
            let store = self.stores.dig(&dig)?;
            let Some(m) = repo::messages::get(&store, &dig, &msg)? else {
                continue;
            };
            let current = repo::classifications::current(&store, &msg)?.map(|k| k.disposition);
            rows.push(BenchRow {
                truth: self.truth_of(&dig, &msg),
                current,
                dig_id: dig,
                message_id: msg,
                subject: m.subject,
                from: m.from.email,
                outputs: vec![o],
            });
        }
        Ok(rows)
    }

    /// Director questions side by side, newest first, with the person's pick.
    pub fn director_comparisons(&self, limit: usize) -> Result<Vec<DirectorComparison>> {
        let index = self.stores.index();
        let outputs = repo::comparison::recent(&index, BenchTask::Director, 2_000)?;
        let mut rows: Vec<DirectorComparison> = Vec::new();
        for o in outputs {
            let (Some(key), Some(q)) = (o.question_key.clone(), o.question.clone()) else {
                continue;
            };
            if let Some(row) = rows.iter_mut().find(|r| r.question_key == key) {
                if !row.outputs.iter().any(|x| x.model_id == o.model_id) {
                    row.outputs.push(o);
                }
                continue;
            }
            if rows.len() >= limit {
                continue;
            }
            let preferred =
                repo::kv::get(&index, &format!("{}{key}", repo::comparison::PICK_PREFIX))?;
            rows.push(DirectorComparison {
                question_key: key,
                question: q,
                outputs: vec![o],
                preferred,
            });
        }
        Ok(rows)
    }
}

/// A stable key for a question, so its answers group across runs.
pub fn question_key(question: &str) -> String {
    sha256_hex(question.trim().as_bytes())[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids_read_as_names() {
        assert_eq!(
            model_label("us.anthropic.claude-haiku-4-5-20251001-v1:0"),
            ("Claude Haiku 4.5".to_string(), "Anthropic".to_string())
        );
        assert_eq!(
            model_label("us.anthropic.claude-sonnet-5-5").0,
            "Claude Sonnet 5.5"
        );
        assert_eq!(
            model_label("us.anthropic.claude-opus-4-6-v1").0,
            "Claude Opus 4.6"
        );
        assert_eq!(
            model_label("us.openai.gpt-6-luna"),
            ("GPT-6 Luna".to_string(), "OpenAI".to_string())
        );
        assert_eq!(model_label("us.openai.gpt-5.6-luna").0, "GPT-5.6 Luna");
    }
}
