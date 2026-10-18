//! The processing engine: one pass per Dig runs sync, drains the Sift
//! backlog newest first (§5 catch-up order), applies Find proposals,
//! projects decisions through the outbox, and ticks Temporal Sift. The job
//! runners call these functions; the CLI and tests call them directly.
//!
//! Nothing here decides anything: Sift decides, Finds dedupe, the outbox
//! writes. The engine only sequences and reports.
//!
//! A pass's housekeeping (the stale-Unresolved retry and the sender review)
//! runs when the pass has something to do: the backlog has mail (a sync
//! added some, or something was sent back to Sift), or the rules changed
//! since housekeeping last ran. An idle pass skips both, with a floor so a
//! judge that came back still drains Unresolved: housekeeping runs anyway
//! on every tenth pass or after ten minutes, whichever comes first. Both
//! had run every 45 seconds on every account, each a read of the whole
//! classification and sender tables (17 Oct 2026).

use crate::domains::Domains;
use crate::window::{decide_window, JUDGE_CONCURRENCY};
use crate::App;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_finds::temporal::TemporalOptions;
use riddle_observability::{rdebug, rinfo, rwarn, swallowed, Category, Timed};
use riddle_sift::{DigContext, PeopleFacts, Sift, SiftInput};
use riddle_store::repo;
use riddle_sync::engine::{CancelFlag, SyncEngine, SyncProgress, SyncReport};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// (mesh id, version, mesh name, disposition kind) -> (message ids, cached, fresh, trace of the first).
type SiftGroups = std::collections::BTreeMap<
    (String, u32, String, String),
    (Vec<MessageId>, u64, u64, Option<TraceId>),
>;

/// The Mesh version whose route did something: (mesh id, version, name).
type MeshKey = (String, u32, String);

/// What a rule did in a pass, per Mesh version, with the trace of the first.
type MeshGroups<T> = std::collections::BTreeMap<MeshKey, (Vec<T>, TraceId)>;

fn mesh_key(t: &DecisionTrace) -> MeshKey {
    (
        t.mesh_id
            .as_ref()
            .map(|m| m.to_string())
            .unwrap_or_default(),
        t.mesh_version.unwrap_or(0),
        t.mesh_name.clone().unwrap_or_default(),
    )
}

fn mesh_provenance((id, version, name): &MeshKey, trace: TraceId) -> Provenance {
    Provenance {
        mechanism: DecisionMechanism::Mesh,
        mesh_id: Some(MeshId::from(id.as_str())),
        mesh_version: Some(*version),
        mesh_name: Some(name.clone()),
        trace_id: Some(trace),
        note: None,
    }
}

/// An Activity a rule's side effect records, undone through `action`.
struct RuleActivity {
    action: &'static str,
    summary: String,
    why: String,
    affected: Vec<EntityRef>,
    restore: serde_json::Value,
    undo_label: &'static str,
    dig: Option<DigId>,
    provenance: Option<Provenance>,
}

/// "Replaced an older Find: Time to write" / "Replaced 3 Finds; 1 Find
/// expired": each closed Find's status says which rule closed it.
fn closed_summary(finds: &[Find]) -> String {
    if let [one] = finds {
        return match one.status {
            FindStatus::Expired => format!("{} expired: {}", one.find_type.label(), one.title),
            _ => format!("Replaced an older Find: {}", one.title),
        };
    }
    let expired = finds
        .iter()
        .filter(|f| f.status == FindStatus::Expired)
        .count();
    let replaced = finds.len() - expired;
    let mut parts = Vec::new();
    if replaced > 0 {
        parts.push(format!("Replaced {}", plural(replaced, "older Find")));
    }
    if expired > 0 {
        parts.push(format!("{} expired", plural(expired, "Find")));
    }
    parts.join("; ")
}

#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiftPassReport {
    pub processed: u64,
    pub finds_created: u64,
    pub finds_joined: u64,
    /// Finds settling closed: replaced by a newer one of their series, or
    /// past their `+ Expires:` when decided.
    pub finds_closed: u64,
    /// Spoil and Filed messages a newer one of their series superseded.
    pub messages_superseded: u64,
    pub unresolved: u64,
    pub hidden: u64,
    pub failed: u64,
    pub judge_unavailable: bool,
    pub remaining: u64,
    pub cached_signals: u64,
    pub fresh_signals: u64,
    /// What the pass cost: wall time, the judge and LLM time its decisions'
    /// traces carry, and the store calls it made (the Dig store and the
    /// index together; another pass or a read on the same stores counts
    /// too, so these are a gauge, not a ledger).
    pub elapsed_ms: u64,
    pub judge_ms: u64,
    pub llm_ms: u64,
    pub store_reads: u64,
    pub store_writes: u64,
    /// The pass's id, stamped on every input it built (`SiftInput::pass_id`).
    pub pass_id: String,
    /// How the pass ran: the window decided together and how many of its
    /// decisions ran at once. `judge_ms` is summed across messages and
    /// `elapsed_ms` is wall time, so the two together show the overlap.
    pub window: u64,
    pub concurrency: u64,
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
        Self {
            kind: format!("{:?}", r.kind).to_lowercase(),
            added: r.added,
            updated: r.updated,
            deleted: r.deleted,
            external_moves: r.external_moves.len() as u64,
            complete: r.complete,
        }
    }
}

