//! A Mesh changed while it runs keeps deciding with its last Active version
//! until the new one is made Active; the new version, in Shadow, records
//! what it would do beside the live decision. Changing Payments put the
//! whole Mesh in Shadow and an "access turned off" notice went a day with no
//! decision at all (13 Oct 2026).

use riddle_core::*;
use riddle_store::repo;
use riddle_sync::engine::CancelFlag;
use serde_json::json;

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_changed_mesh_keeps_deciding_while_its_new_version_is_in_shadow() {
    let (app, ops) = common::boot_scripted(false, |_| {}).await;
    app.runtime.arm(true);
    let dig =
        common::fixture_dig_synced(&app, &ops, "Personal", "me@gmail.com", 5, 1, json!({})).await;
    let mesh = common::install_mesh(
        &app,
        "Mesh: Lists first\nPriority: -99\nROUTES\nIF mailing_list\n    → Filed + Stratum: Newsletters",
        MeshStatus::Active,
    );
    app.runtime
        .invoke_as_user(
            "update_mesh",
            json!({ "meshId": mesh.id, "text": "Mesh: Lists first\nPriority: -99\nROUTES\nIF mailing_list\n    → Find: Know + Stratum: Newsletters" }),
        )
        .await
        .unwrap();
    assert_eq!(
        repo::meshes::latest(&app.stores.index(), &mesh.id)
            .unwrap()
            .unwrap()
            .status,
        MeshStatus::Shadow,
        "the new version starts in Shadow"
    );

    app.runtime
        .invoke_as_user(
            "inject_fixture_message",
            json!({ "digId": dig.id, "template": "mailing_list" }),
        )
        .await
        .unwrap();
    app.full_pass(&dig.id, 500, &CancelFlag::new())
        .await
        .unwrap();

    let store = app.stores.dig(&dig.id).unwrap();
    let (id, live) = repo::messages::ids_since(
        &store,
        &(chrono::Utc::now() - chrono::Duration::days(3650)),
        10_000,
    )
    .unwrap()
    .into_iter()
    .find_map(|id| {
        repo::classifications::current(&store, &id)
            .unwrap()
            .filter(|k| k.mesh_id.as_ref() == Some(&mesh.id))
            .map(|k| (id, k))
    })
    .expect("the live version decided the list mail");
    assert_eq!(
        (live.disposition, live.mesh_version, live.shadow),
        (Disposition::Filed, Some(1), false)
    );
    let proposal = repo::classifications::current_shadow(&store, &id)
        .unwrap()
        .expect("a Shadow proposal beside it");
    assert_eq!(
        (proposal.disposition, proposal.mesh_version),
        (
            Disposition::Find {
                find_type: FindType::Know
            },
            Some(2)
        )
    );
}
