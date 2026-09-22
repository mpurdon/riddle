//! The Activity sentences (README "Activity & Provenance"): "Sift moved 14
//! messages to Spoil", "You changed "AWS Billing Alert" from Spoil to Find".
//! Pure functions so every producer (the runtime, Sift, the Director loop,
//! the undo engine) reads the same way and the wording is tested once.
//! Activity is a product surface, so a message subject may appear here; it
//! never appears in a log line.

use riddle_core::{Actor, Disposition, FindType};

/// The subject of the sentence.
pub fn actor_name(actor: Actor) -> &'static str {
    match actor {
        Actor::User => "You",
        Actor::Director => "Director",
        Actor::Sift => "Sift",
        Actor::Mesh => "Mesh",
        Actor::Automation => "Automation",
        Actor::System => "Riddle",
    }
}

pub fn disposition_name(d: &Disposition) -> String {
    match d {
        Disposition::Find { find_type } => format!("Find ({})", find_type_name(find_type)),
        Disposition::Filed => "Filed".into(),
        Disposition::Spoil => "Spoil".into(),
        Disposition::Contamination => "Contamination".into(),
        Disposition::Unresolved => "Unresolved".into(),
    }
}

pub fn find_type_name(t: &FindType) -> String {
    let raw = format!("{t:?}");
    let mut out = String::new();
    for (i, ch) in raw.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

/// What a sentence is about.
#[derive(Debug, Clone)]
pub struct Sentence<'a> {
    pub actor: Actor,
    pub action: &'a str,
    pub count: usize,
    /// Display names for the affected entities (subjects, Find titles, Mesh names).
    pub names: &'a [&'a str],
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
}

impl<'a> Sentence<'a> {
    pub fn new(actor: Actor, action: &'a str) -> Self {
        Self {
            actor,
            action,
            count: 0,
            names: &[],
            from: None,
            to: None,
        }
    }
    pub fn count(mut self, n: usize) -> Self {
        self.count = n;
        self
    }
    pub fn names(mut self, names: &'a [&'a str]) -> Self {
        self.names = names;
        self
    }
    pub fn from(mut self, f: &'a str) -> Self {
        self.from = Some(f);
        self
    }
    pub fn to(mut self, t: &'a str) -> Self {
        self.to = Some(t);
        self
    }
    pub fn render(&self) -> String {
        sentence(self)
    }
}

fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else if noun == "Mesh" {
        format!("{n} Meshes")
    } else {
        format!("{n} {noun}s")
    }
}

/// `"name"` when there is exactly one named entity, otherwise a count.
fn object(s: &Sentence, noun: &str) -> String {
    let count = s.count.max(s.names.len());
    match (count, s.names) {
        (1, [name]) => format!("\"{name}\""),
        (0, _) => noun.to_string(),
        (n, _) => plural(n, noun),
    }
}

fn named(s: &Sentence, noun: &str) -> String {
    match s.names {
        [name] => format!("{noun} \"{name}\""),
        [] => noun.to_string(),
        many => plural(many.len(), noun),
    }
}

fn suffix(s: &Sentence, from_word: &str, to_word: &str) -> String {
    match (s.from, s.to) {
        (Some(f), Some(t)) => format!(" {from_word} {f} {to_word} {t}"),
        (None, Some(t)) => format!(" {to_word} {t}"),
        (Some(f), None) => format!(" {from_word} {f}"),
        (None, None) => String::new(),
    }
}

