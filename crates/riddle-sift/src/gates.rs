//! The gates that run before any Signal (§17, §58 control 8, README "Never
//! Hidden"). Both are deterministic, both are recorded in the trace, and
//! neither can be overridden by a Signal or a route.
//!
//! The Contamination rules follow the incidents in `ARCHITECTURE-REVIEW.md`
//! §5.3: a lookalike domain that fails authentication, a first-time sender
//! asking for money or credentials with urgency, and Gmail's own verdict.
//! Mail that argues for its own importance is evidence against it, so
//! urgency language raises suspicion rather than priority here.

use crate::SiftInput;
use riddle_core::Facts;

/// Why the sender can never be Spoil or Contamination, if so.
pub fn never_hidden_reason(facts: &Facts, input: &SiftInput) -> Option<String> {
    let sender = input.message.from.email.to_ascii_lowercase();
    if facts.from_me {
        return Some("the message is yours".into());
    }
    if facts.sender_in_never_hidden {
        return Some("sender is on the Never Hidden list".into());
    }
    if facts.sender_is_vip {
        return Some("sender is marked important".into());
    }
    if input.profile.is_family(&sender) {
        return Some("sender is family".into());
    }
    if facts.same_organisation {
        return Some("sender is at your organisation".into());
    }
    if facts.sender_replied_to_before {
        return Some("you have written to this sender".into());
    }
    if facts.sender_in_contacts {
        return Some("sender is in your contacts".into());
    }
    None
}

/// Deterministic Contamination evidence, if any. The judge never sees a
/// message that trips this.
pub fn contamination_evidence(facts: &Facts, people: &crate::PeopleFacts) -> Option<String> {
    if facts.gmail_spam {
        return Some("Gmail marked it as spam".into());
    }
    // "I have access to your account", sent as if from the user: their own
    // address on the From line of mail that failed authentication.
    if facts.spoofs_me {
        return Some("it claims to come from your own address and fails authentication".into());
    }
    if facts.lookalike_domain && !facts.dmarc_pass {
        return Some("the sender's domain imitates a known one and fails authentication".into());
    }
    if facts.lookalike_domain
        && (facts.asks_for_payment_or_credentials || facts.urgency_language_present)
    {
        return Some("the sender's domain imitates a known one and the message presses for payment, credentials or urgency".into());
    }
    if facts.first_time_sender
        && facts.asks_for_payment_or_credentials
        && facts.urgency_language_present
    {
        return Some("a first-time sender asks for payment or credentials with urgency".into());
    }
    if facts.first_time_sender && facts.reply_to_mismatch && facts.asks_for_payment_or_credentials {
        return Some(
            "a first-time sender with a mismatched Reply-To asks for payment or credentials".into(),
        );
    }
    if people.spam_verdicts >= 3 && facts.first_time_sender {
        return Some("the sender has repeated spam verdicts".into());
    }
    None
}

/// Whether a Find may be created for an unknown sender (§41): a
/// deterministic fact must support it, because senders can steer Signals.
pub fn find_supported(facts: &Facts) -> Result<(), String> {
    let known = facts.known()
        || facts.same_organisation
        || facts.sender_in_never_hidden
        || facts.sender_is_vip;
    if !facts.first_time_sender || known {
        return Ok(());
    }
    // At a free mailbox, authentication proves only that someone with an
    // account there sent it: a scam opener impersonating an investor from a
    // numbered Gmail address passed SPF, DKIM and DMARC.
    let authenticated =
        !facts.free_mail_sender && (facts.spf_pass || facts.dkim_pass || facts.dmarc_pass);
    if (facts.directly_addressed && authenticated)
        || facts.has_calendar_part
        || facts.has_structured_receipt
        || facts.gmail_important
        || facts.thread_length > 1
    {
        return Ok(());
    }
    Err("first-time sender with no deterministic support for a Find".into())
}
