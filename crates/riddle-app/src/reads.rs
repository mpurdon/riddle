//! Read services shared by every client. These compose repositories across
//! Digs; they never mutate.

use crate::App;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use riddle_store::repo::messages::MessageFilter;
use std::collections::HashMap;

impl App {
    pub fn list_digs(&self) -> Result<Vec<Dig>> {
        repo::digs::list(&self.stores.index())
    }

    pub fn dig(&self, id: &DigId) -> Result<Dig> {
        repo::digs::get(&self.stores.index(), id)?
            .ok_or_else(|| RiddleError::not_found(format!("dig {id}")))
    }

    /// The morning summary (README "The Core Experience"). Honest: `caught_up`
    /// is false while any Dig has unclassified mail or a running catch-up.
    pub fn summary(&self, since: chrono::DateTime<chrono::Utc>) -> Result<Summary> {
        let index = self.stores.index();
        let digs = repo::digs::list(&index)?;
        let mut arrived = 0u64;
        let mut counts: HashMap<DispositionKind, u64> = HashMap::new();
        let mut remaining = 0u64;
        let mut total_to_sift = 0u64;
        let mut projection_pending = 0u64;
        let mut degraded = Vec::new();
        let mut last_synced: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut shadow: Option<ShadowSummary> = None;
        let mut shadow_counts: HashMap<DispositionKind, u64> = HashMap::new();
        for dig in &digs {
            let store = self.stores.dig(&dig.id)?;
            arrived += repo::messages::count_since(&store, &since)?;
            for (k, n) in repo::classifications::counts(&store, Some(&since), false)? {
                *counts.entry(k).or_insert(0) += n;
            }
            for (k, n) in repo::classifications::counts(&store, Some(&since), true)? {
                *shadow_counts.entry(k).or_insert(0) += n;
            }
            let un = repo::messages::unclassified_count(&store)?;
            remaining += un;
            total_to_sift += repo::messages::count(&store)?;
            projection_pending += repo::outbox::pending_count(&store)?;
            if let DigStatus::Degraded { reason } = &dig.status {
                degraded.push(format!("{}: {reason}", dig.name));
            }
            if let Some(ts) = repo::sync_state::get(&store, repo::sync_state::LAST_SYNC_AT)? {
                if let Ok(t) = ts.parse::<chrono::DateTime<chrono::Utc>>() {
                    last_synced = Some(last_synced.map_or(t, |p| p.max(t)));
                }
            }
            if let DigStatus::Shadow { day, of } = &dig.status {
                let s = shadow.get_or_insert(ShadowSummary {
                    day: *day,
                    of: *of,
                    would_spoil: 0,
                    would_find: 0,
                    disagreements: 0,
                });
                s.would_spoil += shadow_counts
                    .get(&DispositionKind::Spoil)
                    .copied()
                    .unwrap_or(0);
                s.would_find += shadow_counts
                    .get(&DispositionKind::Find)
                    .copied()
                    .unwrap_or(0);
                s.disagreements += repo::corrections::active(&store, 10_000)?.len() as u64;
            }
        }
        let finds_open = repo::finds::count_open(&index)?;
        let running_catch_up = repo::jobs::active(&index)?.iter().any(|j| {
            matches!(
                j.kind,
                JobKind::CatchUp | JobKind::InitialSync | JobKind::Sift
            ) && matches!(j.status, JobStatus::Running | JobStatus::Queued)
        });
        Ok(Summary {
            as_of: chrono::Utc::now(),
            caught_up: remaining == 0 && !running_catch_up && degraded.is_empty(),
            since,
            arrived,
            dig_count: digs.len() as u32,
            finds_open,
            unresolved: counts
                .get(&DispositionKind::Unresolved)
                .copied()
                .unwrap_or(0),
            filed: counts.get(&DispositionKind::Filed).copied().unwrap_or(0),
            spoil: counts.get(&DispositionKind::Spoil).copied().unwrap_or(0),
            contamination: counts
                .get(&DispositionKind::Contamination)
                .copied()
                .unwrap_or(0),
            remaining_to_sift: remaining,
            total_to_sift,
            director_note: None,
            shadow,
            last_synced_at: last_synced,
            projection_pending,
            degraded,
        })
    }

    /// Messages across Digs (or one Dig), merged newest first. Cross-Dig
    /// paging uses one offset applied per Dig, which over-fetches by the
    /// number of Digs; acceptable for a desktop list.
    pub fn list_messages(
        &self,
        dig_id: Option<&DigId>,
        filter: &MessageFilter,
        page: &PageRequest,
    ) -> Result<Page<MessageSummary>> {
        let digs = match dig_id {
            Some(d) => vec![self.dig(d)?],
            None => self.list_digs()?,
        };
        if digs.len() == 1 {
            let store = self.stores.dig(&digs[0].id)?;
            return repo::messages::list(&store, &digs[0].id, filter, page);
        }
        let mut items = Vec::new();
        let mut total = 0u64;
        for dig in &digs {
            let store = self.stores.dig(&dig.id)?;
            let p = repo::messages::list(&store, &dig.id, filter, page)?;
            total += p.total.unwrap_or(0);
            items.extend(p.items);
        }
        items.sort_by_key(|m| std::cmp::Reverse(m.date));
        let limit = page.limit() as usize;
        items.truncate(limit);
        Ok(Page::from_offset(
            items,
            page.offset(),
            page.limit(),
            Some(total),
        ))
    }

