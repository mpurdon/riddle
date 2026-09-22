//! The system Signal vocabulary (§23, §28): Riddle's initial semantic
//! instruction set. Every definition is written for a literal reader, as one
//! positive property, without dates, counts or negations, and each pair of
//! neighbouring Signals is phrased to contrast (`receipt` is proof of a
//! completed payment; `invoice` presents an amount still due). Each version
//! here is v1; changing any text means a new version through the registry,
//! never an edit in place, because Meshes pin `name@vN`.

use riddle_core::{
    CreatedBy, Signal, SignalChoice, SignalId, SignalInputs, SignalPrimitive, SignalStatus,
};

struct Def {
    name: &'static str,
    definition: &'static str,
    yes: &'static str,
    no: &'static str,
    thread: bool,
    slices: &'static [&'static str],
}

const DEFS: &[Def] = &[
    Def { name: "requires_response", definition: "The sender expects the recipient to write back.", yes: "The sender is waiting for a written reply from the recipient.", no: "The message is informational; the sender expects silence.", thread: true, slices: &[] },
    Def { name: "requires_action", definition: "The sender expects the recipient to do something beyond reading the message.", yes: "The recipient is expected to act: approve, pay, fix, attend, decide or provide something.", no: "Reading the message is all that is expected of the recipient.", thread: true, slices: &[] },
    Def { name: "direct_request", definition: "The message asks the recipient, by name or by address, to do one specific thing.", yes: "A specific ask is aimed at the recipient personally.", no: "Any request is general, aimed at a group, or absent.", thread: false, slices: &[] },
    Def { name: "personally_relevant", definition: "The content concerns the recipient personally rather than a general audience.", yes: "The message is about the recipient, the recipient's work, projects, accounts or people.", no: "The message would read the same for anyone on the list.", thread: false, slices: &["user_profile"] },
    Def { name: "human_authored", definition: "A person wrote this message by hand for this occasion.", yes: "A human composed this text for this recipient or this conversation.", no: "A system, template or campaign produced the text.", thread: false, slices: &[] },
    Def { name: "routine", definition: "This message is a routine instance of a recurring kind of mail.", yes: "The message is a familiar, repeating kind of notification or report.", no: "The message stands out from the recurring pattern.", thread: false, slices: &["sender_history"] },
    Def { name: "novel", definition: "This message describes a situation that is new for this recipient.", yes: "The situation, sender or topic is new for the recipient.", no: "The situation is a familiar continuation of ongoing matters.", thread: false, slices: &["sender_history"] },
    Def { name: "marketing", definition: "The message promotes a product, service or offer to the recipient.", yes: "The purpose of the message is to sell or promote something.", no: "The message serves a purpose other than promotion.", thread: false, slices: &[] },
    Def { name: "newsletter", definition: "The message is a periodic editorial mailing sent to a list of subscribers.", yes: "The message is an issue of a regular publication sent to subscribers.", no: "The message is a one-off or a personal message.", thread: false, slices: &[] },
    Def { name: "transactional", definition: "The message reports a specific transaction or account event that already happened.", yes: "The message confirms an event such as an order, a login, a shipment or a change that occurred.", no: "The message is about something other than a completed event.", thread: false, slices: &[] },
    Def { name: "payment_related", definition: "The message concerns money the recipient owes, paid, or is asked to pay.", yes: "Money owed, paid or requested is the subject of the message.", no: "The message is about something other than money.", thread: false, slices: &[] },
    Def { name: "meeting_related", definition: "The message concerns scheduling, changing or holding a meeting or event.", yes: "The message is about a meeting, call, appointment or event and its time.", no: "The message is about something other than a meeting or event.", thread: false, slices: &[] },
    Def { name: "review_requested", definition: "Someone explicitly asks the recipient to review a change, document or submission.", yes: "The recipient is named or assigned as a reviewer.", no: "The message mentions a review only in passing, or asks someone else.", thread: false, slices: &[] },
    Def { name: "ci_failure", definition: "An automated build, test or check reports a failure.", yes: "A build, test run, lint or check is reported as failed or broken.", no: "Checks passed, or the message is about something other than automated checks.", thread: false, slices: &[] },
    Def { name: "deployment_failed", definition: "An automated system reports that a deployment or release failed.", yes: "A deployment, rollout or release is reported as failed or rolled back.", no: "The deployment succeeded, or the message is about something else.", thread: false, slices: &[] },
    Def { name: "owned_work_failed", definition: "Something the recipient owns or authored is reported as failing or broken.", yes: "The failing item belongs to the recipient: their pull request, service, job or project.", no: "The failing item belongs to someone else, or nothing is failing.", thread: false, slices: &["user_profile"] },
    Def { name: "asks_question", definition: "The message asks a question that expects an answer from the reader.", yes: "A question is posed to the reader and an answer is expected.", no: "Any question is rhetorical, or the message makes only statements.", thread: false, slices: &[] },
    Def { name: "makes_commitment", definition: "The author promises to do something.", yes: "The author commits to a future action of their own.", no: "The author makes no promise about their own future action.", thread: false, slices: &[] },
    Def { name: "requests_action", definition: "The author asks the reader to do something.", yes: "The author asks the reader to take an action.", no: "The author asks for nothing from the reader.", thread: false, slices: &[] },
    Def { name: "acknowledges_cancellation", definition: "The message confirms that something was cancelled.", yes: "A booking, order, subscription, meeting or service is confirmed as cancelled.", no: "Nothing is confirmed as cancelled.", thread: false, slices: &[] },
    Def { name: "represents_charge", definition: "The message states that the recipient was charged or will be charged an amount.", yes: "A specific charge to the recipient is stated.", no: "No charge to the recipient is stated.", thread: false, slices: &[] },
    Def { name: "security_alert", definition: "The message warns about the security of an account, device or credential.", yes: "The message reports a sign-in, password change, breach, suspicious activity or security setting.", no: "The message is about something other than account or device security.", thread: false, slices: &[] },
    Def { name: "account_notification", definition: "A service the recipient uses reports a change or event on the recipient's account.", yes: "A service reports an account-level event such as a plan change, a setting update or a policy notice.", no: "The message is from a person, or a promotion, or about something other than the account.", thread: false, slices: &[] },
    Def { name: "social_notification", definition: "A social platform reports activity such as likes, follows, mentions or comments.", yes: "A social network or community platform reports activity on the recipient's profile or posts.", no: "The message is about something other than social platform activity.", thread: false, slices: &[] },
    Def { name: "receipt", definition: "The message is proof of a completed purchase or payment.", yes: "The message confirms a payment that has already been made.", no: "The message asks for payment, or is about something other than a completed payment.", thread: false, slices: &[] },
    Def { name: "invoice", definition: "The message presents an amount that is due to be paid.", yes: "An amount is presented as owed, with a due date or a request to pay.", no: "The message confirms a completed payment, or is about something other than an amount due.", thread: false, slices: &[] },
    Def { name: "travel_reservation", definition: "The message confirms or changes a travel booking such as a flight, hotel, train or car.", yes: "A travel booking is confirmed, changed or ticketed.", no: "The message is about something other than a travel booking.", thread: false, slices: &[] },
];

