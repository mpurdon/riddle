//! The default Meshes seeded on first run (§2, README "Never Hidden"). They
//! are written in the DSL so the user reads exactly what runs, and they pin
//! the v1 system Signals. Every default that hides mail is scoped by a
//! deterministic fact where one exists (`sender_domain`, `automated_sender`)
//! so a human writing "unsubscribe me from this thread" does not become a
//! newsletter. The "System guards" Mesh carries the Never Hidden vetoes in
//! Mesh form so they show in the Mesh Inspector; the Sift gate enforces them
//! before any Signal regardless (invariant 42).

use crate::parse::parse;
use riddle_core::{CreatedBy, Mesh, MeshId, MeshStatus};

pub struct DefaultMesh {
    pub id: &'static str,
    pub text: &'static str,
}

pub const DEFAULTS: &[DefaultMesh] = &[
    DefaultMesh {
        id: "mesh_system_guards",
        text: "Mesh: System guards v1\nScope:\nAll Digs\nPriority: -100\n\nGUARDS\n\nNEVER Spoil IF sender IN never_hidden\nNEVER Contamination IF sender IN never_hidden\nNEVER Contamination IF sender IN contacts\nNEVER Spoil IF sender_is_vip\nNEVER Spoil IF sender IN family\nNEVER Spoil IF has_calendar_part\n",
    },
    DefaultMesh {
        id: "mesh_security_alerts",
        text: "Mesh: Security and account alerts v1\nScope:\nAll Digs\nPriority: 0\n\nROUTES\n\nIF security_alert@v1 > .80\n    → Find: Know + Stratum: Notifications\n\nELSE IF account_notification@v1 > .85 AND routine@v1 > .80\n    → Filed + Stratum: Notifications\n\nGUARDS\n\nNEVER Spoil IF security_alert@v1 > .80\n",
    },
    DefaultMesh {
        id: "mesh_calendar",
        text: "Mesh: Calendar v1\nScope:\nAll Digs\nPriority: 5\n\nROUTES\n\nIF has_calendar_part\n    → Find: Attend\n\nELSE IF meeting_related@v1 > .85 AND requires_response@v1 > .70\n    → Find: Attend\n",
    },
    DefaultMesh {
        id: "mesh_github_notifications",
        text: "Mesh: GitHub Notifications v1\nScope:\nAll Digs\nPriority: 10\n\nROUTES\n\nIF sender_domain = github.com AND review_requested@v1 > .80\n    → Find: Review + Stratum: Engineering\n\nELSE IF sender_domain = github.com AND ci_failure@v1 > .85 AND personally_relevant@v1 > .70\n    → Find: Do + Stratum: Engineering\n\nELSE IF sender_domain = github.com AND deployment_failed@v1 > .85 AND owned_work_failed@v1 > .60\n    → Find: Do + Stratum: Engineering\n\nELSE IF sender_domain = github.com AND routine@v1 > .90 AND personally_relevant@v1 < .30\n    → Spoil + Stratum: Engineering\n\nELSE IF sender_domain = github.com\n    → Escalate\n\nGUARDS\n\nNEVER Spoil IF sender IN never_hidden\n",
    },
    DefaultMesh {
        id: "mesh_receipts",
        text: "Mesh: Receipts v1\nScope:\nAll Digs\nPriority: 20\n\nROUTES\n\nIF has_structured_receipt\n    → Filed + Stratum: Purchases + Stratum: Receipts\n\nELSE IF receipt@v1 > .85\n    → Filed + Stratum: Purchases + Stratum: Receipts\n",
    },
    DefaultMesh {
        id: "mesh_payments",
        text: "Mesh: Payments v1\nScope:\nAll Digs\nPriority: 25\n\nROUTES\n\nIF invoice@v1 > .80 AND requires_action@v1 > .60\n    → Find: Pay + Stratum: Finance\n\nELSE IF payment_related@v1 > .80 AND requires_action@v1 > .70\n    → Find: Pay + Stratum: Finance\n\nELSE IF represents_charge@v1 > .80\n    → Filed + Stratum: Finance\n",
    },
    DefaultMesh {
        id: "mesh_newsletters",
        text: "Mesh: Newsletters v1\nScope:\nAll Digs\nPriority: 30\n\nROUTES\n\nIF automated_sender AND newsletter@v1 > .80 AND personally_relevant@v1 < .40\n    → Spoil + Stratum: Newsletters\n\nELSE IF automated_sender AND marketing@v1 > .80 AND personally_relevant@v1 < .40\n    → Spoil + Stratum: Newsletters\n",
    },
    DefaultMesh {
        id: "mesh_human_correspondence",
        text: "Mesh: Human correspondence v1\nScope:\nAll Digs\nPriority: 40\n\nROUTES\n\nIF human_authored@v1 > .70 AND requires_response@v1 > .70\n    → Find: Respond + Stratum: People\n\nELSE IF human_authored@v1 > .70 AND asks_question@v1 > .80\n    → Find: Respond + Stratum: People\n\nELSE IF human_authored@v1 > .70 AND requires_action@v1 > .75\n    → Find: Do + Stratum: People\n\nELSE IF human_authored@v1 > .70 AND routine@v1 > .60\n    → Filed + Stratum: People\n\nELSE IF human_authored@v1 > .70\n    → Filed + Stratum: People\n",
    },
    DefaultMesh {
        id: "mesh_automated_notifications",
        text: "Mesh: Automated notifications v1\nScope:\nAll Digs\nPriority: 50\n\nROUTES\n\nIF automated_sender AND routine@v1 > .85 AND personally_relevant@v1 < .40\n    → Spoil + Stratum: Notifications\n\nELSE IF automated_sender AND social_notification@v1 > .85\n    → Spoil + Stratum: Notifications\n\nELSE IF automated_sender AND transactional@v1 > .80\n    → Filed + Stratum: Notifications\n",
    },
];

