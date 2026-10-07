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
