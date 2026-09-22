//! Dig → provider. Registered in `riddle_actions::Services` so Actions and
//! the sync engine reach a Dig's provider without knowing which kind it is.
//! Gmail providers are built through a factory the `riddle-gmail` crate
//! supplies, which keeps this crate free of OAuth and HTTP; fixture
//! providers are built here from the spec persisted in the Dig's store.

use crate::fixture::{FixtureProvider, FixtureSpec};
use crate::provider::MailProvider;
use parking_lot::RwLock;
use riddle_core::{Dig, DigId, ProviderKind};
use riddle_errors::{Result, RiddleError};
use riddle_observability::{rinfo, Category};
use riddle_store::{repo, Stores};
use std::collections::HashMap;
use std::sync::Arc;

/// `sync_state` key holding the fixture spec JSON.
pub const FIXTURE_SPEC_KEY: &str = "fixture_spec";

pub trait ProviderFactory: Send + Sync {
    fn build(&self, dig: &Dig) -> Result<Arc<dyn MailProvider>>;
}

pub struct ProviderRegistry {
    stores: Arc<Stores>,
    providers: RwLock<HashMap<DigId, Arc<dyn MailProvider>>>,
    fixtures: RwLock<HashMap<DigId, Arc<FixtureProvider>>>,
    gmail: RwLock<Option<Arc<dyn ProviderFactory>>>,
}

impl ProviderRegistry {
    pub fn new(stores: Arc<Stores>) -> Self {
        Self {
            stores,
            providers: RwLock::new(HashMap::new()),
            fixtures: RwLock::new(HashMap::new()),
            gmail: RwLock::new(None),
        }
    }

    pub fn set_gmail_factory(&self, f: Arc<dyn ProviderFactory>) {
        *self.gmail.write() = Some(f);
    }

    pub fn insert(&self, dig: DigId, provider: Arc<dyn MailProvider>) {
        self.providers.write().insert(dig, provider);
    }

    /// Register a fixture provider under both maps so `inject` can reach it.
    pub fn insert_fixture(&self, dig: DigId, provider: Arc<FixtureProvider>) {
        self.fixtures.write().insert(dig.clone(), provider.clone());
        self.providers.write().insert(dig, provider);
    }

    pub fn remove(&self, dig: &DigId) {
        self.providers.write().remove(dig);
        self.fixtures.write().remove(dig);
    }

    pub fn get(&self, dig: &DigId) -> Option<Arc<dyn MailProvider>> {
        self.providers.read().get(dig).cloned()
    }

    pub fn fixture(&self, dig: &DigId) -> Option<Arc<FixtureProvider>> {
        self.fixtures.read().get(dig).cloned()
    }

    pub fn dig_ids(&self) -> Vec<DigId> {
        self.providers.read().keys().cloned().collect()
    }

    /// Persist the spec so the provider can be rebuilt after a restart.
    pub fn save_fixture_spec(&self, dig: &DigId, spec: &FixtureSpec) -> Result<()> {
        let store = self.stores.dig(dig)?;
        repo::sync_state::set(&store, FIXTURE_SPEC_KEY, &serde_json::to_string(spec)?)
    }

    pub fn fixture_spec(&self, dig: &Dig) -> Result<FixtureSpec> {
        let store = self.stores.dig(&dig.id)?;
        if let Some(json) = repo::sync_state::get(&store, FIXTURE_SPEC_KEY)? {
            if let Ok(spec) = serde_json::from_str::<FixtureSpec>(&json) {
                return Ok(spec);
            }
        }
        // No spec recorded: derive a stable seed from the Dig id.
        let seed =
            u64::from_str_radix(&riddle_core::sha256_hex(dig.id.as_str())[..12], 16).unwrap_or(1);
        Ok(FixtureSpec {
            seed,
            email: dig.email.clone(),
            name: dig.name.clone(),
            ..Default::default()
        })
    }

    /// The provider for a Dig, built on first use.
    pub fn provider_for(&self, dig: &Dig) -> Result<Arc<dyn MailProvider>> {
        if let Some(p) = self.get(&dig.id) {
            return Ok(p);
        }
        let provider: Arc<dyn MailProvider> = match dig.provider {
            ProviderKind::Fixture => {
                let spec = self.fixture_spec(dig)?;
                let fx = Arc::new(FixtureProvider::new(dig.id.clone(), spec));
                self.insert_fixture(dig.id.clone(), fx.clone());
                rinfo!(Category::Sync, "built fixture provider for dig {}", dig.id);
                return Ok(fx);
            }
            ProviderKind::Gmail => {
                let factory = self.gmail.read().clone().ok_or_else(|| {
                    RiddleError::provider_auth(
                        "Gmail is not configured: add the OAuth client id in settings and sign in",
                    )
                })?;
                factory.build(dig)?
            }
        };
        self.insert(dig.id.clone(), provider.clone());
        rinfo!(
            Category::Sync,
            "built {:?} provider for dig {}",
            dig.provider,
            dig.id
        );
        Ok(provider)
    }
}
