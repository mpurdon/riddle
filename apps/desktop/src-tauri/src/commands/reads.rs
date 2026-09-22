//! Read commands: thin adapters over `riddle_app::App`'s read services.

use crate::state::AppState;
use riddle_core::*;
use riddle_errors::Result;
use riddle_store::repo;
use serde::Deserialize;
use tauri::State;

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

#[tauri::command]
pub fn summary(state: State<'_, AppState>) -> Result<Summary> {
    let since = chrono::Utc::now() - chrono::Duration::hours(24);
    state.app.summary(since)
}

#[tauri::command]
pub fn list_digs(state: State<'_, AppState>) -> Result<Vec<Dig>> { state.app.list_digs() }

#[tauri::command]
pub fn list_dig_groups(state: State<'_, AppState>) -> Result<Vec<DigGroup>> { repo::digs::list_groups(&state.stores().index()) }

#[tauri::command]
pub fn list_messages(state: State<'_, AppState>, dig_id: Option<DigId>, filter: Option<MessageFilterArgs>, page: Option<PageRequest>) -> Result<Page<MessageSummary>> {
    let filter = filter.unwrap_or_default().into();
    state.app.list_messages(dig_id.as_ref(), &filter, &page.unwrap_or_default())
}

#[tauri::command]
pub fn get_message(state: State<'_, AppState>, dig_id: DigId, message_id: MessageId) -> Result<Message> {
    state.app.get_message(&dig_id, &message_id)
}

#[tauri::command]
pub fn get_thread(state: State<'_, AppState>, dig_id: DigId, thread_id: ThreadId) -> Result<Thread> {
    state.app.get_thread(&dig_id, &thread_id)
}

#[tauri::command]
pub fn explain(state: State<'_, AppState>, dig_id: DigId, message_id: MessageId) -> Result<Explanation> {
    state.app.explain(&dig_id, &message_id)
}

#[tauri::command]
pub fn search(state: State<'_, AppState>, query: String, dig_id: Option<DigId>, page: Option<PageRequest>) -> Result<Page<MessageSummary>> {
    let filter = repo::messages::MessageFilter { query: Some(query), ..Default::default() };
    state.app.list_messages(dig_id.as_ref(), &filter, &page.unwrap_or_default())
}

#[tauri::command]
pub fn list_finds(state: State<'_, AppState>, status: Option<String>, find_type: Option<String>, dig_id: Option<String>, page: Option<PageRequest>) -> Result<Page<Find>> {
    let status = status.as_deref().and_then(|s| match s { "open" => Some(FindStatus::Open), "snoozed" => Some(FindStatus::Snoozed), "resolved" => Some(FindStatus::Resolved), "dismissed" => Some(FindStatus::Dismissed), "expired" => Some(FindStatus::Expired), "superseded" => Some(FindStatus::Superseded), _ => None });
    let find_type = find_type.as_deref().and_then(FindType::parse);
    state.app.list_finds(&repo::finds::FindFilter { status, find_type, dig_id, min_priority: None }, &page.unwrap_or_default())
}

#[tauri::command]
pub fn get_find(state: State<'_, AppState>, find_id: FindId) -> Result<Find> {
    repo::finds::get(&state.stores().index(), &find_id)?.ok_or_else(|| riddle_errors::RiddleError::not_found("find"))
}

#[tauri::command]
pub fn list_activity(state: State<'_, AppState>, dig_id: Option<String>, actor: Option<Actor>, action: Option<String>, since: Option<chrono::DateTime<chrono::Utc>>, until: Option<chrono::DateTime<chrono::Utc>>, page: Option<PageRequest>) -> Result<Page<Activity>> {
    state.app.list_activity(&repo::activities::ActivityFilter { dig_id, actor, action, since, until }, &page.unwrap_or_default())
}

#[tauri::command]
pub fn get_activity(state: State<'_, AppState>, activity_id: ActivityId) -> Result<Activity> {
    repo::activities::get(&state.stores().index(), &activity_id)?.ok_or_else(|| riddle_errors::RiddleError::not_found("activity"))
}

#[tauri::command]
pub fn list_meshes(state: State<'_, AppState>, include_deleted: Option<bool>) -> Result<Vec<Mesh>> {
    repo::meshes::list_latest(&state.stores().index(), include_deleted.unwrap_or(false))
}

#[tauri::command]
pub fn get_mesh(state: State<'_, AppState>, mesh_id: MeshId, version: Option<u32>) -> Result<Mesh> {
    let index = state.stores().index();
    let m = match version { Some(v) => repo::meshes::get(&index, &mesh_id, v)?, None => repo::meshes::latest(&index, &mesh_id)? };
    m.ok_or_else(|| riddle_errors::RiddleError::not_found("mesh"))
}

#[tauri::command]
pub fn mesh_versions(state: State<'_, AppState>, mesh_id: MeshId) -> Result<Vec<Mesh>> { repo::meshes::versions(&state.stores().index(), &mesh_id) }

#[tauri::command]
pub fn list_signals(state: State<'_, AppState>) -> Result<Vec<Signal>> { repo::signals::list(&state.stores().index()) }

#[tauri::command]
pub fn signal_versions(state: State<'_, AppState>, name: String) -> Result<Vec<Signal>> { repo::signals::versions(&state.stores().index(), &name) }

#[tauri::command]
pub fn signal_evaluations(state: State<'_, AppState>, dig_id: DigId, message_id: MessageId) -> Result<Vec<SignalEvaluation>> {
    state.app.signal_evaluations(&dig_id, &message_id)
}

#[tauri::command]
pub fn list_strata(state: State<'_, AppState>) -> Result<Vec<Stratum>> { repo::strata::list(&state.stores().index(), false) }

#[tauri::command]
pub fn list_expeditions(state: State<'_, AppState>) -> Result<Vec<Expedition>> { repo::expeditions::list(&state.stores().index(), 100) }

#[tauri::command]
pub fn get_expedition(state: State<'_, AppState>, expedition_id: ExpeditionId) -> Result<Expedition> {
    repo::expeditions::get(&state.stores().index(), &expedition_id)?.ok_or_else(|| riddle_errors::RiddleError::not_found("expedition"))
}

#[tauri::command]
pub fn list_drafts(state: State<'_, AppState>, dig_id: DigId) -> Result<Vec<Draft>> {
    let store = state.stores().dig(&dig_id)?;
    repo::drafts::list(&store)
}

#[tauri::command]
pub fn get_user_profile(state: State<'_, AppState>) -> Result<UserProfile> { repo::user_profile::get(&state.stores().index()) }

#[tauri::command]
pub fn read_trace(state: State<'_, AppState>, dig_id: DigId, trace_id: TraceId) -> Result<DecisionTrace> { state.app.read_trace(&dig_id, &trace_id) }

#[tauri::command]
pub fn recent_traces(state: State<'_, AppState>, dig_id: Option<DigId>, limit: Option<u32>) -> Result<Vec<DecisionTrace>> {
    state.app.recent_traces(dig_id.as_ref(), limit.unwrap_or(50))
}

#[tauri::command]
pub fn usage_summary(state: State<'_, AppState>, since_days: Option<u32>) -> Result<UsageSummary> {
    let since = chrono::Utc::now() - chrono::Duration::days(since_days.unwrap_or(30) as i64);
    state.app.usage_summary(since)
}

#[tauri::command]
pub fn trace_stats(state: State<'_, AppState>, dig_id: Option<DigId>, since_days: Option<u32>) -> Result<repo::traces::TraceStats> {
    let since = chrono::Utc::now() - chrono::Duration::days(since_days.unwrap_or(30) as i64);
    let digs = match dig_id { Some(d) => vec![state.app.dig(&d)?], None => state.app.list_digs()? };
    let mut total = repo::traces::TraceStats::default();
    let mut n = 0u64;
    for d in digs {
        let store = state.stores().dig(&d.id)?;
        let s = repo::traces::stats(&store, &since)?;
        total.count += s.count;
        total.escalated += s.escalated;
        total.unresolved += s.unresolved;
        total.judge_tokens += s.judge_tokens;
        total.judge_cost_usd += s.judge_cost_usd;
        total.llm_cost_usd += s.llm_cost_usd;
        total.avg_total_ms += s.avg_total_ms * s.count as f64;
        total.avg_judge_ms += s.avg_judge_ms * s.count as f64;
        n += s.count;
    }
    if n > 0 {
        total.avg_total_ms /= n as f64;
        total.avg_judge_ms /= n as f64;
    }
    Ok(total)
}
