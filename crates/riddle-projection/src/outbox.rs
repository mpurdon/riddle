//! The projection outbox: queued provider writes applied by the
//! `OutboxProcessor` (§15). Rows are idempotent (a re-classification with
//! the same plan is one row), ordered per thread by the store's `due` query,
//! retried with backoff, and checked against the provider before they are
//! applied: Gmail is authoritative, so a message whose labels changed
//! underneath (someone moved it from another client) is not overwritten.
//! The row is closed as a conflict and reported so the integrator records it
//! as Activity.
//!
//! Label names are resolved to provider ids at apply time through
//! `ensure_label`, cached per Dig, so `Riddle/Spoil` is created on first use.

use crate::plan::{
    expected_after, is_volatile, ProjectionPlan, CONTAMINATION_LABEL, INBOX, SPOIL_LABEL,
};
use parking_lot::Mutex;
use riddle_core::{Dig, DigId, Message, MessageId, ProjectionMode, ProviderLabel, ThreadId};
use riddle_errors::Result;
use riddle_mail::{Detail, MailProvider, ProviderRegistry};
use riddle_observability::{rinfo, rwarn, Category};
use riddle_store::{repo, Store, Stores};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub const OP_MODIFY_LABELS: &str = "modify_labels";

/// The outbox payload. `remove_managed` strips every `Riddle/*` label the
/// message carries at apply time (undo, restore).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionOp {
    pub provider_id: String,
    pub add: Vec<String>,
    pub remove: Vec<String>,
    #[serde(default)]
    pub remove_managed: bool,
    pub reason: String,
}

/// Enqueue a plan for a message. Returns `None` when nothing needs writing.
pub fn enqueue(
    store: &Store,
    message: &Message,
    plan: &ProjectionPlan,
    reason: &str,
    key_suffix: Option<&str>,
) -> Result<Option<i64>> {
    if plan.is_empty() {
        return Ok(None);
    }
    let op = ProjectionOp {
        provider_id: message.provider_id.clone(),
        add: plan.add.clone(),
        remove: plan.remove.clone(),
        remove_managed: false,
        reason: reason.into(),
    };
    let key = match key_suffix {
        Some(s) => format!("proj:{}:{}", message.id, s),
        None => format!(
            "proj:{}:{}",
            message.id,
            riddle_core::sha256_hex(serde_json::to_string(&(&op.add, &op.remove))?)
        ),
    };
    let id = repo::outbox::enqueue(
        store,
        OP_MODIFY_LABELS,
        Some(message.thread_id.as_str()),
        Some(message.id.as_str()),
        &op,
        Some(&key),
    )?;
    Ok(Some(id))
}

/// Enqueue the projection of a classification under the Dig's mode.
pub fn enqueue_classification(
    store: &Store,
    dig: &Dig,
    message: &Message,
    disposition: &riddle_core::Disposition,
    strata: &[String],
) -> Result<Option<i64>> {
    let plan = crate::plan::plan(disposition, strata, dig.projection, false);
    enqueue(store, message, &plan, disposition.kind().as_str(), None)
}

/// Undo a projection: put the message back in the inbox (when `restore_inbox`)
/// and remove every managed label. Pending rows for the message are cancelled
/// first so an unapplied projection simply never happens.
pub fn undo_projection(
    store: &Store,
    dig: &Dig,
    message: &Message,
    restore_inbox: bool,
    key_suffix: &str,
) -> Result<Option<i64>> {
    let cancelled = repo::outbox::cancel_for_message(store, message.id.as_str())?;
    if cancelled > 0 {
        rinfo!(
            Category::Projection,
            "cancelled {} pending projection rows for {}",
            cancelled,
            message.id
        );
    }
    if dig.projection == ProjectionMode::None {
        return Ok(None);
    }
    let op = ProjectionOp {
        provider_id: message.provider_id.clone(),
        add: if restore_inbox && dig.projection == ProjectionMode::Full {
            vec![INBOX.into()]
        } else {
            vec![]
        },
        remove: vec![],
        remove_managed: true,
        reason: "undo".into(),
    };
    let key = format!("undo:{}:{}", message.id, key_suffix);
    Ok(Some(repo::outbox::enqueue(
        store,
        OP_MODIFY_LABELS,
        Some(message.thread_id.as_str()),
        Some(message.id.as_str()),
        &op,
        Some(&key),
    )?))
}