pub fn sentence(s: &Sentence) -> String {
    let who = actor_name(s.actor);
    let rest = match s.action {
        "archive_messages" => format!("archived {}", object(s, "message")),
        "move_to_spoil" => format!("moved {} to Spoil", object(s, "message")),
        "mark_contamination" => format!("marked {} as Contamination", object(s, "message")),
        "restore_messages" => format!("restored {}", object(s, "message")),
        "set_disposition" => format!(
            "changed {}{}",
            object(s, "message"),
            suffix(s, "from", "to")
        ),
        "label_messages" => format!(
            "labelled {}{}",
            object(s, "message"),
            suffix(s, "from", "with")
        ),
        "snooze_messages" => format!(
            "snoozed {}{}",
            object(s, "message"),
            suffix(s, "from", "until")
        ),
        "resolve_find" => format!("resolved {}", object(s, "Find")),
        "dismiss_find" => format!("dismissed {}", object(s, "Find")),
        "snooze_find" => format!(
            "snoozed {}{}",
            object(s, "Find"),
            suffix(s, "from", "until")
        ),
        "retype_find" => format!("retyped {}{}", object(s, "Find"), suffix(s, "from", "to")),
        "undo_activity" => format!(
            "undid {}",
            s.names
                .first()
                .map(|n| format!("\"{n}\""))
                .unwrap_or_else(|| "an action".into())
        ),
        "create_mesh" => format!("created {}", named(s, "Mesh")),
        "update_mesh" => format!("updated {}{}", named(s, "Mesh"), suffix(s, "from", "to")),
        "set_mesh_status" => format!("set {}{}", named(s, "Mesh"), suffix(s, "from", "to")),
        "delete_mesh" => format!("deleted {}", named(s, "Mesh")),
        "restore_mesh" => format!("restored {}", named(s, "Mesh")),
        "propose_signal" => format!("proposed {}", named(s, "Signal")),
        "promote_signal" => format!("promoted {}{}", named(s, "Signal"), suffix(s, "from", "to")),
        "add_never_hidden" => format!("added {} to Never Hidden", object(s, "entry")),
        "remove_never_hidden" => format!("removed {} from Never Hidden", object(s, "entry")),
        "save_settings" => "changed settings".to_string(),
        "save_user_profile" => "updated the profile".to_string(),
        "send_message" => format!("sent {}", object(s, "message")),
        "create_draft" => format!("created {}", object(s, "draft")),
        "start_expedition" => format!("started {}", named(s, "Expedition")),
        "cancel_expedition" => format!("cancelled {}", named(s, "Expedition")),
        "cancel_job" => format!("cancelled {}", named(s, "job")),
        "pause_job" => format!("paused {}", named(s, "job")),
        "resume_job" => format!("resumed {}", named(s, "job")),
        "arm_dev" => "armed the dev build".to_string(),
        other => {
            let verb = other.replace('_', " ");
            if s.names.is_empty() && s.count == 0 {
                verb
            } else {
                format!("{verb} {}", object(s, "item"))
            }
        }
    };
    format!("{who} {rest}")
}

/// "You undid: Sift moved 14 messages to Spoil".
pub fn undo_sentence(actor: Actor, original_summary: &str) -> String {
    format!("{} undid: {original_summary}", actor_name(actor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bulk_sift_move_reads_like_the_readme() {
        assert_eq!(
            Sentence::new(Actor::Sift, "move_to_spoil")
                .count(14)
                .render(),
            "Sift moved 14 messages to Spoil"
        );
        assert_eq!(
            Sentence::new(Actor::Director, "move_to_spoil")
                .count(127)
                .render(),
            "Director moved 127 messages to Spoil"
        );
    }

    #[test]
    fn a_single_named_correction_reads_like_the_readme() {
        let names = ["AWS Billing Alert"];
        let s = Sentence::new(Actor::User, "set_disposition")
            .count(1)
            .names(&names)
            .from("Spoil")
            .to("Find")
            .render();
        assert_eq!(s, "You changed \"AWS Billing Alert\" from Spoil to Find");
    }

    #[test]
    fn one_message_without_a_name_is_counted() {
        assert_eq!(
            Sentence::new(Actor::User, "archive_messages")
                .count(1)
                .render(),
            "You archived 1 message"
        );
        assert_eq!(
            Sentence::new(Actor::User, "archive_messages")
                .count(3)
                .render(),
            "You archived 3 messages"
        );
    }

    #[test]
    fn mesh_and_find_sentences_name_the_object() {
        let names = ["AWS Notifications"];
        assert_eq!(
            Sentence::new(Actor::User, "update_mesh")
                .names(&names)
                .to("v8")
                .render(),
            "You updated Mesh \"AWS Notifications\" to v8"
        );
        assert_eq!(
            Sentence::new(Actor::Director, "create_mesh")
                .names(&names)
                .render(),
            "Director created Mesh \"AWS Notifications\""
        );
        let finds = ["Renew the domain"];
        assert_eq!(
            Sentence::new(Actor::User, "resolve_find")
                .count(1)
                .names(&finds)
                .render(),
            "You resolved \"Renew the domain\""
        );
        assert_eq!(
            Sentence::new(Actor::System, "resolve_find")
                .count(2)
                .render(),
            "Riddle resolved 2 Finds"
        );
        assert_eq!(
            Sentence::new(Actor::User, "delete_mesh")
                .count(2)
                .names(&["a", "b"])
                .render(),
            "You deleted 2 Meshes"
        );
    }

    #[test]
    fn unknown_actions_fall_back_to_their_name() {
        assert_eq!(
            Sentence::new(Actor::User, "rename_stratum")
                .count(1)
                .render(),
            "You rename stratum 1 item"
        );
        assert_eq!(
            Sentence::new(Actor::User, "refresh_now").render(),
            "You refresh now"
        );
    }

    #[test]
    fn undo_and_disposition_helpers() {
        assert_eq!(
            undo_sentence(Actor::User, "Sift moved 14 messages to Spoil"),
            "You undid: Sift moved 14 messages to Spoil"
        );
        assert_eq!(disposition_name(&Disposition::Spoil), "Spoil");
        let names = ["Sift moved 14 messages to Spoil"];
        assert_eq!(
            Sentence::new(Actor::User, "undo_activity")
                .names(&names)
                .render(),
            "You undid \"Sift moved 14 messages to Spoil\""
        );
    }
}
