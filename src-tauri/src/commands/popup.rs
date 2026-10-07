use tauri::{AppHandle, Emitter, Manager, State};

use crate::capture::{CaptureService, CaptureStatus, POPUP};
use crate::error::{Error, Result};

pub const EVENT_MAIN_LOOKUP: &str = "main:lookup";

#[tauri::command]
pub async fn hide_popup(app: AppHandle) -> Result<()> {
    if let Some(w) = app.get_webview_window(POPUP) {
        w.hide()?;
    }
    Ok(())
}

/// 弹窗里按 Enter：在主窗口里查同一个词（SPEC 9.4）。
#[tauri::command]
pub async fn open_in_main(app: AppHandle, query: String) -> Result<()> {
    if let Some(w) = app.get_webview_window(POPUP) {
        w.hide()?;
    }
    if let Some(main) = app.get_webview_window("main") {
        main.unminimize()?;
        main.show()?;
        main.set_focus()?;
        main.emit_to("main", EVENT_MAIN_LOOKUP, query)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn capture_status(service: State<'_, CaptureService>) -> Result<CaptureStatus> {
    Ok(service.status.lock().map_err(|_| Error::LockPoisoned)?.clone())
}

/// 写剪贴板（SPEC 10）：纯文本必写；给出 html 时同时写 CF_HTML（Word 等粘贴保留格式）。
#[tauri::command]
pub async fn write_clipboard(text: String, html: Option<String>) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || crate::clipboard::write(&text, html.as_deref()))
        .await
        .map_err(|e| Error::Other(format!("写剪贴板任务失败：{e}")))?
}