/// The default Meshes as ACTIVE v1 with stable ids.
pub fn default_meshes() -> Vec<Mesh> {
    DEFAULTS
        .iter()
        .map(|d| {
            let mut m = parse(d.text).expect("default Mesh text parses");
            m.id = MeshId(d.id.to_string());
            m.version = 1;
            m.status = MeshStatus::Active;
            m.created_by = CreatedBy::System;
            m.created_at = chrono::DateTime::from_timestamp(1_758_499_200, 0).unwrap_or_default();
            m.change_note = Some("default policy v1".into());
            m
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::{validate, StaticResolver};
    use riddle_core::Signal;

    #[test]
    fn every_default_mesh_parses_and_references_only_system_signals_and_known_facts() {
        let names = [
            "requires_response",
            "requires_action",
            "direct_request",
            "personally_relevant",
            "human_authored",
            "routine",
            "novel",
            "marketing",
            "newsletter",
            "transactional",
            "payment_related",
            "meeting_related",
            "review_requested",
            "ci_failure",
            "deployment_failed",
            "owned_work_failed",
            "asks_question",
            "makes_commitment",
            "requests_action",
            "acknowledges_cancellation",
            "represents_charge",
            "security_alert",
            "account_notification",
            "social_notification",
            "receipt",
            "invoice",
            "travel_reservation",
        ];
        let resolver = StaticResolver(names.iter().map(|n| Signal::noul(n, "x")).collect());
        let meshes = default_meshes();
        assert_eq!(meshes.len(), DEFAULTS.len());
        for m in &meshes {
            validate(m, &resolver).unwrap_or_else(|e| panic!("{}: {}", m.name, e.message));
            assert_eq!(m.status, MeshStatus::Active);
        }
        let ids: std::collections::HashSet<_> = meshes.iter().map(|m| m.id.clone()).collect();
        assert_eq!(ids.len(), meshes.len());
    }
}
