//! Suggestions about the rules, assembled from the store for the Director
//! and the Director page.
//!
//! Series: the rows `riddle_agent::series::suggest_series` reads. An open
//! Find knows its message (sender, subject) and the decision that raised it
//! (Mesh, the rule as written); the latest Mesh says whether that route
//! keeps a series now. A route is matched by the text before its arrow, so
//! a route edited to add `+ Replaces` still matches the decisions its
//! earlier text made and the suggestion goes away.
//!
//! Senders: the rows `riddle_agent::senders::suggest_lists` reads. A sender
//! with enough mail in the window is on some lists or none; its recent
//! messages were or were not scored as invoices; the user restored some of
//! its promotions or none; and once per domain the judge has said what kind
//! of organisation it is, kept in the index store with the judge's
//! identity. The judging runs a few senders per Sift pass
//! (`review_senders`) from one grouped query and a few index reads, none
//! per sender, so an idle pass is cheap and a new vendor is known within
//! minutes for one call. A sender an installed pack recognises in its
//! `[detect]` block is neither judged nor offered: it is skipped where the
//! rows are built, so the detector never sees it.

use crate::App;
use riddle_agent::senders::{
    sender_kind_question, suggest_lists, SenderRow, RECENT_PER_SENDER, SUGGEST_AT,
};
use riddle_agent::series::{suggest_series, OpenFindRow};
use riddle_core::*;
use riddle_errors::Result;
use riddle_observability::{rinfo, Category};
use riddle_privacy::{allowed, Purpose};
use riddle_store::{repo, Store};
use std::collections::{HashMap, HashSet};

/// Senders judged per Sift pass: one or two judge calls each.
pub const SENDERS_PER_PASS: usize = 8;
/// How far back a sender's mail counts.
const SENDER_WINDOW_DAYS: i64 = 120;
/// Messages in the window from which a sender is judged on two of them.
const SECOND_SAMPLE_AT: u64 = 6;
/// Characters of a message the judge reads to say what kind of sender it is.
const JUDGE_CHARS: usize = 8_000;

impl App {
    /// Every verdict in the index store from the judge in use, by domain.
    /// A verdict from another judge does not count, so a model change
    /// judges afresh.
    fn sender_verdicts(&self) -> Result<HashMap<String, SenderKindVerdict>> {
        let Some(engine) = self.services.judge().map(|j| j.identity().key()) else {
            return Ok(HashMap::new());
        };
        let index = self.stores.index();
        Ok(repo::kv::list_prefix(&index, SenderKindVerdict::PREFIX)?
            .into_iter()
            .filter_map(|(_, json)| serde_json::from_str::<SenderKindVerdict>(&json).ok())
            .filter(|v| v.engine == engine)
            .map(|v| (v.domain.clone(), v))
            .collect())
    }

    /// Senders with mail in the window and what the store knows of each,
    /// with the lists that exist; `only` narrows to some accounts (empty:
    /// all). A handful of reads per account, none per sender.
    fn sender_rows(&self, only: &[DigId]) -> Result<(Vec<SenderRow>, Vec<NamedListDef>)> {
        let index = self.stores.index();
        let lists = repo::lists::all(&index)?;
        let named = NamedLists::from_defs(lists.iter().cloned());
        let verdicts = self.sender_verdicts()?;
        let recognised = riddle_mesh::packs::recognised_domains(&index)?;
        let since = self.services.clock.now() - chrono::Duration::days(SENDER_WINDOW_DAYS);
        let invoice_signal = repo::signals::latest(&index, "invoice", &[SignalStatus::Active])?;
        let mut rows = Vec::new();
        for dig in self.list_digs()? {
            if !only.is_empty() && !only.contains(&dig.id) {
                continue;
            }
            let store = self.stores.dig(&dig.id)?;
            // Each sender's newest few messages scored as an invoice: the
            // newest ids in one read, their evaluations of the Signal in a
            // few more.
            let mut invoices: HashMap<String, u32> = HashMap::new();
            if let Some(sig) = &invoice_signal {
                let recent = repo::messages::sender_recent_ids(&store, &since, RECENT_PER_SENDER)?;
                let ids: Vec<MessageId> = recent.values().flatten().cloned().collect();
                let scored: HashSet<String> = repo::signals::evaluations_of_signal_for_ids(
                    &store,
                    "invoice",
                    sig.version,
                    &ids,
                )?
                .into_iter()
                .filter(|e| e.distribution.get("YES").copied().unwrap_or(0.0) >= 0.8)
                .map(|e| e.message_id.as_str().to_string())
                .collect();
                for (sender, ids) in recent {
                    let n = ids.iter().filter(|id| scored.contains(id.as_str())).count();
                    if n > 0 {
                        invoices.insert(sender, n as u32);
                    }
                }
            }
            // Promotions the user restored from Spoil, by sender domain.
            let restored_ids: Vec<MessageId> = repo::corrections::active(&store, 500)?
                .into_iter()
                .filter(|c| {
                    c.kind == CorrectionKind::Restore
                        && matches!(c.from_disposition, Some(Disposition::Spoil))
                })
                .map(|c| c.message_id)
                .collect();
            let mut restored: HashMap<String, u32> = HashMap::new();
            for m in repo::messages::summaries_for_ids(&store, &dig.id, &restored_ids)? {
                *restored
                    .entry(m.from.domain().to_ascii_lowercase())
                    .or_default() += 1;
            }
            for (sender, messages) in repo::messages::sender_counts_since(&store, &since)? {
                let domain = email_domain(&sender);
                if domain.is_empty() || recognised_by_a_pack(&recognised, &domain) {
                    continue;
                }
                rows.push(SenderRow {
                    kind: verdicts
                        .get(&domain)
                        .filter(|v| v.agreed)
                        .map(|v| (v.kind, v.score)),
                    invoices: invoices.get(&sender).copied().unwrap_or(0),
                    restored_promotions: restored.get(&domain).copied().unwrap_or(0),
                    lists: lists
                        .iter()
                        .filter(|l| named.contains(&l.name, &sender))
                        .map(|l| l.name.clone())
                        .collect(),
                    messages: messages as u32,
                    sender,
                    domain,
                });
            }
        }
        Ok((rows, lists))
    }