/// Create the labels Riddle manages so the first projection does not race
/// label creation, and so the user sees them in Gmail from day one.
pub async fn bootstrap_managed_labels(
    provider: &dyn MailProvider,
    strata: &[String],
) -> Result<Vec<ProviderLabel>> {
    let mut out = Vec::new();
    for name in [SPOIL_LABEL.to_string(), CONTAMINATION_LABEL.to_string()]
        .into_iter()
        .chain(strata.iter().map(|s| crate::plan::stratum_label(s)))
    {
        out.push(provider.ensure_label(&name).await?);
    }
    rinfo!(Category::Projection, "managed labels ready: {}", out.len());
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub message_id: MessageId,
    pub thread_id: Option<ThreadId>,
    pub op: ProjectionOp,
    /// What the store believed the provider had.
    pub expected: Vec<String>,
    /// What the provider actually has now.
    pub actual: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessReport {
    pub applied: u64,
    pub failed: u64,
    pub skipped: u64,
    pub conflicts: Vec<Conflict>,
    pub remaining: u64,
}

/// Name → id map for one provider, filled through `ensure_label`.
#[derive(Default)]
pub struct LabelResolver {
    by_name: HashMap<String, String>,
    by_id: HashMap<String, String>,
}

impl LabelResolver {
    pub fn seed(&mut self, labels: &[ProviderLabel]) {
        for l in labels {
            self.by_name.insert(l.name.clone(), l.id.clone());
            self.by_id.insert(l.id.clone(), l.name.clone());
        }
    }
    pub fn name_of(&self, id: &str) -> Option<&str> {
        self.by_id.get(id).map(|s| s.as_str())
    }
    pub fn id_of(&self, name: &str) -> Option<&str> {
        self.by_name.get(name).map(|s| s.as_str())
    }
    pub fn is_managed_id(&self, id: &str) -> bool {
        self.by_id
            .get(id)
            .is_some_and(|n| riddle_mail::is_managed_label_name(n))
            || riddle_mail::is_managed_label_name(id)
    }
    pub async fn resolve(&mut self, provider: &dyn MailProvider, name: &str) -> Result<String> {
        if let Some(id) = self.by_name.get(name) {
            return Ok(id.clone());
        }
        // System labels are their own ids.
        if name.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
            self.by_name.insert(name.into(), name.into());
            self.by_id.insert(name.into(), name.into());
            return Ok(name.into());
        }
        let label = provider.ensure_label(name).await?;
        self.by_name.insert(label.name.clone(), label.id.clone());
        self.by_id.insert(label.id.clone(), label.name.clone());
        Ok(label.id)
    }
}

/// One row ready to apply: (outbox id, message id, provider id, provider labels now).
type Applicable = (i64, MessageId, String, Vec<String>);

pub struct OutboxProcessor {
    stores: Arc<Stores>,
    registry: Arc<ProviderRegistry>,
    resolvers: Mutex<HashMap<DigId, Arc<tokio::sync::Mutex<LabelResolver>>>>,
}

fn non_volatile(labels: &[String]) -> HashSet<&str> {
    labels
        .iter()
        .map(|s| s.as_str())
        .filter(|l| !is_volatile(l))
        .collect()
}

impl OutboxProcessor {
    pub fn new(stores: Arc<Stores>, registry: Arc<ProviderRegistry>) -> Self {
        Self {
            stores,
            registry,
            resolvers: Mutex::new(HashMap::new()),
        }
    }

    pub fn resolver(&self, dig: &DigId) -> Arc<tokio::sync::Mutex<LabelResolver>> {
        self.resolvers
            .lock()
            .entry(dig.clone())
            .or_default()
            .clone()
    }

