use tauri::State;

use crate::app_state::AppState;
use crate::error::Result;
use crate::settings::Settings;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    state.settings.get()
}

/// `patch` 为部分设置对象，按 JSON Merge Patch 合并；返回合并后的完整设置。
#[tauri::command]
pub async fn set_settings(state: State<'_, AppState>, patch: serde_json::Value) -> Result<Settings> {
    state.settings.patch(patch)
}
