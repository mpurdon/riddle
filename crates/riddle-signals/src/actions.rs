//! Signal Actions (§33): the only way the Director reaches the registry or
//! the judge. `test_signal` runs experimentally and never touches production
//! (§34); `promote_signal` is the production step and requires approval.

use crate::registry::{PromoteOptions, Registry};
use crate::sets::{SetId, StateBuilder, StateInput};
use async_trait::async_trait;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview};
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn all() -> Vec<Arc<dyn Action>> {
    vec![
        Arc::new(ProposeSignal),
        Arc::new(TestSignal),
        Arc::new(PromoteSignal),
        Arc::new(EvaluateSignal),
    ]
}

fn created_by(p: &Principal) -> CreatedBy {
    match p {
        Principal::User | Principal::Cli => CreatedBy::User,
        Principal::Director { .. } | Principal::Expedition { .. } => CreatedBy::Director,
        _ => CreatedBy::System,
    }
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| RiddleError::invalid(format!("missing argument {key}")))
}

pub struct ProposeSignal;

#[async_trait]
impl Action for ProposeSignal {
    fn meta(&self) -> ActionMeta {
        meta("propose_signal", "Propose a new Signal version for testing")
            .mutating()
            .reversible()
            .risk(Risk::Low)
            .autonomy(Autonomy::Suggest)
            .build()
    }
    async fn preview(&self, _ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let s: Signal = serde_json::from_value(args.get("signal").cloned().unwrap_or(Value::Null))
            .map_err(|e| RiddleError::invalid(format!("signal: {e}")))?;
        Ok(Preview {
            summary: format!("Propose Signal {} (\"{}\")", s.name, s.definition),
            real_arguments: json!({ "name": s.name, "definition": s.definition, "primitive": s.primitive, "choices": s.choices }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let s: Signal = serde_json::from_value(args.get("signal").cloned().unwrap_or(Value::Null))
            .map_err(|e| RiddleError::invalid(format!("signal: {e}")))?;
        let registry = Registry::new(ctx.services.stores.index());
        let proposed = registry.propose(s, created_by(&ctx.principal))?;
        Ok(ActionResult::ok(
            serde_json::to_value(&proposed)?,
            format!("Proposed Signal {}@v{}", proposed.name, proposed.version),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Signal, &proposed.id)])
        .with_inputs(json!({ "name": proposed.name, "version": proposed.version }))
        .with_undo(
            json!({ "name": proposed.name, "version": proposed.version }),
            "Reject the proposal",
        ))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let name = arg_str(&restore, "name")?;
        let version = restore["version"].as_u64().unwrap_or(0) as u32;
        let registry = Registry::new(ctx.services.stores.index());
        let s = registry.set_status(name, version, SignalStatus::Rejected)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&s)?,
            format!("Rejected Signal {name}@v{version}"),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Signal, &s.id)]))
    }
}

pub struct TestSignal;

#[async_trait]
impl Action for TestSignal {
    fn meta(&self) -> ActionMeta {
        meta(
            "test_signal",
            "Run a Signal version or a candidate definition against the historical corpus",
        )
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let name = arg_str(&args, "name")?.to_string();
        let registry = Registry::new(ctx.services.stores.index());
        let signal = match args.get("definition").and_then(|d| d.as_str()) {
            Some(definition) => {
                let mut s = Signal::noul(&name, definition);
                if let Some(choices) = args
                    .get("choices")
                    .and_then(|c| serde_json::from_value::<Vec<SignalChoice>>(c.clone()).ok())
                {
                    if !choices.is_empty() {
                        s.primitive = SignalPrimitive::Choice;
                        s.choices = choices;
                    }
                }
                s.version = args.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                s.status = SignalStatus::Experimental;
                if let Err(problems) = s.validate() {
                    return Err(RiddleError::invalid(
                        "candidate definition breaks the authoring rules",
                    )
                    .with_detail(json!({ "problems": problems })));
                }
                s
            }
            None => match args.get("version").and_then(|v| v.as_u64()) {
                Some(v) => registry.resolve(&name, v as u32)?,
                None => repo::signals::latest(&ctx.services.stores.index(), &name, &[])?
                    .ok_or_else(|| RiddleError::not_found(format!("signal {name}")))?,
            },
        };
        let judge = ctx
            .services
            .judge()
            .ok_or_else(|| RiddleError::judge_unavailable("no judge configured"))?;
        let sample_size = args
            .get("sampleSize")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as u32;
        let digs: Vec<DigId> = match &ctx.dig_id {
            Some(d) => vec![d.clone()],
            None => repo::digs::list(&ctx.services.stores.index())?
                .into_iter()
                .map(|d| d.id)
                .collect(),
        };
        let report = crate::testing::test_signal(
            &ctx.services.stores,
            &digs,
            judge.as_ref(),
            &signal,
            sample_size,
        )
        .await?;
        Ok(ActionResult::ok(
            serde_json::to_value(&report)?,
            format!(
                "Tested Signal {}@v{} on {} samples",
                report.name, report.version, report.sample_size
            ),
        ))
    }
}

