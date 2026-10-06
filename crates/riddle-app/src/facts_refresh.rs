//! Stored facts refreshed for fields that arrived after the mail was sifted.
//! Facts are computed when a message is sifted and stored beside it; the
//! simulator and "apply to past mail" read the stored ones. `sender`,
//! `sender_name`, `subject`, `headers` and `github_reason` were added after
//! the first mailboxes were sifted, so their stored facts had them empty and
//! a rule on `subject CONTAINS …` or `header X = …` simulated as "nothing
//! matched" over the whole history. This walks every message with a facts
//! row once per Dig at boot and rewrites the message-derived part the way
//! Sift computes it, with no judge call. The flag key is the mechanism's
//! version: widening the refresh means a new key, so it runs once more.
//! v2 recomputed `language`, after link-heavy English sale mail had been
//! stored as Portuguese and held for escalation; v3 adds the sender's
//! rhythm from People (`sender_cadence`, `sender_on_schedule`), so every
//! reader of stored facts (the simulator, Why?, the Director) sees what
//! Sift would see now. It runs after the People cadence backfill. v4 adds
//! `government_sender`. v5 adds `from_me`, `is_draft`, `free_mail_sender`
//! and `glyph_substitution`, after the user's own drafts were read as mail
//! from a first-time sender and a cold scam opener had nothing but Gmail's
//! later spam verdict against it. v6 re-runs the narrowed
//! `glyph_substitution`, whose first version flagged the zero-width
//! characters retailers put inside words. v7 adds `is_reply`, after a
//! shop's answer to a website form was read as cold outreach and hidden.
//! v8 drops ISP mailboxes (`bell.net`, `rogers.com`) from `free_mail_sender`.
//! v9 adds `github_event` and `list_id`, after review requests turned out to
//! be mostly approvals and bot comments on threads a team was asked to review.
//! v10 adds `suspicious_domain`. v11 adds `verification_code`. v12 adds
//! `payment_failed` and `payment_recovered`, and recomputes
//! `lookalike_domain`, which had flagged amazonaws.com as imitating Amazon
//! and hidden genuine AWS past-due notices.

use crate::App;
use riddle_core::DigId;
use riddle_errors::Result;
use riddle_store::repo;

/// The `once_per_dig` key. Renamed when the set of refreshed fields widens.
const KEY: &str = "message_facts_v12";

impl App {
    /// Refresh the message-derived facts of every message in this Dig that
    /// has a stored facts row. Returns how many rows changed.
    pub fn refresh_message_facts(&self, dig_id: &DigId) -> Result<usize> {
        let store = self.stores.dig(dig_id)?;
        let index = self.stores.index();
        let dig = repo::digs::get(&index, dig_id)?;
        let mut profile = repo::user_profile::get(&index)?;
        if let Some(dig) = &dig {
            if !profile.is_me(&dig.email) {
                profile.addresses.push(dig.email.clone());
            }
        }
        let mut changed = 0;
        for id in repo::facts::ids(&store)? {
            let Some(mut facts) = repo::facts::get(&store, &id)? else {
                continue;
            };
            let Some(message) = repo::messages::get(&store, dig_id, &id)? else {
                continue;
            };
            // Detecting a language means rendering the body; only a stored
            // language the old detector could have got wrong is worth it.
            let language = match facts.language.as_str() {
                "en" | "und" => facts.language.clone(),
                _ => riddle_mail::normalize::normalize_message(&message).language,
            };
            let person = riddle_people::lookup(&index, &message.from.email)?;
            let dirty = riddle_sift::facts::refresh_from_message(
                &mut facts,
                &message,
                riddle_sift::facts::Derived {
                    language: &language,
                    cadence: person.as_ref().and_then(|p| p.cadence),
                    on_schedule: person.as_ref().is_some_and(|p| p.on_schedule),
                    me: profile.is_me(&message.from.email),
                    organisation_domains: &profile.organisation_domains,
                },
            );
            if dirty {
                repo::facts::put(&store, &id, &facts)?;
                changed += 1;
            }
        }
        Ok(changed)
    }

    /// Every Dig, once (see `once_per_dig`).
    pub fn refresh_all_message_facts(&self) {
        self.once_per_dig(KEY, |dig_id| self.refresh_message_facts(dig_id));
    }
}
