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
    DigStatusSource, JobContext, JobHandler, JobOutcome, JobQueue, JobSpec, ResourceConfig,
    Resources, Runner, SleepDetector, UsageBudgetGate, WakeHook, WakeReason,
};
use riddle_observability::{rdebug, rinfo, rwarn, Category};
use riddle_settings::{
    ArmControl, JudgeProvider, KeySource, LlmProvider, Secret, SecretPresence, Settings,
    SettingsStore,
};
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
                KeySource::Env => ["TYPESAFE_API_KEY", "RIDDLE_JEV_API_KEY"]
                    .iter()
                    .any(|v| std::env::var(v).is_ok_and(|s| !s.trim().is_empty())),
                KeySource::Keychain => creds
                    .map(|c| c.present(riddle_credentials::JEV_API_KEY))
                    .unwrap_or(false),
                KeySource::Ssm => self.app.services.judge().is_some(),
            },
            Secret::GmailClientSecret => creds
                .map(|c| c.present(riddle_credentials::GMAIL_CLIENT_SECRET))
                .unwrap_or(false),
        }
    }
}

pub fn ai_config(s: &Settings) -> AiConfig {
    AiConfig {
        provider: match s.llm.provider {
            LlmProvider::Bedrock => "bedrock",
            LlmProvider::Scripted => "scripted",
            LlmProvider::Null => "null",
        }
        .into(),
        profile: if s.llm.profile.is_empty() {
            None
        } else {
            Some(s.llm.profile.clone())
        },
        region: s.llm.region.clone(),
        escalation_model: s.llm.escalation_model.clone(),
        director_model: s.llm.director_model.clone(),
        prompt_override_dir: None,
    }
}

pub fn judge_config(s: &Settings) -> JudgeConfig {
    JudgeConfig {
        provider: match s.judge.provider {
            JudgeProvider::Jev => "jev",
            JudgeProvider::Scripted => "scripted",
            JudgeProvider::Null => "null",
        }
        .into(),
        model: s.judge.model.clone(),
        key_source: match s.judge.key_source {
            KeySource::Env => "env",
            KeySource::Keychain => "keychain",
            KeySource::Ssm => "ssm",
        }
        .into(),
        ssm_profile: s.judge.ssm_profile.clone().unwrap_or_else(|| "dev".into()),
        ssm_parameter: s
            .judge
            .ssm_parameter
            .clone()
            .unwrap_or_else(|| "/riddle/jev_api_key".into()),
    }
}

/// Per-Dig status for the Task Centre and the Dig rows (§5).
pub struct AppDigStatus(pub Arc<App>);

impl AppDigStatus {
    /// Whether the Dig can reach its mailbox: a Gmail Dig needs its refresh
    /// token; a Fixture Dig is always reachable.
    fn signed_in(&self, dig: &Dig) -> bool {
        match dig.provider {
            ProviderKind::Fixture => true,
            ProviderKind::Gmail => self
                .0
                .services
                .get::<riddle_credentials::Credentials>()
                .is_some_and(|c| c.present(&riddle_credentials::gmail_refresh_token_key(&dig.id))),
        }
    }

