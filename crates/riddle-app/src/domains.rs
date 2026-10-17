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

#[derive(Debug, Clone, PartialEq, Eq)]
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
        Self {
            provider: "jev".into(),
            model: "jev-1.13.0".into(),
            key_source: "ssm".into(),
            ssm_profile: "dev".into(),
            ssm_parameter: "/riddle/jev_api_key".into(),
        }
    }
}

/// Writes Sift's decisions into the provider through the outbox (§15).
/// The autonomy name projection is governed by; a per-action override in
/// settings can set it apart from the Dig's level.
pub const PROJECTION_AUTONOMY: &str = "apply_projection";

/// Whether a Mesh's `Autonomy:` cap keeps its decisions inside Riddle: a
/// cap below Act does; no cap follows the account. The one predicate behind
/// `mesh_projection_held` and the projection hook's cache.
fn cap_holds(cap: Option<Autonomy>) -> bool {
    cap.is_some_and(|a| a < Autonomy::Act)
}

/// Why this Mesh's decisions stay in Riddle whatever the account allows:
/// the Mesh carries an `Autonomy:` cap below Act (Flow 7, per-rule
/// autonomy). `None` when the Mesh follows the account.
pub fn mesh_projection_held(mesh: &Mesh) -> Option<String> {
    mesh.autonomy
        .filter(|a| cap_holds(Some(*a)))
        .map(|a| format!("Mesh {} is capped at {}", mesh.name, a.as_str()))
}

/// Why Sift's decision for this Dig stays in Riddle, or `None` when it may
/// be queued for the mailbox.
///
/// Projection writes labels and archive state in the user's mailbox, but
/// Sift's decisions reached Gmail without passing the Action layer, so
/// Observe and Suggest let them through: the only brake was a Shadow Period
/// nothing ever started. Sift now needs the Dig at Act or above. The user's
/// own moves are Actions, already authorized, and are not held here.
pub fn sift_projection_held(
    policy: &riddle_actions::policy::AutonomyPolicy,
    dig: &DigId,
) -> Option<String> {
    let level = policy.level_for(Some(dig), PROJECTION_AUTONOMY);
    (level < Autonomy::Act).then(|| {
        format!(
            "autonomy is {}; Sift changes Gmail from Act",
            level.as_str()
        )
    })
}

/// What Sift's decisions do to this account's mailbox right now, from the
/// same functions that gate projection, so every client phrases one typed
/// answer instead of re-deriving the rule.
pub fn mailbox_effect(policy: &riddle_actions::policy::AutonomyPolicy, dig: &Dig) -> MailboxEffect {
    let writes = match dig.projection {
        ProjectionMode::None => MailboxWrites::Nothing,
        ProjectionMode::LabelsOnly => MailboxWrites::Labels,
        ProjectionMode::Full => MailboxWrites::LabelsAndArchive,
    };
    if writes == MailboxWrites::Nothing {
        return MailboxEffect { writes, held: None };
    }
    if sift_projection_held(policy, &dig.id).is_some() {
        return MailboxEffect {
            writes: MailboxWrites::Nothing,
            held: Some(HeldReason::Autonomy),
        };
    }
    if sending_held(policy).is_some() {
        return MailboxEffect {
            writes: MailboxWrites::Nothing,
            held: Some(HeldReason::DevUnarmed),
        };
    }
    MailboxEffect { writes, held: None }
}

/// Why the outbox may not send on its own now, whoever queued it: a dev
/// build on Hold or Ask (§59). On Ask the user sends what is waiting with
/// `send_held_changes`; on Hold nothing goes until they choose otherwise.
pub fn sending_held(policy: &riddle_actions::policy::AutonomyPolicy) -> Option<String> {
    (policy.dev && policy.writes != riddle_core::ProviderWrites::Allow).then(|| {
        format!(
            "this dev build's mailbox writes are on {}",
            policy.writes.as_str()
        )
    })
}

