//! Suggestions about the rules, assembled from the store for the Director
//! and the Director page: the rows `riddle_agent::series::suggest_series`
//! reads. An open Find knows its message (sender, subject) and the decision
//! that raised it (Mesh, version, the rule as written); the latest Mesh says
//! whether that route keeps a series now. A route is matched by the text
//! before its arrow, so a route edited to add `+ Replaces` still matches
//! the decisions its earlier text made and the suggestion goes away.

use crate::App;
use riddle_agent::series::{suggest_series, OpenFindRow};
use riddle_core::*;
use riddle_errors::Result;
use riddle_store::repo;
use std::collections::HashMap;

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
