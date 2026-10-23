use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::capture::{self, CaptureService};
use crate::error::Result;
use crate::settings::Settings;

/// 主窗口画好第一帧：现在显示它（启动时先隐藏，免得闪空白窗口）
#[tauri::command]
pub fn main_ready(app: tauri::AppHandle) {
    crate::first_show_main(&app);
}

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
    // 托盘图标只在「隐藏到托盘」模式下显示
    if before.general.close_to_tray != after.general.close_to_tray {
        crate::set_tray_visible(&app, after.general.close_to_tray);
    }
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

/// 新增或更新一套自定义主题（按 id），返回校验后的主题。
#[tauri::command]
pub async fn save_custom_theme(state: State<'_, AppState>, theme: crate::themes::CustomTheme) -> Result<crate::themes::CustomTheme> {
    let theme = crate::themes::validate(theme)?;
    let mut list = state.settings.get()?.custom_themes;
    match list.iter_mut().find(|t| t.id == theme.id) {
        Some(t) => *t = theme.clone(),
        None => list.push(theme.clone()),
    }
    state.settings.patch(serde_json::json!({ "customThemes": list }))?;
    Ok(theme)
}

/// 删除自定义主题；正在用它的地方退回纸本 / 石墨。
#[tauri::command]
pub async fn delete_custom_theme(state: State<'_, AppState>, id: String) -> Result<Settings> {
    let s = state.settings.get()?;
    let list: Vec<_> = s.custom_themes.into_iter().filter(|t| t.id != id).collect();
    let fix = |cur: &str, fallback: &str| if cur == id { fallback.to_string() } else { cur.to_string() };
    let a = &s.appearance;
    let mut patch = serde_json::json!({
        "customThemes": list,
        "appearance": {
            "theme": fix(&a.theme, "paper"),
            "lightTheme": fix(&a.light_theme, "paper"),
            "darkTheme": fix(&a.dark_theme, "dark"),
        }
    });
    if s.immersive.theme.as_deref() == Some(id.as_str()) {
        patch["immersive"] = serde_json::json!({ "theme": null });
    }
    state.settings.patch(patch)
}

/// 导出主题为 `.lens-theme`。
#[tauri::command]
pub async fn export_theme(path: PathBuf, theme: crate::themes::CustomTheme) -> Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    if ext.as_deref() != Some("lens-theme") {
        return Err(crate::error::Error::Other(crate::i18n::t("主题文件扩展名应为 .lens-theme", "Theme files use the .lens-theme extension").into()));
    }
    let theme = crate::themes::validate(theme)?;
    tauri::async_runtime::spawn_blocking(move || crate::themes::export(&path, &theme))
        .await
        .map_err(|e| crate::error::Error::Other(e.to_string()))?
}

/// 读入 `.lens-theme` 并加入自定义主题。
#[tauri::command]
pub async fn import_theme(state: State<'_, AppState>, path: PathBuf) -> Result<crate::themes::CustomTheme> {
    let theme = tauri::async_runtime::spawn_blocking(move || crate::themes::import(&path))
        .await
        .map_err(|e| crate::error::Error::Other(e.to_string()))??;
    let mut list = state.settings.get()?.custom_themes;
    list.push(theme.clone());
    state.settings.patch(serde_json::json!({ "customThemes": list }))?;
    Ok(theme)
}

/// 第一次点关闭（隐藏到托盘模式）后的提示框：「隐藏到托盘」。以后不再提示。
#[tauri::command]
pub async fn hide_to_tray(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    state.settings.patch(serde_json::json!({ "general": { "trayHintShown": true } }))?;
    if let Some(w) = app.get_webview_window("main") {
        w.hide()?;
    }
    Ok(())
}

/// 提示框里选「退出 Lens」，或托盘菜单「退出」。
#[tauri::command]
pub async fn quit_app(app: AppHandle) -> Result<()> {
    app.exit(0);
    Ok(())
}

const DAY_MS: i64 = 86_400_000;

/// 检查新版本。auto = 启动时自动检查：设置里关了、或一天内查过就直接返回 None，失败也只返回 None。
#[tauri::command]
pub async fn check_update(state: State<'_, AppState>, auto: bool) -> Result<Option<crate::update::UpdateInfo>> {
    if auto {
        let g = state.settings.get()?.general;
        let now = crate::db::now_ms();
        if !g.check_updates || now - g.last_update_check < DAY_MS {
            return Ok(None);
        }
        state.settings.patch(serde_json::json!({ "general": { "lastUpdateCheck": now } }))?;
        return Ok(crate::update::check(state.online.client()).await.ok());
    }
    crate::update::check(state.online.client()).await.map(Some)
}
