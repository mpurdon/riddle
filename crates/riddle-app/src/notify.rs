//! Which Finds interrupt, and what the brief says (Flow 12 of the UX
//! study). This is policy, so it lives here rather than in the desktop
//! shell, which only shows what it is handed.
//!
//! In `Brief` mode only an allow-list interrupts at once: a Find from a
//! Never Hidden sender, or a Find of a type the user chose (Pay, Respond by
//! default) due within the configured hours. Everything else waits for the
//! brief at the chosen times: "7 need you, 5 Unresolved, 41 filed."
//! `Stream` mode is the older behaviour, one notification per Find at or
//! above the priority threshold. A notification carries the Find's type and
//! reason, never its subject, so a locked screen shows nothing private.

use crate::App;
use riddle_core::DispositionKind;
use riddle_core::Find;
use riddle_errors::Result;
use riddle_settings::{NotificationMode, NotificationSettings, Settings};
use riddle_store::repo;

/// Decisions waiting on the user: open Finds plus Unresolved mail. The dock
/// badge counts this, never messages.
pub fn decisions_waiting(app: &App) -> Result<u64> {
    let index = app.stores.index();
    let mut n = repo::finds::count_open(&index, None)?;
    for dig in repo::digs::list_enabled(&index)? {
        let store = app.stores.dig(&dig.id)?;
        n += repo::classifications::counts(&store, None, false)?
            .get(&DispositionKind::Unresolved)
            .copied()
            .unwrap_or(0);
    }
    Ok(n)
}

/// The brief to say now, if one of the configured times is this minute and
/// it has not been said today. Bookkeeping lives in the index store's kv
/// table so a restart in the same minute does not say it twice.
pub fn brief_due(
    app: &App,
    settings: &Settings,
    now: chrono::DateTime<chrono::Local>,
) -> Result<Option<String>> {
    if settings.notifications.mode != NotificationMode::Brief {
        return Ok(None);
    }
    let hhmm = now.format("%H:%M").to_string();
    let due = settings.notifications.brief_times.iter().any(|t| {
        NotificationSettings::parse_time(t).is_some_and(|(h, m)| format!("{h:02}:{m:02}") == hhmm)
    });
    if !due {
        return Ok(None);
    }
    let today = now.format("%Y-%m-%d").to_string();
    let key = format!("brief_sent:{hhmm}");
    let index = app.stores.index();
    if repo::kv::get(&index, &key)?.as_deref() == Some(today.as_str()) {
        return Ok(None);
    }
    repo::kv::set(&index, &key, &today)?;
    let s = app.summary(chrono::Utc::now() - chrono::Duration::hours(24), None)?;
    Ok(brief_line(s.finds_open, s.unresolved, s.filed, s.spoil))
}

/// What the brief says, or nothing when there is nothing to say.
pub fn brief_line(finds_open: u64, unresolved: u64, filed: u64, spoil: u64) -> Option<String> {
    if finds_open == 0 && unresolved == 0 && filed == 0 && spoil == 0 {
        return None;
    }
    let mut parts = Vec::new();
    if finds_open > 0 {
        parts.push(format!("{finds_open} need you"));
    }
    if unresolved > 0 {
        parts.push(format!("{unresolved} Unresolved"));
    }
    if filed > 0 {
        parts.push(format!("{filed} filed"));
    }
    if spoil > 0 {
        parts.push(format!("{spoil} to Spoil"));
    }
    Some(format!("{}.", parts.join(", ")))
}

/// Whether a new Find interrupts now under the policy, given whether its
/// sender is Never Hidden. Pure, so the rule can be tested without a store.
pub fn interrupts_now(
    settings: &Settings,
    find: &Find,
    never_hidden_sender: bool,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    // Only an open Find asks for anything; one that settled closed as it
    // was made (its series or its expiry retired it) interrupts nobody.
    if find.status != riddle_core::FindStatus::Open {
        return false;
    }
    // A rule that said `+ Notify` asked for this one at once, whatever the mode.
    if find.notify {
        return true;
    }
    let policy = &settings.notifications;
    match policy.mode {
        NotificationMode::Stream => u32::from(find.priority) >= settings.notify_priority_threshold,
        NotificationMode::Brief => {
            let due_soon = find.due_at.is_some_and(|due| {
                due - now <= chrono::Duration::hours(policy.due_within_hours as i64)
            });
            let listed_type = policy.immediate_types.contains(&find.find_type);
            (listed_type && due_soon) || (policy.never_hidden_immediate && never_hidden_sender)
        }
    }
}

/// The notification a new Find earns, if any: the title names the type and
/// when it is due, the body is the reason. Never the subject.
pub fn notification_for(app: &App, settings: &Settings, find: &Find) -> Option<(String, String)> {
    let never_hidden = settings.notifications.mode == NotificationMode::Brief
        && settings.notifications.never_hidden_immediate
        && from_never_hidden(app, find);
    if !interrupts_now(settings, find, never_hidden, chrono::Utc::now()) {
        return None;
    }
    Some(notification_text(find))
}

