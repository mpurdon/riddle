//! Assembly of the mail and intelligence domains: providers, credentials,
//! the judge, the projection hook, Sift, Finds, and every domain's Actions.
//! `attach_domains` runs once per process after `App::open`; `attach_judge`
//! may run again when settings change the judge.

use crate::ai::store_recorder;
use crate::App;
use riddle_core::*;
use riddle_credentials::Credentials;
use riddle_errors::Result;
use riddle_finds::service::FindService;
use riddle_judge::jev::{JevConfig, JevJudge};
use riddle_judge::keys::JevKeySource;
use riddle_judge::scripted::ScriptedJudge;
use riddle_mail::registry::ProviderRegistry;
use riddle_mail::triggers::RefreshTriggers;
use riddle_observability::{rinfo, rwarn, Category};
use riddle_projection::outbox::OutboxProcessor;
use riddle_store::repo;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct JudgeConfig {
    /// `jev`, `scripted` or `null`.
    pub provider: String,
    pub model: String,
    /// `ssm`, `env` or `keychain`.
    pub key_source: String,
    pub ssm_profile: String,
    pub ssm_parameter: String,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self { provider: "jev".into(), model: "jev-1.13.0".into(), key_source: "ssm".into(), ssm_profile: "dev".into(), ssm_parameter: "/riddle/jev_api_key".into() }
    }
}

/// Writes Sift's decisions into the provider through the outbox (§15).
pub struct OutboxProjection {
    stores: Arc<riddle_store::Stores>,
}

impl ProjectionHook for OutboxProjection {
    fn project(&self, dig_id: &DigId, classification: &Classification) -> Result<()> {
        let index = self.stores.index();
        let Some(dig) = repo::digs::get(&index, dig_id)? else { return Ok(()) };
        if dig.projection == ProjectionMode::None {
            return Ok(());
        }
        let store = self.stores.dig(dig_id)?;
        let Some(message) = repo::messages::get(&store, dig_id, &classification.message_id)? else { return Ok(()) };
        riddle_projection::outbox::enqueue_classification(&store, &dig, &message, &classification.disposition, &classification.strata)?;
        Ok(())
    }
    fn unproject(&self, dig_id: &DigId, message_id: &MessageId) -> Result<()> {
        let index = self.stores.index();
        let Some(dig) = repo::digs::get(&index, dig_id)? else { return Ok(()) };
        let store = self.stores.dig(dig_id)?;
        let Some(message) = repo::messages::get(&store, dig_id, message_id)? else { return Ok(()) };
        riddle_projection::outbox::undo_projection(&store, &dig, &message, true, "undo")?;
        Ok(())
    }
}

pub struct Domains {
    pub providers: Arc<ProviderRegistry>,
    pub triggers: Arc<RefreshTriggers>,
    pub credentials: Arc<Credentials>,
    pub finds: Arc<FindService>,
    pub outbox: Arc<OutboxProcessor>,
}

impl App {
    /// Register providers, credentials, projection, Finds and every domain
    /// Action. Seeds system Signals, default Meshes and default Strata.
    pub fn attach_domains(self: &Arc<Self>, gmail: Option<riddle_gmail::GmailOAuthConfig>) -> Result<Arc<Domains>> {
        let credentials = Arc::new(if self.config.data_dir.is_some() { Credentials::keyring(self.config.dev) } else { Credentials::in_memory() });
        let providers = Arc::new(ProviderRegistry::new(self.stores.clone()));
        providers.set_gmail_factory(Arc::new(riddle_gmail::GmailProviderFactory::new(gmail, credentials.clone())));
        let triggers = Arc::new(RefreshTriggers::new());
        let finds = Arc::new(FindService::new(self.stores.clone(), self.services.clock.clone(), self.events.clone()));
        let outbox = Arc::new(OutboxProcessor::new(self.stores.clone(), providers.clone()));
        let projection: Arc<dyn ProjectionHook> = Arc::new(OutboxProjection { stores: self.stores.clone() });

        self.services.insert(credentials.clone());
        self.services.insert(providers.clone());
        self.services.insert(triggers.clone());
        self.services.insert(finds.clone());
        self.services.insert(outbox.clone());
        self.services.insert(Arc::new(ProjectionHookHandle(projection)));

        let (signals, meshes) = riddle_sift::bootstrap(&self.stores)?;
        let strata = seed_strata(&self.stores)?;
        rinfo!(Category::App, "seeded {signals} signals, {meshes} meshes, {strata} strata");

        {
            let mut reg = self.runtime.registry.write();
            for a in riddle_mail::actions::actions().into_iter()
                .chain(riddle_projection::actions())
                .chain(riddle_sync::actions())
                .chain(riddle_sift::actions())
                .chain(riddle_finds::actions())
                .chain(riddle_mesh::actions())
                .chain(riddle_signals::actions())
            {
                reg.register(a);
            }
        }
        let domains = Arc::new(Domains { providers, triggers, credentials, finds, outbox });
        self.services.insert(domains.clone());
        rinfo!(Category::App, "domains attached; {} actions registered", self.runtime.list_actions().len());
        Ok(domains)
    }

    /// Build the judge from settings and install it. Key resolution may hit
    /// SSM, so this is async; a failure leaves the judge unset and Sift
    /// degrades to Unresolved (§18), which is logged, not fatal.
    pub async fn attach_judge(self: &Arc<Self>, cfg: &JudgeConfig) -> Option<String> {
        let recorder = store_recorder(self.stores.clone());
        let judge: Option<Arc<dyn SemanticJudge>> = match cfg.provider.as_str() {
            "jev" => {
                let source = match cfg.key_source.as_str() {
                    "env" => JevKeySource::env(),
                    "keychain" => JevKeySource::keychain(self.config.keychain_service.clone()),
                    _ => JevKeySource::ssm(cfg.ssm_profile.clone(), cfg.ssm_parameter.clone()),
                };
                let config = JevConfig { model: cfg.model.clone(), ..Default::default() };
                match JevJudge::from_source(config, &source).await {
                    Ok(j) => Some(Arc::new(j.with_usage_recorder(recorder))),
                    Err(e) => {
                        rwarn!(Category::Judge, "judge unavailable at start ({}): {}", source.describe(), e.kind.as_str());
                        None
                    }
                }
            }
            "scripted" => Some(Arc::new(ScriptedJudge::new())),
            _ => None,
        };
        let identity = judge.as_ref().map(|j| j.identity().key());
        *self.services.judge.write() = judge;
        if let Some(id) = &identity {
            rinfo!(Category::Judge, "judge attached: {id}");
        }
        identity
    }
}

fn seed_strata(stores: &riddle_store::Stores) -> Result<usize> {
    let index = stores.index();
    let existing = repo::strata::list(&index, true)?;
    let mut n = 0;
    for (name, description) in Stratum::DEFAULTS {
        if existing.iter().any(|s| s.name == *name) {
            continue;
        }
        repo::strata::upsert(&index, &Stratum {
            id: StratumId(format!("stratum_{}", name.to_lowercase().replace(' ', "_"))),
            name: name.to_string(),
            version: 1,
            scope_group: None,
            description: description.to_string(),
            proposed_by_riddle: true,
            message_count: 0,
            created_at: chrono::Utc::now(),
            deleted_at: None,
        })?;
        n += 1;
    }
    Ok(n)
}
