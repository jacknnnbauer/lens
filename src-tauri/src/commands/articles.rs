//! 阅读：文章库的命令。业务逻辑在 `crate::db::articles`。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::db::articles::{self, Article, ArticleMeta, BookInfo};
use crate::epub;
use crate::error::{Error, Result};

/// 单个文件最大 5 MB
const MAX_FILE: u64 = 5 * 1024 * 1024;
/// 电子书最大 500 MB（有不少图片的书）
const MAX_BOOK: u64 = 500 * 1024 * 1024;

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

const EXTS: [&str; 4] = ["md", "markdown", "txt", "epub"];

fn is_article_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| EXTS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

fn is_epub(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("epub"))
}

/// 数据目录里这本书的副本：原文件挪走、删掉都不影响阅读
pub fn book_file(root: &Path, hash: &str) -> PathBuf {
    root.join("books").join(format!("{hash}.epub"))
}

fn import_book(state: &AppState, p: &Path) -> Result<i64> {
    if std::fs::metadata(p)?.len() > MAX_BOOK {
        return Err(Error::Other(crate::i18n::t("文件超过 500 MB", "The file is over 500 MB").into()));
    }
    let book = epub::parse(p)?;
    let hash = epub::hash_file(p)?;
    let dest = book_file(&state.root, &hash);
    if !dest.is_file() {
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // 先写临时文件再改名：复制到一半退出不会留下半本书
        let tmp = dest.with_extension("tmp");
        std::fs::copy(p, &tmp)?;
        std::fs::rename(&tmp, &dest)?;
    }
    let name = p.display().to_string();
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let done = state.db.with_conn(|c| articles::import_book(c, &name, &hash, &stem, &book))?;
    if let Some(old) = done.stale_hash {
        if let Err(e) = std::fs::remove_file(book_file(&state.root, &old)) {
            tracing::warn!(error = %e, "删除旧的电子书副本失败");
        }
    }
    Ok(done.id)
}

/// 读一个文件并导入（同一个文件导入过就沿用或更新那篇，见 `articles::import_file`）
fn import_one(state: &AppState, p: &Path) -> std::result::Result<i64, String> {
    if !is_article_file(p) {
        return Err(crate::i18n::t("只支持 .md、.txt 和 .epub 文件", "Only .md, .txt and .epub files are supported").into());
    }
    if is_epub(p) {
        return import_book(state, p).map_err(|e| e.to_string());
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

/// 启动参数或第二次启动转过来的参数里有 .md / .txt / .epub：导入、记下等界面来取，并通知主窗口切到阅读
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
    let hash = state.db.with_conn(|c| articles::book_hash(c, id))?;
    let deleted = state.db.with_conn(|c| articles::delete(c, id))?;
    if let (true, Some(hash)) = (deleted, hash) {
        if let Err(e) = std::fs::remove_file(book_file(&state.root, &hash)) {
            tracing::warn!(error = %e, "删除电子书副本失败");
        }
    }
    Ok(deleted)
}

/// 电子书的章节、目录和读到的位置；普通文章或已删除时返回 None
#[tauri::command]
pub async fn book_open(state: State<'_, AppState>, id: i64) -> Result<Option<BookInfo>> {
    state.db.with_conn(|c| articles::book_info(c, id))
}

/// 一章的原文（XHTML），前端清洗后排版
#[tauri::command]
pub async fn book_chapter(state: State<'_, AppState>, id: i64, idx: i64) -> Result<String> {
    let (hash, href) = state.db.with_conn(|c| Ok((articles::book_hash(c, id)?, articles::chapter_href(c, id, idx)?)))?;
    let (Some(hash), Some(href)) = (hash, href) else {
        return Err(Error::Other(crate::i18n::t("这篇文章已被删除", "This article has been deleted").into()));
    };
    let path = book_file(&state.root, &hash);
    tauri::async_runtime::spawn_blocking(move || epub::read_chapter(&path, &href)).await.map_err(|e| Error::Other(e.to_string()))?
}

#[tauri::command]
pub async fn book_progress(state: State<'_, AppState>, id: i64, chapter: i64, pos: f64) -> Result<()> {
    if !pos.is_finite() {
        return Err(Error::Other("progress".into()));
    }
    state.db.with_conn(|c| articles::set_book_progress(c, id, chapter, pos))
}