pub struct PromoteSignal;

#[async_trait]
impl Action for PromoteSignal {
    fn meta(&self) -> ActionMeta {
        meta("promote_signal", "Promote a Signal version to production")
            .mutating()
            .reversible()
            .risk(Risk::Medium)
            .confirm(ConfirmationPolicy::Approval)
            .autonomy(Autonomy::Trusted)
            .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        let name = arg_str(args, "name")?;
        let version = args["version"].as_u64().unwrap_or(0) as u32;
        let registry = Registry::new(ctx.services.stores.index());
        let s = registry.resolve(name, version)?;
        let report = crate::testing::latest_report(&ctx.services.stores.index(), name, version)?;
        Ok(Preview {
            summary: format!(
                "Promote {}@v{} to ACTIVE{}",
                name,
                version,
                if report.is_none() { " (untested)" } else { "" }
            ),
            real_arguments: json!({ "name": name, "version": version, "definition": s.definition, "tested": report.is_some(), "agreement": report.as_ref().map(|r| r.agreement) }),
        })
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let name = arg_str(&args, "name")?.to_string();
        let version = args["version"].as_u64().unwrap_or(0) as u32;
        let index = ctx.services.stores.index();
        let registry = Registry::new(index.clone());
        let prior_active = registry
            .versions(&name)?
            .into_iter()
            .find(|s| s.status == SignalStatus::Active)
            .map(|s| s.version);
        let prior_status = registry.resolve(&name, version)?.status;
        let report = crate::testing::latest_report(&index, &name, version)?;
        let override_untested = args
            .get("override")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            && ctx.principal.is_user();
        let promoted = registry.promote(
            &name,
            version,
            PromoteOptions {
                report: report.as_ref(),
                override_untested,
            },
        )?;
        Ok(ActionResult::ok(serde_json::to_value(&promoted)?, format!("Promoted Signal {name}@v{version}"))
            .with_affected(vec![EntityRef::new(EntityKind::Signal, &promoted.id)])
            .with_inputs(json!({ "name": name, "version": version }))
            .with_undo(json!({ "name": name, "version": version, "priorActive": prior_active, "priorStatus": prior_status }), "Revert the promotion"))
    }
    async fn undo(&self, ctx: &ActionContext, restore: Value) -> Result<ActionResult> {
        let name = arg_str(&restore, "name")?.to_string();
        let version = restore["version"].as_u64().unwrap_or(0) as u32;
        let prior_status: SignalStatus = serde_json::from_value(restore["priorStatus"].clone())
            .unwrap_or(SignalStatus::Candidate);
        let registry = Registry::new(ctx.services.stores.index());
        let s = registry.set_status(&name, version, prior_status)?;
        if let Some(v) = restore["priorActive"].as_u64() {
            registry.set_status(&name, v as u32, SignalStatus::Active)?;
        }
        Ok(ActionResult::ok(
            serde_json::to_value(&s)?,
            format!("Reverted promotion of {name}@v{version}"),
        )
        .with_affected(vec![EntityRef::new(EntityKind::Signal, &s.id)]))
    }
}

pub struct EvaluateSignal;

#[async_trait]
impl Action for EvaluateSignal {
    fn meta(&self) -> ActionMeta {
        meta(
            "evaluate_signal",
            "Evaluate one Signal version on one message exactly as production would",
        )
        .build()
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let dig = DigId(arg_str(&args, "digId")?.to_string());
        let message_id = MessageId(arg_str(&args, "messageId")?.to_string());
        let name = arg_str(&args, "name")?;
        let version = args["version"].as_u64().unwrap_or(0) as u32;
        let registry = Registry::new(ctx.services.stores.index());
        let signal = registry.resolve(name, version)?;
        let judge = ctx
            .services
            .judge()
            .ok_or_else(|| RiddleError::judge_unavailable("no judge configured"))?;
        let store = ctx.services.stores.dig(&dig)?;
        let m = repo::messages::get(&store, &dig, &message_id)?
            .ok_or_else(|| RiddleError::not_found("message"))?;
        let input = StateInput {
            subject: m.subject.clone(),
            from: m.from.email.clone(),
            normalized_text: m.body_text.clone().unwrap_or_else(|| m.snippet.clone()),
            ..Default::default()
        };
        let built =
            StateBuilder::default().build(SetId::Baseline, std::slice::from_ref(&signal), &input);
        let outcome = crate::cache::cached_or_evaluate(
            &store,
            judge.as_ref(),
            &message_id,
            &[signal],
            &built.state,
            &built.context_hash,
            true,
        )
        .await?;
        let e = outcome
            .evaluations
            .into_iter()
            .next()
            .ok_or_else(|| RiddleError::judge_unavailable("the judge returned no answer"))?;
        Ok(ActionResult::ok(
            serde_json::to_value(&e)?,
            format!("Evaluated {}@v{} on one message", name, version),
        ))
    }
}
