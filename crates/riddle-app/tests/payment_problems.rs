//! A payment that did not go through needs the user now: the service lapses
//! if nobody fixes the card. The Money pack's Payment problems rule makes
//! it a High Pay Find that interrupts, ahead of the rules that had filed it
//! as an ordinary bill; a charge that fell back to a backup card is a
//! heads-up; and the same words from an unauthenticated sender are not
//! trusted. AWS's genuine past-due notices had gone to Contamination because
//! amazonaws.com read as a lookalike of Amazon; they are bills again.

use riddle_core::*;
use riddle_store::repo;
use serde_json::json;

mod common;

fn mail(dig: &DigId, pid: &str, from: &str, subject: &str, body: &str, signed: bool) -> Message {
    let mut m = riddle_mail::fixture::blank_message(dig.as_str(), pid);
    m.from = Address::new(from, None);
    m.to = vec![Address::new("me@gmail.com", None)];
    m.subject = subject.into();
    m.body_text = Some(body.into());
    m.has_body = true;
    m.headers.message_id = Some(format!("<{pid}@example>"));
    if signed {
        m.headers.authentication = AuthenticationResults {
            spf: AuthVerdict::Pass,
            dkim: AuthVerdict::Pass,
            dmarc: AuthVerdict::Pass,
        };
    }
    m
}

fn pack_mesh(app: &riddle_app::App, name: &str, priority: i32) {
    let money = riddle_mesh::packs::builtin_packs()
        .into_iter()
        .find(|p| p.id == "money")
        .unwrap();
    let text = &money
        .meshes
        .iter()
        .find(|m| m.text.contains(&format!("Mesh: {name}\n")))
        .unwrap()
        .text;
    common::install_mesh(
        app,
        &text.replacen("\nScope:", &format!("\nPriority: {priority}\nScope:"), 1),
        MeshStatus::Active,
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_payment_interrupts_and_a_backup_charge_or_an_unsigned_one_does_not() {
    let (app, ops) = common::boot_scripted(true, |_| {}).await;
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 7, 5, json!({})).await;
    pack_mesh(&app, "Payment problems", -2);
    pack_mesh(&app, "Bills and renewals", 23);
    let store = app.stores.dig(&dig.id).unwrap();
    for m in [
        mail(
            &dig.id,
            "failed",
            "failed-payments@icepanel.example",
            "CA$76.84 payment to IcePanel Technologies Inc. was unsuccessful",
            "We were unable to charge your card.",
            true,
        ),
        mail(
            &dig.id,
            "backup",
            "no-reply@amazonaws.com",
            "Payment attempt successful on backup payment method",
            "Your primary payment method failed, so we charged your backup payment method.",
            true,
        ),
        mail(
            &dig.id,
            "unsigned",
            "billing@icepanel.example",
            "Your payment failed",
            "Update your card here.",
            false,
        ),
        mail(
            &dig.id,
            "pastdue",
            "no-reply@amazonaws.com",
            "Action required – Your AWS account is past due",
            "Your payment is past due. Pay now to avoid suspension.",
            true,
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
    let pay = Disposition::Find {
        find_type: FindType::Pay,
    };

    let failed = current("failed");
    assert_eq!(
        (failed.disposition.clone(), failed.importance, failed.notify),
        (pay.clone(), Importance::High, true),
        "{}",
        failed.reason
    );
    let backup = current("backup");
    assert_eq!(
        backup.disposition,
        Disposition::Find {
            find_type: FindType::Know
        },
        "{}",
        backup.reason
    );
    let unsigned = current("unsigned");
    assert!(!unsigned.notify, "{}", unsigned.reason);
    let pastdue = current("pastdue");
    assert_eq!(
        (pastdue.disposition.clone(), pastdue.notify),
        (pay, true),
        "{}",
        pastdue.reason
    );
}