/// Every system Signal at version 1, ACTIVE, created by the system. Ids are
/// stable (`sig_<name>`) so seeding is idempotent and references survive a
/// reinstall.
pub fn system_signals() -> Vec<Signal> {
    DEFS.iter().map(build).collect()
}

pub fn system_signal_names() -> Vec<&'static str> {
    DEFS.iter().map(|d| d.name).collect()
}

/// One system Signal by name, if it exists in the vocabulary.
pub fn system_signal(name: &str) -> Option<Signal> {
    DEFS.iter().find(|d| d.name == name).map(build)
}

fn build(d: &Def) -> Signal {
    Signal {
        id: SignalId(format!("sig_{}", d.name)),
        name: d.name.to_string(),
        version: 1,
        primitive: SignalPrimitive::Noul,
        definition: d.definition.to_string(),
        choices: vec![
            SignalChoice {
                name: "YES".into(),
                definition: d.yes.to_string(),
            },
            SignalChoice {
                name: "NO".into(),
                definition: d.no.to_string(),
            },
        ],
        inputs: SignalInputs {
            message: true,
            thread: d.thread,
            context_slices: d.slices.iter().map(|s| s.to_string()).collect(),
        },
        language_scope: vec!["en".into()],
        status: SignalStatus::Active,
        calibration: None,
        created_at: chrono::DateTime::from_timestamp(1_758_499_200, 0).unwrap_or_default(),
        created_by: CreatedBy::System,
        change_note: Some("system vocabulary v1".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_system_signal_passes_the_authoring_rules() {
        for s in system_signals() {
            if let Err(problems) = s.validate() {
                panic!("{} fails validation: {problems:?}", s.name);
            }
        }
    }

    #[test]
    fn the_vocabulary_covers_the_names_the_architecture_lists() {
        let names = system_signal_names();
        for required in [
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
        ] {
            assert!(names.contains(&required), "missing {required}");
        }
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "duplicate names");
    }
}
