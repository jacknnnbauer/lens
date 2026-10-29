//! 阅读：文章库的命令。业务逻辑在 `crate::db::articles`。

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::db::articles::{self, Article, ArticleMeta};
use crate::error::{Error, Result};

/// 单个文件最大 5 MB
const MAX_FILE: u64 = 5 * 1024 * 1024;

#[tauri::command]
pub async fn article_list(state: State<'_, AppState>) -> Result<Vec<ArticleMeta>> {
    state.db.with_conn(articles::list)
}

#[tauri::command]
pub async fn article_open(state: State<'_, AppState>, id: i64) -> Result<Option<Article>> {
    state.db.with_conn(|c| articles::open(c, id))
}

/// 粘贴的 Markdown 存成一篇；没给标题时取第一个 # 标题或第一行
#[tauri::command]
pub async fn article_create(state: State<'_, AppState>, title: Option<String>, body: String) -> Result<i64> {
    state.db.with_conn(|c| articles::create(c, title.as_deref(), &body, None))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub ids: Vec<i64>,
    /// 没导入的文件和原因
    pub failed: Vec<(String, String)>,
}

/// 导入 .md / .markdown / .txt 文件（可多个）
#[tauri::command]
pub async fn article_import_files(state: State<'_, AppState>, paths: Vec<PathBuf>) -> Result<ImportResult> {
    let mut ids = Vec::new();
    let mut failed = Vec::new();
    for p in paths {
        let name = p.display().to_string();
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !["md", "markdown", "txt"].contains(&ext.as_str()) {
            failed.push((name, crate::i18n::t("只支持 .md 和 .txt 文件", "Only .md and .txt files are supported").into()));
            continue;
        }
        match std::fs::metadata(&p) {
            Ok(m) if m.len() > MAX_FILE => {
                failed.push((name, crate::i18n::t("文件超过 5 MB", "The file is over 5 MB").into()));
                continue;
            }
            Err(e) => {
                failed.push((name, e.to_string()));
                continue;
            }
            Ok(_) => {}
        }
        let text = match std::fs::read(&p) {
            Ok(b) => articles::decode_file(&b),
            Err(e) => {
                failed.push((name, e.to_string()));
                continue;
            }
        };
        match state.db.with_conn(|c| articles::create(c, None, &text, Some(&name))) {
            Ok(id) => ids.push(id),
            Err(e) => failed.push((name, e.to_string())),
        }
    }
    Ok(ImportResult { ids, failed })
}

#[tauri::command]
pub async fn article_rename(state: State<'_, AppState>, id: i64, title: String) -> Result<()> {
    state.db.with_conn(|c| articles::rename(c, id, &title))
}

/// 改正文，返回新的词数
#[tauri::command]
pub async fn article_update_body(state: State<'_, AppState>, id: i64, body: String) -> Result<i64> {
    state.db.with_conn(|c| articles::update_body(c, id, &body))
}

#[tauri::command]
pub async fn article_progress(state: State<'_, AppState>, id: i64, progress: f64) -> Result<()> {
    if !progress.is_finite() {
        return Err(Error::Other("progress".into()));
    }
    state.db.with_conn(|c| articles::set_progress(c, id, progress))
}

#[tauri::command]
pub async fn article_delete(state: State<'_, AppState>, id: i64) -> Result<bool> {
    state.db.with_conn(|c| articles::delete(c, id))
}