/// Whether a message's thread and its sender's domain are ones the user
/// already knows (`Facts::known_thread`, `Facts::known_domain`): the thread
/// has earlier mail from the user or someone they know; someone at the
/// sender's domain is a contact or was replied to, where the domain is one
/// organisation's (`organisation_domain`: not a consumer or ISP mailbox,
/// not a shared help desk).
pub(crate) fn known_context(
    index: &riddle_store::Store,
    store: &riddle_store::Store,
    message: &Message,
    profile: &UserProfile,
) -> Result<(bool, bool)> {
    let known = |email: &str| -> Result<bool> {
        Ok(profile.is_me(email)
            || riddle_people::lookup(index, email)?
                .is_some_and(|p| p.replied_to_by_user || p.in_contacts))
    };
    let mut known_thread = false;
    for (from, sent) in
        repo::messages::thread_senders_before(store, &message.thread_id, &message.date)?
    {
        if sent || known(&from)? {
            known_thread = true;
            break;
        }
    }
    let domain = message.from.domain().to_ascii_lowercase();
    let known_domain = organisation_domain(&domain)
        && !profile.is_me(&message.from.email)
        && repo::people::known_at_domain(index, &domain)?;
    Ok((known_thread, known_domain))
}

/// Stale Unresolved sent back to Sift in one pass, so a rule change costs a
/// few escalations a pass rather than all at once.
const STALE_PER_PASS: u32 = 25;

impl App {
    /// Unresolved mail Riddle decided under rules that have changed since,
    /// or that ran out of escalation budget before today, goes back to Sift.
    /// 27 of 91 Unresolved on a real mailbox had been decided before its
    /// rule packs were installed, or while cloud reasoning was off, and
    /// nothing ever looked at them again (13 Oct 2026).
    fn retry_stale_unresolved(&self, dig: &Dig, store: &riddle_store::Store) -> Result<usize> {
        let index = self.stores.index();
        let rules_changed = repo::kv::rules_changed_at(&index)?;
        let now = self.services.clock.now();
        let today = now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map_or(now, |d| d.and_utc());
        let ids = repo::classifications::stale_unresolved(
            store,
            rules_changed.as_ref(),
            &today,
            STALE_PER_PASS,
        )?;
        if ids.is_empty() {
            return Ok(0);
        }
        let sent = crate::actions::send_back_to_sift(
            &self.services,
            ids.iter().map(|m| (dig.id.clone(), m.clone())),
        )?;
        let n = sent.restored.len();
        let mut note = Activity::note(
            Actor::Sift,
            "decide_again",
            format!(
                "Sift is deciding {} again under the current rules",
                riddle_core::plural(n, "Unresolved message")
            ),
            serde_json::json!({ "digId": dig.id, "count": n }),
            "they were Unresolved; deciding them again hides nothing",
            None,
            now,
        );
        note.dig_id = Some(dig.id.clone());
        self.runtime.record_activity(note);
        rinfo!(
            Category::Sift,
            "sent {n} stale Unresolved in {} back to Sift",
            dig.id
        );
        Ok(n)
    }

    fn domains(&self) -> Result<Arc<Domains>> {
        self.services.require::<Domains>()
    }

    /// Build the full Sift input for a stored message: normalized text,
    /// People facts, Thread State, Context slices (§44: retrieved, not stuffed).
    /// One message on its own; a pass builds one `PassContext` for all of
    /// its messages and calls `sift_input_with`.
    pub fn sift_input(&self, dig: &Dig, message: Message) -> Result<SiftInput> {
        let store = self.stores.dig(&dig.id)?;
        let pass = riddle_context::PassContext::load(&store)?;
        self.sift_input_with(dig, message, &pass)
    }

    /// `sift_input` with the pass's Context: the standing corrections, read
    /// once per pass rather than per message (17 Oct 2026). The Person and
    /// Thread State fetched here for the People facts and the input are
    /// handed to the slices, which read them both again before.
    pub fn sift_input_with(
        &self,
        dig: &Dig,
        message: Message,
        pass: &riddle_context::PassContext,
    ) -> Result<SiftInput> {
        let index = self.stores.index();
        let store = self.stores.dig(&dig.id)?;
        let mut profile = repo::user_profile::get(&index)?;
        if !profile.is_me(&dig.email) {
            profile.addresses.push(dig.email.clone());
        }
        let normalized = riddle_mail::normalize::normalize_message(&message);
        let person = riddle_people::lookup(&index, &message.from.email)?;
        let mut people = PeopleFacts::from_person(person.as_ref());
        (people.known_thread, people.known_domain) =
            known_context(&index, &store, &message, &profile)?;
        let thread_state = repo::thread_state::get(&store, &message.thread_id)?;
        let has_calendar_part = riddle_mail::normalize::has_calendar_part(&message);
        let has_structured_receipt = riddle_mail::normalize::has_structured_receipt(&message);
        let sources = riddle_context::SliceSources {
            person: person.as_ref(),
            thread_state: thread_state.as_ref(),
            pass,
        };
        let mut input =
            SiftInput::simple(message.clone(), profile, people, DigContext::from_dig(dig));
        let mut slices: BTreeMap<String, String> = input.context_slices.clone();
        for s in
            riddle_context::slices_with(&store, &message, &sources, riddle_context::SLICE_NAMES)?
        {
            slices.insert(s.name, s.text);
        }
        input.context_slices = slices;
        input.normalized_text = normalized.text;
        input.language = normalized.language;
        input.thread_state = thread_state;
        input.has_calendar_part = has_calendar_part;
        input.has_structured_receipt = has_structured_receipt;
        Ok(input)
    }