pub struct OutboxProjection {
    stores: Arc<riddle_store::Stores>,
    runtime: Arc<riddle_actions::Runtime>,
    /// A Mesh version's autonomy cap never changes once written, so the hook
    /// remembers each one it has looked up: the Sift pipeline calls this once
    /// per message, and a catch-up must not read the same Mesh row ten
    /// thousand times.
    caps: std::sync::Mutex<std::collections::HashMap<(MeshId, u32), Option<Autonomy>>>,
}

impl OutboxProjection {
    pub fn new(stores: Arc<riddle_store::Stores>, runtime: Arc<riddle_actions::Runtime>) -> Self {
        Self {
            stores,
            runtime,
            caps: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// The cap of one Mesh version, read once. Absent Meshes cache as `None`.
    fn mesh_cap(
        &self,
        index: &riddle_store::Store,
        mid: &MeshId,
        version: u32,
    ) -> Result<Option<Autonomy>> {
        let key = (mid.clone(), version);
        if let Some(cap) = self
            .caps
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&key)
        {
            return Ok(*cap);
        }
        let cap = repo::meshes::get(index, mid, version)?.and_then(|m| m.autonomy);
        self.caps
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(key, cap);
        Ok(cap)
    }
}

impl OutboxProjection {
    /// The Dig a classification is projected to, or `None` when it stays in
    /// Riddle: below Act, under a Mesh's own cap, or with projection off.
    fn projecting(&self, dig_id: &DigId, classification: &Classification) -> Result<Option<Dig>> {
        // Below Act, Sift's decision stays in Riddle. It is not queued for
        // later either, so raising autonomy never releases a backlog. Your own
        // corrections (Move to Spoil, set disposition) come through this hook
        // too, as User classifications, and are already authorized.
        if classification.mechanism != DecisionMechanism::User
            && sift_projection_held(&self.runtime.policy.read(), dig_id).is_some()
        {
            return Ok(None);
        }
        let index = self.stores.index();
        // A rule may hold its own decisions below what the account allows.
        if classification.mechanism != DecisionMechanism::User {
            if let (Some(mid), Some(v)) = (&classification.mesh_id, classification.mesh_version) {
                if cap_holds(self.mesh_cap(&index, mid, v)?) {
                    return Ok(None);
                }
            }
        }
        let Some(dig) = repo::digs::get(&index, dig_id)? else {
            return Ok(None);
        };
        if dig.projection == ProjectionMode::None {
            return Ok(None);
        }
        Ok(Some(dig))
    }
}

impl ProjectionHook for OutboxProjection {
    fn project(&self, dig_id: &DigId, classification: &Classification) -> Result<()> {
        let Some(dig) = self.projecting(dig_id, classification)? else {
            return Ok(());
        };
        let store = self.stores.dig(dig_id)?;
        let Some(message) = repo::messages::get(&store, dig_id, &classification.message_id)? else {
            return Ok(());
        };
        riddle_projection::outbox::enqueue_classification(
            &store,
            &dig,
            &message,
            &classification.disposition,
            &classification.strata,
        )?;
        Ok(())
    }
    /// Sift's path: the message it has just decided, not read again
    /// (17 Oct 2026); the gates are the same.
    fn project_message(
        &self,
        dig_id: &DigId,
        message: &Message,
        classification: &Classification,
    ) -> Result<()> {
        let Some(dig) = self.projecting(dig_id, classification)? else {
            return Ok(());
        };
        let store = self.stores.dig(dig_id)?;
        riddle_projection::outbox::enqueue_classification(
            &store,
            &dig,
            message,
            &classification.disposition,
            &classification.strata,
        )?;
        Ok(())
    }
    fn unproject(&self, dig_id: &DigId, message_id: &MessageId) -> Result<()> {
        let index = self.stores.index();
        let Some(dig) = repo::digs::get(&index, dig_id)? else {
            return Ok(());
        };
        let store = self.stores.dig(dig_id)?;
        let Some(message) = repo::messages::get(&store, dig_id, message_id)? else {
            return Ok(());
        };
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
    pub fn attach_domains(
        self: &Arc<Self>,
        gmail: Option<riddle_gmail::GmailOAuthConfig>,
    ) -> Result<Arc<Domains>> {
        let credentials = Arc::new(if self.config.data_dir.is_some() && !self.config.offline {
            Credentials::keyring(self.config.dev)
        } else {
            Credentials::in_memory()
        });
        let providers = Arc::new(ProviderRegistry::new(self.stores.clone()));
        let gmail_factory = Arc::new(riddle_gmail::GmailProviderFactory::new(
            gmail,
            credentials.clone(),
        ));
        providers.set_gmail_factory(gmail_factory.clone());
        // Kept so a changed client id reaches sync without a restart (`live`).
        self.services.insert(gmail_factory);
        let triggers = Arc::new(RefreshTriggers::new());
        let finds = Arc::new(FindService::new(
            self.stores.clone(),
            self.services.clock.clone(),
            self.events.clone(),
        ));
        let outbox = Arc::new(OutboxProcessor::new(self.stores.clone(), providers.clone()));
        let projection: Arc<dyn ProjectionHook> = Arc::new(OutboxProjection::new(
            self.stores.clone(),
            self.runtime.clone(),
        ));

        self.services.insert(credentials.clone());
        self.services.insert(providers.clone());
        self.services.insert(triggers.clone());
        self.services.insert(finds.clone());
        self.services.insert(outbox.clone());
        self.services
            .insert(Arc::new(ProjectionHookHandle(projection)));

        let (signals, meshes) = riddle_sift::bootstrap(&self.stores)?;
        let strata = seed_strata(&self.stores)?;
        rinfo!(
            Category::App,
            "seeded {signals} signals, {meshes} meshes, {strata} strata"
        );

        {
            let mut reg = self.runtime.registry.write();
            for a in riddle_mail::actions::actions()
                .into_iter()
                .chain(riddle_projection::actions())
                .chain(riddle_sync::actions())
                .chain(riddle_sift::actions())
                .chain(riddle_finds::actions())
                .chain(riddle_mesh::actions())
                .chain(riddle_signals::actions())
                .chain(crate::actions::actions())
                .chain(crate::comparison::actions())
                .chain(crate::places::actions())
            {
                reg.register(a);
            }
        }
        let domains = Arc::new(Domains {
            providers,
            triggers,
            credentials,
            finds,
            outbox,
        });
        self.services.insert(domains.clone());
        rinfo!(
            Category::App,
            "domains attached; {} actions registered",
            self.runtime.list_actions().len()
        );
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
                let config = JevConfig {
                    model: cfg.model.clone(),
                    ..Default::default()
                };
                // The keychain key is the `jev_api_key` item the app stores
                // and reports presence for (`set_secret`). The older
                // `<service>/jev` item is still read if that one is absent.
                let stored = (cfg.key_source == "keychain")
                    .then(|| self.services.get::<Credentials>())
                    .flatten()
                    .and_then(|c| c.get(riddle_credentials::JEV_API_KEY).ok().flatten())
                    .filter(|k| !k.trim().is_empty());
                let built = match stored {
                    Some(key) => {
                        JevJudge::new(config, riddle_judge::secret::SecretString::new(key))
                    }
                    None => JevJudge::from_source(config, &source).await,
                };
                match built {
                    Ok(j) => Some(Arc::new(j.with_usage_recorder(recorder))),
                    Err(e) => {
                        rwarn!(
                            Category::Judge,
                            "judge unavailable at start ({}): {}",
                            source.describe(),
                            e.kind.as_str()
                        );
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
        repo::strata::upsert(
            &index,
            &Stratum {
                id: StratumId(format!("stratum_{}", name.to_lowercase().replace(' ', "_"))),
                name: name.to_string(),
                version: 1,
                scope_group: None,
                description: description.to_string(),
                proposed_by_riddle: true,
                message_count: 0,
                created_at: chrono::Utc::now(),
                deleted_at: None,
            },
        )?;
        n += 1;
    }
    Ok(n)
}
