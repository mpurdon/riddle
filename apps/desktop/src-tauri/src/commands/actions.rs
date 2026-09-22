//! The one mutation path: `invoke_action` hands an `ActionInvocation` to the
//! runtime. Confirmation comes back as a `needs_confirmation` error carrying
//! the `PendingAction`; the client re-invokes with `pendingId`.

use crate::state::AppState;
use riddle_core::{ActionInvocation, ActionMeta, ActionOutcome, PendingAction};
use riddle_errors::Result;
use riddle_store::repo;
use tauri::State;

#[tauri::command]
pub async fn invoke_action(state: State<'_, AppState>, invocation: ActionInvocation) -> Result<ActionOutcome> {
    let runtime = state.runtime();
    // A command panic must not wedge the webview (§7): catch it at the boundary.
    let handle = tauri::async_runtime::spawn(async move { runtime.invoke(invocation).await });
    match handle.await {
        Ok(r) => r,
        Err(e) => Err(riddle_errors::RiddleError::internal(format!("action panicked: {e}"))),
    }
}

#[tauri::command]
pub fn list_actions(state: State<'_, AppState>) -> Result<Vec<ActionMeta>> { Ok(state.runtime().list_actions()) }

#[tauri::command]
pub fn list_pending(state: State<'_, AppState>) -> Result<Vec<PendingAction>> { repo::pending::list_pending(&state.stores().index()) }

#[tauri::command]
pub fn dev_armed(state: State<'_, AppState>) -> Result<bool> { Ok(state.runtime().is_armed()) }
