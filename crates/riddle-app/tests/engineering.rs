//! The Engineering pack: Search Console's indexing problems on the user's
//! own site are something to do, its progress notices are Filed, a cloud
//! cost alert is important to know, and a vendor's product news is Filed
//! under Engineering rather than set aside. Before, all of it was plain
//! notifications because google.com sends everything and a vendor list
//! names domains (18 Oct 2026).

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

fn mail(dig: &DigId, pid: &str, from: &str, subject: &str, body: &str) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    m.from = Address::new(from, None);
    m.to = vec![Address::new("me@gmail.com", None)];
    m.subject = subject.into();
    m.body_text = Some(body.into());
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
async fn search_console_problems_are_to_do_cost_alerts_to_know_and_vendor_news_filed_under_engineering(
) {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    app.runtime
        .invoke_as_user(
            "install_pack",
            json!({ "packId": "engineering", "status": "active", "replace": true }),
        )
        .await
        .unwrap();
    let store = app.stores.dig(&dig.id).unwrap();
    for m in [
        mail(
            &dig.id,
            "indexing",
            "sc-noreply@google.com",
            "New reasons prevent pages from being indexed on site example.me",
            "Search Console has identified that some pages on your site are not being indexed.",
        ),
        mail(
            &dig.id,
            "fixed",
            "sc-noreply@google.com",
            "Page indexing issues successfully fixed for site example.me",
            "Google has validated your fix of Page indexing issues on your site.",
        ),
        mail(
            &dig.id,
            "budget",
            "budgets@costalerts.amazonaws.com",
            "AWS Budgets: monthly has exceeded 80% of the budgeted amount",
            "Your budget monthly has exceeded the threshold. Actual spend is $41.20.",
        ),
        mail(
            &dig.id,
            "news",
            "hi@news.kilocode.ai",
            "What's new in Kilo Code this month",
            "New models, faster edits and a redesigned sidebar. Read the changelog.",
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
    let indexing = current("indexing");
    assert_eq!(
        indexing.disposition,
        Disposition::Find {
            find_type: FindType::Do
        },
        "{}",
        indexing.reason
    );
    assert!(
        indexing.strata.iter().any(|s| s == "Engineering"),
        "{:?}",
        indexing.strata
    );
    let fixed = current("fixed");
    assert_eq!(fixed.disposition, Disposition::Filed, "{}", fixed.reason);
    assert!(fixed.strata.iter().any(|s| s == "Engineering"));
    let budget = current("budget");
    assert_eq!(
        (budget.disposition.clone(), budget.importance),
        (
            Disposition::Find {
                find_type: FindType::Know
            },
            Importance::High
        ),
        "{}",
        budget.reason
    );
    assert!(budget.strata.iter().any(|s| s == "Engineering"));
    let news = current("news");
    assert_eq!(news.disposition, Disposition::Filed, "{}", news.reason);
    assert!(news.strata.iter().any(|s| s == "Engineering"));
}