    pub fn status_of(&self, dig: &Dig) -> Result<DigStatus> {
        let store = self.0.stores.dig(&dig.id)?;
        let initial_done = repo::sync_state::get(&store, repo::sync_state::INITIAL_SYNC_DONE)?
            .as_deref()
            == Some("1");
        let total = repo::messages::count(&store)?;
        let unclassified = repo::messages::unclassified_count(&store)?;
        let pending = repo::outbox::pending_count(&store)?;
        // Degraded is derived below on every call, never read back from the
        // stored Dig: `refresh_dig_status` saves what this returns, so
        // returning a saved Degraded kept "first sync failed" and "judge
        // unavailable" on screen after the cause had cleared.
        if let DigStatus::Shadow { day, of } = &dig.status {
            return Ok(DigStatus::Shadow { day: *day, of: *of });
        }
        if !initial_done {
            // A failed first sync is a problem to show, not "Not connected":
            // the account is signed in and "Sync now" retries it. Any later
            // sync job (Sync now, the poll) supersedes the failure.
            let first = repo::jobs::latest_for_dig(
                &self.0.stores.index(),
                &dig.id,
                &[
                    JobKind::InitialSync.as_str(),
                    JobKind::CatchUp.as_str(),
                    JobKind::IncrementalSync.as_str(),
                ],
            )?;
            if let Some(Job {
                status: JobStatus::Failed { error },
                ..
            }) = first
            {
                return Ok(DigStatus::Degraded {
                    reason: format!("first sync failed ({error}); Sync now retries"),
                });
            }
        }
        // Before the rest: an account that is not syncing is not "Caught
        // up", whatever it held when it last synced.
        if let Some(since) = repo::sync_state::get(&store, repo::sync_state::SYNC_FAILING_SINCE)? {
            let failure = SyncFailure::parse(
                &repo::sync_state::get(&store, repo::sync_state::SYNC_FAILURE)?.unwrap_or_default(),
            );
            let since = chrono::DateTime::parse_from_rfc3339(&since)
                .map(|t| t.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| self.0.services.clock.now());
            if self.0.services.clock.now() - since >= failure.shows_after() {
                let last_synced_at = repo::sync_state::get(&store, repo::sync_state::LAST_SYNC_AT)?
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok())
                    .map(|t| t.with_timezone(&chrono::Utc));
                return Ok(DigStatus::SyncFailing {
                    failure,
                    since,
                    last_synced_at,
                });
            }
        }
        if total == 0 && !initial_done && !self.signed_in(dig) {
            return Ok(DigStatus::NotConnected);
        }
        if !initial_done {
            return Ok(DigStatus::CatchingUp {
                remaining: unclassified,
                total,
            });
        }
        if self.0.services.judge().is_none() && unclassified > 0 {
            return Ok(DigStatus::Degraded {
                reason: "judge unavailable; nothing hidden".into(),
            });
        }
        if unclassified > 0 {
            return Ok(DigStatus::Sifting {
                remaining: unclassified,
            });
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
            out.push(DigStatusRow {
                dig_id: dig.id.clone(),
                name: dig.name.clone(),
                status,
            });
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
                let Some(dig_id) = dig_id else {
                    return Ok(JobOutcome::done());
                };
                let dig = self.app.dig(&dig_id)?;
                // Queued before the account was disabled: nothing to do
                // until it is enabled, when the scheduler queues it again.
                if dig.disabled {
                    rdebug!(Category::Sync, "job for disabled dig {} skipped", dig.id);
                    return Ok(JobOutcome::done());
                }
                let mut progress_fn = {
                    let p = ctx.progress.clone();
                    move |sp: riddle_sync::engine::SyncProgress| {
                        p.report(sp.done, sp.total, format!("{:?}", sp.phase).to_lowercase())
                    }
                };
                if matches!(
                    self.kind,
                    JobKind::InitialSync | JobKind::CatchUp | JobKind::IncrementalSync
                ) {
                    if ctx.cancel.is_cancelled() {
                        cancel.cancel();
                    }
                    // A failing sync is said on the account, not only in the
                    // log: twenty thousand syncs over two weeks failed on an
                    // expired sign-in while the job reported "completed" and
                    // the account "Caught up", so Sync now looked like it worked.
                    let result = self.app.sync_pass(&dig, &cancel, &mut progress_fn).await;
                    self.app
                        .record_sync_result(&dig.id, result.as_ref().err().map(|e| e.kind));
                    match result {
                        Ok(_) => {}
                        Err(e)
                            if e.kind.is_transient()
                                || e.kind == riddle_errors::ErrorKind::ProviderAuth =>
                        {
                            rwarn!(
                                Category::Sync,
                                "sync of {} deferred: {}",
                                dig.id,
                                e.kind.as_str()
                            );
                        }
                        Err(e) => return Err(e),
                    }
                }
                if matches!(
                    self.kind,
                    JobKind::Sift
                        | JobKind::CatchUp
                        | JobKind::IncrementalSync
                        | JobKind::InitialSync
                        | JobKind::HistoricalSift
                ) {
                    let report = self.app.sift_pass(&dig, 100, Some(&cancel)).await?;
                    ctx.progress.report(
                        report.processed,
                        Some(report.processed + report.remaining),
                        "sifting",
                    );
                    if report.remaining > 0 && !ctx.cancel.is_cancelled() && report.processed > 0 {
                        self.app.refresh_dig_status(&dig.id);
                        return Ok(JobOutcome::yield_with(
                            serde_json::json!({ "remaining": report.remaining }),
                        ));
                    }
                }
                if matches!(
                    self.kind,
                    JobKind::Projection
                        | JobKind::CatchUp
                        | JobKind::IncrementalSync
                        | JobKind::Sift
                ) {
                    if let Err(e) = self.app.projection_pass(&dig).await {
                        rwarn!(
                            Category::Projection,
                            "projection on {} deferred: {}",
                            dig.id,
                            e.kind.as_str()
                        );
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
                let tmp =
                    std::env::temp_dir().join(format!("riddle-{}", uuid::Uuid::new_v4().simple()));
                (tmp.join("config"), tmp.join("data"))
            }
        };
        let store = Arc::new(SettingsStore::open(
            &config_dir,
            &data_dir,
            self.config.dev,
            presence,
            events,
        )?);
        self.services.insert(store.clone());
        self.load_autonomy(&store);
        Ok(store)
    }