    /// Judge up to `budget` senders in this account that have enough mail,
    /// are an organisation (never a consumer mailbox), are on no list and
    /// have no verdict from this judge: one question per message, on the
    /// newest, and on one from the middle too when the sender writes often.
    /// Returns how many were judged. An account whose privacy keeps mail off
    /// the cloud is left alone.
    pub async fn review_senders(&self, dig: &Dig, budget: usize) -> Result<usize> {
        if !allowed(dig.privacy, Purpose::Judge) {
            return Ok(0);
        }
        let Some(judge) = self.services.judge() else {
            return Ok(0);
        };
        let index = self.stores.index();
        let store = self.stores.dig(&dig.id)?;
        let named = repo::lists::named(&index)?;
        let verdicts = self.sender_verdicts()?;
        let recognised = riddle_mesh::packs::recognised_domains(&index)?;
        let since = self.services.clock.now() - chrono::Duration::days(SENDER_WINDOW_DAYS);
        let question = sender_kind_question();
        let engine = judge.identity().key();
        let mut judged = 0;
        for (sender, messages) in repo::messages::sender_counts_since(&store, &since)? {
            if judged >= budget {
                break;
            }
            let domain = email_domain(&sender);
            let on_a_list = named.0.keys().any(|l| named.contains(l, &sender));
            if messages < u64::from(SUGGEST_AT)
                || !organisation_domain(&domain)
                || verdicts.contains_key(&domain)
                || on_a_list
                || recognised_by_a_pack(&recognised, &domain)
            {
                continue;
            }
            // Newest first, so the first id is the message the question is
            // asked on; the v3 verdicts were judged on the oldest by mistake
            // (17 Oct 2026), which is why v4 judges every domain afresh.
            let ids = repo::messages::ids_from_sender(&store, &sender)?;
            let Some(newest) = ids.first().cloned() else {
                continue;
            };
            let mut samples = vec![newest.clone()];
            if messages >= SECOND_SAMPLE_AT && ids.len() >= 2 {
                samples.push(ids[ids.len() / 2].clone());
            }
            let mut answers = Vec::new();
            for id in &samples {
                if let Some(a) = self
                    .judge_kind(judge.as_ref(), &question, &store, dig, id)
                    .await?
                {
                    answers.push(a);
                }
            }
            let Some(&(kind, _)) = answers.first() else {
                continue;
            };
            repo::kv::set_json(
                &index,
                &SenderKindVerdict::key(&domain),
                &SenderKindVerdict {
                    domain: domain.clone(),
                    kind,
                    score: answers
                        .iter()
                        .map(|(_, s)| *s)
                        .fold(f64::INFINITY, f64::min),
                    agreed: answers.iter().all(|(k, _)| *k == kind),
                    engine: engine.clone(),
                    message_id: newest,
                    judged_at: self.services.clock.now(),
                },
            )?;
            judged += 1;
        }
        if judged > 0 {
            rinfo!(
                Category::Sift,
                "judged what kind of sender {judged} are in {}",
                dig.id
            );
        }
        Ok(judged)
    }

