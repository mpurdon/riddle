//! Read services shared by every client. These compose repositories across
//! Digs; they never mutate.

use crate::App;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use riddle_store::repo::messages::MessageFilter;
use std::collections::HashMap;

/// `rule_check:<mesh id>:<message id>` = `right` | `wrong`.
pub const GRADE_PREFIX: &str = "rule_check:";

impl App {
    /// The accounts Riddle works on (every one not disabled), with what Sift
    /// may do to each mailbox right now. Sync, Sift, summaries and every
    /// read across accounts go through this, so a disabled account drops out
    /// of all of them at once.
    pub fn list_digs(&self) -> Result<Vec<Dig>> {
        Ok(self
            .list_all_digs()?
            .into_iter()
            .filter(|d| !d.disabled)
            .collect())
    }

    /// Every account, disabled ones included: for managing accounts (the
    /// Settings list, the switcher's way back to a disabled account).
    pub fn list_all_digs(&self) -> Result<Vec<Dig>> {
        let policy = self.runtime.policy.read().clone();
        Ok(repo::digs::list(&self.stores.index())?
            .into_iter()
            .map(|mut d| {
                d.mailbox = Some(crate::domains::mailbox_effect(&policy, &d));
                d
            })
            .collect())
    }

    pub fn dig(&self, id: &DigId) -> Result<Dig> {
        let mut d = repo::digs::get(&self.stores.index(), id)?
            .ok_or_else(|| RiddleError::not_found(format!("dig {id}")))?;
        d.mailbox = Some(crate::domains::mailbox_effect(
            &self.runtime.policy.read(),
            &d,
        ));
        Ok(d)
    }

    /// Groups with the rules that name them, so a client can say why a
    /// group cannot be renamed or deleted without re-deriving the list.
    pub fn list_dig_groups(&self) -> Result<Vec<DigGroup>> {
        let index = self.stores.index();
        Ok(repo::digs::list_groups(&index)?
            .into_iter()
            .map(|mut g| {
                g.used_by = riddle_mail::actions::group_users(&index, &g.name).unwrap_or_default();
                g
            })
            .collect())
    }

    /// The morning summary (README "The Core Experience"). Honest: `caught_up`
    /// is false while any Dig has unclassified mail or a running catch-up.
    /// With `dig_id` the numbers cover that one Dig, so the Summary and the
    /// rail badges can follow the Dig switcher instead of contradicting the
    /// pages that already do.
    pub fn summary(
        &self,
        since: chrono::DateTime<chrono::Utc>,
        dig_id: Option<&DigId>,
    ) -> Result<Summary> {
        let index = self.stores.index();
        let digs = match dig_id {
            Some(d) => vec![self.dig(d)?],
            None => repo::digs::list_enabled(&index)?,
        };
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
            // Inside the sync window: older mail the history backfill is
            // still bringing in does not keep the morning from "caught up".
            let un = self.unclassified_in_window(&store)?;
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
        let finds_open = repo::finds::count_open(&index, dig_id)?;
        let running_catch_up = repo::jobs::active(&index)?.iter().any(|j| {
            matches!(
                j.kind,
                JobKind::CatchUp | JobKind::InitialSync | JobKind::Sift
            ) && matches!(j.status, JobStatus::Running | JobStatus::Queued)
                && dig_id.is_none_or(|d| j.dig_id.as_ref().is_none_or(|jd| jd == d))
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
            // Offers are a card, not the summary: a detection that cannot
            // be read leaves the card empty and the morning intact.
            pack_offers: match self.pack_offers(dig_id.map(std::slice::from_ref).unwrap_or(&[])) {
                Ok(offers) => offers,
                Err(e) => {
                    riddle_observability::swallowed!(
                        riddle_observability::Category::App,
                        "pack_offers",
                        e
                    );
                    vec![]
                }
            },
        })
    }

