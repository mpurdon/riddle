//! Suggestions about the rules, assembled from the store for the Director
//! and the Director page.
//!
//! Series: the rows `riddle_agent::series::suggest_series` reads. An open
//! Find knows its message (sender, subject) and the decision that raised it
//! (Mesh, version, the rule as written); the latest Mesh says whether that
//! route keeps a series now. A route is matched by the text before its
//! arrow, so a route edited to add `+ Replaces` still matches the decisions
//! its earlier text made and the suggestion goes away.
//!
//! Senders: the rows `riddle_agent::senders::suggest_lists` reads. A sender
//! with enough mail in the window is on some lists or none; its recent
//! messages were or were not scored as invoices; the user restored some of
//! its promotions or none; and once per domain the judge has said what kind
//! of organisation it is, from its newest message, kept in the index store.
//! The judging runs a few senders per Sift pass (`review_senders`), so a
//! new vendor is known within minutes and costs one call.

use crate::App;
use riddle_agent::senders::{sender_kind_question, suggest_lists, SenderRow, SUGGEST_AT};
use riddle_agent::series::{suggest_series, OpenFindRow};
use riddle_core::*;
use riddle_errors::Result;
use riddle_observability::{rinfo, Category};
use riddle_store::repo;
use std::collections::HashMap;

/// Senders judged per Sift pass: one judge call each.
pub const SENDERS_PER_PASS: usize = 8;
/// How far back a sender's mail counts.
const SENDER_WINDOW_DAYS: i64 = 120;
/// Recent messages read for invoice scores, per sender.
const RECENT_PER_SENDER: usize = 6;
/// Free mailboxes send people's mail, not a vendor's; never judged.
const FREE_MAILBOXES: [&str; 9] = [
    "gmail.com",
    "googlemail.com",
    "outlook.com",
    "hotmail.com",
    "live.com",
    "icloud.com",
    "me.com",
    "yahoo.com",
    "proton.me",
];

impl App {
    /// Senders with mail in the window and what the store knows of each,
    /// with the names of the lists that exist; `only` narrows to some
    /// accounts (empty: all). `cheap` skips the invoice and correction reads,
    /// enough to pick senders to judge.
    fn sender_rows(&self, only: &[DigId], cheap: bool) -> Result<(Vec<SenderRow>, Vec<String>)> {
        let index = self.stores.index();
        let names: Vec<String> = repo::lists::all(&index)?
            .into_iter()
            .map(|l| l.name)
            .collect();
        let named = repo::lists::named(&index)?;
        let since = self.services.clock.now() - chrono::Duration::days(SENDER_WINDOW_DAYS);
        let mut rows = Vec::new();
        for dig in self.list_digs()? {
            if !only.is_empty() && !only.contains(&dig.id) {
                continue;
            }
            let store = self.stores.dig(&dig.id)?;
            // Promotions the user restored from Spoil, by sender domain.
            let mut restored: HashMap<String, u32> = HashMap::new();
            if !cheap {
                for c in repo::corrections::active(&store, 500)? {
                    if c.kind == CorrectionKind::Restore
                        && matches!(c.from_disposition, Some(Disposition::Spoil))
                    {
                        if let Some(m) = repo::messages::get(&store, &dig.id, &c.message_id)? {
                            *restored.entry(domain_of(&m.from.email)).or_default() += 1;
                        }
                    }
                }
            }
            for sender in repo::messages::distinct_senders(&store)? {
                let sender = sender.trim().to_ascii_lowercase();
                let domain = domain_of(&sender);
                if domain.is_empty() {
                    continue;
                }
                let messages = repo::messages::arrivals_from(&store, &sender, &since)?.len() as u32;
                if messages == 0 {
                    continue;
                }
                let mut invoices = 0;
                if !cheap {
                    let ids = repo::messages::ids_from_sender(&store, &sender)?;
                    for id in ids.iter().rev().take(RECENT_PER_SENDER) {
                        let scored = repo::signals::evaluations_for(&store, id)?.iter().any(|e| {
                            e.signal_name == "invoice"
                                && e.distribution.get("YES").copied().unwrap_or(0.0) >= 0.8
                        });
                        if scored {
                            invoices += 1;
                        }
                    }
                }
                let kind = repo::kv::get_json::<SenderKindVerdict>(
                    &index,
                    &SenderKindVerdict::key(&domain),
                )?
                .map(|v| (v.kind, v.score));
                let lists: Vec<String> = names
                    .iter()
                    .filter(|n| named.contains(n, &sender))
                    .cloned()
                    .collect();
                rows.push(SenderRow {
                    sender,
                    domain: domain.clone(),
                    messages,
                    kind,
                    invoices,
                    restored_promotions: restored.get(&domain).copied().unwrap_or(0),
                    lists,
                });
            }
        }
        Ok((rows, names))
    }