    /// Apply due rows for a Dig: at most `limit` rows, one per thread per
    /// pass so per-thread order holds even when rows are batched by op.
    pub async fn process(&self, dig: &Dig, limit: u32) -> Result<ProcessReport> {
        let store = self.stores.dig(&dig.id)?;
        let provider = self.registry.provider_for(dig)?;
        let mut report = ProcessReport::default();
        let rows = repo::outbox::due::<ProjectionOp>(&store, limit)?;
        if rows.is_empty() {
            report.remaining = repo::outbox::pending_count(&store)?;
            return Ok(report);
        }
        let resolver = self.resolver(&dig.id);
        let mut resolver = resolver.lock().await;
        if resolver.by_name.is_empty() {
            resolver.seed(&provider.list_labels().await?);
        }

        // One row per thread per pass.
        let mut seen_threads: HashSet<String> = HashSet::new();
        let mut selected = Vec::new();
        for row in rows {
            match &row.thread_id {
                Some(t) if !seen_threads.insert(t.clone()) => continue,
                _ => selected.push(row),
            }
        }

        // The provider's view, in one batch, for conflict detection.
        let provider_ids: Vec<String> = selected
            .iter()
            .map(|r| r.payload.provider_id.clone())
            .collect();
        let current: HashMap<String, Message> = provider
            .get_messages_batch(&provider_ids, Detail::Metadata)
            .await?
            .into_iter()
            .map(|m| (m.provider_id.clone(), m))
            .collect();

        // Group applicable rows by (add ids, remove ids).
        let mut groups: HashMap<(Vec<String>, Vec<String>), Vec<Applicable>> = HashMap::new();
        for row in selected {
            let Some(message_id) = row.message_id.clone().map(MessageId) else {
                repo::outbox::mark_skipped(&store, row.id, "row without message id")?;
                report.skipped += 1;
                continue;
            };
            let Some(live) = current.get(&row.payload.provider_id) else {
                repo::outbox::mark_skipped(
                    &store,
                    row.id,
                    "message no longer exists at the provider",
                )?;
                report.skipped += 1;
                continue;
            };
            let known = repo::messages::labels_of(&store, &message_id)?;
            if non_volatile(&known) != non_volatile(&live.labels) {
                // Provider-side change wins (§15). Record, adopt, move on.
                rwarn!(
                    Category::Projection,
                    "conflict on {}: provider labels changed underneath; row {} skipped",
                    message_id,
                    row.id
                );
                repo::outbox::mark_skipped(
                    &store,
                    row.id,
                    "conflict: provider state changed underneath",
                )?;
                repo::messages::set_labels(&store, &message_id, &live.labels)?;
                report.conflicts.push(Conflict {
                    message_id,
                    thread_id: row.thread_id.clone().map(ThreadId),
                    op: row.payload.clone(),
                    expected: known,
                    actual: live.labels.clone(),
                });
                continue;
            }
            let mut add_ids = Vec::new();
            for name in &row.payload.add {
                let id = resolver.resolve(provider.as_ref(), name).await?;
                if !live.labels.contains(&id) && !add_ids.contains(&id) {
                    add_ids.push(id);
                }
            }
            let mut remove_ids = Vec::new();
            for name in &row.payload.remove {
                let id = resolver.resolve(provider.as_ref(), name).await?;
                if live.labels.contains(&id) && !remove_ids.contains(&id) {
                    remove_ids.push(id);
                }
            }
            if row.payload.remove_managed {
                for l in &live.labels {
                    if resolver.is_managed_id(l) && !remove_ids.contains(l) {
                        remove_ids.push(l.clone());
                    }
                }
            }
            add_ids.sort();
            remove_ids.sort();
            if add_ids.is_empty() && remove_ids.is_empty() {
                // Already in the wanted state: idempotent success.
                repo::outbox::mark_applied(&store, row.id)?;
                report.applied += 1;
                continue;
            }
            groups.entry((add_ids, remove_ids)).or_default().push((
                row.id,
                message_id,
                row.payload.provider_id.clone(),
                live.labels.clone(),
            ));
        }

        for ((add, remove), rows) in groups {
            let ids: Vec<String> = rows.iter().map(|r| r.2.clone()).collect();
            match provider.modify_labels(&ids, &add, &remove).await {
                Ok(()) => {
                    rinfo!(
                        Category::Projection,
                        "dig {} applied {} label changes (+{} -{})",
                        dig.id,
                        ids.len(),
                        add.len(),
                        remove.len()
                    );
                    for (row_id, message_id, _, live) in rows {
                        let after = expected_after(
                            &live,
                            &ProjectionPlan {
                                add: add.clone(),
                                remove: remove.clone(),
                            },
                        );
                        repo::messages::set_labels(&store, &message_id, &after)?;
                        repo::outbox::mark_applied(&store, row_id)?;
                        report.applied += 1;
                    }
                }
                Err(e) => {
                    rwarn!(
                        Category::Projection,
                        "dig {} projection batch of {} failed: {}",
                        dig.id,
                        ids.len(),
                        e.kind.as_str()
                    );
                    for (row_id, _, _, _) in rows {
                        repo::outbox::mark_failed(&store, row_id, &e.message)?;
                        report.failed += 1;
                    }
                    if !e.kind.is_transient() && e.kind != riddle_errors::ErrorKind::Provider {
                        return Err(e);
                    }
                    if e.kind == riddle_errors::ErrorKind::ProviderRateLimited
                        || e.kind == riddle_errors::ErrorKind::ProviderAuth
                    {
                        report.remaining = repo::outbox::pending_count(&store)?;
                        return Err(e);
                    }
                }
            }
        }
        report.remaining = repo::outbox::pending_count(&store)?;
        Ok(report)
    }

    /// Drain a Dig's outbox in passes until nothing is due or an error parks it.
    pub async fn drain(&self, dig: &Dig, max_passes: u32) -> Result<ProcessReport> {
        let mut total = ProcessReport::default();
        for _ in 0..max_passes {
            let r = self.process(dig, 200).await?;
            let progressed = r.applied + r.failed + r.skipped + r.conflicts.len() as u64;
            total.applied += r.applied;
            total.failed += r.failed;
            total.skipped += r.skipped;
            total.conflicts.extend(r.conflicts);
            total.remaining = r.remaining;
            if progressed == 0 {
                break;
            }
        }
        Ok(total)
    }
}

/// The projection a classification implies, as label names, for resync
/// reconciliation (§15): report differences, never overwrite.
pub fn expected_managed_labels(
    dig: &Dig,
    classification: Option<&riddle_core::Classification>,
) -> ProjectionPlan {
    match classification {
        Some(k) if !k.shadow => crate::plan::plan(&k.disposition, &k.strata, dig.projection, false),
        _ => ProjectionPlan::default(),
    }
}