    /// The trust card's evidence (Flow 7): decisions, corrections and undos
    /// in the window, what Sift would have archived had the account been at
    /// Act, per-rule precision, and one offer when an account's last Spoil
    /// calls all stood. Everything comes from the stores; nothing is asked.
    pub fn trust_stats(&self, since_days: u32, dig_id: Option<&DigId>) -> Result<TrustStats> {
        const OFFER_STREAK: u64 = 40;
        let since = chrono::Utc::now() - chrono::Duration::days(since_days.max(1) as i64);
        let index = self.stores.index();
        let digs = match dig_id {
            Some(d) => vec![self.dig(d)?],
            None => repo::digs::list_enabled(&index)?,
        };
        let policy = self.runtime.policy.read().clone();
        let mut decisions = 0u64;
        let mut corrected = 0u64;
        let mut would_archive = 0u64;
        let mut per_rule: HashMap<(MeshId, u32), (u64, u64)> = HashMap::new();
        let mut best_offer: Option<TrustOffer> = None;
        for dig in &digs {
            let store = self.stores.dig(&dig.id)?;
            let level = policy.level_for(Some(&dig.id), crate::domains::PROJECTION_AUTONOMY);
            let held = level < Autonomy::Act;
            let (total, spoil) = repo::classifications::sift_totals_since(&store, &since)?;
            decisions += total;
            if held && dig.projection == ProjectionMode::Full {
                would_archive += spoil;
            }
            for (mid, version, n) in repo::classifications::decisions_by_mesh_since(&store, &since)?
            {
                per_rule.entry((MeshId::from(mid), version)).or_default().0 += n;
            }
            let (corrected_here, by_mesh) =
                repo::corrections::active_since_by_mesh(&store, &since)?;
            corrected += corrected_here;
            for (mid, version, n) in by_mesh {
                per_rule.entry((MeshId::from(mid), version)).or_default().1 += n;
            }
            // The seducible moment: consecutive Sift Spoil calls that stood.
            let reversed_spoil: std::collections::HashSet<MessageId> =
                repo::corrections::reversed_from_spoil(&store)?
                    .into_iter()
                    .collect();
            let mut streak = 0u64;
            for id in
                repo::classifications::recent_sift_ids_of_kind(&store, DispositionKind::Spoil, 200)?
            {
                if reversed_spoil.contains(&id) {
                    break;
                }
                streak += 1;
            }
            if held
                && dig.projection != ProjectionMode::None
                && streak >= OFFER_STREAK
                && best_offer.as_ref().is_none_or(|o| streak > o.spoil_calls)
            {
                best_offer = Some(TrustOffer {
                    dig_id: dig.id.clone(),
                    dig_name: dig.name.clone(),
                    spoil_calls: streak,
                    projection: dig.projection,
                    autonomy: level,
                });
            }
        }
        let undone =
            repo::activities::undone_count_since(&index, &since, dig_id.map(|d| d.as_str()))?;
        let mut rules = Vec::new();
        for ((mid, version), (n, reversed)) in per_rule {
            if let Some(m) = repo::meshes::get(&index, &mid, version)? {
                rules.push(RulePrecision {
                    mesh_id: mid,
                    name: m.name,
                    version,
                    status: m.status,
                    decisions: n,
                    reversed,
                    shadow_proposed: m.shadow_proposed,
                    shadow_agreed: m.shadow_agreed,
                    reversed_often: reversed_often(n as usize, reversed as usize),
                });
            }
        }
        rules.sort_by(|a, b| b.decisions.cmp(&a.decisions).then(a.name.cmp(&b.name)));
        Ok(TrustStats {
            since,
            dig_count: digs.len() as u32,
            decisions,
            corrected,
            undone,
            would_archive,
            rules,
            offer: best_offer,
        })
    }

