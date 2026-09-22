//! Assembly of settings, jobs, undo and the Task Centre, and the boot
//! sequence every client uses. Settings drive the AI and judge
//! configuration; the job runner drives the engine; the Task Centre reads
//! both.

use crate::ai::AiConfig;
use crate::domains::JudgeConfig;
use crate::App;
use async_trait::async_trait;
use riddle_activity::{NoProviderCheck, RuntimeHandle, UndoEngine};
use riddle_core::*;
use riddle_errors::Result;
use riddle_jobs::{
    DigStatusSource, JobContext, JobHandler, JobOutcome, JobQueue, JobSpec, ResourceConfig, Resources, Runner, SleepDetector, UsageBudgetGate, WakeHook, WakeReason,
};
use riddle_observability::{rinfo, rwarn, Category};
use riddle_settings::{ArmControl, JudgeProvider, KeySource, LlmProvider, Secret, SecretPresence, Settings, SettingsStore};
use riddle_store::repo;
use riddle_sync::engine::CancelFlag;
use std::sync::Arc;

pub struct Ops {
    pub settings: Arc<SettingsStore>,
    pub queue: Arc<JobQueue>,
    pub runner: Arc<Runner>,
    pub undo: Arc<UndoEngine>,
    pub budget: Arc<UsageBudgetGate>,
}

/// Presence of secrets, derived at read time and never stored (§12).
struct Presence {
    app: Arc<App>,
}

impl SecretPresence for Presence {
    fn is_present(&self, secret: &Secret) -> bool {
        let creds = self.app.services.get::<riddle_credentials::Credentials>();
        match secret {
            Secret::JudgeApiKey { source, .. } => match source {
                KeySource::Env => std::env::var("TYPESAFE_API_KEY").is_ok() || std::env::var("RIDDLE_JEV_API_KEY").is_ok(),
                KeySource::Keychain => creds.map(|c| c.present(riddle_credentials::JEV_API_KEY)).unwrap_or(false),
                KeySource::Ssm => self.app.services.judge().is_some(),
            },
            Secret::GmailClientSecret => creds.map(|c| c.present(riddle_credentials::GMAIL_CLIENT_SECRET)).unwrap_or(false),
        }
    }
}

pub fn ai_config(s: &Settings) -> AiConfig {
    AiConfig {
        provider: match s.llm.provider { LlmProvider::Bedrock => "bedrock", LlmProvider::Scripted => "scripted", LlmProvider::Null => "null" }.into(),
        profile: if s.llm.profile.is_empty() { None } else { Some(s.llm.profile.clone()) },
        region: s.llm.region.clone(),
        escalation_model: s.llm.escalation_model.clone(),
        director_model: s.llm.director_model.clone(),
        prompt_override_dir: None,
    }
}

pub fn judge_config(s: &Settings) -> JudgeConfig {
    JudgeConfig {
        provider: match s.judge.provider { JudgeProvider::Jev => "jev", JudgeProvider::Scripted => "scripted", JudgeProvider::Null => "null" }.into(),
        model: s.judge.model.clone(),
        key_source: match s.judge.key_source { KeySource::Env => "env", KeySource::Keychain => "keychain", KeySource::Ssm => "ssm" }.into(),
        ssm_profile: s.judge.ssm_profile.clone().unwrap_or_else(|| "dev".into()),
        ssm_parameter: s.judge.ssm_parameter.clone().unwrap_or_else(|| "/riddle/jev_api_key".into()),
    }
}

/// Per-Dig status for the Task Centre and the Dig rows (§5).
pub struct AppDigStatus(pub Arc<App>);