    /// One question to the judge about one message: the kind of sender it
    /// shows and the judge's confidence.
    async fn judge_kind(
        &self,
        judge: &dyn SemanticJudge,
        question: &JudgeQuestion,
        store: &Store,
        dig: &Dig,
        id: &MessageId,
    ) -> Result<Option<(SenderKind, f64)>> {
        let Some(message) = repo::messages::get(store, &dig.id, id)? else {
            return Ok(None);
        };
        let mut text = format!(
            "Subject: {}\nFrom: {} <{}>\n\n{}",
            message.subject,
            message.from.name.clone().unwrap_or_default(),
            message.from.email,
            message
                .body_text
                .clone()
                .unwrap_or_else(|| message.snippet.clone())
        );
        text.truncate(floor_char_boundary(&text, JUDGE_CHARS));
        let answers = judge
            .evaluate(&JudgeState::new(text), std::slice::from_ref(question))
            .await?;
        Ok(answers.into_iter().next().map(|a| {
            let kind = SenderKind::parse(&a.selected).unwrap_or(SenderKind::PersonOrOther);
            let score = a.distribution.get(&a.selected).copied().unwrap_or(0.0);
            (kind, score)
        }))
    }

    /// Senders with enough mail that are on no list and look like they
    /// belong on one.
    pub fn sender_suggestions(&self, only: &[DigId]) -> Result<Vec<SenderSuggestion>> {
        let (rows, lists) = self.sender_rows(only)?;
        Ok(suggest_lists(&rows, &lists))
    }

    /// Everything Riddle noticed about its rules, for the Director page.
    pub fn suggestions(&self, only: &[DigId]) -> Result<Suggestions> {
        Ok(Suggestions {
            series: self.series_suggestions(only)?,
            senders: self.sender_suggestions(only)?,
        })
    }

    /// Senders whose open Finds pile up on one subject under a route that
    /// keeps no series; `only` narrows to some accounts (empty: all). The
    /// Finds' messages are read without their bodies, one pass per account.
    pub fn series_suggestions(&self, only: &[DigId]) -> Result<Vec<SeriesSuggestion>> {
        let index = self.stores.index();
        let mut meshes: HashMap<String, Option<Mesh>> = HashMap::new();
        let mut by_dig: HashMap<String, (DigId, Vec<(Find, MessageId)>)> = HashMap::new();
        for find in repo::finds::list_by_status(&index, FindStatus::Open, None)? {
            let Some(src) = find.sources.first() else {
                continue;
            };
            if !only.is_empty() && !only.contains(&src.dig_id) {
                continue;
            }
            let id = src.message_id.clone();
            by_dig
                .entry(src.dig_id.as_str().to_string())
                .or_insert_with(|| (src.dig_id.clone(), Vec::new()))
                .1
                .push((find, id));
        }
        let mut rows = Vec::new();
        for (dig_id, finds) in by_dig.into_values() {
            let store = self.stores.dig(&dig_id)?;
            let ids: Vec<MessageId> = finds.iter().map(|(_, id)| id.clone()).collect();
            let messages: HashMap<String, Message> =
                repo::messages::summaries_for_ids(&store, &dig_id, &ids)?
                    .into_iter()
                    .map(|m| (m.id.as_str().to_string(), m))
                    .collect();
            for (find, id) in finds {
                let Some(message) = messages.get(id.as_str()) else {
                    continue;
                };
                let Some(decision) = repo::classifications::current(&store, &id)? else {
                    continue;
                };
                let Some(mesh_id) = decision.mesh_id.clone() else {
                    continue;
                };
                let mesh = meshes
                    .entry(mesh_id.as_str().to_string())
                    .or_insert_with(|| repo::meshes::latest(&index, &mesh_id).ok().flatten())
                    .clone();
                let route = mesh
                    .as_ref()
                    .zip(decision.matched_rule.as_deref())
                    .and_then(|(m, rule)| route_like(m, rule));
                rows.push(OpenFindRow {
                    find_id: find.id.clone(),
                    mesh_id,
                    mesh_name: mesh.as_ref().map(|m| m.name.clone()),
                    route: route
                        .map(|r| r.text.clone())
                        .or_else(|| decision.matched_rule.clone()),
                    replaces: route.is_some_and(|r| r.output.replaces),
                    sender: message.from.email.clone(),
                    subject: message.subject.clone(),
                });
            }
        }
        Ok(suggest_series(&rows))
    }
}

/// Whether an installed pack's `[detect]` names the domain or one above it:
/// GitHub's mail is the GitHub pack's business, not a vendor waiting for a
/// list. The one skip, for judging and for offering.
fn recognised_by_a_pack(recognised: &[String], domain: &str) -> bool {
    recognised.iter().any(|d| domain_under(domain, d))
}

/// The latest Mesh's route for a decision's rule as written, matched by the
/// condition (the text before the arrow).
fn route_like<'a>(mesh: &'a Mesh, rule: &str) -> Option<&'a Route> {
    let head = |t: &str| t.split('→').next().unwrap_or("").trim().to_string();
    let want = head(rule);
    mesh.routes.iter().find(|r| head(&r.text) == want)
}