    /// Install the saved autonomy levels, keeping the runtime's mailbox writes.
    fn load_autonomy(&self, store: &SettingsStore) {
        let mut policy = store.autonomy_policy();
        policy.writes = self.runtime.writes();
        policy.dev = self.config.dev;
        *self.runtime.policy.write() = policy;
    }

    /// Jobs, undo, Task Centre sources, settings and job Actions.
    pub fn attach_ops(self: &Arc<Self>, settings: Arc<SettingsStore>) -> Result<Arc<Ops>> {
        let queue = JobQueue::open(
            self.stores.index(),
            self.services.clock.clone(),
            self.events.clone(),
        )?;
        queue.watch_store_faults(&self.stores.index());
        let s = settings.settings();
        let budget = Arc::new(UsageBudgetGate::new(
            self.stores.clone(),
            s.budgets.daily_judge_usd,
            s.budgets.daily_llm_usd,
        ));
        let resources = Resources::new(ResourceConfig::default());
        resources.set_budget_gate(budget.clone());
        let runner = Runner::new(queue.clone(), self.services.clone(), resources);
        for kind in [
            JobKind::InitialSync,
            JobKind::CatchUp,
            JobKind::IncrementalSync,
            JobKind::Sift,
            JobKind::HistoricalSift,
            JobKind::Projection,
            JobKind::TemporalSift,
            JobKind::Maintenance,
        ] {
            runner.register(
                kind,
                Arc::new(PassHandler {
                    app: self.clone(),
                    kind,
                }),
            );
        }
        let undo = Arc::new(UndoEngine::new(Arc::new(NoProviderCheck)));
        self.services.insert(queue.clone());
        self.services.insert(undo.clone());
        self.services
            .insert(Arc::new(RuntimeHandle::new(&self.runtime)));
        self.services
            .insert(Arc::new(ArmControl::new(&self.runtime)));
        self.services.insert(Arc::new(AppDigStatus(self.clone())));
        {
            let mut reg = self.runtime.registry.write();
            for a in riddle_settings::actions()
                .into_iter()
                .chain(riddle_jobs::actions())
                .chain(riddle_activity::actions())
            {
                reg.register(a);
            }
        }
        let ops = Arc::new(Ops {
            settings,
            queue,
            runner,
            undo,
            budget,
        });
        self.services.insert(ops.clone());
        Ok(ops)
    }

