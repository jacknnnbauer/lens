use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::capture::{self, CaptureService};
use crate::error::Result;
use crate::settings::Settings;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings> {
    state.settings.get()
}

pub const EVENT_CAPTURE_STATUS: &str = "capture:status";

/// `patch` 为部分设置对象，按 JSON Merge Patch 合并；返回合并后的完整设置。
#[tauri::command]
pub async fn set_settings(app: AppHandle, state: State<'_, AppState>, patch: serde_json::Value) -> Result<Settings> {
    let before = state.settings.get()?;
    let after = state.settings.patch(patch)?;
    // 快捷键改了就立即重新注册，并把结果推给各窗口
    if before.capture.hotkey != after.capture.hotkey {
        let status = capture::register_hotkey(&app, Some(&before.capture.hotkey), &after.capture.hotkey);
        if let Some(service) = app.try_state::<CaptureService>() {
            if let Ok(mut s) = service.status.lock() {
                *s = capture::CaptureStatus { double_copy: s.double_copy, ..status };
                let _ = app.emit(EVENT_CAPTURE_STATUS, s.clone());
            }
        }
    }
    Ok(after)
}
