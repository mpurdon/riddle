//! The command router: one function from (command name, JSON args) to a JSON
//! result, covering the read surface in `docs/COMMANDS.md` plus
//! `invoke_action`. The Tauri commands are typed wrappers over the same
//! read services; the loopback bridge and any automation call this router
//! directly, so behaviour is identical whichever door a client uses.

use crate::App;
use riddle_core::*;
use riddle_errors::{Result, RiddleError};
use riddle_store::repo;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

fn arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<T> {
    let v = args.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| RiddleError::invalid(format!("argument {key}: {e}")))
}

fn opt<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Option<T> {
    args.get(key).cloned().and_then(|v| serde_json::from_value(v).ok())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageFilterArgs {
    pub disposition: Option<String>,
    pub find_type: Option<String>,
    pub label: Option<String>,
    pub query: Option<String>,
    pub from: Option<String>,
    pub thread_id: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub stratum: Option<String>,
    #[serde(default)]
    pub unclassified_only: bool,
}

impl From<MessageFilterArgs> for repo::messages::MessageFilter {
    fn from(a: MessageFilterArgs) -> Self {
        Self {
            disposition: a.disposition,
            find_type: a.find_type,
            label: a.label,
            query: a.query,
            from: a.from,
            thread_id: a.thread_id,
            since: a.since,
            until: a.until,
            unclassified_only: a.unclassified_only,
            include_shadow: false,
            stratum: a.stratum,
        }
    }
}

/// Names the router answers, for `tools/list`-style introspection.
pub const COMMANDS: &[&str] = &[
    "summary", "list_digs", "list_dig_groups", "list_messages", "get_message", "get_thread", "explain", "search",
    "list_finds", "get_find", "list_activity", "get_activity", "list_meshes", "get_mesh", "mesh_versions",
    "list_signals", "signal_versions", "signal_evaluations", "list_strata", "list_expeditions", "get_expedition",
    "list_drafts", "get_user_profile", "read_trace", "recent_traces", "usage_summary", "trace_stats",
    "invoke_action", "list_actions", "list_pending", "dev_armed", "list_conversations", "get_conversation", "director_send",
];

pub async fn call(app: &Arc<App>, command: &str, args: Value) -> Result<Value> {
    let index = app.stores.index();
    Ok(match command {
        "summary" => serde_json::to_value(app.summary(chrono::Utc::now() - chrono::Duration::hours(24))?)?,
        "list_digs" => serde_json::to_value(app.list_digs()?)?,
        "list_dig_groups" => serde_json::to_value(repo::digs::list_groups(&index)?)?,
        "list_messages" => {
            let filter: repo::messages::MessageFilter = opt::<MessageFilterArgs>(&args, "filter").unwrap_or_default().into();
            let page: PageRequest = opt(&args, "page").unwrap_or_default();
            serde_json::to_value(app.list_messages(opt::<DigId>(&args, "digId").as_ref(), &filter, &page)?)?
        }
        "get_message" => serde_json::to_value(app.get_message(&arg(&args, "digId")?, &arg(&args, "messageId")?)?)?,
        "get_thread" => serde_json::to_value(app.get_thread(&arg(&args, "digId")?, &arg(&args, "threadId")?)?)?,
        "explain" => serde_json::to_value(app.explain(&arg(&args, "digId")?, &arg(&args, "messageId")?)?)?,
        "search" => {
            let filter = repo::messages::MessageFilter { query: Some(arg(&args, "query")?), ..Default::default() };
            let page: PageRequest = opt(&args, "page").unwrap_or_default();
            serde_json::to_value(app.list_messages(opt::<DigId>(&args, "digId").as_ref(), &filter, &page)?)?
        }
        "list_finds" => {
            let status = opt::<String>(&args, "status").and_then(|s| match s.as_str() { "open" => Some(FindStatus::Open), "snoozed" => Some(FindStatus::Snoozed), "resolved" => Some(FindStatus::Resolved), "dismissed" => Some(FindStatus::Dismissed), "expired" => Some(FindStatus::Expired), "superseded" => Some(FindStatus::Superseded), _ => None });
            let find_type = opt::<String>(&args, "findType").and_then(|s| FindType::parse(&s));
            let page: PageRequest = opt(&args, "page").unwrap_or_default();
            serde_json::to_value(app.list_finds(&repo::finds::FindFilter { status, find_type, dig_id: opt(&args, "digId"), min_priority: None }, &page)?)?
        }
        "get_find" => serde_json::to_value(repo::finds::get(&index, &arg(&args, "findId")?)?.ok_or_else(|| RiddleError::not_found("find"))?)?,
        "list_activity" => {
            let page: PageRequest = opt(&args, "page").unwrap_or_default();
            let filter = repo::activities::ActivityFilter { dig_id: opt(&args, "digId"), actor: opt(&args, "actor"), action: opt(&args, "action"), since: opt(&args, "since"), until: opt(&args, "until") };
            serde_json::to_value(app.list_activity(&filter, &page)?)?
        }
        "get_activity" => serde_json::to_value(repo::activities::get(&index, &arg(&args, "activityId")?)?.ok_or_else(|| RiddleError::not_found("activity"))?)?,
        "list_meshes" => serde_json::to_value(repo::meshes::list_latest(&index, opt(&args, "includeDeleted").unwrap_or(false))?)?,
        "get_mesh" => {
            let id: MeshId = arg(&args, "meshId")?;
            let m = match opt::<u32>(&args, "version") { Some(v) => repo::meshes::get(&index, &id, v)?, None => repo::meshes::latest(&index, &id)? };
            serde_json::to_value(m.ok_or_else(|| RiddleError::not_found("mesh"))?)?
        }
        "mesh_versions" => serde_json::to_value(repo::meshes::versions(&index, &arg(&args, "meshId")?)?)?,
        "list_signals" => serde_json::to_value(repo::signals::list(&index)?)?,
        "signal_versions" => serde_json::to_value(repo::signals::versions(&index, &arg::<String>(&args, "name")?)?)?,
        "signal_evaluations" => serde_json::to_value(app.signal_evaluations(&arg(&args, "digId")?, &arg(&args, "messageId")?)?)?,
        "list_strata" => serde_json::to_value(repo::strata::list(&index, false)?)?,
        "list_expeditions" => serde_json::to_value(repo::expeditions::list(&index, 100)?)?,
        "get_expedition" => serde_json::to_value(repo::expeditions::get(&index, &arg(&args, "expeditionId")?)?.ok_or_else(|| RiddleError::not_found("expedition"))?)?,
        "list_drafts" => {
            let store = app.stores.dig(&arg(&args, "digId")?)?;
            serde_json::to_value(repo::drafts::list(&store)?)?
        }
        "get_user_profile" => serde_json::to_value(repo::user_profile::get(&index)?)?,
        "read_trace" => serde_json::to_value(app.read_trace(&arg(&args, "digId")?, &arg(&args, "traceId")?)?)?,
        "recent_traces" => serde_json::to_value(app.recent_traces(opt::<DigId>(&args, "digId").as_ref(), opt(&args, "limit").unwrap_or(50))?)?,
        "usage_summary" => serde_json::to_value(app.usage_summary(chrono::Utc::now() - chrono::Duration::days(opt::<i64>(&args, "sinceDays").unwrap_or(30)))?)?,
        "trace_stats" => {
            let since = chrono::Utc::now() - chrono::Duration::days(opt::<i64>(&args, "sinceDays").unwrap_or(30));
            let digs = match opt::<DigId>(&args, "digId") { Some(d) => vec![app.dig(&d)?], None => app.list_digs()? };
            let mut total = repo::traces::TraceStats::default();
            let mut n = 0u64;
            for d in digs {
                let store = app.stores.dig(&d.id)?;
                let s = repo::traces::stats(&store, &since)?;
                total.count += s.count; total.escalated += s.escalated; total.unresolved += s.unresolved;
                total.judge_tokens += s.judge_tokens; total.judge_cost_usd += s.judge_cost_usd; total.llm_cost_usd += s.llm_cost_usd;
                total.avg_total_ms += s.avg_total_ms * s.count as f64; total.avg_judge_ms += s.avg_judge_ms * s.count as f64;
                n += s.count;
            }
            if n > 0 { total.avg_total_ms /= n as f64; total.avg_judge_ms /= n as f64; }
            serde_json::to_value(total)?
        }
        "invoke_action" => {
            let inv: ActionInvocation = arg(&args, "invocation")?;
            serde_json::to_value(app.runtime.invoke(inv).await?)?
        }
        "list_actions" => serde_json::to_value(app.runtime.list_actions())?,
        "list_pending" => serde_json::to_value(repo::pending::list_pending(&index)?)?,
        "dev_armed" => json!(app.runtime.is_armed()),
        "list_conversations" => serde_json::to_value(repo::conversations::list(&index, 100)?)?,
        "get_conversation" => serde_json::to_value(repo::conversations::get(&index, &arg(&args, "conversationId")?)?.ok_or_else(|| RiddleError::not_found("conversation"))?)?,
        "director_send" => {
            let director = app.services.get::<riddle_agent::director::Director>().ok_or_else(|| RiddleError::llm_unavailable("the Director is not configured"))?;
            let req = riddle_agent::director::SendRequest {
                conversation_id: opt(&args, "conversationId"),
                message: arg(&args, "message")?,
                dig_ids: opt(&args, "digIds").unwrap_or_default(),
                allow_contamination: opt(&args, "allowContamination").unwrap_or(false),
            };
            let id = director.send(req)?;
            json!({ "conversationId": id })
        }
        other => return Err(RiddleError::not_found(format!("no command named {other}"))),
    })
}