    pub fn get_message(&self, dig_id: &DigId, id: &MessageId) -> Result<Message> {
        let store = self.stores.dig(dig_id)?;
        repo::messages::get(&store, dig_id, id)?
            .ok_or_else(|| RiddleError::not_found(format!("message {id}")))
    }

    pub fn get_thread(&self, dig_id: &DigId, thread: &ThreadId) -> Result<Thread> {
        let store = self.stores.dig(dig_id)?;
        let messages = repo::messages::thread_messages(&store, dig_id, thread)?;
        if messages.is_empty() {
            return Err(RiddleError::not_found(format!("thread {thread}")));
        }
        Ok(Thread {
            id: thread.clone(),
            dig_id: dig_id.clone(),
            provider_id: thread.provider_id().to_string(),
            subject: messages[0].subject.clone(),
            messages,
        })
    }

    /// The answer to "Why?" from the stored trace (§37, §93).
    pub fn explain(&self, dig_id: &DigId, id: &MessageId) -> Result<Explanation> {
        let store = self.stores.dig(dig_id)?;
        let class = repo::classifications::current(&store, id)?
            .or(repo::classifications::current_shadow(&store, id)?)
            .ok_or_else(|| RiddleError::not_found("message has not been sifted yet"))?;
        let trace = repo::traces::get(&store, &class.trace_id)?;
        let (signals, gate, escalated, rationale) = match &trace {
            Some(t) => (
                t.signals
                    .iter()
                    .map(|s| SignalLine {
                        name: s.signal_name.clone(),
                        version: s.signal_version,
                        score: s.score(None),
                        selected: s.selected.clone(),
                        cached: s.cached,
                    })
                    .collect(),
                t.gate.clone(),
                t.escalation.is_some(),
                t.escalation.as_ref().map(|e| e.rationale.clone()),
            ),
            None => (vec![], None, false, None),
        };
        Ok(Explanation {
            message_id: id.clone(),
            disposition: class.disposition.clone(),
            mechanism: class.mechanism,
            mesh_name: trace.as_ref().and_then(|t| t.mesh_name.clone()),
            mesh_version: class.mesh_version,
            matched: class.matched_rule.clone(),
            signals,
            gate,
            escalated,
            llm_rationale: rationale,
            trace_id: class.trace_id.clone(),
            shadow: class.shadow,
            reason: class.reason.clone(),
        })
    }

    pub fn read_trace(&self, dig_id: &DigId, id: &TraceId) -> Result<DecisionTrace> {
        let store = self.stores.dig(dig_id)?;
        repo::traces::get(&store, id)?.ok_or_else(|| RiddleError::not_found(format!("trace {id}")))
    }

    pub fn recent_traces(&self, dig_id: Option<&DigId>, limit: u32) -> Result<Vec<DecisionTrace>> {
        let digs = match dig_id {
            Some(d) => vec![self.dig(d)?],
            None => self.list_digs()?,
        };
        let mut out = Vec::new();
        for d in digs {
            let store = self.stores.dig(&d.id)?;
            out.extend(repo::traces::recent(&store, limit)?);
        }
        out.sort_by_key(|t| std::cmp::Reverse(t.created_at));
        out.truncate(limit as usize);
        Ok(out)
    }

    pub fn usage_summary(&self, since: chrono::DateTime<chrono::Utc>) -> Result<UsageSummary> {
        let mut total = UsageSummary {
            since,
            calls: 0,
            input_tokens: 0,
            output_tokens: 0,
            priced_usd: 0.0,
            unpriced_calls: 0,
            by_task: vec![],
            by_provider: vec![],
        };
        let mut by_task: HashMap<String, (f64, u64)> = HashMap::new();
        let mut by_provider: HashMap<String, (f64, u64)> = HashMap::new();
        for d in self.list_digs()? {
            let store = self.stores.dig(&d.id)?;
            let s = repo::usage::summary(&store, &since)?;
            total.calls += s.calls;
            total.input_tokens += s.input_tokens;
            total.output_tokens += s.output_tokens;
            total.priced_usd += s.priced_usd;
            total.unpriced_calls += s.unpriced_calls;
            for (k, usd, n) in s.by_task {
                let e = by_task.entry(k).or_insert((0.0, 0));
                e.0 += usd;
                e.1 += n;
            }
            for (k, usd, n) in s.by_provider {
                let e = by_provider.entry(k).or_insert((0.0, 0));
                e.0 += usd;
                e.1 += n;
            }
        }
        total.by_task = by_task.into_iter().map(|(k, (u, n))| (k, u, n)).collect();
        total.by_provider = by_provider
            .into_iter()
            .map(|(k, (u, n))| (k, u, n))
            .collect();
        Ok(total)
    }

    pub fn signal_evaluations(
        &self,
        dig_id: &DigId,
        id: &MessageId,
    ) -> Result<Vec<SignalEvaluation>> {
        let store = self.stores.dig(dig_id)?;
        repo::signals::evaluations_for(&store, id)
    }

    pub fn list_finds(
        &self,
        filter: &repo::finds::FindFilter,
        page: &PageRequest,
    ) -> Result<Page<Find>> {
        repo::finds::list(&self.stores.index(), filter, page)
    }

    pub fn list_activity(
        &self,
        filter: &repo::activities::ActivityFilter,
        page: &PageRequest,
    ) -> Result<Page<Activity>> {
        repo::activities::list(&self.stores.index(), filter, page)
    }
}
