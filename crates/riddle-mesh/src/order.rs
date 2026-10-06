//! The order Meshes run in, with what each decides and which earlier Meshes
//! take mail it also matches. Rules in one ordered list are only as good as
//! the order is visible: a generic "account notification → Filed" rule at
//! Priority 0 filed GitHub push notifications for weeks before the GitHub
//! Mesh at 10 saw them, and nothing said so. This replays the running
//! Meshes over stored facts and cached Signals, as the simulator does, and
//! counts for every Mesh the messages it would decide alone but that an
//! earlier one decides (`simulate::decider`, the same test the preview's
//! "taken first by" uses). No judge is called.

use crate::eval::ordered;
use crate::simulate::{decider, overlaps, ConfidencePolicy};
use riddle_core::{Dig, Mesh, MeshId, MeshOrder, MeshOrderRow};
use riddle_errors::Result;
use riddle_store::{repo, Stores};
use std::collections::HashMap;

/// Run every running Mesh over the window of every given Dig.
pub fn mesh_order(
    stores: &Stores,
    digs: &[Dig],
    window_days: u32,
    now: chrono::DateTime<chrono::Utc>,
    policy: &ConfidencePolicy,
) -> Result<MeshOrder> {
    let running = repo::meshes::list_running(&stores.index())?;
    let named = repo::lists::named(&stores.index())?;
    let since = now - chrono::Duration::days(window_days as i64);
    let mut decided: HashMap<MeshId, u64> = HashMap::new();
    let mut taken: HashMap<MeshId, HashMap<MeshId, u64>> = HashMap::new();
    let mut messages = 0u64;
    for dig in digs {
        let store = stores.dig(&dig.id)?;
        let in_scope: Vec<Mesh> = running
            .iter()
            .filter(|m| m.scope.applies_to(&dig.id, dig.group.as_deref()))
            .cloned()
            .collect();
        for id in repo::messages::ids_since(&store, &since, u32::MAX)? {
            let Some(facts) = repo::facts::get(&store, &id)? else {
                continue;
            };
            if facts.is_sent || facts.is_draft {
                continue;
            }
            messages += 1;
            let evaluations = repo::signals::evaluations_for(&store, &id)?;
            let Some(winner) = decider(&in_scope, &facts, &evaluations, &named, policy) else {
                continue;
            };
            *decided.entry(winner.id.clone()).or_insert(0) += 1;
            for m in in_scope.iter().filter(|m| m.id != winner.id) {
                if decider(
                    std::slice::from_ref(m),
                    &facts,
                    &evaluations,
                    &named,
                    policy,
                )
                .is_some()
                {
                    *taken
                        .entry(m.id.clone())
                        .or_default()
                        .entry(winner.id.clone())
                        .or_insert(0) += 1;
                }
            }
        }
    }
    let rows = ordered(&running)
        .into_iter()
        .enumerate()
        .map(|(i, m)| MeshOrderRow {
            position: i as u32 + 1,
            mesh_id: m.id.clone(),
            mesh_name: m.name.clone(),
            version: m.version,
            status: m.status,
            scope: m.scope.clone(),
            priority: m.priority,
            fallback: m.fallback,
            decided: decided.get(&m.id).copied().unwrap_or(0),
            taken_first_by: overlaps(taken.remove(&m.id).unwrap_or_default(), &running),
        })
        .collect();
    Ok(MeshOrder {
        window_days,
        messages,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use riddle_core::*;

    #[test]
    fn a_generic_mesh_that_runs_first_is_seen_taking_a_specific_meshs_mail() {
        let stores = Stores::in_memory().unwrap();
        let dig = Dig {
            id: DigId::from("d"),
            name: "Gmail".into(),
            email: "me@example.com".into(),
            provider: ProviderKind::Fixture,
            group: None,
            privacy: PrivacyMode::LocalOnly,
            projection: ProjectionMode::None,
            status: DigStatus::Synced,
            shadow_started_at: None,
            created_at: chrono::Utc::now(),
            disabled: false,
            mailbox: None,
        };
        let store = stores.dig(&dig.id).unwrap();
        for (pid, domain) in [
            ("gh1", "github.com"),
            ("gh2", "github.com"),
            ("news", "news.example"),
        ] {
            let mut m = riddle_mail::fixture::blank_message("d", pid);
            m.from = Address::new(format!("x@{domain}"), None);
            repo::messages::upsert(&store, &m).unwrap();
            let facts = Facts {
                sender_domain: domain.into(),
                mailing_list: true,
                ..Default::default()
            };
            repo::facts::put(&store, &m.id, &facts).unwrap();
        }
        for (text, id) in [
            (
                "Mesh: Lists\nPriority: 0\nROUTES\nIF mailing_list → Filed",
                "lists",
            ),
            (
                "Mesh: GitHub\nPriority: 10\nROUTES\nIF sender_domain = github.com → Spoil",
                "gh",
            ),
        ] {
            let mut m = parse(text).unwrap();
            m.id = MeshId(id.into());
            m.status = MeshStatus::Active;
            repo::meshes::upsert(&stores.index(), &m).unwrap();
        }
        let order = mesh_order(
            &stores,
            &[dig],
            30,
            chrono::Utc::now(),
            &ConfidencePolicy::default(),
        )
        .unwrap();
        assert_eq!(order.messages, 3);
        let names: Vec<&str> = order.rows.iter().map(|r| r.mesh_name.as_str()).collect();
        assert_eq!(names, ["Lists", "GitHub"], "Priority order");
        assert_eq!(order.rows[0].decided, 3);
        assert_eq!(order.rows[1].decided, 0);
        let taken = &order.rows[1].taken_first_by;
        assert_eq!(taken.len(), 1);
        assert_eq!((taken[0].mesh_name.as_str(), taken[0].count), ("Lists", 2));
    }
}
