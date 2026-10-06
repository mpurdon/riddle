//! Shared boot for the integration tests: the in-memory app with the
//! scripted judge and no LLM, and a fixture account synced through the job
//! runner. Seven test files carried this incantation by hand; the boot
//! arguments are the thing most likely to change, and every file broke
//! together when they did.
#![allow(dead_code)]

use riddle_app::ops::Ops;
use riddle_app::{App, AppConfig};
use riddle_core::*;
use riddle_settings::{JudgeProvider, LlmProvider, Settings};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// Boot in memory with the scripted judge and no LLM; `tweak` adjusts the
/// settings before boot (a different judge, a default autonomy).
pub async fn boot_scripted(
    background: bool,
    tweak: impl FnOnce(&mut Settings),
) -> (Arc<App>, Arc<Ops>) {
    let app = App::open(AppConfig::in_memory(BuildStamp::from_env_vars(
        "0", None, None, None, "test",
    )))
    .unwrap();
    let mut s = Settings::dev_defaults();
    s.judge.provider = JudgeProvider::Scripted;
    s.llm.provider = LlmProvider::Null;
    tweak(&mut s);
    let ops = app.boot_with(background, Some(s)).await.unwrap();
    (app, ops)
}

/// Add a fixture account and run its catch-up to completion. `extra` merges
/// into the `add_fixture_dig` arguments (a projection, a group).
pub async fn fixture_dig_synced(
    app: &App,
    ops: &Ops,
    name: &str,
    email: &str,
    seed: u64,
    count: u32,
    extra: serde_json::Value,
) -> Dig {
    let mut args = json!({ "name": name, "email": email, "seed": seed, "count": count });
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            args[k] = v.clone();
        }
    }
    let out = app
        .runtime
        .invoke_as_user("add_fixture_dig", args)
        .await
        .unwrap();
    let dig: Dig = serde_json::from_value(out.result).unwrap();
    ops.queue
        .enqueue(
            riddle_jobs::JobSpec::new(JobKind::CatchUp, format!("Catch up {name}"))
                .dig(dig.id.clone()),
        )
        .unwrap();
    // Generous: with the front-end suite or clippy running beside the tests,
    // sixty seconds was not always enough and a synced Dig came back empty.
    ops.runner.run_until_idle(Duration::from_secs(180)).await;
    dig
}

/// Store a Mesh from its text in the given status, as a test's starting
/// rules, without the approval an Action would ask for.
pub fn install_mesh(app: &App, text: &str, status: MeshStatus) -> Mesh {
    let mut mesh = riddle_mesh::parse::parse(text).unwrap();
    mesh.status = status;
    riddle_store::repo::meshes::upsert(&app.stores.index(), &mesh).unwrap();
    mesh
}