/// Title and body for a Find: "Pay: due Fri 17:00" / the reason. A sign-in
/// code is the title itself ("Sign-in code 002534"): the point of telling
/// the user at once is that they can type it without opening anything.
pub fn notification_text(find: &Find) -> (String, String) {
    let title = match (&find.code, find.due_at) {
        (Some(code), _) => format!("Sign-in code {code}"),
        (None, due) => match due {
            Some(due) => format!(
                "{}: due {}",
                find.find_type.label(),
                due.with_timezone(&chrono::Local).format("%a %H:%M")
            ),
            None => format!("{}: needs you", find.find_type.label()),
        },
    };
    let body = if find.reason.trim().is_empty() {
        "Open Riddle to see why.".to_string()
    } else {
        find.reason.clone()
    };
    (title, body)
}

/// Whether the Find's first source was sent by a Never Hidden address or
/// domain, by the same matcher the Never Hidden gate uses. Any failure to
/// look it up reads as "no": a notification is never worth failing over.
fn from_never_hidden(app: &App, find: &Find) -> bool {
    let Some(source) = find.sources.first() else {
        return false;
    };
    let Ok(profile) = repo::user_profile::get(&app.stores.index()) else {
        return false;
    };
    if profile.never_hidden.is_empty() {
        return false;
    }
    match app.get_message(&source.dig_id, &source.message_id) {
        Ok(message) => profile.is_never_hidden(&message.from.email),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use riddle_core::*;
    use riddle_settings::NotificationSettings;

    fn find(t: FindType, priority: u8, due_in_hours: Option<i64>) -> Find {
        Find {
            code: None,
            id: FindId::new(),
            find_type: t,
            title: "subject".into(),
            summary: "summary".into(),
            priority,
            status: FindStatus::Open,
            due_at: due_in_hours.map(|h| chrono::Utc::now() + chrono::Duration::hours(h)),
            snoozed_until: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            resolved_at: None,
            resolved_by: None,
            sources: vec![],
            dig_ids: vec![],
            identity_key: "k".into(),
            reason: "Jamie asked twice.".into(),
            confidence: None,
            classification_id: None,
            trace_id: None,
            superseded_by: None,
            series: None,
            expires_at: None,
            kept_open_at: None,
            notify: false,
        }
    }

    #[test]
    fn the_brief_says_only_what_there_is_to_say() {
        assert_eq!(
            brief_line(7, 5, 41, 12).as_deref(),
            Some("7 need you, 5 Unresolved, 41 filed, 12 to Spoil.")
        );
        assert_eq!(brief_line(0, 2, 0, 0).as_deref(), Some("2 Unresolved."));
        assert!(brief_line(0, 0, 0, 0).is_none());
    }

    #[test]
    fn brief_mode_interrupts_only_for_the_allow_list_and_stream_mode_by_priority() {
        let mut s = Settings::prod_defaults();
        s.notifications = NotificationSettings::default();
        let now = chrono::Utc::now();
        assert!(interrupts_now(
            &s,
            &find(FindType::Pay, 90, Some(6)),
            false,
            now
        ));
        assert!(
            !interrupts_now(&s, &find(FindType::Pay, 90, Some(72)), false, now),
            "due later waits for the brief"
        );
        assert!(
            !interrupts_now(&s, &find(FindType::Review, 95, Some(1)), false, now),
            "a type not on the list waits"
        );
        assert!(
            interrupts_now(&s, &find(FindType::Know, 10, None), true, now),
            "Never Hidden always interrupts"
        );
        s.notifications.mode = NotificationMode::Stream;
        s.notify_priority_threshold = 70;
        assert!(interrupts_now(
            &s,
            &find(FindType::Know, 70, None),
            false,
            now
        ));
        assert!(!interrupts_now(
            &s,
            &find(FindType::Pay, 69, Some(1)),
            false,
            now
        ));
    }

    #[test]
    fn a_find_that_settled_closed_as_it_was_made_interrupts_nobody() {
        let mut s = Settings::prod_defaults();
        s.notifications = NotificationSettings::default();
        let now = chrono::Utc::now();
        for status in [FindStatus::Superseded, FindStatus::Expired] {
            let f = Find {
                status,
                ..find(FindType::Pay, 90, Some(1))
            };
            assert!(!interrupts_now(&s, &f, true, now), "{status:?}");
        }
    }

    #[test]
    fn a_find_from_a_notify_route_interrupts_in_brief_mode_too() {
        let mut s = Settings::prod_defaults();
        s.notifications = NotificationSettings::default();
        let quiet = find(FindType::Do, 50, None);
        assert!(!interrupts_now(&s, &quiet, false, chrono::Utc::now()));
        let urgent = Find {
            notify: true,
            ..quiet
        };
        assert!(interrupts_now(&s, &urgent, false, chrono::Utc::now()));
    }

    #[test]
    fn the_notification_carries_the_type_and_reason_never_the_subject() {
        let (title, body) = notification_text(&find(FindType::Respond, 80, None));
        assert_eq!(title, "Respond: needs you");
        assert_eq!(body, "Jamie asked twice.");
        assert!(!title.contains("subject") && !body.contains("subject"));
    }

    #[test]
    fn a_sign_in_code_is_the_notification_title() {
        let f = Find {
            code: Some("002534".into()),
            ..find(FindType::Know, 95, None)
        };
        assert_eq!(notification_text(&f).0, "Sign-in code 002534");
    }
}
