//! `refresh_now`: wake the poller(s). The Notify lives in `riddle-mail`'s
//! `RefreshTriggers` so Demo Mode's injection can wake the same loop.

use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult};
use riddle_core::{ActionMeta, Autonomy, DigId, Risk, SyncFailure};
use riddle_errors::{Result, RiddleError};
use riddle_mail::actions::util::opt;
use riddle_mail::RefreshTriggers;
use riddle_store::repo;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct RefreshNow;
#[async_trait]
impl Action for RefreshNow {
    fn meta(&self) -> ActionMeta {
        // Mutating only in the sense that it schedules work; nothing in the store changes.
        meta("refresh_now", "Sync now")
            .mutating()
            .risk(Risk::Low)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let dig: Option<DigId> = opt(&args, "digId")?;
        let dig = dig.or_else(|| ctx.dig_id.clone());
        // An account the provider will not let in cannot sync however often
        // it is asked; say so rather than "Syncing now". It used to answer
        // "Syncing now" through two weeks of an expired sign-in.
        if let Some(d) = &dig {
            let store = ctx.services.stores.dig(d)?;
            let failure = repo::sync_state::get(&store, repo::sync_state::SYNC_FAILURE)?
                .map(|f| SyncFailure::parse(&f));
            match failure {
                Some(SyncFailure::SignIn) => {
                    return Err(RiddleError::provider_auth(
                        "this account's sign-in has expired; sign in again under Settings → Accounts, then sync",
                    ))
                }
                Some(SyncFailure::AdminBlocked) => {
                    return Err(RiddleError::new(
                        riddle_errors::ErrorKind::ProviderAdminBlocked,
                        "a Workspace administrator blocks Riddle from this account",
                    ))
                }
                _ => {}
            }
        }
        let triggers = ctx.services.require::<RefreshTriggers>()?;
        let heard = triggers.trigger(dig.as_ref());
        let summary = match (heard, &dig) {
            (true, Some(_)) => "Syncing now".to_string(),
            (true, None) => "Syncing all accounts now".to_string(),
            // A CLI without background work has no drain task; say so
            // rather than reporting a sync that will not happen.
            (false, _) => "Sync requested, but nothing is listening in this process".to_string(),
        };
        let mut r = ActionResult::ok(json!({"digId": dig, "heard": heard}), summary)
            .with_inputs(json!({"digId": dig}));
        r.undo = riddle_core::UndoInfo::NotUndoable {
            reason: "a refresh only reads".into(),
        };
        Ok(r)
    }
}

pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![Arc::new(RefreshNow)]
}
