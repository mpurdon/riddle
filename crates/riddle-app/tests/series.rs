//! `+ Replaces`: a daily reminder is one Find, not a week of them. The Find
//! from the newest message of a series (one Mesh, one sender) stays open,
//! the older one closes as Superseded, the ledger says so, and undo from
//! Activity keeps it open for good. `+ Replaces: same subject` keys the
//! series by the subject too, one per expiring domain. `+ Expires:` lets a
//! Find nobody acted on lapse, counted from the message's date, with the
//! same ledger and undo.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

const SENDER: &str = "coach@jots.example";

fn reminder(dig: &DigId, pid: &str, hours_ago: i64) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    // Milliseconds, as the store keeps them.
    m.date = chrono::SubsecRound::trunc_subsecs(chrono::Utc::now(), 3)
        - chrono::Duration::hours(hours_ago);
    m.subject = "Time to write, Matthew!".into();
    m.from = Address::new(SENDER, None);
    m.to = vec![Address::new("me@example.com", None)];
    m.headers.message_id = Some(format!("<{pid}@jots.example>"));
    // Addressed to me and signed: enough deterministic support for a Find
    // from a sender Riddle has not seen before (§58).
    m.headers.authentication.dkim = AuthVerdict::Pass;
    m
}

fn activity_named(app: &riddle_app::App, action: &str) -> Option<Activity> {
    repo::activities::list(
        &app.stores.index(),
        &Default::default(),
        &Default::default(),
    )
    .unwrap()
    .items
    .into_iter()
    .find(|a| a.action == action)
}

