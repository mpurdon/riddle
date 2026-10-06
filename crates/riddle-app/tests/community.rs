//! Mail from communities the user belongs to is kept under Community, and
//! posts from creators they back under Creators. Nextdoor's neighbourhood
//! posts had been filed with LinkedIn and Pinterest, two became Pay Finds
//! ("Jon owes me $2500"), and Patreon posts were spoiled as promotions. A
//! platform's own marketing is not a community post, and a group of the
//! user's own joins by the `communities` list.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

fn mail(dig: &DigId, pid: &str, from: &str, subject: &str) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    m.from = Address::new(from, None);
    m.to = vec![Address::new("me@gmail.com", None)];
    m.subject = subject.into();
    m.body_text = Some("Posted in your neighbourhood.".into());
    m.has_body = true;
    m.headers.message_id = Some(format!("<{pid}@example>"));
    m.headers.authentication = AuthenticationResults {
        spf: AuthVerdict::Pass,
        dkim: AuthVerdict::Pass,
        dmarc: AuthVerdict::Pass,
    };
    m
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn community_posts_and_creator_posts_are_kept_apart_from_marketing() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    app.runtime
        .invoke_as_user(
            "save_list",
            json!({ "name": "communities", "entries": ["lakeviewresidents.example"] }),
        )
        .await
        .unwrap();
    let social = riddle_mesh::packs::builtin_packs()
        .into_iter()
        .find(|p| p.id == "reading-social")
        .unwrap();
    let text = &social
        .meshes
        .iter()
        .find(|m| m.text.contains("Mesh: Community\n"))
        .unwrap()
        .text;
    common::install_mesh(
        &app,
        &text.replacen("\nScope:", "\nPriority: 24\nScope:", 1),
        MeshStatus::Active,
    );
    let store = app.stores.dig(&dig.id).unwrap();
    for m in [
        mail(
            &dig.id,
            "post",
            "no-reply@is.ca.nextdoor.com",
            "Jon owes me $2500 for unpaid work",
        ),
        mail(
            &dig.id,
            "ad",
            "no-reply@ms.email.nextdoor.com",
            "Fave awards are here: cast your vote",
        ),
        mail(
            &dig.id,
            "creator",
            "garandthumb@creator.patreon.com",
            "(Ad Free) How To Survive",
        ),
        mail(
            &dig.id,
            "hoa",
            "board@lakeviewresidents.example",
            "Road closure on Elm St",
        ),
    ] {
        repo::messages::upsert(&store, &m).unwrap();
    }
    app.sift_pass(&dig, 100, None).await.unwrap();
    let current = |pid: &str| {
        repo::classifications::current(&store, &MessageId::for_provider(&dig.id, pid))
            .unwrap()
            .unwrap()
    };
    let community = Some("Community".to_string());
    let post = current("post");
    assert_eq!(
        (post.disposition.clone(), post.strata.first().cloned()),
        (Disposition::Filed, community.clone()),
        "{}",
        post.reason
    );
    assert_eq!(current("hoa").strata.first().cloned(), community);
    let creator = current("creator");
    assert_eq!(
        (creator.disposition.clone(), creator.strata.first().cloned()),
        (Disposition::Filed, Some("Creators".to_string()))
    );
    assert!(
        !current("ad").strata.contains(&"Community".to_string()),
        "Nextdoor's own marketing is not a post"
    );
}
