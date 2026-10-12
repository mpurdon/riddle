//! Assembly of the AI boundary: the LLM, the escalation refiner, the
//! Director and the Expedition runner, wired to the usage ledger and the
//! Action runtime. Settings feed `AiConfig`; until the settings track lands
//! the defaults match `docs/COMMANDS.md`.

use crate::director_reads::AppReads;
use crate::App;
use riddle_agent::director::{Director, DirectorConfig};
use riddle_agent::reads::DirectorReadsHandle;
use riddle_agent::tool_loop::ToolLoop;
use riddle_ai::bedrock::{BedrockConfig, BedrockLlm};
use riddle_ai::catalog::{ModelCatalog, ModelTier};
use riddle_ai::llm::Llm;
use riddle_ai::prompts::PromptStore;
use riddle_ai::refiner::LlmRefiner;
use riddle_ai::scripted::ScriptedLlm;
use riddle_ai::usage::UsageRecorder;
use riddle_core::{PrivacyMode, SignalRefiner, UsageRow};
use riddle_expeditions::runner::{ExpeditionConfig, ExpeditionRunner};
use riddle_observability::{rerror, rinfo, Category};
use riddle_store::{repo, Stores};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiConfig {
    /// `bedrock`, `scripted` or `null`.
    pub provider: String,
    pub profile: Option<String>,
    pub region: String,
    pub escalation_model: String,
    pub director_model: String,
    pub prompt_override_dir: Option<std::path::PathBuf>,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            provider: "bedrock".into(),
            profile: Some("dev".into()),
            region: "us-east-1".into(),
            escalation_model: riddle_ai::catalog::HAIKU_4_5.into(),
            director_model: riddle_ai::catalog::SONNET_4_5.into(),
            prompt_override_dir: None,
        }
    }
}

tokio::task_local! {
    /// The Dig a model call is being made for, set by the engine around
    /// each Sift so rows from the judge and the refiner (which know no Dig)
    /// are attributed (§56: task-local attribution).
    pub static CURRENT_DIG: riddle_core::DigId;
}

/// Writes usage rows to the Dig store when attributed, else the index store.
pub fn store_recorder(stores: Arc<Stores>) -> UsageRecorder {
    Arc::new(move |mut row: UsageRow| {
        if row.dig_id.is_none() {
            row.dig_id = CURRENT_DIG.try_with(|d| d.clone()).ok();
        }
        let target = match &row.dig_id {
            Some(d) => stores.dig(d),
            None => Ok(stores.index()),
        };
        match target.and_then(|s| repo::usage::record(&s, &row)) {
            Ok(()) => {}
            Err(e) => rerror!(Category::Llm, "usage row not recorded: {e}"),
        }
    })
}

pub struct AiServices {
    pub llm: Arc<dyn Llm>,
    pub catalog: Arc<ModelCatalog>,
    pub prompts: Arc<PromptStore>,
    pub director: Arc<Director>,
    pub expeditions: Arc<ExpeditionRunner>,
}

impl App {
    /// Build the LLM, refiner, Director and Expedition runner and register
    /// them with `Services`. Idempotent per process: calling it again
    /// replaces the previous set (settings changed).
    pub fn attach_ai(self: &Arc<Self>, cfg: &AiConfig) -> Arc<AiServices> {
        self.attach_ai_with(cfg, None)
    }

    /// `attach_ai` with a given model client, for tests that script one.
    pub fn attach_ai_with(
        self: &Arc<Self>,
        cfg: &AiConfig,
        given: Option<Arc<dyn Llm>>,
    ) -> Arc<AiServices> {
        let mut catalog = ModelCatalog::default();
        catalog.set_tier(ModelTier::Escalation, cfg.escalation_model.clone());
        catalog.set_tier(ModelTier::Verification, cfg.escalation_model.clone());
        catalog.set_tier(ModelTier::Director, cfg.director_model.clone());
        catalog.set_tier(ModelTier::Expedition, cfg.director_model.clone());
        let catalog = Arc::new(catalog);
        let recorder = store_recorder(self.stores.clone());
        let llm: Arc<dyn Llm> = match (given, cfg.provider.as_str()) {
            (Some(l), _) => l,
            (None, provider) => match provider {
                "bedrock" => Arc::new(
                    BedrockLlm::new(
                        BedrockConfig {
                            profile: cfg.profile.clone(),
                            region: cfg.region.clone(),
                            default_model: cfg.escalation_model.clone(),
                            ..Default::default()
                        },
                        catalog.clone(),
                    )
                    .with_recorder(recorder),
                ),
                _ => Arc::new(ScriptedLlm::new(vec![])),
            },
        };
        let prompts = Arc::new(match &cfg.prompt_override_dir {
            Some(d) => PromptStore::with_override_dir(d),
            None => PromptStore::embedded(),
        });
        // Escalation: one refiner per privacy mode that allows the LLM; Sift
        // picks by Dig. Cloud reasoning is the only mode that permits it.
        let refiner: Arc<dyn SignalRefiner> = Arc::new(
            LlmRefiner::new(
                llm.clone(),
                prompts.clone(),
                Some(cfg.escalation_model.clone()),
            )
            .with_privacy(PrivacyMode::CloudReasoning),
        );
        // Re-run on a settings change, so "null" must also clear a refiner
        // an earlier provider installed.
        *self.services.refiner.write() = (cfg.provider != "null").then_some(refiner);
        let reads: Arc<dyn riddle_agent::reads::DirectorReads> = Arc::new(AppReads(self.clone()));
        self.services
            .insert(Arc::new(DirectorReadsHandle(reads.clone())));
        let events: Arc<dyn riddle_core::EventSink> = self.events.clone();
        let tool_loop = Arc::new(ToolLoop::new(
            llm.clone(),
            self.runtime.clone(),
            reads,
            events.clone(),
        ));
        let director = Arc::new(Director::new(
            tool_loop.clone(),
            self.stores.clone(),
            events.clone(),
            prompts.clone(),
            self.services.clock.clone(),
            DirectorConfig {
                model_id: Some(cfg.director_model.clone()),
                ..Default::default()
            },
        ));
        self.services.insert(director.clone());
        let expeditions = ExpeditionRunner::new(
            tool_loop,
            self.stores.clone(),
            events,
            prompts.clone(),
            self.services.clock.clone(),
            ExpeditionConfig {
                model_id: Some(cfg.director_model.clone()),
                ..Default::default()
            },
        );
        expeditions.register(&self.services);
        for a in riddle_expeditions::actions() {
            self.runtime.registry.write().register(a);
        }
        rinfo!(
            Category::Llm,
            "ai attached: provider={} escalation={} director={}",
            cfg.provider,
            cfg.escalation_model,
            cfg.director_model
        );
        let ai = Arc::new(AiServices {
            llm,
            catalog,
            prompts,
            director,
            expeditions,
        });
        self.services.insert(ai.clone());
        ai
    }
}
