//! Settings take effect when they are saved, not on the next launch.
//!
//! Boot read each setting once, and nothing listened afterwards: a saved
//! autonomy level, budget, Gmail client id or judge choice changed the file
//! and the form but not the running app, which kept the old values until a
//! restart. `watch` subscribes to the settings store and to `SecretsChanged`
//! (a key stored in the keychain) and re-applies whatever part changed. The
//! judge and the LLM are rebuilt only when their own configuration changed,
//! because a rebuild may resolve a key from SSM.

use crate::ops::{ai_config, judge_config, Ops};
use crate::App;
use riddle_observability::{rinfo, Category};
use riddle_settings::{Settings, SettingsStore};
use std::sync::Arc;
use tokio::sync::Notify;

/// Signalled after a secret is stored or cleared, so the judge can pick up a
/// new key. The value is never carried; the judge reads it from the keychain.
#[derive(Default)]
pub struct SecretsChanged(pub Notify);

impl App {
    /// Apply the parts of `next` that differ from `prev` to the running app.
    /// `secrets` forces a judge rebuild even when its configuration is equal.
    pub async fn apply_settings(
        self: &Arc<Self>,
        ops: &Ops,
        prev: &Settings,
        next: &Settings,
        secrets: bool,
    ) {
        let mut applied: Vec<&str> = Vec::new();
        if prev.autonomy != next.autonomy {
            let mut policy = ops.settings.autonomy_policy();
            let mut current = self.runtime.policy.write();
            policy.writes = current.writes;
            policy.dev = current.dev;
            *current = policy;
            applied.push("autonomy");
        }
        if prev.budgets != next.budgets {
            ops.budget
                .set_limits(next.budgets.daily_judge_usd, next.budgets.daily_llm_usd);
            applied.push("budgets");
        }
        if prev.gmail.client_id != next.gmail.client_id {
            if let Some(factory) = self.services.get::<riddle_gmail::GmailProviderFactory>() {
                factory.set_config(gmail_config(next));
            }
            // A client is set up now: "not set up" is history, so Sync now
            // runs instead of refusing until the next poll.
            for dig in self.list_all_digs().unwrap_or_default() {
                let Ok(store) = self.stores.dig(&dig.id) else {
                    continue;
                };
                let not_set_up = riddle_store::repo::sync_state::get(
                    &store,
                    riddle_store::repo::sync_state::SYNC_FAILURE,
                )
                .ok()
                .flatten()
                .is_some_and(|f| {
                    riddle_core::SyncFailure::parse(&f) == riddle_core::SyncFailure::NotSetUp
                });
                if not_set_up {
                    self.record_sync_result(&dig.id, None);
                }
            }
            applied.push("gmail");
        }
        // The level changes the Director's register, not its client: tell
        // the running Director and leave the LLM alone.
        if prev.ui_level != next.ui_level {
            if let Some(ai) = self.services.get::<crate::ai::AiServices>() {
                ai.director.set_ui_level(next.ui_level);
            }
            applied.push("level");
        }
        if ai_config(prev) != ai_config(next) {
            self.attach_ai(&ai_config(next));
            applied.push("llm");
        }
        if secrets || judge_config(prev) != judge_config(next) {
            self.attach_judge(&judge_config(next)).await;
            applied.push("judge");
        }
        if !applied.is_empty() {
            rinfo!(
                Category::App,
                "settings applied live: {}",
                applied.join(", ")
            );
        }
    }

    /// Follow the settings store and `SecretsChanged` for the life of the
    /// process. Requires a Tokio runtime; `boot_with` starts it.
    pub fn watch_settings(self: &Arc<Self>, ops: Arc<Ops>, store: Arc<SettingsStore>) {
        let secrets = self.services.get::<SecretsChanged>();
        let app = self.clone();
        let mut rx = store.subscribe();
        tokio::spawn(async move {
            let mut prev = rx.borrow_and_update().clone();
            loop {
                let secrets_changed = async {
                    match &secrets {
                        Some(s) => s.0.notified().await,
                        None => std::future::pending().await,
                    }
                };
                let from_secrets = tokio::select! {
                    changed = rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        false
                    }
                    _ = secrets_changed => true,
                };
                let next = rx.borrow_and_update().clone();
                app.apply_settings(&ops, &prev, &next, from_secrets).await;
                prev = next;
            }
        });
    }
}

/// The Gmail OAuth client from settings; `None` until a client id is set.
pub fn gmail_config(s: &Settings) -> Option<riddle_gmail::GmailOAuthConfig> {
    let id = s.gmail.client_id.trim();
    (!id.is_empty()).then(|| riddle_gmail::GmailOAuthConfig::new(id.to_string(), None))
}