    /// Messages across Digs (or one Dig), merged newest first.
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
        self.merge_messages(&digs, |_| filter.clone(), page)
    }

    /// The Priority inbox: only what the user chose to see first. Messages
    /// carrying a chosen Stratum, a chosen provider label (Starred,
    /// Important), or belonging to an open Find at or above the chosen
    /// priority, OR-combined (`Settings.priority_inbox`). The Find sources
    /// come from the index store and are handed to each Dig's query as ids,
    /// since a Dig store cannot join across databases.
    pub fn priority_inbox(
        &self,
        dig_id: Option<&DigId>,
        page: &PageRequest,
    ) -> Result<Page<MessageSummary>> {
        let settings = self
            .services
            .require::<riddle_settings::SettingsStore>()?
            .settings();
        let chosen = settings.priority_inbox;
        let digs = match dig_id {
            Some(d) => vec![self.dig(d)?],
            None => self.list_digs()?,
        };
        let mut by_dig: HashMap<DigId, Vec<MessageId>> = HashMap::new();
        for (dig, message) in repo::finds::open_source_ids(
            &self.stores.index(),
            chosen.min_find_priority.unwrap_or(0),
            &chosen.find_types,
        )? {
            by_dig.entry(dig).or_default().push(message);
        }
        self.merge_messages(
            &digs,
            |dig| MessageFilter {
                priority: Some(repo::messages::PriorityCriteria {
                    strata: chosen.strata.clone(),
                    labels: chosen.labels.clone(),
                    message_ids: by_dig.get(&dig.id).cloned().unwrap_or_default(),
                    min_importance: chosen.min_importance,
                }),
                ..Default::default()
            },
            page,
        )
    }

    /// One page across Digs, newest first, with the filter built per Dig.
    /// Cross-Dig paging uses one offset applied per Dig, which over-fetches
    /// by the number of Digs; acceptable for a desktop list.
    pub(crate) fn merge_messages(
        &self,
        digs: &[Dig],
        filter_for: impl Fn(&Dig) -> MessageFilter,
        page: &PageRequest,
    ) -> Result<Page<MessageSummary>> {
        if digs.len() == 1 {
            let store = self.stores.dig(&digs[0].id)?;
            return repo::messages::list(&store, &digs[0].id, &filter_for(&digs[0]), page);
        }
        let mut items = Vec::new();
        let mut total = 0u64;
        for dig in digs {
            let store = self.stores.dig(&dig.id)?;
            let p = repo::messages::list(&store, &dig.id, &filter_for(dig), page)?;
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
        let yours = repo::importance::get(&store, id)?;
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
            routes: self
                .route_checks(&class, trace.as_ref())
                .unwrap_or_default(),
            importance_set_by_you: yours.is_some(),
            verification_code: repo::facts::get(&store, id)?
                .map(|f| f.verification_code)
                .filter(|c| !c.is_empty()),
            importance: yours.unwrap_or(class.importance),
            earlier_meshes: trace
                .as_ref()
                .map(|t| t.earlier_meshes())
                .unwrap_or_default(),
        })
    }

    /// The deciding Mesh version, replayed against the facts, scores and
    /// list results the decision used. Best effort: an explanation without
    /// it still answers.
    fn route_checks(
        &self,
        class: &Classification,
        trace: Option<&DecisionTrace>,
    ) -> Option<Vec<RouteCheck>> {
        let trace = trace?;
        let index = self.stores.index();
        let mesh =
            repo::meshes::get(&index, class.mesh_id.as_ref()?, class.mesh_version?).ok()??;
        let profile = repo::user_profile::get(&index).unwrap_or_default();
        let sender = trace.facts.sender.clone();
        let named = repo::lists::named(&index).unwrap_or_default();
        let today = riddle_mesh::ProfileLists {
            profile: &profile,
            sender: &sender,
            facts: &trace.facts,
            named: &named,
        };
        // The lists as the decision read them; today's only for a clause
        // the decision never reached, or a trace stored before the
        // snapshot was kept.
        let lists = riddle_mesh::TraceLists {
            snapshot: &trace.list_results,
            sender: &sender,
            fallback: &today,
        };
        Some(riddle_mesh::check_routes(
            &mesh,
            &trace.facts,
            &trace.signals,
            &lists,
        ))
    }

    pub fn read_trace(&self, dig_id: &DigId, id: &TraceId) -> Result<DecisionTrace> {
        let store = self.stores.dig(dig_id)?;
        repo::traces::get(&store, id)?.ok_or_else(|| RiddleError::not_found(format!("trace {id}")))
    }

    /// One Director conversation's whole story (`DirectorAudit`), by its id
    /// or by a link copied from the Director page
    /// (`riddle://director/<id> (Director: <title>)`).
    pub fn director_audit(&self, id_or_link: &str) -> Result<DirectorAudit> {
        let id = id_or_link.trim().trim_start_matches("riddle://director/");
        let conversation_id = &ConversationId::from(id.split_whitespace().next().unwrap_or(""));
        let index = self.stores.index();
        let conversation = repo::conversations::get(&index, conversation_id)?
            .ok_or_else(|| RiddleError::not_found(format!("conversation {conversation_id}")))?;
        let proposals = repo::pending::proposals(&index, Some(conversation_id.as_str()), 500)?;
        let activity = self
            .list_activity(
                &repo::activities::ActivityFilter {
                    conversation_id: Some(conversation_id.as_str().to_string()),
                    ..Default::default()
                },
                &PageRequest {
                    cursor: None,
                    limit: Some(500),
                },
            )?
            .items;
        Ok(DirectorAudit {
            conversation,
            proposals,
            activity,
        })
    }

    /// Every label of the enabled accounts (or one), named from the
    /// account's label list and counted over stored mail. Gmail's own labels
    /// with no mail, and the Unread and Chat bookkeeping labels, are left
    /// out; an account's own labels stay listed when empty.
    pub fn list_labels(&self, dig_id: Option<&DigId>) -> Result<Vec<LabelSummary>> {
        let digs = match dig_id {
            Some(d) => vec![self.dig(d)?],
            None => self.list_digs()?,
        };
        let mut out = Vec::new();
        for d in digs {
            let store = self.stores.dig(&d.id)?;
            let mut counts = repo::provider_labels::counts(&store)?;
            let mut seen = std::collections::HashSet::new();
            let mut push =
                |id: String, name: String, system: bool, managed: bool, n: (u64, u64)| {
                    let kind = if id.starts_with("CATEGORY_") {
                        LabelKind::Category
                    } else if managed || name.starts_with("Riddle/") {
                        LabelKind::Riddle
                    } else if system || id.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
                        LabelKind::System
                    } else {
                        LabelKind::User
                    };
                    if matches!(id.as_str(), "UNREAD" | "CHAT")
                        || (kind != LabelKind::User && kind != LabelKind::Riddle && n.0 == 0)
                    {
                        return;
                    }
                    out.push(LabelSummary {
                        dig_id: d.id.clone(),
                        id,
                        name,
                        kind,
                        messages: n.0,
                        unread: n.1,
                    });
                };
            for l in repo::provider_labels::all(&store)? {
                let n = counts.remove(&l.id).unwrap_or_default();
                seen.insert(l.id.clone());
                push(l.id, l.name, l.system, l.managed_by_riddle, n);
            }
            // Labels on stored mail the list has not named yet (no sync since
            // this build): shown by id until the next sync names them.
            for (id, n) in counts {
                if !seen.contains(&id) {
                    push(id.clone(), id, false, false, n);
                }
            }
        }
        out.sort_by(|a, b| {
            (a.dig_id.as_str(), a.kind as u8, a.name.to_lowercase()).cmp(&(
                b.dig_id.as_str(),
                b.kind as u8,
                b.name.to_lowercase(),
            ))
        });
        Ok(out)
    }

    /// The dev build's mailbox writes and how many queued changes wait.
    pub fn mailbox_writes(&self) -> Result<MailboxWritesState> {
        let policy = self.runtime.policy.read().clone();
        Ok(MailboxWritesState {
            dev: policy.dev,
            writes: policy.writes,
            held: crate::actions::held_changes(&self.services)?,
        })
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
        // Money already spent counts whether or not the account is on now.
        for d in self.list_all_digs()? {
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

    /// Every running Mesh in the order it runs, what each decided over the
    /// window, and which earlier Meshes take mail it also matches
    /// (`riddle_mesh::order`). Over enabled accounts, with Sift's policy.
    pub fn mesh_order(&self, window_days: u32) -> Result<MeshOrder> {
        riddle_mesh::order::mesh_order(
            &self.stores,
            &self.list_digs()?,
            window_days,
            self.services.clock.now(),
            &riddle_mesh::ConfidencePolicy::default(),
        )
    }

    /// Every running Mesh with what it decided and how the checked ones
    /// were graded (`grade_decision`), in run order.
    pub fn rule_check(&self) -> Result<Vec<RuleCheckRow>> {
        let index = self.stores.index();
        let digs = self.list_digs()?;
        let running = repo::meshes::list_running(&index)?;
        let mut out = Vec::new();
        for m in riddle_mesh::eval::ordered(&running) {
            let mut decided = 0;
            for d in &digs {
                decided +=
                    repo::classifications::count_by_mesh(&*self.stores.dig(&d.id)?, m.id.as_str())?;
            }
            let grades = repo::kv::list_prefix(&index, &format!("{GRADE_PREFIX}{}:", m.id))?;
            out.push(RuleCheckRow {
                mesh_id: m.id.clone(),
                mesh_name: m.name.clone(),
                priority: m.priority,
                status: m.status,
                fallback: m.fallback,
                decided,
                right: grades.iter().filter(|(_, v)| v == "right").count() as u64,
                wrong: grades.iter().filter(|(_, v)| v == "wrong").count() as u64,
            });
        }
        Ok(out)
    }

    /// Up to `limit` recent decisions a Mesh made, ungraded ones first and
    /// spread over the recent ones rather than the newest run of one sender.
    pub fn rule_samples(&self, mesh_id: &MeshId, limit: usize) -> Result<Vec<RuleSample>> {
        let index = self.stores.index();
        let mut pool = Vec::new();
        for d in self.list_digs()? {
            let store = self.stores.dig(&d.id)?;
            for k in repo::classifications::recent_by_mesh(&store, mesh_id.as_str(), 300)? {
                let Some(m) = repo::messages::get(&store, &d.id, &k.message_id)? else {
                    continue;
                };
                let grade = repo::kv::get(
                    &index,
                    &format!("{GRADE_PREFIX}{}:{}", mesh_id, k.message_id),
                )?;
                pool.push(RuleSample {
                    dig_id: d.id.clone(),
                    message_id: k.message_id.clone(),
                    from: m.from.clone(),
                    subject: m.subject.clone(),
                    date: m.date,
                    disposition: k.disposition.clone(),
                    strata: k.strata.clone(),
                    grade,
                });
            }
        }
        pool.sort_by_key(|s| std::cmp::Reverse(s.date));
        let (ungraded, graded): (Vec<_>, Vec<_>) =
            pool.into_iter().partition(|s| s.grade.is_none());
        // One per sender first, so ten checks are not ten mails from one shop.
        let mut seen = std::collections::HashSet::new();
        let (first, rest): (Vec<_>, Vec<_>) = ungraded
            .into_iter()
            .partition(|s| seen.insert(s.from.email.to_ascii_lowercase()));
        Ok(first
            .into_iter()
            .chain(rest)
            .chain(graded)
            .take(limit)
            .collect())
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