    /// Judge up to `budget` senders in this account that have enough mail,
    /// are on no list and have no verdict yet: one question per domain on
    /// its newest message. Returns how many were judged. An account whose
    /// privacy keeps mail off the cloud is left alone.
    pub async fn review_senders(&self, dig: &Dig, budget: usize) -> Result<usize> {
        if matches!(dig.privacy, PrivacyMode::LocalOnly) {
            return Ok(0);
        }
        let Some(judge) = self.services.judge() else {
            return Ok(0);
        };
        let index = self.stores.index();
        let store = self.stores.dig(&dig.id)?;
        let (rows, _) = self.sender_rows(std::slice::from_ref(&dig.id), true)?;
        let question = sender_kind_question();
        let mut judged = 0;
        for row in rows.iter().filter(|r| {
            r.messages >= SUGGEST_AT
                && r.kind.is_none()
                && r.lists.is_empty()
                && !FREE_MAILBOXES.contains(&r.domain.as_str())
        }) {
            if judged >= budget {
                break;
            }
            let Some(id) = repo::messages::ids_from_sender(&store, &row.sender)?
                .into_iter()
                .last()
            else {
                continue;
            };
            let Some(message) = repo::messages::get(&store, &dig.id, &id)? else {
                continue;
            };
            let text: String = format!(
                "From: {} <{}>\nSubject: {}\n\n{}",
                message.from.name.clone().unwrap_or_default(),
                message.from.email,
                message.subject,
                message
                    .body_text
                    .clone()
                    .unwrap_or_else(|| message.snippet.clone())
            )
            .chars()
            .take(8_000)
            .collect();
            let answers = judge
                .evaluate(&JudgeState::new(text), std::slice::from_ref(&question))
                .await?;
            let Some(answer) = answers.into_iter().next() else {
                continue;
            };
            let kind = SenderKind::parse(&answer.selected).unwrap_or(SenderKind::PersonOrOther);
            let score = answer
                .distribution
                .get(&answer.selected)
                .copied()
                .unwrap_or(0.0);
            repo::kv::set_json(
                &index,
                &SenderKindVerdict::key(&row.domain),
                &SenderKindVerdict {
                    domain: row.domain.clone(),
                    kind,
                    score,
                    message_id: id,
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

    /// Senders with enough mail that are on no list and look like they
    /// belong on one.
    pub fn sender_suggestions(&self, only: &[DigId]) -> Result<Vec<SenderSuggestion>> {
        let (rows, names) = self.sender_rows(only, false)?;
        Ok(suggest_lists(&rows, &names))
    }

    /// Everything Riddle noticed about its rules, for the Director page.
    pub fn suggestions(&self, only: &[DigId]) -> Result<Suggestions> {
        Ok(Suggestions {
            series: self.series_suggestions(only)?,
            senders: self.sender_suggestions(only)?,
        })
    }
}

fn domain_of(email: &str) -> String {
    email
        .rsplit_once('@')
        .map(|(_, d)| d.trim().to_ascii_lowercase())
        .unwrap_or_default()
}

impl App {
    /// Senders whose open Finds pile up on one subject under a route that
    /// keeps no series; `only` narrows to some accounts (empty: all).
    pub fn series_suggestions(&self, only: &[DigId]) -> Result<Vec<SeriesSuggestion>> {
        let index = self.stores.index();
        let mut meshes: HashMap<String, Option<Mesh>> = HashMap::new();
        let mut rows = Vec::new();
        for find in repo::finds::list_by_status(&index, FindStatus::Open, None)? {
            let Some(src) = find.sources.first() else {
                continue;
            };
            if !only.is_empty() && !only.contains(&src.dig_id) {
                continue;
            }
            let store = self.stores.dig(&src.dig_id)?;
            let Some(message) = repo::messages::get(&store, &src.dig_id, &src.message_id)? else {
                continue;
            };
            let Some(decision) = repo::classifications::current(&store, &src.message_id)? else {
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
                mesh_version: decision.mesh_version,
                mesh_name: mesh.as_ref().map(|m| m.name.clone()),
                route: route
                    .map(|r| r.text.clone())
                    .or_else(|| decision.matched_rule.clone()),
                replaces: route.is_some_and(|r| r.output.replaces),
                sender: message.from.email.clone(),
                subject: message.subject.clone(),
            });
        }
        Ok(suggest_series(&rows))
    }
}

/// The latest Mesh's route for a decision's rule as written, matched by the
/// condition (the text before the arrow).
fn route_like<'a>(mesh: &'a Mesh, rule: &str) -> Option<&'a Route> {
    let head = |t: &str| t.split('→').next().unwrap_or("").trim().to_string();
    let want = head(rule);
    mesh.routes.iter().find(|r| head(&r.text) == want)
}
