//! Authorization and confirmation policy (§51). Every check happens here,
//! in the Action layer, never only in a dialog.

use riddle_core::{ActionMeta, Autonomy, ConfirmationPolicy, DigId, Principal, ProviderWrites};
use riddle_errors::{Result, RiddleError};
use std::collections::HashMap;

/// Per-Dig, per-Action autonomy with a default per Dig.
#[derive(Debug, Clone, Default)]
pub struct AutonomyPolicy {
    pub default: Autonomy,
    pub per_dig: HashMap<DigId, Autonomy>,
    pub per_action: HashMap<(Option<DigId>, String), Autonomy>,
    /// What a dev build may write to the mailbox (§59); it starts in Hold.
    /// Release builds ignore it.
    pub writes: ProviderWrites,
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

/// Why an Action that calls the provider was refused in Hold.
pub const HOLD_DENIAL: &str =
    "this dev build holds mailbox changes; choose Ask or Allow under Developer → Mailbox writes";

pub fn decide(
    meta: &ActionMeta,
    principal: &Principal,
    dig: Option<&DigId>,
    policy: &AutonomyPolicy,
    confirmed: bool,
) -> Decision {
    // First, so a refused agent never reaches `Confirm`, which would store
    // the arguments in the pending table.
    if meta.user_only && !matches!(principal, Principal::User | Principal::Cli) {
        return Decision::Deny(format!("{} can only be run by you", meta.name));
    }
    if meta.read_only {
        return Decision::Proceed;
    }
    // Dev builds: an Action that calls the provider waits on Hold / Ask /
    // Allow, whoever asks. Projected Actions run; their queued writes wait
    // in the outbox instead.
    if policy.dev && meta.external_side_effect {
        match policy.writes {
            ProviderWrites::Hold => return Decision::Deny(HOLD_DENIAL.into()),
            ProviderWrites::Ask if !confirmed => {
                let tier = match meta.confirmation {
                    ConfirmationPolicy::None | ConfirmationPolicy::InPlaceUndo => {
                        ConfirmationPolicy::Approval
                    }
                    other => other,
                };
                return Decision::Confirm(
                    tier,
                    format!(
                        "{} changes the mailbox; this dev build asks first",
                        meta.name
                    ),
                );
            }
            ProviderWrites::Ask | ProviderWrites::Allow => {}
        }
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
    fn a_user_only_action_is_refused_to_every_agent_even_when_confirmed() {
        let p = AutonomyPolicy {
            default: Autonomy::Trusted,
            ..Default::default()
        };
        let secret = crate::meta("set_secret", "store a secret")
            .mutating()
            .user_only()
            .build();
        let agents = [
            Principal::Director {
                conversation_id: ConversationId::new(),
                read_untrusted: false,
            },
            Principal::Mcp,
            Principal::Sift,
            Principal::System,
        ];
        for agent in agents {
            assert!(matches!(
                decide(&secret, &agent, None, &p, true),
                Decision::Deny(_)
            ));
        }
        assert_eq!(
            decide(&secret, &Principal::User, None, &p, false),
            Decision::Proceed
        );
        assert_eq!(
            decide(&secret, &Principal::Cli, None, &p, false),
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
    fn a_dev_build_on_hold_denies_provider_calls_even_for_the_user_and_runs_corrections() {
        let p = AutonomyPolicy {
            dev: true,
            writes: ProviderWrites::Hold,
            ..Default::default()
        };
        let send = crate::meta("send_message", "send")
            .mutating()
            .external()
            .build();
        assert!(matches!(
            decide(&send, &Principal::User, None, &p, true),
            Decision::Deny(_)
        ));
        let correct = crate::meta("set_disposition", "correct")
            .mutating()
            .projected()
            .build();
        assert_eq!(
            decide(&correct, &Principal::User, None, &p, true),
            Decision::Proceed
        );
    }

    #[test]
    fn a_dev_build_on_ask_asks_before_a_provider_call_and_allow_lets_it_through() {
        let delete = crate::meta("unsubscribe", "unsubscribe")
            .mutating()
            .external()
            .build();
        let ask = AutonomyPolicy {
            dev: true,
            writes: ProviderWrites::Ask,
            ..Default::default()
        };
        assert!(matches!(
            decide(&delete, &Principal::User, None, &ask, false),
            Decision::Confirm(ConfirmationPolicy::Approval, _)
        ));
        assert_eq!(
            decide(&delete, &Principal::User, None, &ask, true),
            Decision::Proceed
        );
        let allow = AutonomyPolicy {
            dev: true,
            writes: ProviderWrites::Allow,
            ..Default::default()
        };
        assert_eq!(
            decide(&delete, &Principal::User, None, &allow, false),
            Decision::Proceed
        );
    }
}
