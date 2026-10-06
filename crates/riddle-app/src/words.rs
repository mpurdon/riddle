//! Reasons in words for decisions made before there were words. A
//! decision's reason is written when the decision is made, so the first
//! build that wrote sentences left every earlier decision reading as the
//! rule grammar, and the answer given at the time, "sift everything again",
//! was wrong: it costs judge calls and a wait to change a string. This runs
//! once per Dig at boot and reads each current Mesh decision against its
//! stored trace, exactly as the Why card replays it, then rewrites its
//! reason and the reason on the Finds that carry it. No judge, no network.

use crate::App;
use riddle_core::{DecisionMechanism, DigId, Disposition, Mesh, MeshId};
use riddle_errors::Result;
use riddle_mesh::humanize::{route_decision, Evidence};
use riddle_store::repo;
use std::collections::HashMap;

/// The `once_per_dig` key.
const KEY: &str = "reasons_in_words";

impl App {
    /// Rewrite this Dig's current Mesh and LLM decisions' reasons as
    /// sentences. Returns how many changed.
    pub fn rewrite_reasons_in_words(&self, dig_id: &DigId) -> Result<usize> {
        let store = self.stores.dig(dig_id)?;
        let index = self.stores.index();
        let mut meshes: HashMap<(MeshId, u32), Option<Mesh>> = HashMap::new();
        let mut changed = 0;
        for mut k in repo::classifications::current_mesh_decisions(&store)? {
            if !matches!(
                k.mechanism,
                DecisionMechanism::Mesh | DecisionMechanism::Llm
            ) || k.disposition == Disposition::Unresolved
            {
                continue;
            }
            let (Some(mesh_id), Some(version), Some(rule)) =
                (k.mesh_id.clone(), k.mesh_version, k.matched_rule.clone())
            else {
                continue;
            };
            let Some(trace) = repo::traces::get(&store, &k.trace_id)? else {
                continue;
            };
            let mesh = meshes
                .entry((mesh_id.clone(), version))
                .or_insert_with(|| repo::meshes::get(&index, &mesh_id, version).ok().flatten());
            let Some(route) = mesh
                .as_ref()
                .and_then(|m| m.routes.iter().find(|r| r.text == rule))
            else {
                continue;
            };
            let words = route_decision(
                route,
                &Evidence {
                    facts: &trace.facts,
                    evaluations: &trace.signals,
                },
            );
            if words == k.reason {
                continue;
            }
            k.reason = words.clone();
            repo::classifications::update(&store, &k)?;
            for mut f in repo::finds::for_message(&index, &k.message_id)? {
                if f.classification_id.as_ref() == Some(&k.id) && f.reason != words {
                    f.reason = words.clone();
                    repo::finds::upsert(&index, &f)?;
                }
            }
            changed += 1;
        }
        Ok(changed)
    }

    /// Every Dig, once (see `once_per_dig`).
    pub fn rewrite_all_reasons_in_words(&self) {
        self.once_per_dig(KEY, |dig_id| self.rewrite_reasons_in_words(dig_id));
    }
}