impl AppDigStatus {
    pub fn status_of(&self, dig: &Dig) -> Result<DigStatus> {
        let store = self.0.stores.dig(&dig.id)?;
        let initial_done = repo::sync_state::get(&store, repo::sync_state::INITIAL_SYNC_DONE)?.as_deref() == Some("1");
        let total = repo::messages::count(&store)?;
        let unclassified = repo::messages::unclassified_count(&store)?;
        let pending = repo::outbox::pending_count(&store)?;
        if let DigStatus::Degraded { reason } = &dig.status {
            return Ok(DigStatus::Degraded { reason: reason.clone() });
        }
        if let DigStatus::Shadow { day, of } = &dig.status {
            return Ok(DigStatus::Shadow { day: *day, of: *of });
        }
        if total == 0 && !initial_done {
            return Ok(DigStatus::NotConnected);
        }
        if !initial_done {
            return Ok(DigStatus::CatchingUp { remaining: unclassified, total });
        }
        if self.0.services.judge().is_none() && unclassified > 0 {
            return Ok(DigStatus::Degraded { reason: "judge unavailable; nothing hidden".into() });
        }
        if unclassified > 0 {
            return Ok(DigStatus::Sifting { remaining: unclassified });
        }
        if pending > 0 {
            return Ok(DigStatus::Projection { pending });
        }
        Ok(DigStatus::Synced)
    }
}

impl DigStatusSource for AppDigStatus {
    fn dig_statuses(&self) -> Result<Vec<DigStatusRow>> {
        let mut out = Vec::new();
        for dig in self.0.list_digs()? {
            let status = self.status_of(&dig)?;
            out.push(DigStatusRow { dig_id: dig.id.clone(), name: dig.name.clone(), status });
        }
        Ok(out)
    }
}

/// One job kind → one engine pass. Long passes yield with a checkpoint so
/// the runner can pre-empt them (§55).
struct PassHandler {
    app: Arc<App>,
    kind: JobKind,
}

#[async_trait]
impl JobHandler for PassHandler {
    async fn run(&self, ctx: JobContext) -> Result<JobOutcome> {
        let cancel = CancelFlag::new();
        let dig_id = ctx.job.dig_id.clone();
        match self.kind {
            JobKind::TemporalSift => {
                let n = self.app.temporal_pass()?.len();
                ctx.progress.report(n as u64, Some(n as u64), "temporal");
                Ok(JobOutcome::done())
            }
            JobKind::Maintenance => {
                let cutoff = self.app.services.clock.now() - chrono::Duration::days(7);
                let pruned = repo::jobs::prune_finished(&self.app.stores.index(), &cutoff)?;
                let _ = self.app.stores.index().compact();
                for (_, s) in self.app.stores.open_digs() {
                    let _ = s.compact();
                }
                ctx.progress.report(pruned, Some(pruned), "pruned jobs");
                Ok(JobOutcome::done())
            }
            _ => {
                let Some(dig_id) = dig_id else { return Ok(JobOutcome::done()) };
                let dig = self.app.dig(&dig_id)?;
                let mut progress_fn = {
                    let p = ctx.progress.clone();
                    move |sp: riddle_sync::engine::SyncProgress| p.report(sp.done, sp.total, format!("{:?}", sp.phase).to_lowercase())
                };
                if matches!(self.kind, JobKind::InitialSync | JobKind::CatchUp | JobKind::IncrementalSync) {
                    if ctx.cancel.is_cancelled() { cancel.cancel(); }
                    match self.app.sync_pass(&dig, &cancel, &mut progress_fn).await {
                        Ok(_) => {}
                        Err(e) if e.kind.is_transient() || e.kind == riddle_errors::ErrorKind::ProviderAuth => {
                            rwarn!(Category::Sync, "sync of {} deferred: {}", dig.id, e.kind.as_str());
                        }
                        Err(e) => return Err(e),
                    }
                }
                if matches!(self.kind, JobKind::Sift | JobKind::CatchUp | JobKind::IncrementalSync | JobKind::InitialSync | JobKind::HistoricalSift) {
                    let report = self.app.sift_pass(&dig, 100, Some(&cancel)).await?;
                    ctx.progress.report(report.processed, Some(report.processed + report.remaining), "sifting");
                    if report.remaining > 0 && !ctx.cancel.is_cancelled() && report.processed > 0 {
                        self.app.refresh_dig_status(&dig.id);
                        return Ok(JobOutcome::yield_with(serde_json::json!({ "remaining": report.remaining })));
                    }
                }
                if matches!(self.kind, JobKind::Projection | JobKind::CatchUp | JobKind::IncrementalSync | JobKind::Sift) {
                    if let Err(e) = self.app.projection_pass(&dig).await {
                        rwarn!(Category::Projection, "projection on {} deferred: {}", dig.id, e.kind.as_str());
                    }
                }
                self.app.refresh_dig_status(&dig.id);
                Ok(JobOutcome::done())
            }
        }
    }
}

