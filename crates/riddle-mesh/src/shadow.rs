//! Shadow Period bookkeeping per Mesh (§51, README "The Shadow Period"). A
//! Mesh in SUGGEST records what it would have done; the user's agreement is
//! the score it must earn before it acts. Disagreements come from
//! corrections on messages the Mesh proposed for.

use riddle_core::{Mesh, MeshId, MeshStatus};
use riddle_errors::{Result, RiddleError};
use riddle_store::{repo, Store};

fn latest(store: &Store, id: &MeshId) -> Result<Mesh> {
    repo::meshes::latest(store, id)?.ok_or_else(|| RiddleError::not_found(format!("mesh {id}")))
}

/// The Mesh made a proposal in Shadow: one more to be judged.
pub fn record_shadow_proposal(store: &Store, id: &MeshId) -> Result<Mesh> {
    let mut m = latest(store, id)?;
    m.shadow_proposed += 1;
    if m.shadow_started_at.is_none() {
        m.shadow_started_at = Some(chrono::Utc::now());
    }
    repo::meshes::upsert(store, &m)?;
    Ok(m)
}

/// The user acted as the proposal said (or left it alone at review).
pub fn record_agreement(store: &Store, id: &MeshId) -> Result<Mesh> {
    let mut m = latest(store, id)?;
    m.shadow_agreed = (m.shadow_agreed + 1).min(m.shadow_proposed.max(m.shadow_agreed + 1));
    repo::meshes::upsert(store, &m)?;
    Ok(m)
}

/// The user contradicted a proposal. Nothing is subtracted: agreement is
/// counted explicitly, so a disagreement simply is not an agreement.
pub fn record_disagreement(store: &Store, id: &MeshId) -> Result<Mesh> {
    let mut m = latest(store, id)?;
    if m.shadow_proposed == 0 {
        m.shadow_proposed = 1;
    }
    repo::meshes::upsert(store, &m)?;
    Ok(m)
}

/// Whether a Shadow Mesh has earned ACT: the period has run, it proposed at
/// least `min_proposals` times, and the agreement ratio meets the threshold.
pub fn ready_to_act(
    m: &Mesh,
    now: chrono::DateTime<chrono::Utc>,
    days: u32,
    threshold: f64,
    min_proposals: u32,
) -> bool {
    if m.status != MeshStatus::Shadow {
        return false;
    }
    let Some(started) = m.shadow_started_at else {
        return false;
    };
    if now - started < chrono::Duration::days(days as i64) {
        return false;
    }
    if m.shadow_proposed < min_proposals.max(1) {
        return false;
    }
    (m.shadow_agreed as f64 / m.shadow_proposed as f64) >= threshold
}

/// The summary line the Shadow Period card shows.
pub fn agreement_summary(m: &Mesh) -> String {
    if m.shadow_proposed == 0 {
        return format!("{} has made no proposals yet", m.name);
    }
    let disagreed = m.shadow_proposed.saturating_sub(m.shadow_agreed);
    format!(
        "{} proposed {} times; you disagreed with {}",
        m.name, m.shadow_proposed, disagreed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_store::StoreKind;

    #[test]
    fn a_mesh_earns_act_after_the_period_with_enough_agreement() {
        let store = Store::in_memory(StoreKind::Index).unwrap();
        let mut m = crate::parse::parse("Mesh: T\nROUTES\nIF mailing_list → Filed").unwrap();
        m.status = MeshStatus::Shadow;
        m.shadow_started_at = Some(chrono::Utc::now() - chrono::Duration::days(8));
        repo::meshes::upsert(&store, &m).unwrap();
        for _ in 0..10 {
            record_shadow_proposal(&store, &m.id).unwrap();
        }
        for _ in 0..9 {
            record_agreement(&store, &m.id).unwrap();
        }
        let m = record_disagreement(&store, &m.id).unwrap();
        assert!(ready_to_act(&m, chrono::Utc::now(), 7, 0.9, 5));
        assert!(!ready_to_act(&m, chrono::Utc::now(), 7, 0.95, 5));
        assert!(
            !ready_to_act(
                &m,
                chrono::Utc::now() - chrono::Duration::days(5),
                7,
                0.5,
                5
            ),
            "period not over"
        );
        assert_eq!(
            agreement_summary(&m),
            "T proposed 10 times; you disagreed with 1"
        );
    }
}
