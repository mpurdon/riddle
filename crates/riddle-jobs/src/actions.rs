//! Job control Actions: cancel, pause, resume. Mutating (they change what
//! Riddle does next) but local and not reversible: a cancelled job is gone,
//! and resuming is its own Action. The Director may control jobs at ACT so
//! "stop the backfill" works without a prompt.

use crate::queue::JobQueue;
use async_trait::async_trait;
use riddle_actions::policy::ensure_args;
use riddle_actions::{meta, Action, ActionContext, ActionResult, Preview};
use riddle_core::{ActionMeta, Autonomy, EntityKind, EntityRef, JobId, Risk};
use riddle_errors::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn actions() -> Vec<Arc<dyn Action>> {
    vec![Arc::new(CancelJob), Arc::new(PauseJob), Arc::new(ResumeJob)]
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobArgs {
    job_id: JobId,
}

fn queue(ctx: &ActionContext) -> Result<Arc<JobQueue>> {
    ctx.services.require::<JobQueue>()
}

async fn preview_for(ctx: &ActionContext, args: &Value, verb: &str) -> Result<Preview> {
    let a: JobArgs = ensure_args(args)?;
    let job = queue(ctx)?.get(&a.job_id)?.ok_or_else(|| {
        riddle_errors::RiddleError::not_found(format!("job {} not found", a.job_id))
    })?;
    Ok(Preview {
        summary: format!("{verb} {}", job.label),
        real_arguments: json!({ "jobId": job.id, "kind": job.kind, "label": job.label }),
    })
}

pub struct CancelJob;

#[async_trait]
impl Action for CancelJob {
    fn meta(&self) -> ActionMeta {
        meta("cancel_job", "Cancel a background job")
            .mutating()
            .risk(Risk::Low)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        preview_for(ctx, args, "Cancel").await
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: JobArgs = ensure_args(&args)?;
        let job = queue(ctx)?.cancel(&a.job_id)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&job)?,
            format!("Cancelled {}", job.label),
        )
        .with_inputs(json!({ "jobId": job.id }))
        .with_affected(vec![EntityRef::new(EntityKind::Job, &job.id)]))
    }
}

pub struct PauseJob;

#[async_trait]
impl Action for PauseJob {
    fn meta(&self) -> ActionMeta {
        meta("pause_job", "Pause a background job")
            .mutating()
            .risk(Risk::Low)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        preview_for(ctx, args, "Pause").await
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: JobArgs = ensure_args(&args)?;
        let job = queue(ctx)?.pause(&a.job_id)?;
        Ok(
            ActionResult::ok(serde_json::to_value(&job)?, format!("Paused {}", job.label))
                .with_inputs(json!({ "jobId": job.id }))
                .with_affected(vec![EntityRef::new(EntityKind::Job, &job.id)]),
        )
    }
}

pub struct ResumeJob;

#[async_trait]
impl Action for ResumeJob {
    fn meta(&self) -> ActionMeta {
        meta("resume_job", "Resume a paused job")
            .mutating()
            .risk(Risk::Low)
            .autonomy(Autonomy::Act)
            .build()
    }
    async fn preview(&self, ctx: &ActionContext, args: &Value) -> Result<Preview> {
        preview_for(ctx, args, "Resume").await
    }
    async fn execute(&self, ctx: &ActionContext, args: Value) -> Result<ActionResult> {
        let a: JobArgs = ensure_args(&args)?;
        let job = queue(ctx)?.resume(&a.job_id)?;
        Ok(ActionResult::ok(
            serde_json::to_value(&job)?,
            format!("Resumed {}", job.label),
        )
        .with_inputs(json!({ "jobId": job.id }))
        .with_affected(vec![EntityRef::new(EntityKind::Job, &job.id)]))
    }
}
