//! The processing engine: one pass per Dig runs sync, drains the Sift
//! backlog newest first (§5 catch-up order), applies Find proposals,
//! projects decisions through the outbox, and ticks Temporal Sift. The job
//! runners call these functions; the CLI and tests call them directly.
//!
//! Nothing here decides anything: Sift decides, Finds dedupe, the outbox
//! writes. The engine only sequences and reports.

use crate::domains::Domains;
use crate::App;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_finds::temporal::TemporalOptions;
use riddle_observability::{rdebug, rinfo, rwarn, Category, Timed};
use riddle_sift::{DigContext, PeopleFacts, Sift, SiftInput};
use riddle_store::repo;
use riddle_sync::engine::{CancelFlag, SyncEngine, SyncProgress, SyncReport};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiftPassReport {
    pub processed: u64,
    pub finds_created: u64,
    pub finds_joined: u64,
    pub unresolved: u64,
    pub hidden: u64,
    pub failed: u64,
    pub judge_unavailable: bool,
    pub remaining: u64,
    pub cached_signals: u64,
    pub fresh_signals: u64,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassReport {
    pub dig_id: String,
    pub sync: Option<SyncSummary>,
    pub sift: SiftPassReport,
    pub projected: u64,
    pub projection_conflicts: u64,
    pub temporal: Vec<String>,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub kind: String,
    pub added: u64,
    pub updated: u64,
    pub deleted: u64,
    pub external_moves: u64,
    pub complete: bool,
}

impl From<&SyncReport> for SyncSummary {
    fn from(r: &SyncReport) -> Self {
        Self { kind: format!("{:?}", r.kind).to_lowercase(), added: r.added, updated: r.updated, deleted: r.deleted, external_moves: r.external_moves.len() as u64, complete: r.complete }
    }
}

impl App {
    fn domains(&self) -> Result<Arc<Domains>> { self.services.require::<Domains>() }

    /// Build the full Sift input for a stored message: normalized text,
    /// People facts, Thread State, Context slices (§44: retrieved, not stuffed).
    pub fn sift_input(&self, dig: &Dig, message: Message) -> Result<SiftInput> {
        let index = self.stores.index();
        let store = self.stores.dig(&dig.id)?;
        let mut profile = repo::user_profile::get(&index)?;
        if !profile.is_me(&dig.email) {
            profile.addresses.push(dig.email.clone());
        }
        let normalized = riddle_mail::normalize::normalize_message(&message);
        let person = riddle_people::lookup(&index, &message.from.email)?;
        let people = PeopleFacts::from_person(person.as_ref());
        let thread_state = repo::thread_state::get(&store, &message.thread_id)?;
        let has_calendar_part = riddle_mail::normalize::has_calendar_part(&message);
        let has_structured_receipt = riddle_mail::normalize::has_structured_receipt(&message);
        let mut input = SiftInput::simple(message.clone(), profile, people, DigContext::from_dig(dig));
        input.normalized_text = normalized.text;
        input.language = normalized.language;
        input.thread_state = thread_state;
        input.has_calendar_part = has_calendar_part;
        input.has_structured_receipt = has_structured_receipt;
        let mut slices: BTreeMap<String, String> = input.context_slices.clone();
        for s in riddle_context::slices(&index, &store, &message, riddle_context::SLICE_NAMES)? {
            slices.insert(s.name, s.text);
        }
        input.context_slices = slices;
        Ok(input)
    }

    /// Sift the backlog of one Dig, newest first, up to `limit` messages.
    pub async fn sift_pass(&self, dig: &Dig, limit: u32, cancel: Option<&CancelFlag>) -> Result<SiftPassReport> {
        let _t = Timed::start(Category::Sift, format!("sift pass {}", dig.id), Duration::from_secs(120));
        let domains = self.domains()?;
        let store = self.stores.dig(&dig.id)?;
        let sift = Sift::from_services(&self.services);
        let ids = repo::messages::unclassified_ids(&store, limit)?;
        let mut report = SiftPassReport::default();
        for id in ids {
            if cancel.is_some_and(|c| c.is_cancelled()) {
                break;
            }
            let Some(message) = repo::messages::get(&store, &dig.id, &id)? else { continue };
            let input = match self.sift_input(dig, message) {
                Ok(i) => i,
                Err(e) => {
                    rwarn!(Category::Sift, "input for {} failed: {}", id, e.kind.as_str());
                    report.failed += 1;
                    continue;
                }
            };
            match crate::ai::CURRENT_DIG.scope(dig.id.clone(), sift.run_and_persist(input)).await {
                Ok(outcome) => {
                    report.processed += 1;
                    report.cached_signals += outcome.cached as u64;
                    report.fresh_signals += outcome.fresh as u64;
                    if outcome.judge_unavailable { report.judge_unavailable = true; }
                    match outcome.classification.disposition {
                        Disposition::Unresolved => report.unresolved += 1,
                        ref d if d.hides() => report.hidden += 1,
                        _ => {}
                    }
                    if let Some(update) = &outcome.thread_state_update {
                        let profile = repo::user_profile::get(&self.stores.index())?;
                        let _ = riddle_context::update_thread_state(&store, &dig.id, &update.thread_id, &profile, self.services.clock.now());
                    }
                    if let Some(proposal) = outcome.find.as_ref().filter(|_| !outcome.shadow) {
                        match domains.finds.apply_proposal(proposal) {
                            Ok((_, joined)) => if joined { report.finds_joined += 1 } else { report.finds_created += 1 },
                            Err(e) => rwarn!(Category::Sift, "find proposal for {} failed: {}", id, e.kind.as_str()),
                        }
                    }
                }
                Err(e) => {
                    report.failed += 1;
                    rwarn!(Category::Sift, "sift of {} failed: {}", id, e.kind.as_str());
                    if e.kind == riddle_errors::ErrorKind::Store {
                        return Err(e);
                    }
                }
            }
        }
        report.remaining = repo::messages::unclassified_count(&store)?;
        rinfo!(Category::Sift, "pass {}: processed {} ({} cached, {} fresh signals), finds +{} joined {}, unresolved {}, hidden {}, remaining {}",
            dig.id, report.processed, report.cached_signals, report.fresh_signals, report.finds_created, report.finds_joined, report.unresolved, report.hidden, report.remaining);
        self.events.emit_summary();
        Ok(report)
    }

    /// One sync cycle for a Dig through its provider.
    pub async fn sync_pass(&self, dig: &Dig, cancel: &CancelFlag, on_progress: &mut (dyn FnMut(SyncProgress) + Send)) -> Result<SyncReport> {
        let domains = self.domains()?;
        let provider = domains.providers.provider_for(dig)?;
        let engine = SyncEngine::new(dig.clone(), provider, self.stores.clone(), self.events.clone());
        let mut externals = Vec::new();
        let report = engine.sync_once(on_progress, cancel, &mut |m| externals.push(m)).await?;
        if !externals.is_empty() {
            rinfo!(Category::Sync, "{} external moves attributed on {}", externals.len(), dig.id);
        }
        let _ = engine.fetch_bodies(200, on_progress, cancel).await;
        Ok(report)
    }

    /// Apply queued projection writes for a Dig.
    pub async fn projection_pass(&self, dig: &Dig) -> Result<(u64, u64)> {
        if dig.projection == ProjectionMode::None {
            return Ok((0, 0));
        }
        let domains = self.domains()?;
        let r = domains.outbox.drain(dig, 5).await?;
        if !r.conflicts.is_empty() {
            rwarn!(Category::Projection, "{} projection conflicts on {}", r.conflicts.len(), dig.id);
        }
        Ok((r.applied, r.conflicts.len() as u64))
    }

    /// Temporal Sift across Digs (§19).
    pub fn temporal_pass(&self) -> Result<Vec<String>> {
        let domains = self.domains()?;
        let outcomes = domains.finds.tick(&TemporalOptions::default())?;
        for o in &outcomes {
            rdebug!(Category::Sift, "temporal: {}", o.summary);
        }
        Ok(outcomes.into_iter().map(|o| o.summary).collect())
    }

    /// The whole pass for one Dig. `sift_limit` bounds one cycle so the
    /// runner can checkpoint and yield.
    pub async fn full_pass(&self, dig_id: &DigId, sift_limit: u32, cancel: &CancelFlag) -> Result<PassReport> {
        let dig = self.dig(dig_id)?;
        let mut report = PassReport { dig_id: dig_id.to_string(), ..Default::default() };
        let mut progress = |p: SyncProgress| {
            rdebug!(Category::Sync, "{} {:?} {}/{}", p.dig_id, p.phase, p.done, p.total.unwrap_or(0));
        };
        match self.sync_pass(&dig, cancel, &mut progress).await {
            Ok(r) => report.sync = Some(SyncSummary::from(&r)),
            Err(e) if e.kind.is_transient() || e.kind == riddle_errors::ErrorKind::ProviderAuth => {
                rwarn!(Category::Sync, "sync of {} deferred: {}", dig.id, e.kind.as_str());
            }
            Err(e) => return Err(e),
        }
        let dig = self.dig(dig_id)?;
        report.sift = self.sift_pass(&dig, sift_limit, Some(cancel)).await?;
        let (applied, conflicts) = self.projection_pass(&dig).await.unwrap_or_else(|e| {
            rwarn!(Category::Projection, "projection on {} deferred: {}", dig.id, e.kind.as_str());
            (0, 0)
        });
        report.projected = applied;
        report.projection_conflicts = conflicts;
        report.temporal = self.temporal_pass()?;
        Ok(report)
    }

    /// A pass over every Dig.
    pub async fn full_pass_all(&self, sift_limit: u32, cancel: &CancelFlag) -> Result<Vec<PassReport>> {
        let mut out = Vec::new();
        for dig in self.list_digs()? {
            match self.full_pass(&dig.id, sift_limit, cancel).await {
                Ok(r) => out.push(r),
                Err(e) => return Err(RiddleError::new(e.kind, format!("dig {}: {}", dig.name, e.message))),
            }
        }
        Ok(out)
    }
}

trait EmitSummary {
    fn emit_summary(&self);
}
impl EmitSummary for Arc<crate::events::Broadcast> {
    fn emit_summary(&self) { self.emit(RiddleEvent::SummaryChanged); }
}
