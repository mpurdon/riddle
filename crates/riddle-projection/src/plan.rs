//! Classification → provider operations (`ARCHITECTURE.md` §15).
//!
//! ```text
//! Spoil          archive + label Riddle/Spoil
//! Contamination  label Riddle/Contamination; Gmail spam untouched
//! Stratum        label Riddle/<Stratum>
//! Find           none
//! Filed          archive, optional label
//! Unresolved     none; stays in the inbox
//! Snooze         Riddle-only
//! ```
//! `LabelsOnly` never archives; `None` never writes.

use riddle_core::{Disposition, ProjectionMode};
use serde::{Deserialize, Serialize};

pub const SPOIL_LABEL: &str = "Riddle/Spoil";
pub const CONTAMINATION_LABEL: &str = "Riddle/Contamination";
pub const FILED_LABEL: &str = "Riddle/Filed";
pub const INBOX: &str = "INBOX";

/// Labels that change constantly for reasons that are not moves; never
/// evidence of a conflict or a correction.
pub const VOLATILE_LABELS: &[&str] = &["UNREAD", "STARRED", "IMPORTANT"];

pub fn stratum_label(name: &str) -> String {
    format!("{}{}", riddle_mail::MANAGED_PREFIX, name.trim())
}

pub fn is_volatile(label: &str) -> bool {
    VOLATILE_LABELS.contains(&label)
}

/// Label names to add and remove. Names, not ids: ids are resolved when the
/// row is applied, since a managed label may not exist yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionPlan {
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

impl ProjectionPlan {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }
}

/// The plan for one classification. `filed_label` adds `Riddle/Filed` on
/// Filed, which is optional in §15.
pub fn plan(
    disposition: &Disposition,
    strata: &[String],
    mode: ProjectionMode,
    filed_label: bool,
) -> ProjectionPlan {
    let mut p = ProjectionPlan::default();
    if mode == ProjectionMode::None {
        return p;
    }
    let archive = mode == ProjectionMode::Full;
    match disposition {
        Disposition::Spoil => {
            p.add.push(SPOIL_LABEL.into());
            if archive {
                p.remove.push(INBOX.into());
            }
        }
        Disposition::Contamination => p.add.push(CONTAMINATION_LABEL.into()),
        Disposition::Filed => {
            if filed_label {
                p.add.push(FILED_LABEL.into());
            }
            if archive {
                p.remove.push(INBOX.into());
            }
        }
        Disposition::Find { .. } | Disposition::Unresolved => {}
    }
    for s in strata {
        let l = stratum_label(s);
        if !p.add.contains(&l) {
            p.add.push(l);
        }
    }
    p
}

/// The labels a message should carry after `plan` is applied to `current`.
pub fn expected_after(current: &[String], plan: &ProjectionPlan) -> Vec<String> {
    let mut out: Vec<String> = current
        .iter()
        .filter(|l| !plan.remove.contains(l))
        .cloned()
        .collect();
    for a in &plan.add {
        if !out.contains(a) {
            out.push(a.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::FindType;

    #[test]
    fn full_projection_archives_spoil_and_labels_everything_it_should() {
        let p = plan(
            &Disposition::Spoil,
            &["Newsletters".into()],
            ProjectionMode::Full,
            false,
        );
        assert_eq!(
            p.add,
            vec!["Riddle/Spoil".to_string(), "Riddle/Newsletters".to_string()]
        );
        assert_eq!(p.remove, vec!["INBOX".to_string()]);
        let c = plan(
            &Disposition::Contamination,
            &[],
            ProjectionMode::Full,
            false,
        );
        assert_eq!(c.add, vec!["Riddle/Contamination".to_string()]);
        assert!(c.remove.is_empty());
        let f = plan(&Disposition::Filed, &[], ProjectionMode::Full, true);
        assert_eq!(f.add, vec!["Riddle/Filed".to_string()]);
        assert_eq!(f.remove, vec!["INBOX".to_string()]);
        assert!(plan(
            &Disposition::Find {
                find_type: FindType::Respond
            },
            &[],
            ProjectionMode::Full,
            false
        )
        .is_empty());
        assert!(plan(&Disposition::Unresolved, &[], ProjectionMode::Full, false).is_empty());
    }

    #[test]
    fn labels_only_never_archives_and_none_never_writes() {
        let p = plan(&Disposition::Spoil, &[], ProjectionMode::LabelsOnly, false);
        assert_eq!(p.add, vec!["Riddle/Spoil".to_string()]);
        assert!(p.remove.is_empty());
        assert!(plan(
            &Disposition::Spoil,
            &["X".into()],
            ProjectionMode::None,
            true
        )
        .is_empty());
    }

    #[test]
    fn expected_labels_follow_the_plan() {
        let p = plan(&Disposition::Spoil, &[], ProjectionMode::Full, false);
        let after = expected_after(&["INBOX".into(), "UNREAD".into()], &p);
        assert_eq!(
            after,
            vec!["UNREAD".to_string(), "Riddle/Spoil".to_string()]
        );
    }
}
