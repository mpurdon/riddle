//! Authorization and confirmation policy (§51). Every check happens here,
//! in the Action layer, never only in a dialog.

use riddle_core::{ActionMeta, Autonomy, ConfirmationPolicy, DigId, Principal};
use riddle_errors::{Result, RiddleError};
use std::collections::HashMap;

/// Per-Dig, per-Action autonomy with a default per Dig.
#[derive(Debug, Clone, Default)]
pub struct AutonomyPolicy {
    pub default: Autonomy,
    pub per_dig: HashMap<DigId, Autonomy>,
    pub per_action: HashMap<(Option<DigId>, String), Autonomy>,
    /// Dev builds start in OBSERVE and must be armed before provider mutations (§59).
    pub armed: bool,
    pub dev: bool,
}

impl AutonomyPolicy {
    pub fn level_for(&self, dig: Option<&DigId>, action: &str) -> Autonomy {
        if let Some(a) = self.per_action.get(&(dig.cloned(), action.to_string())) {
            return *a;
        }
        if let Some(a) = self.per_action.get(&(None, action.to_string())) {
            return *a;
        }
        if let Some(d) = dig {
            if let Some(a) = self.per_dig.get(d) {
                return *a;
            }
        }
        self.default
    }
}

/// The decision for one invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Proceed,
    /// Return a pending action with this confirmation tier.
    Confirm(ConfirmationPolicy, String),
    Deny(String),
}

pub fn decide(
    meta: &ActionMeta,
    principal: &Principal,
    dig: Option<&DigId>,
    policy: &AutonomyPolicy,
    confirmed: bool,
) -> Decision {
    if meta.read_only {
        return Decision::Proceed;
    }
    // Dev builds: provider mutations need arming, whoever asks.
    if policy.dev && meta.external_side_effect && !policy.armed {
        return Decision::Deny(
            "dev build is not armed for provider mutations; arm it in Developer Mode".into(),
        );
    }
    match principal {
        Principal::User | Principal::Cli => {
            if confirmed || meta.confirmation == ConfirmationPolicy::None {
                Decision::Proceed
            } else {
                Decision::Confirm(
                    meta.confirmation,
                    format!("{} requires confirmation", meta.name),
                )
            }
        }
        Principal::Sift | Principal::Temporal | Principal::System => {
            // Automation obeys the autonomy ladder; SUGGEST records a proposal
            // upstream and never reaches a mutating Action.
            let level = policy.level_for(dig, &meta.name);
            if level >= meta.minimum_autonomy && level >= Autonomy::Act {
                Decision::Proceed
            } else {
                Decision::Deny(format!(
                    "{} is at {} for this Dig; {} needs {}",
                    meta.name,
                    level.as_str(),
                    meta.name,
                    meta.minimum_autonomy.as_str()
                ))
            }
        }
        Principal::Director { read_untrusted, .. }
        | Principal::Expedition { read_untrusted, .. } => {
            if meta.egress && *read_untrusted && !confirmed {
                return Decision::Confirm(
                    ConfirmationPolicy::Approval,
                    "this task has read untrusted content; egress needs your approval".into(),
                );
            }
            let level = policy.level_for(dig, &meta.name);
            if confirmed {
                return Decision::Proceed;
            }
            if level < Autonomy::Act || level < meta.minimum_autonomy {
                return Decision::Confirm(
                    if meta.confirmation == ConfirmationPolicy::None {
                        ConfirmationPolicy::Approval
                    } else {
                        meta.confirmation
                    },
                    format!("{} is at {} for this Dig", meta.name, level.as_str()),
                );
            }
            if meta.confirmation != ConfirmationPolicy::None
                && meta.confirmation != ConfirmationPolicy::InPlaceUndo
            {
                return Decision::Confirm(
                    meta.confirmation,
                    format!("{} requires confirmation", meta.name),
                );
            }
            Decision::Proceed
        }
        Principal::Mcp => {
            if confirmed {
                Decision::Proceed
            } else {
                Decision::Confirm(
                    ConfirmationPolicy::Approval,
                    "external agents confirm every mutation".into(),
                )
            }
        }
    }
}

pub fn deny(reason: String) -> RiddleError {
    RiddleError::policy_denied(reason)
}

pub fn ensure_args<T: serde::de::DeserializeOwned>(args: &serde_json::Value) -> Result<T> {
    serde_json::from_value(args.clone())
        .map_err(|e| RiddleError::invalid(format!("invalid arguments: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::{ConversationId, Risk};

    fn mutating() -> ActionMeta {
        crate::meta("move_to_spoil", "move")
            .mutating()
            .reversible()
            .risk(Risk::Medium)
            .confirm(ConfirmationPolicy::InPlaceUndo)
            .autonomy(Autonomy::Act)
            .build()
    }

    #[test]
    fn a_user_proceeds_on_an_in_place_undo_action_only_after_confirming() {
        let p = AutonomyPolicy::default();
        assert!(matches!(
            decide(&mutating(), &Principal::User, None, &p, false),
            Decision::Confirm(ConfirmationPolicy::InPlaceUndo, _)
        ));
        assert_eq!(
            decide(&mutating(), &Principal::User, None, &p, true),
            Decision::Proceed
        );
    }

    #[test]
    fn sift_is_denied_below_act_and_proceeds_at_act() {
        let mut p = AutonomyPolicy {
            default: Autonomy::Suggest,
            ..Default::default()
        };
        assert!(matches!(
            decide(&mutating(), &Principal::Sift, None, &p, false),
            Decision::Deny(_)
        ));
        p.default = Autonomy::Act;
        assert_eq!(
            decide(&mutating(), &Principal::Sift, None, &p, false),
            Decision::Proceed
        );
    }

    #[test]
    fn a_director_that_read_untrusted_content_needs_approval_for_egress() {
        let p = AutonomyPolicy {
            default: Autonomy::Trusted,
            ..Default::default()
        };
        let send = crate::meta("send_message", "send")
            .mutating()
            .external()
            .egress()
            .risk(Risk::High)
            .confirm(ConfirmationPolicy::TwoStep)
            .autonomy(Autonomy::Trusted)
            .build();
        let principal = Principal::Director {
            conversation_id: ConversationId::new(),
            read_untrusted: true,
        };
        assert!(matches!(
            decide(&send, &principal, None, &p, false),
            Decision::Confirm(ConfirmationPolicy::Approval, _)
        ));
    }

    #[test]
    fn an_unarmed_dev_build_denies_provider_mutations_even_for_the_user() {
        let p = AutonomyPolicy {
            dev: true,
            armed: false,
            ..Default::default()
        };
        let archive = crate::meta("archive_messages", "archive")
            .mutating()
            .external()
            .build();
        assert!(matches!(
            decide(&archive, &Principal::User, None, &p, true),
            Decision::Deny(_)
        ));
    }
}
