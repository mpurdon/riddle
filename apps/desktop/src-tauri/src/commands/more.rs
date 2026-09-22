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
pub async fn task_centre(state: State<'_, AppState>) -> Result<Value> { route(&state, "task_centre", json!({})).await }

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Value> { route(&state, "get_settings", json!({})).await }

#[tauri::command]
pub async fn privacy_statement(state: State<'_, AppState>, dig_id: Option<String>) -> Result<Value> { route(&state, "privacy_statement", json!({ "digId": dig_id })).await }

#[tauri::command]
pub async fn probe_judge(state: State<'_, AppState>) -> Result<Value> { route(&state, "probe_judge", json!({})).await }

#[tauri::command]
pub async fn probe_llm(state: State<'_, AppState>) -> Result<Value> { route(&state, "probe_llm", json!({})).await }

#[tauri::command]
pub async fn probe_provider(state: State<'_, AppState>, dig_id: String) -> Result<Value> { route(&state, "probe_provider", json!({ "digId": dig_id })).await }

#[tauri::command]
pub async fn update_preview(state: State<'_, AppState>) -> Result<Value> { route(&state, "update_preview", json!({})).await }

#[tauri::command]
pub async fn director_send(state: State<'_, AppState>, conversation_id: Option<String>, message: String, dig_ids: Option<Vec<String>>, allow_contamination: Option<bool>) -> Result<Value> {
    route(&state, "director_send", json!({ "conversationId": conversation_id, "message": message, "digIds": dig_ids.unwrap_or_default(), "allowContamination": allow_contamination.unwrap_or(false) })).await
}

#[tauri::command]
pub async fn list_conversations(state: State<'_, AppState>) -> Result<Value> { route(&state, "list_conversations", json!({})).await }

#[tauri::command]
pub async fn get_conversation(state: State<'_, AppState>, conversation_id: String) -> Result<Value> { route(&state, "get_conversation", json!({ "conversationId": conversation_id })).await }
