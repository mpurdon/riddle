//! The spot check: every running Mesh with how much it decided, a sample of
//! its recent decisions (one per sender before a second from the same
//! sender), and a right/wrong grade per decision that the tally counts and
//! Undo takes back. It is how a person, or a new one onboarding, finds out
//! whether a rule does what it says without asking anyone in chat.

use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn decisions_are_sampled_one_sender_at_a_time_graded_and_tallied() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    common::fixture_dig_synced(&app, &ops, "Work", "me@example.com", 9, 120, json!({})).await;

    let rows = app.rule_check().unwrap();
    let busy = rows
        .iter()
        .find(|r| r.decided >= 3)
        .expect("some Mesh decided the fixture mail")
        .clone();
    let samples = app.rule_samples(&busy.mesh_id, 10).unwrap();
    assert!(!samples.is_empty() && samples.len() <= 10);
    let senders: Vec<String> = samples
        .iter()
        .map(|s| s.from.email.to_ascii_lowercase())
        .collect();
    let distinct: std::collections::HashSet<&String> = senders.iter().collect();
    // Repeats only after every sender has had a turn.
    let first_repeat = senders
        .iter()
        .enumerate()
        .position(|(i, s)| senders[..i].contains(s));
    if let Some(i) = first_repeat {
        assert_eq!(i, distinct.len(), "{senders:?}");
    }

    let out = app
        .runtime
        .invoke_as_user(
            "grade_decision",
            json!({ "meshId": busy.mesh_id, "messageId": samples[0].message_id, "verdict": "right" }),
        )
        .await
        .unwrap();
    let row = |app: &riddle_app::App| {
        app.rule_check()
            .unwrap()
            .into_iter()
            .find(|r| r.mesh_id == busy.mesh_id)
            .unwrap()
    };
    assert_eq!((row(&app).right, row(&app).wrong), (1, 0));
    assert_eq!(
        app.rule_samples(&busy.mesh_id, usize::MAX)
            .unwrap()
            .iter()
            .find(|s| s.message_id == samples[0].message_id)
            .unwrap()
            .grade
            .as_deref(),
        Some("right")
    );
    app.runtime
        .invoke_as_user(
            "undo_activity",
            json!({ "activityId": out.activity_id.unwrap() }),
        )
        .await
        .unwrap();
    assert_eq!(row(&app).right, 0);
}
