//! A judge that could not resolve its key at start is attached again by a
//! later pass's housekeeping once the key is there, without a restart. The
//! key source is the environment, which the test controls; no keychain, no
//! network (`JevJudge::new` builds a client and makes no call). The floor
//! between attempts is a unit test on `Housekeeping`.

use riddle_settings::{JudgeProvider, KeySource};

mod common;

const KEY_VAR: &str = "RIDDLE_JEV_API_KEY";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_judge_whose_key_arrives_after_start_is_attached_by_the_next_housekeeping() {
    std::env::remove_var(KEY_VAR);
    std::env::remove_var("TYPESAFE_API_KEY");
    let (app, ops) = common::boot_scripted(true, |s| {
        s.judge.provider = JudgeProvider::Jev;
        s.judge.key_source = KeySource::Env;
    })
    .await;
    assert!(
        app.services.judge().is_none(),
        "the key was not there at start, so there is no judge"
    );

    // The key arrives (a session renewed, a variable exported); the first
    // pass's housekeeping finds it.
    std::env::set_var(KEY_VAR, "test-key-for-the-retry");
    common::fixture_dig_synced(
        &app,
        &ops,
        "Personal",
        "me@example.com",
        3,
        3,
        serde_json::json!({}),
    )
    .await;
    assert!(
        app.services.judge().is_some(),
        "housekeeping attached the judge once the key was there"
    );
    std::env::remove_var(KEY_VAR);
}