    /// Start the runner, the sleep detector and the periodic schedule.
    pub fn start_background(self: &Arc<Self>, ops: &Arc<Ops>) {
        ops.runner.start();
        SleepDetector::new(
            ops.queue.clone(),
            self.services.clock.clone(),
            Arc::new(Wake(self.clone())),
        )
        .start();
        let app = self.clone();
        let queue = ops.queue.clone();
        tokio::spawn(async move {
            let mut sync_tick = tokio::time::interval(std::time::Duration::from_secs(45));
            let mut temporal_tick = tokio::time::interval(std::time::Duration::from_secs(15 * 60));
            let mut maintenance_tick =
                tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
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
        // Refresh requests from Actions (refresh_now, inject) become jobs. One
        // drain task for every Dig, looked up at request time, so an account
        // added after launch is refreshed like any other.
        if let Some(domains) = self.services.get::<crate::domains::Domains>() {
            let mut rx = domains.triggers.subscribe();
            let queue = ops.queue.clone();
            let app = self.clone();
            tokio::spawn(async move {
                loop {
                    let target = match rx.recv().await {
                        Ok(t) => t,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    };
                    let digs = match target {
                        Some(id) => app.dig(&id).ok().into_iter().collect(),
                        None => app.list_digs().unwrap_or_default(),
                    };
                    for d in digs {
                        let _ = queue.enqueue(
                            JobSpec::new(JobKind::IncrementalSync, format!("Refresh {}", d.name))
                                .dig(d.id.clone())
                                .priority(Priority::Interactive),
                        );
                    }
                }
            });
        }
        rinfo!(Category::Jobs, "background work started");
    }

    pub fn enqueue_catch_up_all(&self) {
        let Some(ops) = self.services.get::<Ops>() else {
            return;
        };
        if let Ok(digs) = self.list_digs() {
            for d in digs {
                let _ = ops.queue.enqueue(
                    JobSpec::new(JobKind::CatchUp, format!("Catch up {}", d.name))
                        .dig(d.id.clone()),
                );
            }
        }
    }

    /// Recompute and store a Dig's status; emit the change (§5).
    /// Why a sync failed, from its error and the setup: a refused Gmail
    /// sign-in with no OAuth client configured is a missing setup, not an
    /// expired sign-in.
    fn sync_failure_of(&self, dig_id: &DigId, kind: riddle_errors::ErrorKind) -> SyncFailure {
        let failure = SyncFailure::from_kind(kind);
        if failure != SyncFailure::SignIn {
            return failure;
        }
        let gmail = self
            .dig(dig_id)
            .is_ok_and(|d| d.provider == ProviderKind::Gmail);
        let no_client = self
            .services
            .get::<riddle_settings::SettingsStore>()
            .is_some_and(|s| s.settings().gmail.client_id.trim().is_empty());
        if gmail && no_client {
            SyncFailure::NotSetUp
        } else {
            failure
        }
    }

    /// Record how a sync went, so the account can say it is not syncing.
    /// Every failure counts, not only an expired sign-in; the first one
    /// sets when it started, a sync that gets through clears it. A failure
    /// to record is logged and swallowed (bookkeeping never fails the work).
    pub fn record_sync_result(&self, dig_id: &DigId, error: Option<riddle_errors::ErrorKind>) {
        let result = self.stores.dig(dig_id).and_then(|store| {
            use repo::sync_state::{SYNC_FAILING_SINCE, SYNC_FAILURE};
            match error {
                Some(kind) => {
                    if repo::sync_state::get(&store, SYNC_FAILING_SINCE)?.is_none() {
                        let now = self.services.clock.now().to_rfc3339();
                        repo::sync_state::set(&store, SYNC_FAILING_SINCE, &now)?;
                    }
                    repo::sync_state::set(
                        &store,
                        SYNC_FAILURE,
                        self.sync_failure_of(dig_id, kind).as_str(),
                    )?;
                    Ok(true)
                }
                None => {
                    let was = repo::sync_state::get(&store, SYNC_FAILING_SINCE)?.is_some();
                    if was {
                        repo::sync_state::clear(&store, SYNC_FAILING_SINCE)?;
                        repo::sync_state::clear(&store, SYNC_FAILURE)?;
                    }
                    // Signing in again clears the record before this sync; the
                    // account's shown status still has to catch up.
                    let shown = self
                        .dig(dig_id)
                        .is_ok_and(|d| matches!(d.status, DigStatus::SyncFailing { .. }));
                    Ok(was || shown)
                }
            }
        });
        match result {
            // Each failing attempt re-reads the status, so a network outage
            // shows once it has lasted, not only on its first attempt.
            Ok(true) => self.refresh_dig_status(dig_id),
            Ok(false) => {}
            Err(e) => rwarn!(
                Category::Sync,
                "sync result of {dig_id} not recorded: {}",
                e.kind.as_str()
            ),
        }
    }

    pub fn refresh_dig_status(&self, dig_id: &DigId) {
        let Some(src) = self.services.get::<AppDigStatus>() else {
            return;
        };
        let Ok(mut dig) = self.dig(dig_id) else {
            return;
        };
        if let Ok(status) = src.status_of(&dig) {
            if status != dig.status {
                // Starting to fail is worth an interruption once; the banner
                // carries it after that.
                if let DigStatus::SyncFailing { failure, .. } = &status {
                    if !matches!(dig.status, DigStatus::SyncFailing { .. }) {
                        self.events.emit(RiddleEvent::Notify {
                            title: format!("{} is not syncing", dig.name),
                            body: failure.sentence().to_string(),
                            find_id: None,
                        });
                    }
                }
                dig.status = status.clone();
                let _ = repo::digs::upsert(&self.stores.index(), &dig);
                self.events.emit(RiddleEvent::DigStatusChanged {
                    dig_id: dig_id.clone(),
                    status,
                });
            }
        }
    }

    pub fn task_centre(&self) -> Result<TaskCentre> {
        let ops = self.services.require::<Ops>()?;
        let src = self.services.require::<AppDigStatus>()?;
        Ok(riddle_jobs::snapshot(
            &self.stores,
            &ops.queue,
            src.as_ref(),
            self.services.clock.as_ref(),
        ))
    }

    /// The full boot: settings, domains, AI, judge, ops. Background work
    /// starts only when `background` is true (the CLI runs passes itself).
    pub async fn boot(self: &Arc<Self>, background: bool) -> Result<Arc<Ops>> {
        self.boot_with(background, None).await
    }

    /// `boot` with a settings override applied first (tests, the CLI's flags).
    /// An `offline` config with no explicit override gets the offline one:
    /// the person's settings with the scripted judge, no LLM and no Gmail
    /// client, so no run reaches the network.
    pub async fn boot_with(
        self: &Arc<Self>,
        background: bool,
        override_settings: Option<Settings>,
    ) -> Result<Arc<Ops>> {
        let settings = self.attach_settings()?;
        let override_settings = override_settings.or_else(|| {
            self.config.offline.then(|| {
                let mut s = settings.settings();
                s.judge.provider = JudgeProvider::Scripted;
                s.llm.provider = LlmProvider::Null;
                s.gmail.client_id = String::new();
                s
            })
        });
        if let Some(o) = override_settings {
            // In memory only: an override is this run's, never the person's file.
            settings.override_in_memory(o)?;
            // The watcher starts below and only sees later saves.
            self.load_autonomy(&settings);
        }
        let s = settings.settings();
        self.services
            .insert(Arc::new(crate::live::SecretsChanged::default()));
        self.attach_domains(crate::live::gmail_config(&s))?;
        self.attach_ai(&ai_config(&s));
        self.attach_judge(&judge_config(&s)).await;
        let ops = self.attach_ops(settings.clone())?;
        self.watch_settings(ops.clone(), settings);
        for d in self.list_digs()? {
            self.refresh_dig_status(&d.id);
        }
        self.record_system_timezone();
        // Stored decisions and facts from before this build catch up now:
        // off the boot path in the app, inline for the CLI and tests.
        if background {
            let app = self.clone();
            tokio::task::spawn_blocking(move || app.backfill_all());
            self.start_background(&ops);
            self.enqueue_catch_up_all();
        } else {
            self.backfill_all();
        }
        Ok(ops)
    }

    /// Keep the profile's timezone the system's, so "Expires: end of day"
    /// ends the user's day. Every boot, because the person travels and the
    /// Mac follows; nothing else sets it. Bookkeeping: a failure is logged
    /// and the Finds it would time fall back to UTC.
    pub fn record_system_timezone(&self) {
        let zone = match iana_time_zone::get_timezone() {
            Ok(z) => z,
            Err(e) => {
                rwarn!(Category::App, "system timezone unreadable: {e}");
                return;
            }
        };
        let index = self.stores.index();
        let result = repo::user_profile::get(&index).and_then(|mut p| {
            if p.timezone.as_deref() == Some(zone.as_str()) {
                return Ok(());
            }
            p.timezone = Some(zone.clone());
            repo::user_profile::put(&index, &p)
        });
        if let Err(e) = result {
            rwarn!(Category::App, "timezone not recorded: {}", e.kind.as_str());
        }
    }

    /// Every boot-time backfill, each once per Dig.
    pub fn backfill_all(&self) {
        self.rewrite_all_reasons_in_words();
        // People first: the facts refresh copies each sender's rhythm.
        self.refresh_all_people_cadence();
        self.refresh_all_message_facts();
    }

    /// People recorded before `Person::cadence` existed gain it: every
    /// address of the Dig is refreshed once. Sync keeps it current after.
    pub fn refresh_all_people_cadence(&self) {
        // v2: `on_schedule` joined `cadence`; v3: `every_few_days`.
        // A person's mail spans Digs, and `open_digs` holds only those opened
        // so far: open every one first, or a Dig not yet touched this boot
        // would count as having no mail and be marked done.
        if let Err(e) = self.list_digs().and_then(|digs| {
            digs.iter()
                .try_for_each(|d| self.stores.dig(&d.id).map(drop))
        }) {
            rwarn!(
                Category::App,
                "people cadence deferred: {}",
                e.kind.as_str()
            );
            return;
        }
        self.once_per_dig("people_cadence_v3", |dig_id| {
            let profile = repo::user_profile::get(&self.stores.index())?;
            riddle_people::rebuild_for_dig(
                &self.stores.index(),
                &self.stores.open_digs(),
                dig_id,
                &profile,
                self.services.clock.now(),
            )
        });
    }

    /// Run `body` once on each Dig, remembering in the index store
    /// (`<key>:<dig id>`) which Digs it has run on. The one mechanism behind
    /// every boot-time backfill: two of them carried their own flag, their
    /// own "warn and go on" loop and their own count line. A failure on one
    /// Dig is reported and the rest go on; that Dig runs again next boot,
    /// because the flag is set only after the body succeeds. Tests clear the
    /// flag to run a body again.
    pub fn once_per_dig(&self, key: &str, body: impl Fn(&DigId) -> Result<usize>) {
        let index = self.stores.index();
        let digs = match self.list_digs() {
            Ok(d) => d,
            Err(e) => {
                rwarn!(Category::Sift, "{key} skipped: {}", e.kind.as_str());
                return;
            }
        };
        for d in digs {
            let flag = format!("{key}:{}", d.id);
            match repo::kv::get(&index, &flag) {
                Ok(Some(v)) if v == "1" => continue,
                Ok(_) => {}
                Err(e) => {
                    rwarn!(
                        Category::Sift,
                        "{key} on {} skipped: {}",
                        d.id,
                        e.kind.as_str()
                    );
                    continue;
                }
            }
            match body(&d.id) {
                Ok(n) => {
                    if let Err(e) = repo::kv::set(&index, &flag, "1") {
                        rwarn!(
                            Category::Sift,
                            "{key} on {} done but not recorded: {}",
                            d.id,
                            e.kind.as_str()
                        );
                    }
                    rinfo!(Category::Sift, "{key} on {}: {n} changed", d.id);
                }
                Err(e) => rwarn!(
                    Category::Sift,
                    "{key} on {} skipped: {}",
                    d.id,
                    e.kind.as_str()
                ),
            }
        }
    }
}
