use std::path::PathBuf;

use tauri::{AppHandle, State, WebviewWindow};

use crate::app_state::AppState;
use crate::db::history::{self, ContextRow};
use crate::dict::index::normalize_key;
use crate::error::Result;
use crate::export::{self, Paper};
use crate::query::normalize_query;

/// 确保导出窗口存在；返回是否为新建（新建时前端要等它报告就绪）。
#[tauri::command]
pub async fn export_prepare(app: AppHandle) -> Result<bool> {
    export::ensure_window(&app)
}

/// 由导出窗口调用：把自己当前的页面打印成 PDF。
#[tauri::command]
pub async fn export_print_pdf(window: WebviewWindow, path: PathBuf, paper: Paper) -> Result<()> {
    export::print_pdf(&window, &path, paper).await
}

/// 由导出窗口调用：写出单文件 HTML。
#[tauri::command]
pub async fn export_save_html(path: PathBuf, html: String) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || export::save_html(&path, &html))
        .await
        .map_err(|e| crate::error::Error::Other(format!("写文件任务失败：{e}")))?
}

/// 这个词最近一次划词捕获的语境句（导出「我的语境」用）。
#[tauri::command]
pub async fn latest_context(state: State<'_, AppState>, query: String) -> Result<Option<ContextRow>> {
    let lemma = normalize_key(&normalize_query(&query));
    state.db.with_conn(|c| history::latest_context(c, &lemma))
}