struct Wake(Arc<App>);
impl WakeHook for Wake {
    fn on_wake(&self, reason: WakeReason) {
        rinfo!(Category::Jobs, "wake: {:?}; refreshing every Dig", reason);
        self.0.enqueue_catch_up_all();
    }
}

impl App {
    pub fn attach_settings(self: &Arc<Self>) -> Result<Arc<SettingsStore>> {
        let presence: Arc<dyn SecretPresence> = Arc::new(Presence { app: self.clone() });
        let events: Arc<dyn EventSink> = self.events.clone();
        let (config_dir, data_dir) = match (&self.config.config_dir, &self.config.data_dir) {
            (Some(c), Some(d)) => (c.clone(), d.clone()),
            _ => {
                let tmp = std::env::temp_dir().join(format!("riddle-{}", uuid::Uuid::new_v4().simple()));
                (tmp.join("config"), tmp.join("data"))
            }
        };
        let store = Arc::new(SettingsStore::open(&config_dir, &data_dir, self.config.dev, presence, events)?);
        self.services.insert(store.clone());
        let mut policy = store.autonomy_policy();
        policy.armed = self.runtime.is_armed();
        policy.dev = self.config.dev;
        *self.runtime.policy.write() = policy;
        Ok(store)
    }

    /// Jobs, undo, Task Centre sources, settings and job Actions.
    pub fn attach_ops(self: &Arc<Self>, settings: Arc<SettingsStore>) -> Result<Arc<Ops>> {
        let queue = JobQueue::open(self.stores.index(), self.services.clock.clone(), self.events.clone())?;
        queue.watch_store_faults(&self.stores.index());
        let s = settings.settings();
        let budget = Arc::new(UsageBudgetGate::new(self.stores.clone(), s.budgets.daily_judge_usd, s.budgets.daily_llm_usd));
        let resources = Resources::new(ResourceConfig::default());
        resources.set_budget_gate(budget.clone());
        let runner = Runner::new(queue.clone(), self.services.clone(), resources);
        for kind in [JobKind::InitialSync, JobKind::CatchUp, JobKind::IncrementalSync, JobKind::Sift, JobKind::HistoricalSift, JobKind::Projection, JobKind::TemporalSift, JobKind::Maintenance] {
            runner.register(kind, Arc::new(PassHandler { app: self.clone(), kind }));
        }
        let undo = Arc::new(UndoEngine::new(Arc::new(NoProviderCheck)));
        self.services.insert(queue.clone());
        self.services.insert(undo.clone());
        self.services.insert(Arc::new(RuntimeHandle::new(&self.runtime)));
        self.services.insert(Arc::new(ArmControl::new(&self.runtime)));
        self.services.insert(Arc::new(AppDigStatus(self.clone())));
        {
            let mut reg = self.runtime.registry.write();
            for a in riddle_settings::actions().into_iter().chain(riddle_jobs::actions()).chain(riddle_activity::actions()) {
                reg.register(a);
            }
        }
        let ops = Arc::new(Ops { settings, queue, runner, undo, budget });
        self.services.insert(ops.clone());
        Ok(ops)
    }