fn series_finds(app: &riddle_app::App, status: FindStatus) -> Vec<Find> {
    repo::finds::list_by_status(&app.stores.index(), status, Some(FindType::Do))
        .unwrap()
        .into_iter()
        .filter(|f| f.series.is_some())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_newer_daily_reminder_replaces_the_open_one_and_undo_keeps_it_open() {
    let (app, dig, mesh_id) = with_mesh(
        "Mesh: Daily reminders\nPriority: 0\nROUTES\nIF subject CONTAINS \"Time to write\" → Find: Do + Replaces",
    )
    .await;
    let index = app.stores.index();
    let store = app.stores.dig(&dig.id).unwrap();

    repo::messages::upsert(&store, &reminder(&dig.id, "day1", 24)).unwrap();
    app.sift_pass(&dig, 100, None).await.unwrap();
    let first = series_finds(&app, FindStatus::Open);
    assert_eq!(first.len(), 1, "{first:?}");

    repo::messages::upsert(&store, &reminder(&dig.id, "day2", 0)).unwrap();
    let report = app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(report.finds_closed, 1);
    let open = series_finds(&app, FindStatus::Open);
    assert_eq!(open.len(), 1);
    assert_ne!(open[0].id, first[0].id);
    let old = repo::finds::get(&index, &first[0].id).unwrap().unwrap();
    assert_eq!(old.status, FindStatus::Superseded);
    assert_eq!(old.superseded_by, Some(open[0].id.clone()));

    let activity = activity_named(&app, "close_finds").expect("the replacement is in Activity");
    assert_eq!(
        activity.provenance.as_ref().and_then(|p| p.mesh_id.clone()),
        Some(mesh_id)
    );
    assert!(matches!(activity.undo, UndoInfo::Undoable { .. }));

    app.runtime
        .invoke_as_user("undo_activity", json!({ "activityId": activity.id }))
        .await
        .unwrap();
    assert_eq!(
        repo::finds::get(&index, &first[0].id)
            .unwrap()
            .unwrap()
            .status,
        FindStatus::Open
    );

    // Undo kept it: tomorrow's reminder replaces today's, not the kept one.
    repo::messages::upsert(&store, &reminder(&dig.id, "day3", 0)).unwrap();
    app.sift_pass(&dig, 100, None).await.unwrap();
    let kept = repo::finds::get(&index, &first[0].id).unwrap().unwrap();
    assert_eq!(kept.status, FindStatus::Open);
    assert!(kept.kept_open_at.is_some());
    assert_eq!(
        repo::finds::get(&index, &open[0].id)
            .unwrap()
            .unwrap()
            .status,
        FindStatus::Superseded
    );
}

async fn with_mesh(text: &str) -> (std::sync::Arc<riddle_app::App>, Dig, MeshId) {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@example.com", 5, 5, json!({})).await;
    let mesh = common::install_mesh(&app, text, MeshStatus::Active);
    (app, dig, mesh.id)
}

fn find_for(app: &riddle_app::App, dig: &Dig, pid: &str) -> Find {
    repo::finds::for_message(&app.stores.index(), &MessageId::for_provider(&dig.id, pid))
        .unwrap()
        .remove(0)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_expiring_reminder_lapses_on_the_temporal_pass_and_undo_keeps_it_open() {
    let (app, dig, _) = with_mesh(
        "Mesh: Daily reminders\nPriority: 0\nROUTES\nIF subject CONTAINS \"Time to write\" → Find: Do + Expires: 18h",
    )
    .await;
    let index = app.stores.index();
    let store = app.stores.dig(&dig.id).unwrap();
    let message = reminder(&dig.id, "today", 1);
    repo::messages::upsert(&store, &message).unwrap();
    app.sift_pass(&dig, 100, None).await.unwrap();
    let mut find = find_for(&app, &dig, "today");
    assert_eq!(find.status, FindStatus::Open);
    assert_eq!(
        find.expires_at,
        Some(message.date + chrono::Duration::hours(18)),
        "counted from the message, not from when Sift saw it"
    );

    // Eighteen hours on.
    find.expires_at = Some(chrono::Utc::now() - chrono::Duration::minutes(1));
    repo::finds::upsert(&index, &find).unwrap();
    let summaries = app.temporal_pass().unwrap();
    assert!(
        summaries.iter().any(|s| s.starts_with("Do expired")),
        "{summaries:?}"
    );
    assert_eq!(find_for(&app, &dig, "today").status, FindStatus::Expired);

    let activity = activity_named(&app, "close_finds").expect("the expiry is in Activity");
    app.runtime
        .invoke_as_user("undo_activity", json!({ "activityId": activity.id }))
        .await
        .unwrap();
    assert_eq!(find_for(&app, &dig, "today").status, FindStatus::Open);

    // Undo kept it: the next Temporal pass leaves it open though its
    // expiry has passed.
    app.temporal_pass().unwrap();
    assert_eq!(find_for(&app, &dig, "today").status, FindStatus::Open);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reminder_sifted_after_its_expiry_never_shows_as_open() {
    let (app, dig, _) = with_mesh(
        "Mesh: Daily reminders\nPriority: 0\nROUTES\nIF subject CONTAINS \"Time to write\" → Find: Do + Expires: 18h",
    )
    .await;
    let store = app.stores.dig(&dig.id).unwrap();
    repo::messages::upsert(&store, &reminder(&dig.id, "stale", 20)).unwrap();
    app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(find_for(&app, &dig, "stale").status, FindStatus::Expired);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn end_of_day_expiry_is_midnight_in_the_recorded_timezone() {
    let (app, dig, _) = with_mesh(
        "Mesh: Daily reminders\nPriority: 0\nROUTES\nIF subject CONTAINS \"Time to write\" → Find: Do + Expires: end of day",
    )
    .await;
    let index = app.stores.index();
    let mut profile = repo::user_profile::get(&index).unwrap();
    profile.timezone = Some("America/Toronto".into());
    repo::user_profile::put(&index, &profile).unwrap();
    let store = app.stores.dig(&dig.id).unwrap();
    let message = reminder(&dig.id, "tz", 1);
    repo::messages::upsert(&store, &message).unwrap();
    app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(
        find_for(&app, &dig, "tz").expires_at,
        riddle_sift::expiry::expires_at(
            Expiry::EndOfDay,
            message.date,
            None,
            Some("America/Toronto")
        )
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_rule_on_cadence_finds_the_daily_sender_without_naming_it() {
    let (app, dig, _) = with_mesh(
        "Mesh: Daily reminders\nPriority: 0\nROUTES\nIF sender_cadence = daily AND sender_on_schedule → Find: Do + Replaces + Expires: next morning",
    )
    .await;
    let index = app.stores.index();
    let store = app.stores.dig(&dig.id).unwrap();
    // A week of one reminder a day, then People learns the rhythm.
    for d in (0..7).rev() {
        repo::messages::upsert(&store, &reminder(&dig.id, &format!("d{d}"), d * 24 + 1)).unwrap();
    }
    let profile = repo::user_profile::get(&index).unwrap();
    riddle_people::refresh_address(
        &index,
        &app.stores.open_digs(),
        SENDER,
        &profile,
        chrono::Utc::now(),
    )
    .unwrap();
    assert_eq!(
        riddle_people::lookup(&index, SENDER)
            .unwrap()
            .unwrap()
            .cadence,
        Some(Cadence::Daily)
    );

    app.sift_pass(&dig, 100, None).await.unwrap();
    let open = series_finds(&app, FindStatus::Open);
    assert_eq!(open.len(), 1, "today's reminder, the week before replaced");
    let facts = repo::facts::get(&store, &MessageId::for_provider(&dig.id, "d0"))
        .unwrap()
        .unwrap();
    assert_eq!(facts.sender_cadence, "daily");
    assert!(facts.sender_on_schedule, "one a day, at the same hour");
}

fn sale(dig: &DigId, pid: &str, hours_ago: i64) -> Message {
    let mut m = reminder(dig, pid, hours_ago);
    m.from = Address::new("sales@store.example", None);
    m.subject = format!("Surplus sale {pid}");
    m
}

fn spoil_list(app: &riddle_app::App, dig: &Dig, superseded: Option<bool>) -> Vec<MessageSummary> {
    let store = app.stores.dig(&dig.id).unwrap();
    repo::messages::list(
        &store,
        &dig.id,
        &repo::messages::MessageFilter {
            disposition: Some("spoil".into()),
            from: Some("sales@store.example".into()),
            superseded,
            ..Default::default()
        },
        &PageRequest {
            cursor: None,
            limit: Some(50),
        },
    )
    .unwrap()
    .items
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn only_the_newest_sale_stands_and_undo_keeps_an_older_one() {
    let (app, dig, _) = with_mesh(
        "Mesh: Store sales\nPriority: 0\nROUTES\nIF sender = \"sales@store.example\" → Spoil + Replaces",
    )
    .await;
    let store = app.stores.dig(&dig.id).unwrap();
    for (pid, hours) in [("s1", 150), ("s2", 90), ("s3", 30)] {
        repo::messages::upsert(&store, &sale(&dig.id, pid, hours)).unwrap();
    }
    let report = app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(report.messages_superseded, 2);

    let standing = spoil_list(&app, &dig, Some(false));
    assert_eq!(standing.len(), 1, "the newest sale");
    assert_eq!(standing[0].id, MessageId::for_provider(&dig.id, "s3"));
    let moot = spoil_list(&app, &dig, Some(true));
    assert_eq!(moot.len(), 2);
    assert!(moot
        .iter()
        .all(|m| m.superseded_by == Some(standing[0].id.clone())));
    assert_eq!(
        spoil_list(&app, &dig, None).len(),
        3,
        "still there, still Spoil"
    );

    let activity =
        activity_named(&app, "supersede_messages").expect("the superseding is in Activity");
    app.runtime
        .invoke_as_user("undo_activity", json!({ "activityId": activity.id }))
        .await
        .unwrap();
    assert_eq!(
        spoil_list(&app, &dig, Some(false)).len(),
        3,
        "undo keeps them"
    );

    // The next sale supersedes the newest standing one, not the kept ones.
    repo::messages::upsert(&store, &sale(&dig.id, "s4", 1)).unwrap();
    app.sift_pass(&dig, 100, None).await.unwrap();
    let moot = spoil_list(&app, &dig, Some(true));
    assert_eq!(moot.len(), 1);
    assert_eq!(moot[0].id, MessageId::for_provider(&dig.id, "s3"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn applying_a_replaces_rule_to_sales_already_in_spoil_supersedes_the_older_ones() {
    // Spoil already, by a rule that knew nothing of series.
    let (app, dig, _) = with_mesh(
        "Mesh: Old sales\nPriority: 1\nROUTES\nIF sender = \"sales@store.example\" → Spoil",
    )
    .await;
    let store = app.stores.dig(&dig.id).unwrap();
    for (pid, hours) in [("s1", 150), ("s2", 90), ("s3", 30)] {
        repo::messages::upsert(&store, &sale(&dig.id, pid, hours)).unwrap();
    }
    app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(spoil_list(&app, &dig, Some(false)).len(), 3);

    let mesh = common::install_mesh(
        &app,
        "Mesh: Store sales\nPriority: 0\nROUTES\nIF sender = \"sales@store.example\" → Spoil + Replaces",
        MeshStatus::Active,
    );
    let out = app
        .runtime
        .invoke_as_user(
            "apply_mesh_to_history",
            json!({ "meshId": mesh.id, "windowDays": 30 }),
        )
        .await
        .unwrap();
    // The three sales, plus any fixture mail still being sifted under load.
    assert!(
        out.result["count"].as_u64().unwrap() >= 3,
        "no disposition changes, but all three join the series: {}",
        out.result
    );
    app.sift_pass(&dig, 100, None).await.unwrap();
    let standing = spoil_list(&app, &dig, Some(false));
    assert_eq!(standing.len(), 1);
    assert_eq!(standing[0].id, MessageId::for_provider(&dig.id, "s3"));
    assert_eq!(spoil_list(&app, &dig, Some(true)).len(), 2);
}

/// A registrar's reminders run one per domain: the newest about a domain
/// replaces the earlier ones about it and leaves another domain's alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_subject_series_keeps_one_find_per_domain() {
    let (app, dig, _) = with_mesh(
        "Mesh: Renewals\nPriority: 0\nROUTES\nIF subject CONTAINS \"expires in\" → Find: Do + Replaces: same subject",
    )
    .await;
    let store = app.stores.dig(&dig.id).unwrap();
    let notice = |pid: &str, subject: &str, hours_ago: i64| {
        let mut m = reminder(&dig.id, pid, hours_ago);
        m.subject = subject.into();
        m
    };
    repo::messages::upsert(
        &store,
        &notice("a28", "nasghoul.com Expires in 28 days", 48),
    )
    .unwrap();
    repo::messages::upsert(&store, &notice("b28", "iprm.net Expires in 28 days", 24)).unwrap();
    app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(series_finds(&app, FindStatus::Open).len(), 2);

    repo::messages::upsert(
        &store,
        &notice("a14", "Re: nasghoul.com Expires in 14 days", 0),
    )
    .unwrap();
    let report = app.sift_pass(&dig, 100, None).await.unwrap();
    assert_eq!(report.finds_closed, 1);
    assert_eq!(series_finds(&app, FindStatus::Open).len(), 2);
    assert_eq!(find_for(&app, &dig, "a28").status, FindStatus::Superseded);
    assert_eq!(find_for(&app, &dig, "b28").status, FindStatus::Open);
    assert_eq!(find_for(&app, &dig, "a14").status, FindStatus::Open);
}