    /// Sift the backlog of one Dig, newest first, up to `limit` messages:
    /// one window, decided `JUDGE_CONCURRENCY` at a time and persisted in
    /// order (`crate::window`).
    pub async fn sift_pass(
        self: &Arc<Self>,
        dig: &Dig,
        limit: u32,
        cancel: Option<&CancelFlag>,
    ) -> Result<SiftPassReport> {
        // One pass per account at a time. A sync, a "decide again" and a
        // catch-up each run a pass; side by side they read the same
        // unclassified ids, and after a two-week gap 455 of 924 messages
        // were judged twice, paying for both. The one that waits finds them
        // classified.
        let lock = self.services.get::<SiftLocks>().map(|l| l.for_dig(&dig.id));
        let _held = match &lock {
            Some(l) => Some(l.lock().await),
            None => None,
        };
        let _t = Timed::start(
            Category::Sift,
            format!("sift pass {}", dig.id),
            Duration::from_secs(120),
        );
        let domains = self.domains()?;
        let store = self.stores.dig(&dig.id)?;
        let shadow_decides = matches!(dig.status, DigStatus::Shadow { .. });
        // Housekeeping is due when the backlog has mail, the rules changed,
        // or the floor says so; an idle pass skips it. Bookkeeping: if the
        // question itself fails, the housekeeping runs.
        let housekeeping = self
            .housekeeping_due(dig, &store, shadow_decides)
            .unwrap_or_else(|e| {
                swallowed!(
                    Category::Sift,
                    "housekeeping_due",
                    e,
                    "could not tell whether housekeeping is due for {}",
                    dig.id
                );
                true
            });
        if housekeeping {
            // Bookkeeping: a failure to look for stale Unresolved never stops the pass.
            if let Err(e) = self.retry_stale_unresolved(dig, &store) {
                swallowed!(
                    Category::Sift,
                    "retry_stale_unresolved",
                    e,
                    "could not retry stale Unresolved for {}",
                    dig.id
                );
            }
            // Bookkeeping: judging what kind of sender the new ones are costs
            // one call each and never stops the pass.
            if let Err(e) = self
                .review_senders(dig, crate::suggestions::SENDERS_PER_PASS)
                .await
            {
                swallowed!(
                    Category::Sift,
                    "review_senders",
                    e,
                    "could not review senders for {}",
                    dig.id
                );
            }
        }
        let mut sift = Sift::from_services(&self.services);
        if let Some(settings) = self.services.get::<riddle_settings::SettingsStore>() {
            sift = sift.with_budget(Arc::new(EscalationCap::new(self.stores.clone(), settings)));
        }
        // While the sync's body fetch is behind its listing (the Dig's
        // `bodies_pending` flag), messages whose bodies have landed come
        // first: a message judged on its snippet is judged for good, and
        // bodies arrive a cycle behind the listing. Mail older than the body
        // window (the sync window; the backfill brings its bodies with it)
        // is judged on what it has, and so is everything once nothing is
        // left to fetch, or when the mail came some other way than a sync.
        let bodies_pending = repo::sync_state::get(&store, riddle_sync::engine::BODIES_PENDING)?
            .as_deref()
            == Some("1");
        let ids = if bodies_pending {
            let body_window = self
                .sync_options()
                .window_days
                .map(|d| self.services.clock.now() - chrono::Duration::days(d));
            repo::messages::unclassified_ids_preferring_bodies(
                &store,
                limit,
                shadow_decides,
                body_window,
            )?
        } else {
            repo::messages::unclassified_ids(&store, limit, shadow_decides)?
        };
        let mut report = SiftPassReport::default();
        // The store calls the pass makes, on the Dig store and the index.
        let stats_before = store.stats() + self.stores.index().stats();
        // Grouped for the ledger: (mesh id, version, name, disposition kind) -> (ids, cached, fresh, trace of first).
        let mut groups: SiftGroups = std::collections::BTreeMap::new();
        // Finds settling closed, by the Mesh version whose route closed them.
        let mut closed: MeshGroups<Find> = std::collections::BTreeMap::new();
        // Messages superseded, by the Mesh version whose route did it.
        let mut superseded: MeshGroups<MessageId> = std::collections::BTreeMap::new();
        // The pass's Context: the standing corrections, read once.
        let pass_context = riddle_context::PassContext::load(&store)?;
        let pass_id = riddle_sift::new_pass_id();
        report.pass_id = pass_id.clone();
        // The batch is one window (`crate::window`). 1. Its inputs, in
        // series: the facts read the store and are cheap. A message whose
        // input fails is counted here and stays unclassified.
        let mut inputs: Vec<SiftInput> = Vec::with_capacity(ids.len());
        for id in ids {
            if cancel.is_some_and(|c| c.is_cancelled()) {
                break;
            }
            let Some(message) = repo::messages::get(&store, &dig.id, &id)? else {
                continue;
            };
            match self.sift_input_with(dig, message, &pass_context) {
                Ok(mut input) => {
                    input.pass_id = pass_id.clone();
                    inputs.push(input);
                }
                Err(e) => {
                    rwarn!(
                        Category::Sift,
                        "input for {} failed: {}",
                        id,
                        e.kind.as_str()
                    );
                    report.failed += 1;
                }
            }
        }
        report.window = inputs.len() as u64;
        report.concurrency = JUDGE_CONCURRENCY as u64;
        // What the window's decisions share, read once here rather than by
        // the first six at once; a read that fails here fails them each,
        // as it did one by one.
        let _ = sift.pass_snapshot();
        let _ = sift.meshes_in_scope(&dig.id, dig.group.as_deref());
        // 2. Decide the window, `JUDGE_CONCURRENCY` at a time: the time is
        // spent waiting on the judge, in parallel instead of in series. The
        // judge client's bucket and breaker pace the calls; a breaker that
        // opens mid-window leaves the rest Unresolved, as one by one. The
        // escalation cap tallies under a lock, so a cap reached mid-window
        // stops further escalations and not the window. A cancellation
        // starts no further decision; those running finish and are kept.
        let window_ids: Vec<MessageId> = inputs.iter().map(|i| i.message.id.clone()).collect();
        let decided = crate::ai::CURRENT_DIG
            .scope(
                dig.id.clone(),
                decide_window(
                    inputs,
                    JUDGE_CONCURRENCY,
                    || cancel.is_some_and(|c| c.is_cancelled()),
                    |input| sift.run(input),
                ),
            )
            .await;
        // 3. Persist in the window's order, so dispositions, Activity and
        // the projection outbox keep newest-first and a thread's replies
        // are spread after its root. One write per decision.
        for (id, decision) in window_ids.into_iter().zip(decided) {
            let persisted = decision.and_then(|mut outcome| {
                sift.persist(&mut outcome)?;
                Ok(outcome)
            });
            match persisted {
                Ok(outcome) => {
                    report.processed += 1;
                    report.cached_signals += outcome.cached as u64;
                    report.fresh_signals += outcome.fresh as u64;
                    report.judge_ms += outcome.trace.timing.judge_ms;
                    report.llm_ms += outcome.trace.timing.llm_ms;
                    if outcome.judge_unavailable {
                        report.judge_unavailable = true;
                    }
                    if !outcome.shadow {
                        let k = &outcome.classification;
                        let key =
                            (
                                k.mesh_id
                                    .as_ref()
                                    .map(|m| m.to_string())
                                    .unwrap_or_default(),
                                k.mesh_version.unwrap_or(0),
                                outcome.trace.mesh_name.clone().unwrap_or_else(|| {
                                    match k.mechanism {
                                        DecisionMechanism::Gate => "a gate".into(),
                                        _ => "deterministic facts".into(),
                                    }
                                }),
                                k.disposition.kind().as_str().to_string(),
                            );
                        let g = groups
                            .entry(key)
                            .or_insert_with(|| (Vec::new(), 0, 0, Some(k.trace_id.clone())));
                        g.0.push(id.clone());
                        g.1 += outcome.cached as u64;
                        g.2 += outcome.fresh as u64;
                    }
                    match outcome.classification.disposition {
                        Disposition::Unresolved => report.unresolved += 1,
                        ref d if d.hides() => report.hidden += 1,
                        _ => {}
                    }
                    if let Some(update) = &outcome.thread_state_update {
                        let profile = repo::user_profile::get(&self.stores.index())?;
                        // Bookkeeping: the decision stands without its Thread State.
                        if let Err(e) = riddle_context::update_thread_state(
                            &store,
                            &dig.id,
                            &update.thread_id,
                            &profile,
                            self.services.clock.now(),
                        ) {
                            swallowed!(
                                Category::Sift,
                                "thread_state_update",
                                e,
                                "thread state of {} not updated",
                                update.thread_id
                            );
                        }
                    }
                    if !outcome.superseded.is_empty() {
                        report.messages_superseded += outcome.superseded.len() as u64;
                        superseded
                            .entry(mesh_key(&outcome.trace))
                            .or_insert_with(|| (Vec::new(), outcome.trace.id.clone()))
                            .0
                            .extend(outcome.superseded.iter().cloned());
                    }
                    // A real escalation: the challengers answer the same
                    // message beside it (Developer → Models), never applied.
                    if outcome.trace.escalation.is_some() && !outcome.shadow {
                        self.spawn_challengers(&dig.id, &id);
                    }
                    if let Some(proposal) = outcome.find.as_ref().filter(|_| !outcome.shadow) {
                        match domains.finds.apply_proposal(proposal) {
                            Ok(applied) => {
                                if applied.joined {
                                    report.finds_joined += 1
                                } else {
                                    report.finds_created += 1
                                }
                                if !applied.closed.is_empty() {
                                    report.finds_closed += applied.closed.len() as u64;
                                    closed
                                        .entry(mesh_key(&outcome.trace))
                                        .or_insert_with(|| (Vec::new(), outcome.trace.id.clone()))
                                        .0
                                        .extend(applied.closed);
                                }
                            }
                            Err(e) => swallowed!(
                                Category::Sift,
                                "find_proposal",
                                e,
                                "find proposal for {} failed",
                                id
                            ),
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
        self.record_sift_activity(dig, groups, &report);
        for (key, (finds, trace)) in closed {
            let ids: Vec<&FindId> = finds.iter().map(|f| &f.id).collect();
            self.record_rule_activity(RuleActivity {
                action: "close_finds",
                summary: closed_summary(&finds),
                why: format!(
                    "Mesh: {} v{} says a newer one replaces them, or when they expire",
                    key.2, key.1
                ),
                affected: ids
                    .iter()
                    .map(|id| EntityRef::new(EntityKind::Find, *id))
                    .collect(),
                restore: serde_json::json!({ "findIds": ids }),
                undo_label: if ids.len() > 1 {
                    "Keep them open"
                } else {
                    "Keep it open"
                },
                dig: Some(dig.id.clone()),
                provenance: Some(mesh_provenance(&key, trace)),
            });
        }
        for (key, (ids, trace)) in superseded {
            self.record_rule_activity(RuleActivity {
                action: "supersede_messages",
                summary: format!("{} superseded by newer mail", plural(ids.len(), "message")),
                why: format!(
                    "Mesh: {} v{} says only the newest from a sender matters",
                    key.2, key.1
                ),
                affected: ids
                    .iter()
                    .map(|id| EntityRef::new(EntityKind::Message, id))
                    .collect(),
                restore: serde_json::json!({ "digId": dig.id, "messageIds": ids }),
                undo_label: if ids.len() > 1 {
                    "Keep them"
                } else {
                    "Keep it"
                },
                dig: Some(dig.id.clone()),
                provenance: Some(mesh_provenance(&key, trace)),
            });
        }
        report.remaining = repo::messages::unclassified_count(&store)?;
        let calls = (store.stats() + self.stores.index().stats()).since(stats_before);
        report.store_reads = calls.reads;
        report.store_writes = calls.writes;
        report.elapsed_ms = _t.elapsed_ms();
        rinfo!(Category::Sift, "pass {}: processed {} ({} cached, {} fresh signals), finds +{} joined {}, unresolved {}, hidden {}, failed {}, judge unavailable {}, remaining {}; {}ms (judge {}ms, llm {}ms), store reads {} writes {}, window {} concurrency {}, pass id {}",
            dig.id, report.processed, report.cached_signals, report.fresh_signals, report.finds_created, report.finds_joined, report.unresolved, report.hidden, report.failed, report.judge_unavailable, report.remaining,
            report.elapsed_ms, report.judge_ms, report.llm_ms, report.store_reads, report.store_writes, report.window, report.concurrency, report.pass_id);
        self.events.emit_summary();
        Ok(report)
    }

    /// The Activity for what a rule closed as a side effect: Finds a newer
    /// one or an expiry closed (`close_finds`), mail a newer one superseded
    /// (`supersede_messages`). Bookkeeping: a failure is logged, never the
    /// pass's.
    fn record_rule_activity(&self, a: RuleActivity) {
        let mut entry = riddle_activity::Entry::new(Actor::Sift, a.action, a.summary)
            .why(a.why)
            .inputs(serde_json::json!({ "count": a.affected.len() }))
            .affected(a.affected)
            .undoable(a.restore, a.undo_label);
        if let Some(dig) = a.dig {
            entry = entry.dig(dig);
        }
        if let Some(p) = a.provenance {
            if let Some(trace) = p.trace_id.clone() {
                entry = entry.trace(trace);
            }
            entry = entry.provenance(p);
        }
        riddle_activity::Ledger::new(
            self.stores.index(),
            self.events.clone(),
            self.services.clock.clone(),
        )
        .try_append(entry);
    }

    /// One Activity per (Mesh version, disposition) group of a pass (§47).
    /// Bookkeeping: a failure here is logged and never fails the pass. The
    /// note says what the pass as a whole met: how many messages failed and
    /// whether the judge was unavailable, so an Unresolved group reads as
    /// "the judge was down", not "the rules said so".
    fn record_sift_activity(&self, dig: &Dig, groups: SiftGroups, report: &SiftPassReport) {
        let index = self.stores.index();
        let pass_note = {
            let mut parts = Vec::new();
            if report.failed > 0 {
                parts.push(format!(
                    "{} failed and stay unclassified.",
                    plural(report.failed as usize, "message")
                ));
            }
            if report.judge_unavailable {
                parts.push("The judge was unavailable; nothing was hidden for it.".to_string());
            }
            parts.join(" ")
        };
        for ((mesh_id, version, mesh_name, kind), (ids, cached, fresh, trace)) in groups {
            if ids.is_empty() {
                continue;
            }
            let n = ids.len();
            let verb = match kind.as_str() {
                "spoil" => format!("Sift moved {n} messages to Spoil"),
                "contamination" => format!("Sift marked {n} messages as Contamination"),
                "filed" => format!("Sift filed {n} messages"),
                "find" => format!("Sift surfaced {n} messages as Finds"),
                _ => format!("Sift left {n} messages Unresolved"),
            };
            let mut note = match (cached, fresh) {
                (0, 0) => "No Signals were needed.".to_string(),
                (c, 0) => format!("{c} matched on cached Signals."),
                (0, f) => format!("{f} needed a fresh evaluation."),
                (c, f) => format!("{c} matched on cached Signals. {f} needed a fresh evaluation."),
            };
            if !pass_note.is_empty() {
                note.push(' ');
                note.push_str(&pass_note);
            }
            let hides = matches!(kind.as_str(), "spoil" | "contamination" | "filed");
            let activity = Activity {
                id: ActivityId::new(),
                actor: Actor::Sift,
                action: "sift_classify".into(),
                summary: verb,
                why: Some(if mesh_id.is_empty() {
                    format!("Decided by {mesh_name}")
                } else {
                    format!("Triggered by Mesh: {mesh_name} v{version}")
                }),
                inputs: serde_json::json!({ "digId": dig.id, "passId": report.pass_id, "count": n, "cached": cached, "fresh": fresh, "failed": report.failed, "judgeUnavailable": report.judge_unavailable }),
                affected: ids
                    .iter()
                    .map(|i| EntityRef::new(EntityKind::Message, i))
                    .collect(),
                result: ActivityResult::Ok,
                provenance: Some(Provenance {
                    mechanism: if mesh_id.is_empty() {
                        DecisionMechanism::Deterministic
                    } else {
                        DecisionMechanism::Mesh
                    },
                    mesh_id: if mesh_id.is_empty() {
                        None
                    } else {
                        Some(MeshId::from(mesh_id.as_str()))
                    },
                    mesh_version: if version == 0 { None } else { Some(version) },
                    mesh_name: Some(mesh_name.clone()),
                    trace_id: trace.clone(),
                    note: Some(note),
                }),
                undo: if hides {
                    UndoInfo::Undoable {
                        restore: serde_json::json!({ "digId": dig.id, "messageIds": ids }),
                        label: if n > 1 {
                            "Undo All".into()
                        } else {
                            "Undo".into()
                        },
                    }
                } else {
                    UndoInfo::NotUndoable {
                        reason: "nothing was hidden".into(),
                    }
                },
                dig_id: Some(dig.id.clone()),
                trace_id: trace,
                undoes: None,
                conversation_id: None,
                created_at: self.services.clock.now(),
            };
            match repo::activities::append(&index, &activity) {
                Ok(()) => self.events.emit(RiddleEvent::ActivityAppended { activity }),
                Err(e) => swallowed!(
                    Category::Sift,
                    "sift_activity",
                    e,
                    "sift activity for {} not recorded",
                    dig.id
                ),
            }
        }
    }

    /// One sync cycle for a Dig through its provider.
    pub async fn sync_pass(
        &self,
        dig: &Dig,
        cancel: &CancelFlag,
        on_progress: &mut (dyn FnMut(SyncProgress) + Send),
    ) -> Result<SyncReport> {
        let engine = self.sync_engine(dig)?;
        let mut externals = Vec::new();
        let report = engine
            .sync_once(on_progress, cancel, &mut |m| externals.push(m))
            .await?;
        if !externals.is_empty() {
            rinfo!(
                Category::Sync,
                "{} external moves attributed on {}",
                externals.len(),
                dig.id
            );
        }
        self.redecide_on_spam_verdicts(dig, &externals);
        let _ = engine.fetch_bodies(200, on_progress, cancel).await;
        Ok(report)
    }

    /// One run of the history backfill for a Dig: older mail than the first
    /// sync's window, `pages_per_run` pages with their bodies (§53).
    pub async fn backfill_pass(
        &self,
        dig: &Dig,
        cancel: &CancelFlag,
        on_progress: &mut (dyn FnMut(SyncProgress) + Send),
    ) -> Result<SyncReport> {
        self.sync_engine(dig)?
            .backfill_history(on_progress, cancel)
            .await
    }

    /// Whether the Dig's first listing is done and its history behind the
    /// window still has pages to fetch. Read from sync state alone, so an
    /// account whose provider cannot be built (no sign-in yet) answers too.
    pub fn backfill_pending(&self, dig: &Dig) -> Result<bool> {
        use riddle_sync::engine::{window_start_or_derived, BACKFILL_DONE};
        let store = self.stores.dig(&dig.id)?;
        Ok(
            repo::sync_state::get(&store, repo::sync_state::INITIAL_SYNC_DONE)?.as_deref()
                == Some("1")
                && repo::sync_state::get(&store, BACKFILL_DONE)?.as_deref() != Some("1")
                // An account synced before the window start was recorded
                // derives it from its oldest message here.
                && window_start_or_derived(&store)?.is_some(),
        )
    }

    fn sync_engine(&self, dig: &Dig) -> Result<SyncEngine> {
        let domains = self.domains()?;
        let provider = domains.providers.provider_for(dig)?;
        Ok(SyncEngine::new(
            dig.clone(),
            provider,
            self.stores.clone(),
            self.events.clone(),
        )
        .with_options(self.sync_options()))
    }

    /// The sync options this process runs with: a `SyncOptions` registered
    /// in `Services` (tests shorten the runs), else the defaults.
    pub fn sync_options(&self) -> riddle_sync::SyncOptions {
        self.services
            .get::<riddle_sync::SyncOptions>()
            .map(|o| (*o).clone())
            .unwrap_or_default()
    }

    /// Gmail's spam verdict is read when a message is sifted; a verdict that
    /// arrives later used to be stored on the message and read by nothing, so
    /// a scam Gmail moved to Spam days after it arrived stayed Unresolved
    /// (`ARCHITECTURE.md` §15: Gmail is authoritative for what it owns).
    /// Riddle never writes Gmail's `SPAM` label, so a change to it is Gmail's
    /// or the user's: the message goes back to Sift, which recomputes its
    /// facts and meets the Contamination gate, or, after "Not spam", no
    /// longer does. A decision the user made stays. A failure here is
    /// reported and the sync still succeeds.
    fn redecide_on_spam_verdicts(&self, dig: &Dig, externals: &[riddle_sync::ExternalMove]) {
        let changed: Vec<(DigId, MessageId)> = externals
            .iter()
            .filter(|m| m.added.iter().chain(&m.removed).any(|l| l == "SPAM"))
            .map(|m| (dig.id.clone(), m.message_id.clone()))
            .collect();
        if changed.is_empty() {
            return;
        }
        let count = changed.len();
        match crate::actions::send_back_to_sift(&self.services, changed.into_iter()) {
            Ok(sent) => rinfo!(
                Category::Sync,
                "{} of {count} messages whose Gmail spam verdict changed went back to Sift on {}",
                sent.restored.len(),
                dig.id
            ),
            Err(e) => rwarn!(
                Category::Sync,
                "spam verdict changes on {} not sent back to Sift: {e}",
                dig.id
            ),
        }
    }

    /// Apply queued projection writes for a Dig.
    pub async fn projection_pass(&self, dig: &Dig) -> Result<(u64, u64)> {
        if dig.projection == ProjectionMode::None {
            return Ok((0, 0));
        }
        // Queued rows wait, not drop, so arming releases them.
        if let Some(reason) = crate::domains::sending_held(&self.runtime.policy.read()) {
            rdebug!(
                Category::Projection,
                "projection held for {}: {reason}",
                dig.id
            );
            return Ok((0, 0));
        }
        let domains = self.domains()?;
        let r = domains.outbox.drain(dig, 5).await?;
        if !r.conflicts.is_empty() {
            rwarn!(
                Category::Projection,
                "{} projection conflicts on {}",
                r.conflicts.len(),
                dig.id
            );
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
        let expired: Vec<_> = outcomes
            .iter()
            .filter(|o| o.kind == riddle_finds::temporal::TemporalOutcomeKind::Expired)
            .collect();
        if !expired.is_empty() {
            let summary = match expired.as_slice() {
                [one] => one.summary.clone(),
                many => format!("{} expired", plural(many.len(), "Find")),
            };
            let ids: Vec<FindId> = expired.iter().map(|o| o.find_id.clone()).collect();
            let dig = match expired.as_slice() {
                [one] => one.dig_id.clone(),
                _ => None,
            };
            self.record_rule_activity(RuleActivity {
                action: "close_finds",
                summary,
                why: "Their time passed: a date, or the Expires: of the Mesh that raised them"
                    .into(),
                affected: ids
                    .iter()
                    .map(|id| EntityRef::new(EntityKind::Find, id))
                    .collect(),
                restore: serde_json::json!({ "findIds": ids }),
                undo_label: if ids.len() > 1 {
                    "Keep them open"
                } else {
                    "Keep it open"
                },
                dig,
                provenance: None,
            });
        }
        Ok(outcomes.into_iter().map(|o| o.summary).collect())
    }

    /// The whole pass for one Dig. `sift_limit` bounds one cycle so the
    /// runner can checkpoint and yield.
    pub async fn full_pass(
        self: &Arc<Self>,
        dig_id: &DigId,
        sift_limit: u32,
        cancel: &CancelFlag,
    ) -> Result<PassReport> {
        let dig = self.dig(dig_id)?;
        let mut report = PassReport {
            dig_id: dig_id.to_string(),
            ..Default::default()
        };
        let mut progress = |p: SyncProgress| {
            rdebug!(
                Category::Sync,
                "{} {:?} {}/{}",
                p.dig_id,
                p.phase,
                p.done,
                p.total.unwrap_or(0)
            );
        };
        // Inline, with no queue to interleave for it: a listing with pages
        // left runs again, a Sift of what landed between runs, until done.
        loop {
            match self.sync_pass(&dig, cancel, &mut progress).await {
                Ok(r) => {
                    let more = !r.complete && !cancel.is_cancelled();
                    report.sync = Some(SyncSummary::from(&r));
                    if !more {
                        break;
                    }
                    self.sift_pass(&dig, sift_limit, Some(cancel)).await?;
                }
                Err(e)
                    if e.kind.is_transient()
                        || e.kind == riddle_errors::ErrorKind::ProviderAuth =>
                {
                    rwarn!(
                        Category::Sync,
                        "sync of {} deferred: {}",
                        dig.id,
                        e.kind.as_str()
                    );
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        let dig = self.dig(dig_id)?;
        report.sift = self.sift_pass(&dig, sift_limit, Some(cancel)).await?;
        let (applied, conflicts) = self.projection_pass(&dig).await.unwrap_or_else(|e| {
            rwarn!(
                Category::Projection,
                "projection on {} deferred: {}",
                dig.id,
                e.kind.as_str()
            );
            (0, 0)
        });
        report.projected = applied;
        report.projection_conflicts = conflicts;
        report.temporal = self.temporal_pass()?;
        Ok(report)
    }

    /// A pass over every Dig.
    pub async fn full_pass_all(
        self: &Arc<Self>,
        sift_limit: u32,
        cancel: &CancelFlag,
    ) -> Result<Vec<PassReport>> {
        let mut out = Vec::new();
        for dig in self.list_digs()? {
            match self.full_pass(&dig.id, sift_limit, cancel).await {
                Ok(r) => out.push(r),
                Err(e) => {
                    return Err(RiddleError::new(
                        e.kind,
                        format!("dig {}: {}", dig.name, e.message),
                    ))
                }
            }
        }
        Ok(out)
    }
}

trait EmitSummary {
    fn emit_summary(&self);
}
impl EmitSummary for Arc<crate::events::Broadcast> {
    fn emit_summary(&self) {
        self.emit(RiddleEvent::SummaryChanged);
    }
}

/// The per-Dig daily cap on escalation spend (`budgets.escalationPerDigUsd`).
/// The cap and the day's spend so far are read once per pass, on the first
/// escalation, and the pass tallies its own escalations in memory from
/// there (17 Oct 2026): every check used to read the settings and sum the
/// usage ledger again, once per escalation. A saved change to the cap
/// applies from the next pass. A ledger that cannot be read allows
/// escalation: bookkeeping never fails the work, and the daily LLM budget
/// still bounds spend. The tally is a `PassTally`, locked: a window's
/// decisions escalate concurrently, and each books its own cost, so a cap
/// reached mid-window stops the escalations after it and not the window.
struct EscalationCap {
    stores: Arc<riddle_store::Stores>,
    settings: Arc<riddle_settings::SettingsStore>,
    tally: PassTally,
}

/// The cap, what the ledger held when the pass first asked, and what the
/// pass has escalated since.
#[derive(Debug, Clone, Copy)]
struct PassLedger {
    limit: f64,
    spent_before: f64,
    spent_in_pass: f64,
}

/// The in-memory tally per Dig, read on first use and shared by every
/// decision of the window under one lock, so no spend is lost between two
/// escalations that finish together. A check and the booking that follows
/// it are two critical sections: up to the window's concurrency can pass
/// the check before the first books, and the overshoot is bounded by that.
#[derive(Default)]
struct PassTally(parking_lot::Mutex<std::collections::HashMap<DigId, PassLedger>>);

impl PassTally {
    fn allows(&self, dig: &DigId, read: impl FnOnce() -> PassLedger) -> bool {
        let mut ledger = self.0.lock();
        let l = ledger.entry(dig.clone()).or_insert_with(read);
        l.spent_before + l.spent_in_pass < l.limit
    }

    fn spent(&self, dig: &DigId, usd: f64, read: impl FnOnce() -> PassLedger) {
        let mut ledger = self.0.lock();
        let l = ledger.entry(dig.clone()).or_insert_with(read);
        l.spent_in_pass += usd;
    }

    #[cfg(test)]
    fn spent_in_pass(&self, dig: &DigId) -> f64 {
        self.0.lock().get(dig).map_or(0.0, |l| l.spent_in_pass)
    }
}

impl EscalationCap {
    fn new(
        stores: Arc<riddle_store::Stores>,
        settings: Arc<riddle_settings::SettingsStore>,
    ) -> Self {
        Self {
            stores,
            settings,
            tally: PassTally::default(),
        }
    }

    fn read_ledger(&self, dig: &DigId) -> PassLedger {
        let settings = self.settings.settings();
        let limit = settings.budgets.escalation_per_dig_usd;
        // The provider the spend is booked under is whichever is configured;
        // a hardcoded name silently disabled the cap for every other one.
        let provider = crate::ops::ai_config(&settings).provider;
        let spent_before = self
            .stores
            .dig(dig)
            .and_then(|s| repo::usage::spent_today_on(&s, &provider, UsageTask::Sift))
            .unwrap_or(0.0);
        PassLedger {
            limit,
            spent_before,
            spent_in_pass: 0.0,
        }
    }
}

impl riddle_sift::EscalationBudget for EscalationCap {
    fn allows(&self, dig: &DigId) -> bool {
        self.tally.allows(dig, || self.read_ledger(dig))
    }

    /// The same figure the refiner's usage row carries, so the tally stays
    /// level with what the ledger would sum.
    fn spent(&self, dig: &DigId, usd: f64) {
        self.tally.spent(dig, usd, || self.read_ledger(dig))
    }
}

#[cfg(test)]
mod escalation_cap_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn the_escalation_tally_holds_under_concurrent_spending_and_closes_once_the_cap_is_reached() {
        // Eight decisions escalating at once, a quarter each against a cap
        // of two: in series eight get through. Together, no booking is
        // lost (the tally is exactly what was spent), at most the
        // concurrency's worth pass the check before the first books, and
        // once the cap is reached nothing more is allowed.
        let tally = PassTally::default();
        let dig = DigId::from("dig-cap");
        let read = || PassLedger {
            limit: 2.0,
            spent_before: 0.0,
            spent_in_pass: 0.0,
        };
        let escalations = AtomicUsize::new(0);
        let threads = 8;
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    for _ in 0..10 {
                        if tally.allows(&dig, read) {
                            tally.spent(&dig, 0.25, read);
                            escalations.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                });
            }
        });
        let n = escalations.load(Ordering::SeqCst);
        assert!(n >= 8, "the cap's worth got through: {n}");
        assert!(n < 8 + threads, "overshoot beyond the concurrency: {n}");
        assert_eq!(tally.spent_in_pass(&dig), 0.25 * n as f64);
        assert!(!tally.allows(&dig, read), "the cap is reached");
        // Another account's tally is its own.
        assert!(tally.allows(&DigId::from("dig-other"), read));
    }
}

/// Passes in a row an account may skip housekeeping before one runs it anyway.
const HOUSEKEEPING_EVERY_PASSES: u32 = 10;
/// Minutes since housekeeping last ran after which the next pass runs it.
const HOUSEKEEPING_EVERY_MINUTES: i64 = 10;

/// When housekeeping last ran for an account and what it saw (`Housekeeping`).
#[derive(Debug, Clone, Default)]
struct HousekeepingState {
    last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    rules_seen: Option<chrono::DateTime<chrono::Utc>>,
    skipped: u32,
}

/// Whether a pass's housekeeping is due, per account (`App::sift_pass`,
/// module header). The decision is recorded as it is made.
#[derive(Default)]
pub struct Housekeeping(std::sync::Mutex<std::collections::HashMap<DigId, HousekeepingState>>);

impl Housekeeping {
    /// Due when the backlog has mail, the rules changed since the last run,
    /// or the floor is reached (the first pass always); records the answer.
    pub fn due(
        &self,
        dig: &DigId,
        now: chrono::DateTime<chrono::Utc>,
        backlog: bool,
        rules_changed_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> bool {
        let mut map = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let s = map.entry(dig.clone()).or_default();
        let due = backlog
            || s.rules_seen != rules_changed_at
            || s.skipped + 1 >= HOUSEKEEPING_EVERY_PASSES
            || s.last_run_at
                .is_none_or(|t| now - t >= chrono::Duration::minutes(HOUSEKEEPING_EVERY_MINUTES));
        if due {
            s.last_run_at = Some(now);
            s.rules_seen = rules_changed_at;
            s.skipped = 0;
        } else {
            s.skipped += 1;
        }
        due
    }
}

impl App {
    /// The inputs of `Housekeeping::due` for one pass: one probe of the
    /// backlog index and one key-value read. Without the service registered
    /// (a bare `App`), housekeeping always runs.
    fn housekeeping_due(
        &self,
        dig: &Dig,
        store: &riddle_store::Store,
        shadow_decides: bool,
    ) -> Result<bool> {
        let Some(h) = self.services.get::<Housekeeping>() else {
            return Ok(true);
        };
        let backlog = !repo::messages::unclassified_ids(store, 1, shadow_decides)?.is_empty();
        let rules = repo::kv::rules_changed_at(&self.stores.index())?;
        Ok(h.due(&dig.id, self.services.clock.now(), backlog, rules))
    }
}

#[cfg(test)]
mod housekeeping_tests {
    use super::*;

    #[test]
    fn housekeeping_runs_on_new_mail_rule_changes_and_the_floor_and_skips_an_idle_pass() {
        let h = Housekeeping::default();
        let dig = DigId::from("dig-1");
        let t0 = chrono::Utc::now();
        let tick = |n: i64| t0 + chrono::Duration::seconds(45 * n);
        // The first pass runs; an idle one after it does not.
        assert!(h.due(&dig, tick(0), false, None));
        assert!(!h.due(&dig, tick(1), false, None));
        // Mail in the backlog, then a rule change, each run it.
        assert!(h.due(&dig, tick(2), true, None));
        let changed = Some(tick(2));
        assert!(h.due(&dig, tick(3), false, changed));
        assert!(!h.due(&dig, tick(4), false, changed));
        // The pass floor: the tenth pass since the last run runs it.
        let mut ran = 0;
        for n in 5..=13 {
            if h.due(&dig, tick(n), false, changed) {
                ran += 1;
            }
        }
        assert_eq!(ran, 1, "one run in nine idle passes");
        // The time floor: ten minutes idle runs it on the next pass.
        assert!(h.due(&dig, tick(14), true, changed));
        assert!(!h.due(&dig, tick(15), false, changed));
        assert!(h.due(
            &dig,
            tick(14) + chrono::Duration::minutes(10),
            false,
            changed
        ));
        // Accounts are independent.
        assert!(h.due(&DigId::from("dig-2"), tick(16), false, changed));
    }
}

/// One Sift pass per account at a time (`App::sift_pass`).
#[derive(Default)]
pub struct SiftLocks(
    std::sync::Mutex<std::collections::HashMap<DigId, Arc<tokio::sync::Mutex<()>>>>,
);

impl SiftLocks {
    pub fn for_dig(&self, dig: &DigId) -> Arc<tokio::sync::Mutex<()>> {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(dig.clone())
            .or_default()
            .clone()
    }
}
