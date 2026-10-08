use std::path::PathBuf;

use serde::Serialize;
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
    crate::i18n::set_language(&after.appearance.language);
    let Some(service) = app.try_state::<CaptureService>() else { return Ok(after) };
    // 快捷键改了就立即重新注册；Ctrl+C+C 开关立即生效。结果推给各窗口
    let hotkey_changed = before.capture.hotkey != after.capture.hotkey;
    if hotkey_changed {
        let status = capture::register_hotkey(&app, Some(&before.capture.hotkey), &after.capture.hotkey);
        if let Ok(mut s) = service.status.lock() {
            *s = capture::CaptureStatus { double_copy: s.double_copy, ..status };
        }
    }
    let double_copy_changed = before.capture.double_copy != after.capture.double_copy;
    if double_copy_changed {
        service.set_double_copy(&app, after.capture.double_copy)?;
    }
    if hotkey_changed || double_copy_changed {
        if let Ok(s) = service.status.lock() {
            let _ = app.emit(EVENT_CAPTURE_STATUS, s.clone());
        }
    }
    Ok(after)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPaths {
    pub data: PathBuf,
    pub settings: PathBuf,
    pub logs: PathBuf,
    pub version: &'static str,
}

/// 设置页「数据」一节：数据目录、设置文件与日志的位置。
#[tauri::command]
pub async fn app_paths(state: State<'_, AppState>) -> Result<AppPaths> {
    let data = state.root.clone();
    Ok(AppPaths {
        settings: data.join("settings.json"),
        logs: data.join("logs"),
        data,
        version: env!("CARGO_PKG_VERSION"),
    })
}
