//! 阅读：文章库的命令。业务逻辑在 `crate::db::articles`。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

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

const EXTS: [&str; 3] = ["md", "markdown", "txt"];

fn is_article_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| EXTS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

/// 读一个文件并导入（同一个文件导入过就沿用或更新那篇，见 `articles::import_file`）
fn import_one(state: &AppState, p: &Path) -> std::result::Result<i64, String> {
    if !is_article_file(p) {
        return Err(crate::i18n::t("只支持 .md 和 .txt 文件", "Only .md and .txt files are supported").into());
    }
    match std::fs::metadata(p) {
        Ok(m) if m.len() > MAX_FILE => return Err(crate::i18n::t("文件超过 5 MB", "The file is over 5 MB").into()),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let text = std::fs::read(p).map(|b| articles::decode_file(&b)).map_err(|e| e.to_string())?;
    let name = p.display().to_string();
    state.db.with_conn(|c| articles::import_file(c, &name, &text)).map_err(|e| e.to_string())
}

/// 导入 .md / .markdown / .txt 文件（可多个）
#[tauri::command]
pub async fn article_import_files(state: State<'_, AppState>, paths: Vec<PathBuf>) -> Result<ImportResult> {
    let mut ids = Vec::new();
    let mut failed = Vec::new();
    for p in paths {
        match import_one(&state, &p) {
            Ok(id) => ids.push(id),
            Err(e) => failed.push((p.display().to_string(), e)),
        }
    }
    Ok(ImportResult { ids, failed })
}

/// 资源管理器右键「用 Lens 打开」传进来、等界面来取的文章
#[derive(Default)]
pub struct PendingOpen(Mutex<Option<i64>>);

/// 启动参数或第二次启动转过来的参数里有 .md / .txt：导入、记下等界面来取，并通知主窗口切到阅读
pub fn open_from_args(app: &AppHandle, args: &[String]) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let mut last = None;
    for a in args {
        let p = Path::new(a);
        if !is_article_file(p) || !p.is_file() {
            continue;
        }
        match import_one(&state, p) {
            Ok(id) => last = Some(id),
            Err(e) => tracing::warn!(path = %p.display(), error = %e, "打开文章失败"),
        }
    }
    let Some(id) = last else { return };
    if let Some(pending) = app.try_state::<PendingOpen>() {
        if let Ok(mut g) = pending.0.lock() {
            *g = Some(id);
        }
    }
    let _ = app.emit_to("main", "reader:open", id);
}

/// 界面取走等着打开的文章（启动时先取一次，之后收到 reader:open 再取）
#[tauri::command]
pub fn article_take_pending(pending: State<'_, PendingOpen>) -> Option<i64> {
    pending.0.lock().ok().and_then(|mut g| g.take())
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
