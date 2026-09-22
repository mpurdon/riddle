//! Behavioural discovery from corrections (§28, §46): repeated corrections
//! against the same Mesh route accumulate into a suggestion the Director can
//! voice ("You've corrected GitHub review requests three times. I can update
//! the Mesh."). Riddle never rewrites policy from a correction on its own,
//! and a correction cannot become a Mesh without the user saying yes to a
//! proposal that shows the rule text: that is the memory-poisoning defence
//! (§58 control 6; the Microsoft red-team "quietly forward" case).
//!
//! Pure function: it reads active corrections and produces text. Retracted
//! corrections (undone actions) teach nothing and are ignored.

use riddle_core::{Correction, CorrectionKind, Disposition, MeshId};
use std::collections::BTreeMap;

/// Corrections against one route before a suggestion is made.
pub const SUGGEST_AT: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub mesh_id: MeshId,
    pub mesh_version: Option<u32>,
    pub mesh_name: Option<String>,
    pub count: usize,
    /// The dominant correction, e.g. "spoil -> find".
    pub pattern: String,
    /// The sentence the Director says.
    pub text: String,
    pub correction_ids: Vec<String>,
}

fn describe(d: &Option<Disposition>) -> String {
    match d {
        Some(Disposition::Find { find_type }) => format!("Find ({})", find_type.label()),
        Some(Disposition::Filed) => "Filed".into(),
        Some(Disposition::Spoil) => "Spoil".into(),
        Some(Disposition::Contamination) => "Contamination".into(),
        Some(Disposition::Unresolved) => "Unresolved".into(),
        None => "unchanged".into(),
    }
}

/// Group active corrections by the Mesh version that made the corrected
/// decision; three or more against one Mesh yield a suggestion.
pub fn suggest_mesh_updates(
    corrections: &[Correction],
    mesh_name: impl Fn(&MeshId) -> Option<String>,
) -> Vec<Suggestion> {
    let mut groups: BTreeMap<(String, Option<u32>), Vec<&Correction>> = BTreeMap::new();
    for c in corrections.iter().filter(|c| c.retracted_at.is_none()) {
        let Some(mesh) = &c.mesh_id else { continue };
        if !matches!(
            c.kind,
            CorrectionKind::Move
                | CorrectionKind::Restore
                | CorrectionKind::Retype
                | CorrectionKind::Dismiss
        ) {
            continue;
        }
        groups
            .entry((mesh.as_str().to_string(), c.mesh_version))
            .or_default()
            .push(c);
    }
    let mut out = Vec::new();
    for ((mesh, version), items) in groups {
        if items.len() < SUGGEST_AT {
            continue;
        }
        // The dominant from -> to pattern.
        let mut patterns: BTreeMap<String, usize> = BTreeMap::new();
        for c in &items {
            *patterns
                .entry(format!(
                    "{} -> {}",
                    describe(&c.from_disposition),
                    describe(&c.to_disposition)
                ))
                .or_default() += 1;
        }
        let (pattern, _) = patterns
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .unwrap_or_default();
        let mesh_id = MeshId::from(mesh.as_str());
        let name = mesh_name(&mesh_id);
        let what = match &name {
            Some(n) => format!("the {n} Mesh"),
            None => "this Mesh".to_string(),
        };
        let version_text = version.map(|v| format!(" v{v}")).unwrap_or_default();
        let text = format!(
            "You've corrected {what}{version_text} {} times ({pattern}). I can propose an update to the Mesh; you would see the rule text and a simulation before anything changes.",
            items.len()
        );
        out.push(Suggestion {
            mesh_id,
            mesh_version: version,
            mesh_name: name,
            count: items.len(),
            pattern,
            text,
            correction_ids: items.iter().map(|c| c.id.as_str().to_string()).collect(),
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.count));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::{CorrectionId, DigId, FindType, MessageId};

    fn correction(mesh: &str, from: Disposition, to: Disposition, retracted: bool) -> Correction {
        Correction {
            id: CorrectionId::new(),
            kind: CorrectionKind::Move,
            dig_id: DigId::from("d1"),
            message_id: MessageId::new(),
            from_disposition: Some(from),
            to_disposition: Some(to),
            from_find_type: None,
            to_find_type: None,
            mesh_id: Some(MeshId::from(mesh)),
            mesh_version: Some(7),
            activity_id: None,
            created_at: chrono::Utc::now(),
            retracted_at: if retracted {
                Some(chrono::Utc::now())
            } else {
                None
            },
        }
    }

    #[test]
    fn three_corrections_against_one_mesh_route_yield_a_suggestion() {
        let mut cs: Vec<Correction> = (0..3)
            .map(|_| {
                correction(
                    "github",
                    Disposition::Spoil,
                    Disposition::Find {
                        find_type: FindType::Review,
                    },
                    false,
                )
            })
            .collect();
        cs.push(correction(
            "newsletters",
            Disposition::Spoil,
            Disposition::Filed,
            false,
        ));
        cs.push(correction(
            "newsletters",
            Disposition::Spoil,
            Disposition::Filed,
            false,
        ));
        let s = suggest_mesh_updates(&cs, |id| {
            if id.as_str() == "github" {
                Some("GitHub notifications".into())
            } else {
                None
            }
        });
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].count, 3);
        assert_eq!(s[0].mesh_version, Some(7));
        assert!(s[0]
            .text
            .contains("You've corrected the GitHub notifications Mesh v7 3 times"));
        assert!(s[0].text.contains("Spoil -> Find (Review)"));
        assert!(s[0].text.contains("simulation"));
    }

    #[test]
    fn retracted_corrections_teach_nothing() {
        let mut cs: Vec<Correction> = (0..2)
            .map(|_| correction("github", Disposition::Spoil, Disposition::Filed, false))
            .collect();
        cs.push(correction(
            "github",
            Disposition::Spoil,
            Disposition::Filed,
            true,
        ));
        assert!(suggest_mesh_updates(&cs, |_| None).is_empty());
    }
}
