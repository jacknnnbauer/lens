//! 阅读：文章库的命令。业务逻辑在 `crate::db::articles`。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::db::articles::{self, Article, ArticleMeta, BookInfo};
use crate::db::Db;
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

const EXTS: [&str; 7] = ["md", "markdown", "txt", "epub", "mobi", "azw3", "azw"];

fn is_article_file(p: &Path) -> bool {
    p.extension().is_some_and(|e| EXTS.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

fn is_epub(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("epub"))
}

/// Kindle 格式：导入时转成 EPUB
fn is_kindle(p: &Path) -> bool {
    p.extension().is_some_and(|e| ["mobi", "azw3", "azw"].iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// 数据目录里这本书的副本：原文件挪走、删掉都不影响阅读
pub fn book_file(root: &Path, hash: &str) -> PathBuf {
    root.join("books").join(format!("{hash}.epub"))
}

/// 数据目录里 PDF 书的原件（原版页面用）
pub fn pdf_file(root: &Path, hash: &str) -> PathBuf {
    root.join("books").join(format!("{hash}.pdf"))
}

/// 把一本书存进书库：`write` 负责把 EPUB 写到给它的路径（同样内容的书已经在了就不调用），
/// 然后解析、入库。`record_path` 记作这本书的来源；`pdf_pages` 只有 PDF 书才有。
pub fn store_book(db: &Db, root: &Path, record_path: &Path, hash: &str, write: impl FnOnce(&Path) -> Result<()>, pdf_pages: Option<&[i64]>) -> Result<i64> {
    let dest = book_file(root, hash);
    let fresh = !dest.is_file();
    if fresh {
        if let Some(dir) = dest.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // 先写临时文件再改名：写到一半退出不会留下半本书
        let tmp = dest.with_extension("tmp");
        let written = write(&tmp).and_then(|()| Ok(std::fs::rename(&tmp, &dest)?));
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    }
    let book = match epub::parse(&dest) {
        Ok(b) => b,
        Err(e) => {
            if fresh {
                let _ = std::fs::remove_file(&dest);
            }
            return Err(e);
        }
    };
    let name = record_path.display().to_string();
    let stem = record_path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let done = db.with_conn(|c| articles::import_book(c, &name, hash, &stem, &book))?;
    if let Some(pages) = pdf_pages {
        db.with_conn(|c| articles::set_pdf_pages(c, done.id, pages))?;
    }
    if let Some(old) = done.stale_hash {
        if let Err(e) = std::fs::remove_file(book_file(root, &old)) {
            tracing::warn!(error = %e, "删除旧的电子书副本失败");
        }
        let _ = std::fs::remove_file(pdf_file(root, &old));
    }
    Ok(done.id)
}

/// 导入 EPUB 或 Kindle 书（Kindle 的转成 EPUB）
fn import_book_at(db: &Db, root: &Path, p: &Path) -> Result<i64> {
    if std::fs::metadata(p)?.len() > MAX_BOOK {
        return Err(Error::Other(crate::i18n::t("文件超过 500 MB", "The file is over 500 MB").into()));
    }
    let hash = epub::hash_file(p)?;
    if is_kindle(p) {
        store_book(db, root, p, &hash, |dest| crate::epub_write::write_epub(dest, &crate::mobi::read(p)?), None)
    } else {
        store_book(db, root, p, &hash, |dest| Ok(std::fs::copy(p, dest).map(|_| ())?), None)
    }
}

fn import_book(state: &AppState, p: &Path) -> Result<i64> {
    import_book_at(&state.db, &state.root, p)
}

fn is_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// 前端用 pdf.js 重排好的 PDF 书
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfBook {
    pub title: Option<String>,
    pub author: Option<String>,
    pub chapters: Vec<PdfChapterIn>,
    pub toc: Vec<PdfTocIn>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfChapterIn {
    pub title: String,
    pub html: String,
    /// 这一章从原 PDF 第几页开始（从 1 起）
    pub page: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfTocIn {
    pub title: String,
    pub level: i64,
    pub chapter: usize,
}

/// 重排结果写成 EPUB 入库，原 PDF 另存一份（原版页面用）
fn pdf_import_at(db: &Db, root: &Path, src: &Path, book: &PdfBook) -> Result<i64> {
    use crate::epub_write::{write_epub, BookSource, SourceChapter, SourceToc};
    if book.chapters.is_empty() {
        return Err(Error::Other(crate::i18n::t("这本书里没有可读的内容", "This book has no readable content").into()));
    }
    let hash = epub::hash_file(src)?;
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned());
    let source = BookSource {
        title: book.title.clone().filter(|t| !t.trim().is_empty()).or(stem),
        author: book.author.clone(),
        chapters: book.chapters.iter().map(|c| SourceChapter { title: c.title.clone(), body: c.html.clone() }).collect(),
        toc: book.toc.iter().map(|t| SourceToc { title: t.title.clone(), level: t.level, chapter: t.chapter, anchor: None }).collect(),
        ..Default::default()
    };
    let pages: Vec<i64> = book.chapters.iter().map(|c| c.page).collect();
    let id = store_book(db, root, src, &hash, |dest| write_epub(dest, &source), Some(&pages))?;
    let pdf = pdf_file(root, &hash);
    if !pdf.is_file() {
        let tmp = pdf.with_extension("pdftmp");
        std::fs::copy(src, &tmp)?;
        std::fs::rename(&tmp, &pdf)?;
    }
    Ok(id)
}

/// 读整个 PDF 给前端的 pdf.js（原始字节，不经 JSON）
#[tauri::command]
pub async fn pdf_read(path: PathBuf) -> Result<tauri::ipc::Response> {
    if std::fs::metadata(&path)?.len() > MAX_BOOK {
        return Err(Error::Other(crate::i18n::t("文件超过 500 MB", "The file is over 500 MB").into()));
    }
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(path)).await.map_err(|e| Error::Other(e.to_string()))??;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
pub async fn pdf_import(state: State<'_, AppState>, path: PathBuf, book: PdfBook) -> Result<i64> {
    pdf_import_at(&state.db, &state.root, &path, &book)
}

/// 这本 PDF 书的原件在哪
fn pdf_path_at(db: &Db, root: &Path, id: i64) -> Result<PathBuf> {
    let hash = db.with_conn(|c| articles::book_hash(c, id))?;
    hash.map(|h| pdf_file(root, &h))
        .filter(|p| p.is_file())
        .ok_or_else(|| Error::Other(crate::i18n::t("这本书没有 PDF 原件", "This book has no original PDF").into()))
}

#[cfg(test)]
fn read_pdf_at(db: &Db, root: &Path, id: i64) -> Result<Vec<u8>> {
    Ok(std::fs::read(pdf_path_at(db, root, id)?)?)
}

fn pdf_state_at(db: &Db, id: i64, view: &str, page: i64) -> Result<()> {
    if view != "reflow" && view != "pages" {
        return Err(Error::Other("view".into()));
    }
    db.with_conn(|c| articles::set_pdf_state(c, id, view, page.max(0)))
}

/// 原版页面要显示的 PDF（原始字节）
#[tauri::command]
pub async fn book_pdf(state: State<'_, AppState>, id: i64) -> Result<tauri::ipc::Response> {
    let path = pdf_path_at(&state.db, &state.root, id)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(path)).await.map_err(|e| Error::Other(e.to_string()))??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// 记住 PDF 书上次用的视图（reflow / pages）和原版页面看到第几页
#[tauri::command]
pub async fn book_pdf_state(state: State<'_, AppState>, id: i64, view: String, page: i64) -> Result<()> {
    pdf_state_at(&state.db, id, &view, page)
}

fn words_exist_in(words: &[String]) -> Vec<bool> {
    let f = crate::freq::get();
    words.iter().map(|w| f.rank(&w.to_lowercase()).is_some()).collect()
}

/// 这些词在不在词频表里（PDF 重排时判断行尾连字符要不要去掉）
#[tauri::command]
pub fn words_exist(words: Vec<String>) -> Vec<bool> {
    words_exist_in(&words)
}

/// 读一个文件并导入（同一个文件导入过就沿用或更新那篇，见 `articles::import_file`）
fn import_one(state: &AppState, p: &Path) -> std::result::Result<i64, String> {
    if !is_article_file(p) {
        return Err(crate::i18n::t("只支持 .md、.txt、.epub、.mobi 和 .azw3 文件", "Only .md, .txt, .epub, .mobi and .azw3 files are supported").into());
    }
    if is_epub(p) || is_kindle(p) {
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

/// 同上，PDF：要在界面里用 pdf.js 重排，只记下路径
#[derive(Default)]
pub struct PendingPdfs(Mutex<Vec<String>>);

/// 启动参数或第二次启动转过来的参数里有 .md / .txt / .epub / .mobi / .azw3：导入、记下等界面来取，并通知主窗口切到阅读
pub fn open_from_args(app: &AppHandle, args: &[String]) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let mut last = None;
    let mut pdfs = Vec::new();
    for a in args {
        let p = Path::new(a);
        if is_pdf(p) && p.is_file() {
            pdfs.push(a.clone());
            continue;
        }
        if !is_article_file(p) || !p.is_file() {
            continue;
        }
        match import_one(&state, p) {
            Ok(id) => last = Some(id),
            Err(e) => tracing::warn!(path = %p.display(), error = %e, "打开文章失败"),
        }
    }
    if !pdfs.is_empty() {
        if let Some(pending) = app.try_state::<PendingPdfs>() {
            if let Ok(mut g) = pending.0.lock() {
                g.extend(pdfs);
            }
        }
        let _ = app.emit_to("main", "reader:open", ());
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

/// 界面取走等着导入的 PDF 路径
#[tauri::command]
pub fn article_take_pending_pdfs(pending: State<'_, PendingPdfs>) -> Vec<String> {
    pending.0.lock().map(|mut g| std::mem::take(&mut *g)).unwrap_or_default()
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

/// 删掉一篇文章；电子书连同书库里的副本（EPUB，PDF 书还有原件）一起删
fn delete_at(db: &Db, root: &Path, id: i64) -> Result<bool> {
    let hash = db.with_conn(|c| articles::book_hash(c, id))?;
    let deleted = db.with_conn(|c| articles::delete(c, id))?;
    if let (true, Some(hash)) = (deleted, hash) {
        if let Err(e) = std::fs::remove_file(book_file(root, &hash)) {
            tracing::warn!(error = %e, "删除电子书副本失败");
        }
        let pdf = pdf_file(root, &hash);
        if pdf.exists() {
            if let Err(e) = std::fs::remove_file(&pdf) {
                tracing::warn!(error = %e, "删除 PDF 原件失败");
            }
        }
    }
    Ok(deleted)
}

#[tauri::command]
pub async fn article_delete(state: State<'_, AppState>, id: i64) -> Result<bool> {
    delete_at(&state.db, &state.root, id)
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

/// 这篇文章 / 这本书的书签
#[tauri::command]
pub async fn bookmark_list(state: State<'_, AppState>, id: i64) -> Result<Vec<articles::Bookmark>> {
    state.db.with_conn(|c| articles::bookmarks(c, id))
}

/// 在当前位置加书签：chapter = 第几章（普通文章 0），pos = 位置 0–1，label = 书签处的原文
#[tauri::command]
pub async fn bookmark_add(state: State<'_, AppState>, id: i64, chapter: i64, pos: f64, label: String) -> Result<i64> {
    if !pos.is_finite() {
        return Err(Error::Other("bookmark".into()));
    }
    state.db.with_conn(|c| articles::add_bookmark(c, id, chapter, pos, &label))
}

#[tauri::command]
pub async fn bookmark_delete(state: State<'_, AppState>, bookmark: i64) -> Result<()> {
    state.db.with_conn(|c| articles::delete_bookmark(c, bookmark))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn alice() -> PathBuf {
        crate::mobi::tests::fixture("alice-old.mobi")
    }

    #[test]
    fn imports_mobi_as_book() -> Result<()> {
        let db = Db::open_in_memory()?;
        let root = tempfile::tempdir()?;
        let id = import_book_at(&db, root.path(), &alice())?;
        let info = db.with_conn(|c| articles::book_info(c, id))?.expect("书");
        assert_eq!(info.chapters.len(), crate::mobi::read(&alice())?.chapters.len());
        assert!(info.chapters.iter().any(|c| c.title.contains("Rabbit-Hole")));
        let hash = db.with_conn(|c| articles::book_hash(c, id))?.expect("hash");
        assert_eq!(hash, epub::hash_file(&alice())?, "哈希取原始文件");
        assert!(book_file(root.path(), &hash).is_file());
        assert_eq!((info.pdf_pages, info.pdf_view), (None, None), "不是 PDF");
        Ok(())
    }

    #[test]
    fn reimport_same_mobi_keeps_one_book() -> Result<()> {
        let db = Db::open_in_memory()?;
        let root = tempfile::tempdir()?;
        let a = import_book_at(&db, root.path(), &alice())?;
        let copy = root.path().join("moved.mobi");
        std::fs::copy(alice(), &copy)?;
        let b = import_book_at(&db, root.path(), &copy)?;
        assert_eq!(a, b);
        assert_eq!(db.with_conn(articles::list)?.len(), 1);
        assert_eq!(std::fs::read_dir(root.path().join("books"))?.count(), 1);
        Ok(())
    }

    #[test]
    fn delete_removes_pdf_copy() -> Result<()> {
        let db = Db::open_in_memory()?;
        let root = tempfile::tempdir()?;
        let id = import_book_at(&db, root.path(), &alice())?;
        let hash = db.with_conn(|c| articles::book_hash(c, id))?.expect("hash");
        std::fs::write(pdf_file(root.path(), &hash), b"%PDF")?;
        assert!(delete_at(&db, root.path(), id)?);
        assert!(!book_file(root.path(), &hash).exists() && !pdf_file(root.path(), &hash).exists());
        Ok(())
    }

    fn pdf_book() -> PdfBook {
        PdfBook {
            title: Some("A Tale".into()),
            author: Some("Someone".into()),
            chapters: vec![
                PdfChapterIn { title: "One".into(), html: "<h2>One</h2><p>First &amp; best.</p>".into(), page: 1 },
                PdfChapterIn { title: "Two".into(), html: "<p>Second.</p>".into(), page: 9 },
            ],
            toc: vec![PdfTocIn { title: "One".into(), level: 0, chapter: 0 }, PdfTocIn { title: "Two".into(), level: 0, chapter: 1 }],
        }
    }

    #[test]
    fn pdf_import_stores_pages() -> Result<()> {
        let db = Db::open_in_memory()?;
        let root = tempfile::tempdir()?;
        let src = root.path().join("tale.pdf");
        std::fs::write(&src, b"%PDF-1.7 fake bytes")?;
        let id = pdf_import_at(&db, root.path(), &src, &pdf_book())?;
        let info = db.with_conn(|c| articles::book_info(c, id))?.expect("书");
        assert_eq!(info.pdf_pages, Some(vec![1, 9]));
        assert_eq!(info.chapters.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["One", "Two"]);
        let hash = epub::hash_file(&src)?;
        assert_eq!(std::fs::read(pdf_file(root.path(), &hash))?, b"%PDF-1.7 fake bytes", "原件另存一份");
        let meta = db.with_conn(articles::list)?;
        assert_eq!(meta[0].title, "A Tale");
        Ok(())
    }

    #[test]
    fn pdf_import_same_hash_reuses_article() -> Result<()> {
        let db = Db::open_in_memory()?;
        let root = tempfile::tempdir()?;
        let src = root.path().join("tale.pdf");
        std::fs::write(&src, b"%PDF same")?;
        let a = pdf_import_at(&db, root.path(), &src, &pdf_book())?;
        let b = pdf_import_at(&db, root.path(), &src, &pdf_book())?;
        assert_eq!(a, b);
        let names: Vec<String> = std::fs::read_dir(root.path().join("books"))?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.len(), 2, "一份 .epub 一份 .pdf：{names:?}");
        Ok(())
    }

    #[test]
    fn pdf_view_state_is_remembered() -> Result<()> {
        let db = Db::open_in_memory()?;
        let root = tempfile::tempdir()?;
        let src = root.path().join("tale.pdf");
        std::fs::write(&src, b"%PDF state")?;
        let id = pdf_import_at(&db, root.path(), &src, &pdf_book())?;
        assert_eq!(read_pdf_at(&db, root.path(), id)?, b"%PDF state");
        pdf_state_at(&db, id, "pages", 7)?;
        let info = db.with_conn(|c| articles::book_info(c, id))?.expect("书");
        assert_eq!((info.pdf_view.as_deref(), info.pdf_page), (Some("pages"), 7));
        assert!(pdf_state_at(&db, id, "sideways", 1).is_err(), "只认 reflow / pages");
        Ok(())
    }

    #[test]
    fn words_exist_checks_word_list() {
        assert_eq!(words_exist_in(&["Transport".into(), "wellknown".into()]), vec![true, false]);
    }

    #[test]
    fn bad_kindle_file_leaves_nothing_behind() -> Result<()> {
        let db = Db::open_in_memory()?;
        let root = tempfile::tempdir()?;
        let junk = root.path().join("junk.azw3");
        std::fs::write(&junk, b"nope")?;
        assert!(import_book_at(&db, root.path(), &junk).is_err());
        let left = std::fs::read_dir(root.path().join("books")).map(|d| d.count()).unwrap_or(0);
        assert_eq!(left, 0);
        Ok(())
    }
}
