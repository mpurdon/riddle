//! Commands routed through the shared dispatcher so the desktop, the bridge
//! and the CLI answer identically.

use crate::state::AppState;
use riddle_errors::Result;
use serde_json::{json, Value};
use tauri::State;

async fn route(state: &State<'_, AppState>, command: &str, args: Value) -> Result<Value> {
    riddle_app::dispatch::call(&state.app, command, args).await
}

#[tauri::command]
pub async fn task_centre(state: State<'_, AppState>) -> Result<Value> {
    route(&state, "task_centre", json!({})).await
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Value> {
    route(&state, "get_settings", json!({})).await
}

#[tauri::command]
pub async fn privacy_statement(
    state: State<'_, AppState>,
    dig_id: Option<String>,
) -> Result<Value> {
    route(&state, "privacy_statement", json!({ "digId": dig_id })).await
}

#[tauri::command]
pub async fn probe_judge(state: State<'_, AppState>) -> Result<Value> {
    route(&state, "probe_judge", json!({})).await
}

#[tauri::command]
pub async fn probe_llm(state: State<'_, AppState>) -> Result<Value> {
    route(&state, "probe_llm", json!({})).await
}

#[tauri::command]
pub async fn probe_provider(state: State<'_, AppState>, dig_id: String) -> Result<Value> {
    route(&state, "probe_provider", json!({ "digId": dig_id })).await
}

#[tauri::command]
pub async fn update_preview(state: State<'_, AppState>) -> Result<Value> {
    route(&state, "update_preview", json!({})).await
}

#[tauri::command]
pub async fn director_retry(state: State<'_, AppState>, conversation_id: String) -> Result<Value> {
    route(
        &state,
        "director_retry",
        json!({ "conversationId": conversation_id }),
    )
    .await
}

#[tauri::command]
pub async fn director_send(
    state: State<'_, AppState>,
    conversation_id: Option<String>,
    message: String,
    dig_ids: Option<Vec<String>>,
    allow_contamination: Option<bool>,
) -> Result<Value> {
    route(&state, "director_send", json!({ "conversationId": conversation_id, "message": message, "digIds": dig_ids.unwrap_or_default(), "allowContamination": allow_contamination.unwrap_or(false) })).await
}

#[tauri::command]
pub async fn list_conversations(state: State<'_, AppState>) -> Result<Value> {
    route(&state, "list_conversations", json!({})).await
}

#[tauri::command]
pub async fn get_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Value> {
    route(
        &state,
        "get_conversation",
        json!({ "conversationId": conversation_id }),
    )
    .await
}

#[tauri::command]
pub async fn parse_mesh(state: State<'_, AppState>, text: String) -> Result<Value> {
    route(&state, "parse_mesh", json!({ "text": text })).await
}

#[tauri::command]
pub async fn simulate_mesh(
    state: State<'_, AppState>,
    text: Option<String>,
    mesh_id: Option<String>,
    version: Option<u32>,
    window_days: Option<u32>,
) -> Result<Value> {
    route(
        &state,
        "simulate_mesh",
        json!({ "text": text, "meshId": mesh_id, "version": version, "windowDays": window_days }),
    )
    .await
}

/// The running Meshes in order, with what each decides and what earlier
/// Meshes take from it.
#[tauri::command]
pub async fn mesh_order(state: State<'_, AppState>, window_days: Option<u32>) -> Result<Value> {
    route(&state, "mesh_order", json!({ "windowDays": window_days })).await
}

/// Every running Mesh with its graded spot checks.
#[tauri::command]
pub async fn rule_check(state: State<'_, AppState>) -> Result<Value> {
    route(&state, "rule_check", json!({})).await
}

/// Recent decisions a Mesh made, to grade.
#[tauri::command]
pub async fn rule_samples(
    state: State<'_, AppState>,
    mesh_id: String,
    limit: Option<usize>,
) -> Result<Value> {
    route(
        &state,
        "rule_samples",
        json!({ "meshId": mesh_id, "limit": limit }),
    )
    .await
}

/// Every rule pack Riddle knows, with what it would do here.
#[tauri::command]
pub async fn list_packs(state: State<'_, AppState>) -> Result<Value> {
    route(&state, "list_packs", json!({})).await
}

/// The user's named lists (`sender IN list:banks`).
#[tauri::command]
pub async fn list_named_lists(state: State<'_, AppState>) -> Result<Value> {
    route(&state, "list_named_lists", json!({})).await
}

/// Send a system notification now, so the user can see whether they arrive.
#[tauri::command]
pub async fn test_notification(state: State<'_, AppState>) -> Result<Value> {
    crate::system::notify(
        &state.handle,
        "Riddle",
        "Notifications work. Finds at or above your threshold will appear like this.",
    );
    Ok(json!({ "sent": true }))
}
