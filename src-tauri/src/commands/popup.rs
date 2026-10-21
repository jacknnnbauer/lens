use tauri::{AppHandle, Emitter, Manager, State};

use crate::capture::{CaptureService, CaptureStatus, POPUP};
use crate::error::{Error, Result};

pub const EVENT_MAIN_LOOKUP: &str = "main:lookup";

#[tauri::command]
pub async fn hide_popup(app: AppHandle) -> Result<()> {
    if let Some(w) = app.get_webview_window(POPUP) {
        crate::capture::remember_popup_size(&app);
        w.hide()?;
    }
    Ok(())
}

/// 弹窗里按 Enter：在主窗口里查同一个词（SPEC 9.4）。
#[tauri::command]
pub async fn open_in_main(app: AppHandle, query: String) -> Result<()> {
    if let Some(w) = app.get_webview_window(POPUP) {
        crate::capture::remember_popup_size(&app);
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

#[derive(Debug, serde::Deserialize)]
pub struct AnchorInput {
    pub x: f64,
    pub y: f64,
    pub h: f64,
}

/// Lens 窗口内划词查词：弹出查词小窗（带语境句）。`anchor` 为词所在矩形左下角的屏幕物理像素；从小窗里点词时不传。
#[tauri::command]
pub async fn lookup_in_popup(
    app: AppHandle,
    window: tauri::WebviewWindow,
    text: String,
    paragraph: Option<String>,
    sel16: Option<(usize, usize)>,
    anchor: Option<AnchorInput>,
) -> Result<()> {
    let title = if window.label() == crate::capture::POPUP { "词镜 Lens · 查词" } else { "词镜 Lens" };
    let anchor = anchor.map(|a| crate::capture::Anchor { x: a.x, y: a.y, h: a.h });
    crate::capture::show_in_app(&app, &text, paragraph.as_deref(), sel16, anchor, title)
}

/// 主窗口内划词：存语境句，返回 id，供随后的查询与收藏引用。
#[tauri::command]
pub async fn save_in_app_context(app: AppHandle, text: String, paragraph: String, sel16: (usize, usize)) -> Result<Option<i64>> {
    Ok(crate::capture::save_in_app_context(&app, &text, &paragraph, sel16))
}