    /// Start the runner, the sleep detector and the periodic schedule.
    pub fn start_background(self: &Arc<Self>, ops: &Arc<Ops>) {
        ops.runner.start();
        SleepDetector::new(ops.queue.clone(), self.services.clock.clone(), Arc::new(Wake(self.clone()))).start();
        let app = self.clone();
        let queue = ops.queue.clone();
        tokio::spawn(async move {
            let mut sync_tick = tokio::time::interval(std::time::Duration::from_secs(45));
            let mut temporal_tick = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
            let mut maintenance_tick = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
            loop {
                tokio::select! {
                    _ = sync_tick.tick() => {
                        if let Ok(digs) = app.list_digs() {
                            for d in digs {
                                let _ = queue.enqueue(JobSpec::new(JobKind::IncrementalSync, format!("Sync {}", d.name)).dig(d.id.clone()));
                            }
                        }
                    }
                    _ = temporal_tick.tick() => { let _ = queue.enqueue(JobSpec::new(JobKind::TemporalSift, "Temporal Sift")); }
                    _ = maintenance_tick.tick() => { let _ = queue.enqueue(JobSpec::new(JobKind::Maintenance, "Maintenance")); }
                }
            }
        });
        // Refresh triggers from Actions (refresh_now, inject) become jobs.
        if let Some(domains) = self.services.get::<crate::domains::Domains>() {
            if let Ok(digs) = self.list_digs() {
                for d in digs {
                    let notify = domains.triggers.for_dig(&d.id);
                    let queue = ops.queue.clone();
                    let name = d.name.clone();
                    let id = d.id.clone();
                    tokio::spawn(async move {
                        loop {
                            notify.notified().await;
                            let _ = queue.enqueue(JobSpec::new(JobKind::IncrementalSync, format!("Refresh {name}")).dig(id.clone()).priority(Priority::Interactive));
                        }
                    });
                }
            }
        }
        rinfo!(Category::Jobs, "background work started");
    }

    pub fn enqueue_catch_up_all(&self) {
        let Some(ops) = self.services.get::<Ops>() else { return };
        if let Ok(digs) = self.list_digs() {
            for d in digs {
                let _ = ops.queue.enqueue(JobSpec::new(JobKind::CatchUp, format!("Catch up {}", d.name)).dig(d.id.clone()));
            }
        }
    }

    /// Recompute and store a Dig's status; emit the change (§5).
    pub fn refresh_dig_status(&self, dig_id: &DigId) {
        let Some(src) = self.services.get::<AppDigStatus>() else { return };
        let Ok(mut dig) = self.dig(dig_id) else { return };
        if let Ok(status) = src.status_of(&dig) {
            if status != dig.status {
                dig.status = status.clone();
                let _ = repo::digs::upsert(&self.stores.index(), &dig);
                self.events.emit(RiddleEvent::DigStatusChanged { dig_id: dig_id.clone(), status });
            }
        }
    }

    pub fn task_centre(&self) -> Result<TaskCentre> {
        let ops = self.services.require::<Ops>()?;
        let src = self.services.require::<AppDigStatus>()?;
        Ok(riddle_jobs::snapshot(&self.stores, &ops.queue, src.as_ref(), self.services.clock.as_ref()))
    }

    /// The full boot: settings, domains, AI, judge, ops. Background work
    /// starts only when `background` is true (the CLI runs passes itself).
    pub async fn boot(self: &Arc<Self>, background: bool) -> Result<Arc<Ops>> {
        self.boot_with(background, None).await
    }

    /// `boot` with a settings override applied first (tests, the CLI's flags).
    pub async fn boot_with(self: &Arc<Self>, background: bool, override_settings: Option<Settings>) -> Result<Arc<Ops>> {
        let settings = self.attach_settings()?;
        if let Some(o) = override_settings {
            settings.save(o)?;
        }
        let s = settings.settings();
        let gmail = if s.gmail.client_id.is_empty() { None } else { Some(riddle_gmail::GmailOAuthConfig::new(s.gmail.client_id.clone(), None)) };
        self.attach_domains(gmail)?;
        self.attach_ai(&ai_config(&s));
        self.attach_judge(&judge_config(&s)).await;
        let ops = self.attach_ops(settings)?;
        for d in self.list_digs()? {
            self.refresh_dig_status(&d.id);
        }
        if background {
            self.start_background(&ops);
            self.enqueue_catch_up_all();
        }
        Ok(ops)
    }
}
