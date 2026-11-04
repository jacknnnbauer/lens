//! `article` 表：阅读里存的文章（Markdown 原文）。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::now_ms;
use crate::epub::{Book, TocItem};
use crate::error::{Error, Result};

/// 标题最长这么多字
const MAX_TITLE: usize = 120;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleMeta {
    pub id: i64,
    pub title: String,
    pub source_path: Option<String>,
    /// 英文词数
    pub words: i64,
    pub progress: f64,
    pub created_at: i64,
    pub updated_at: i64,
    pub opened_at: Option<i64>,
    /// 开头一段纯文字，列表里预览用
    pub excerpt: String,
    /// 电子书的作者
    pub author: Option<String>,
    /// 电子书的章节数；普通文章是 0
    pub chapters: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    #[serde(flatten)]
    pub meta: ArticleMeta,
    pub body: String,
}

/// 去掉 Markdown 标记，留下大致的纯文字（标题、预览、词数用；正文渲染在前端）
pub fn plain(md: &str) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in md.lines() {
        let t = line.trim();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_code = !in_code;
            continue;
        }
        if in_code || t.starts_with("<!--") || (t.starts_with('|') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))) {
            continue;
        }
        let t = t.trim_start_matches(['#', '>', ' ']).trim_start_matches(['-', '*', '+']).trim_start();
        // ![alt](url) → 去掉；[text](url) → text
        let mut s = String::new();
        let mut rest = t;
        while let Some(i) = rest.find(['[', '!']) {
            s.push_str(&rest[..i]);
            let tail = &rest[i..];
            let img = tail.starts_with("![");
            let open = if img { 1 } else { 0 };
            match (tail[open..].starts_with('['), tail.find("](")) {
                (true, Some(close)) if tail[close..].contains(')') => {
                    let end = close + tail[close..].find(')').unwrap_or(0);
                    if !img {
                        s.push_str(&tail[open + 1..close]);
                    }
                    rest = &tail[end + 1..];
                }
                _ => {
                    s.push_str(&tail[..1]);
                    rest = &tail[1..];
                }
            }
        }
        s.push_str(rest);
        let s: String = s.chars().filter(|c| !matches!(c, '*' | '_' | '`')).collect();
        if !s.trim().is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(s.trim());
        }
    }
    out
}

/// 英文词数（连续字母算一个词）
pub fn count_words(text: &str) -> i64 {
    text.split(|c: char| !c.is_ascii_alphabetic() && c != '\'' && c != '’').filter(|w| w.chars().any(|c| c.is_ascii_alphabetic())).count() as i64
}

/// 标题：第一个 `# 标题`；没有就取第一行文字；再没有就用文件名 / 「未命名」
pub fn guess_title(md: &str, fallback: Option<&str>) -> String {
    let heading = md.lines().map(str::trim).find(|l| l.starts_with("# ")).map(|l| l.trim_start_matches('#').trim().to_string());
    let first = || plain(md).lines().next().map(str::to_string);
    let t = heading
        .filter(|h| !h.is_empty())
        .or_else(|| fallback.map(str::to_string).filter(|f| !f.trim().is_empty()))
        .or_else(first)
        .unwrap_or_else(|| crate::i18n::t("未命名文章", "Untitled").to_string());
    let t = t.trim();
    if t.chars().count() > MAX_TITLE {
        format!("{}…", t.chars().take(MAX_TITLE).collect::<String>())
    } else {
        t.to_string()
    }
}

/// 预览：去掉和标题重复的第一行，取前 160 字
fn excerpt(md: &str, title: &str) -> String {
    let p = plain(md);
    let mut lines = p.lines().peekable();
    if lines.peek().is_some_and(|l| l.trim() == title.trim()) {
        lines.next();
    }
    lines.collect::<Vec<_>>().join(" ").chars().take(160).collect()
}

fn meta_of(r: &rusqlite::Row) -> rusqlite::Result<(ArticleMeta, String)> {
    let body: String = r.get(9)?;
    Ok((
        ArticleMeta {
            id: r.get(0)?,
            title: r.get(1)?,
            source_path: r.get(2)?,
            words: r.get(3)?,
            progress: r.get(4)?,
            created_at: r.get(5)?,
            updated_at: r.get(6)?,
            opened_at: r.get(7)?,
            excerpt: String::new(),
            author: r.get(10)?,
            chapters: r.get(11)?,
        },
        body,
    ))
}

const COLS: &str = "id, title, source_path, words, progress, created_at, updated_at, opened_at, 0, body, author, (SELECT COUNT(*) FROM article_chapter WHERE article_id = article.id)";

/// 最近打开（或新建）的在前
pub fn list(conn: &Connection) -> Result<Vec<ArticleMeta>> {
    let mut st = conn.prepare(&format!("SELECT {COLS} FROM article ORDER BY COALESCE(opened_at, created_at) DESC, id DESC"))?;
    let rows = st.query_map([], meta_of)?;
    let mut out = Vec::new();
    for r in rows {
        let (mut m, body) = r?;
        m.excerpt = excerpt(&body, &m.title);
        out.push(m);
    }
    Ok(out)
}

/// 取一篇并记下打开时间
pub fn open(conn: &Connection, id: i64) -> Result<Option<Article>> {
    conn.execute("UPDATE article SET opened_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
    let row = conn.query_row(&format!("SELECT {COLS} FROM article WHERE id = ?1"), params![id], meta_of).optional()?;
    Ok(row.map(|(mut meta, body)| {
        meta.excerpt = excerpt(&body, &meta.title);
        Article { meta, body }
    }))
}

pub fn create(conn: &Connection, title: Option<&str>, body: &str, source_path: Option<&str>) -> Result<i64> {
    if body.trim().is_empty() {
        return Err(Error::Other(crate::i18n::t("文章是空的", "The article is empty").into()));
    }
    let file_stem = source_path.and_then(|p| std::path::Path::new(p).file_stem()).map(|s| s.to_string_lossy().into_owned());
    let title = match title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => t.to_string(),
        None => guess_title(body, file_stem.as_deref()),
    };
    let now = now_ms();
    conn.execute(
        "INSERT INTO article (title, body, source_path, words, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![title, body, source_path, count_words(&plain(body)), now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn rename(conn: &Connection, id: i64, title: &str) -> Result<()> {
    let t = title.trim();
    if t.is_empty() {
        return Err(Error::Other(crate::i18n::t("标题不能为空", "The title can't be empty").into()));
    }
    conn.execute("UPDATE article SET title = ?2, updated_at = ?3 WHERE id = ?1", params![id, t, now_ms()])?;
    Ok(())
}

/// 改正文（只改 Lens 里存的这份，不动导入时的原文件）；词数跟着重算，标题不变
pub fn update_body(conn: &Connection, id: i64, body: &str) -> Result<i64> {
    if body.trim().is_empty() {
        return Err(Error::Other(crate::i18n::t("文章是空的", "The article is empty").into()));
    }
    let words = count_words(&plain(body));
    let n = conn.execute("UPDATE article SET body = ?2, words = ?3, edited = 1, updated_at = ?4 WHERE id = ?1", params![id, body, words, now_ms()])?;
    if n == 0 {
        return Err(Error::Other(crate::i18n::t("这篇文章已被删除", "This article has been deleted").into()));
    }
    Ok(words)
}

/// 从文件导入（打开文件、拖入、资源管理器右键「用 Lens 打开」）：同一个文件导入过就不重复加。
/// - 内容没变：直接用原来那篇；
/// - 文件改过、Lens 里没改过正文：换成文件的新内容（标题、阅读位置不变）；
/// - Lens 里也改过：另存一篇，不覆盖在 Lens 里改的内容。
pub fn import_file(conn: &Connection, path: &str, text: &str) -> Result<i64> {
    let found: Option<(i64, String, bool)> = conn
        .query_row(
            "SELECT id, body, edited FROM article WHERE source_path = ?1 ORDER BY id DESC LIMIT 1",
            params![path],
            |r| Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0)),
        )
        .optional()?;
    match found {
        Some((id, body, _)) if body == text => Ok(id),
        Some((id, _, false)) if !text.trim().is_empty() => {
            conn.execute(
                "UPDATE article SET body = ?2, words = ?3, updated_at = ?4 WHERE id = ?1",
                params![id, text, count_words(&plain(text)), now_ms()],
            )?;
            Ok(id)
        }
        _ => create(conn, None, text, Some(path)),
    }
}

pub fn set_progress(conn: &Connection, id: i64, progress: f64) -> Result<()> {
    conn.execute("UPDATE article SET progress = ?2 WHERE id = ?1", params![id, progress.clamp(0.0, 1.0)])?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM article WHERE id = ?1", params![id])? > 0)
}

// ── 电子书 ──

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterInfo {
    pub idx: i64,
    pub title: String,
    pub href: String,
    pub words: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookInfo {
    pub chapters: Vec<ChapterInfo>,
    pub toc: Vec<TocItem>,
    /// 读到第几章、这一章读到的位置（0–1）
    pub chapter: i64,
    pub chapter_pos: f64,
}

fn insert_chapters(conn: &Connection, id: i64, book: &Book) -> Result<()> {
    let mut st = conn.prepare("INSERT INTO article_chapter (article_id, idx, href, title, words) VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for (i, c) in book.chapters.iter().enumerate() {
        st.execute(params![id, i as i64, c.href, c.title, c.words])?;
    }
    Ok(())
}

fn toc_json(book: &Book) -> Result<String> {
    serde_json::to_string(&book.toc).map_err(|e| Error::Other(e.to_string()))
}

/// 这本书导入后的文章 id，以及不再用的旧文件（文件换了内容时）
pub struct ImportedBook {
    pub id: i64,
    pub stale_hash: Option<String>,
}

/// 导入电子书：同样内容的书不重复加（文件挪了位置也认得，阅读位置不变）；
/// 同一个路径的文件换了内容：原地换成新内容，标题和读到第几章不变。
pub fn import_book(conn: &Connection, path: &str, hash: &str, fallback_title: &str, book: &Book) -> Result<ImportedBook> {
    let same: Option<i64> = conn.query_row("SELECT id FROM article WHERE book_hash = ?1", params![hash], |r| r.get(0)).optional()?;
    if let Some(id) = same {
        conn.execute("UPDATE article SET source_path = ?2 WHERE id = ?1", params![id, path])?;
        return Ok(ImportedBook { id, stale_hash: None });
    }
    let total: i64 = book.chapters.iter().map(|c| c.words).sum();
    let now = now_ms();
    let replaced: Option<(i64, String)> = conn
        .query_row("SELECT id, book_hash FROM article WHERE source_path = ?1 AND book_hash IS NOT NULL ORDER BY id DESC LIMIT 1", params![path], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    if let Some((id, old)) = replaced {
        conn.execute("DELETE FROM article_chapter WHERE article_id = ?1", params![id])?;
        insert_chapters(conn, id, book)?;
        let last = book.chapters.len() as i64 - 1;
        conn.execute(
            "UPDATE article SET book_hash = ?2, body = ?3, words = ?4, toc = ?5, author = ?6, chapter = MIN(chapter, ?7), updated_at = ?8 WHERE id = ?1",
            params![id, hash, book.preview, total, toc_json(book)?, book.author, last, now],
        )?;
        return Ok(ImportedBook { id, stale_hash: Some(old) });
    }
    let title = book.title.as_deref().unwrap_or(fallback_title);
    let title = if title.chars().count() > MAX_TITLE { format!("{}…", title.chars().take(MAX_TITLE).collect::<String>()) } else { title.to_string() };
    conn.execute(
        "INSERT INTO article (title, body, source_path, words, created_at, updated_at, book_hash, author, toc) VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8)",
        params![title, book.preview, path, total, now, hash, book.author, toc_json(book)?],
    )?;
    let id = conn.last_insert_rowid();
    insert_chapters(conn, id, book)?;
    Ok(ImportedBook { id, stale_hash: None })
}

pub fn book_hash(conn: &Connection, id: i64) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT book_hash FROM article WHERE id = ?1", params![id], |r| r.get(0)).optional()?.flatten())
}

pub fn book_info(conn: &Connection, id: i64) -> Result<Option<BookInfo>> {
    let row: Option<(Option<String>, i64, f64)> = conn.query_row("SELECT toc, chapter, chapter_pos FROM article WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let Some((toc, chapter, chapter_pos)) = row else { return Ok(None) };
    let mut st = conn.prepare("SELECT idx, title, href, words FROM article_chapter WHERE article_id = ?1 ORDER BY idx")?;
    let chapters = st
        .query_map(params![id], |r| Ok(ChapterInfo { idx: r.get(0)?, title: r.get(1)?, href: r.get(2)?, words: r.get(3)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let toc = toc.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    Ok(Some(BookInfo { chapters, toc, chapter, chapter_pos }))
}

pub fn chapter_href(conn: &Connection, id: i64, idx: i64) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT href FROM article_chapter WHERE article_id = ?1 AND idx = ?2", params![id, idx], |r| r.get(0)).optional()?)
}

/// 记下读到哪一章的哪里；全书进度按词数折算（列表里显示）
pub fn set_book_progress(conn: &Connection, id: i64, chapter: i64, pos: f64) -> Result<()> {
    let pos = pos.clamp(0.0, 1.0);
    let (before, here, total): (i64, i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(CASE WHEN idx < ?2 THEN words END), 0), COALESCE(SUM(CASE WHEN idx = ?2 THEN words END), 0), COALESCE(SUM(words), 0) FROM article_chapter WHERE article_id = ?1",
        params![id, chapter],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let overall = if total > 0 { (before as f64 + here as f64 * pos) / total as f64 } else { 0.0 };
    conn.execute("UPDATE article SET chapter = ?2, chapter_pos = ?3, progress = ?4 WHERE id = ?1", params![id, chapter, pos, overall.clamp(0.0, 1.0)])?;
    Ok(())
}

/// 读 .md / .txt：UTF-8（去 BOM）、UTF-16（有 BOM），否则按 GBK 解
pub fn decode_file(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode(bytes).0.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode(bytes).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::GBK.decode(bytes).0.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    const MD: &str = "# The Rusty Bike\n\nAn **old** bike, [rusted](https://x.com) by rain. ![pic](a.png)\n\n```\ncode block\n```\n\n- It's still *ubiquitous*.\n";

    #[test]
    fn plain_text_drops_markup() {
        assert_eq!(plain(MD), "The Rusty Bike\nAn old bike, rusted by rain.\nIt's still ubiquitous.");
        assert_eq!(count_words(&plain(MD)), 12);
    }

    #[test]
    fn titles() {
        assert_eq!(guess_title(MD, None), "The Rusty Bike");
        assert_eq!(guess_title("Just a line\n\nmore", Some("file")), "file");
        assert_eq!(guess_title("Just a line\n\nmore", None), "Just a line");
    }

    #[test]
    fn create_list_open_delete() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let a = create(c, None, MD, Some("C:\\notes\\bike.md"))?;
            let b = create(c, None, "Hello world

Second line", None)?;
            let all = list(c)?;
            assert_eq!(all.iter().map(|m| m.id).collect::<Vec<_>>(), vec![b, a], "新建的在前");
            assert_eq!(all[1].title, "The Rusty Bike");
            assert_eq!(all[1].excerpt, "An old bike, rusted by rain. It's still ubiquitous.");
            assert_eq!((all[0].title.as_str(), all[0].excerpt.as_str()), ("Hello world", "Second line"), "标题取自第一行时预览不重复");
            set_progress(c, a, 0.4)?;
            std::thread::sleep(std::time::Duration::from_millis(3));
            let art = open(c, a)?.ok_or_else(|| Error::Other("missing".into()))?;
            assert_eq!(art.body, MD);
            assert!((art.meta.progress - 0.4).abs() < 1e-9);
            assert_eq!(list_ids(c)?, vec![a, b], "刚打开的排到最前");
            rename(c, a, " Bike ")?;
            assert_eq!(update_body(c, a, "# Other

Three new words.")?, 4);
            let art = open(c, a)?.ok_or_else(|| Error::Other("missing".into()))?;
            assert_eq!((art.meta.title.as_str(), art.meta.words), ("Bike", 4), "改正文不改标题");
            assert!(update_body(c, a, " 
").is_err());
            assert!(update_body(c, 9999, "x").is_err());
            assert!(delete(c, b)?);
            assert_eq!(list(c)?.len(), 1);
            assert!(create(c, None, "  \n", None).is_err());
            Ok(())
        })
    }

    #[test]
    fn importing_the_same_file_again() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let p = "C:\\notes\\bike.md";
            let a = import_file(c, p, MD)?;
            assert_eq!(import_file(c, p, MD)?, a, "内容没变：用原来那篇");
            set_progress(c, a, 0.3)?;
            assert_eq!(import_file(c, p, "# The Rusty Bike\n\nNew text here.")?, a, "文件改过：原地更新");
            let art = open(c, a)?.ok_or_else(|| Error::Other("missing".into()))?;
            assert_eq!((art.body.as_str(), art.meta.words), ("# The Rusty Bike\n\nNew text here.", 6));
            assert!((art.meta.progress - 0.3).abs() < 1e-9, "阅读位置不变");
            update_body(c, a, "Edited in Lens.")?;
            let b = import_file(c, p, "# The Rusty Bike\n\nNewer.")?;
            assert_ne!(b, a, "Lens 里改过：另存一篇");
            assert_eq!(open(c, a)?.map(|x| x.body), Some("Edited in Lens.".to_string()));
            assert_eq!(import_file(c, p, "# The Rusty Bike\n\nNewest.")?, b, "新的那篇没改过，接着原地更新");
            Ok(())
        })
    }

    fn sample_book(words: &[i64]) -> Book {
        Book {
            title: Some("A Book".into()),
            author: Some("Writer".into()),
            chapters: words.iter().enumerate().map(|(i, &w)| crate::epub::Chapter { href: format!("c{i}.xhtml"), title: format!("Ch {i}"), words: w }).collect(),
            toc: vec![TocItem { title: "Ch 0".into(), level: 0, chapter: 0, anchor: None }],
            preview: "Once upon a time".into(),
        }
    }

    #[test]
    fn books_keep_chapters_and_progress() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let id = import_book(c, "C:\\b.epub", "h1", "b", &sample_book(&[100, 300]))?.id;
            let meta = list(c)?.remove(0);
            assert_eq!((meta.title.as_str(), meta.words, meta.chapters, meta.author.as_deref()), ("A Book", 400, 2, Some("Writer")));
            assert_eq!(meta.excerpt, "Once upon a time");
            set_book_progress(c, id, 1, 0.5)?;
            let info = book_info(c, id)?.ok_or_else(|| Error::Other("missing".into()))?;
            assert_eq!((info.chapter, info.chapters.len(), info.toc.len()), (1, 2, 1));
            assert!((info.chapter_pos - 0.5).abs() < 1e-9);
            assert!((list(c)?[0].progress - 0.625).abs() < 1e-9, "全书进度按词数折算");
            assert_eq!(chapter_href(c, id, 1)?.as_deref(), Some("c1.xhtml"));
            assert_eq!(chapter_href(c, id, 9)?, None);

            // 同样内容换了路径：还是这本，进度不变
            let again = import_book(c, "D:\\moved.epub", "h1", "b", &sample_book(&[100, 300]))?;
            assert_eq!((again.id, again.stale_hash), (id, None));
            // 同一路径换了内容：原地更新，章节少了就退到最后一章，旧文件待删
            rename(c, id, "My Name")?;
            let changed = import_book(c, "D:\\moved.epub", "h2", "b", &sample_book(&[50]))?;
            assert_eq!((changed.id, changed.stale_hash.as_deref()), (id, Some("h1")));
            let info = book_info(c, id)?.ok_or_else(|| Error::Other("missing".into()))?;
            assert_eq!((info.chapter, info.chapters.len()), (0, 1));
            assert_eq!(list(c)?[0].title, "My Name");
            // 删掉文章，章节跟着删
            assert_eq!(book_hash(c, id)?.as_deref(), Some("h2"));
            assert!(delete(c, id)?);
            let n: i64 = c.query_row("SELECT COUNT(*) FROM article_chapter", [], |r| r.get(0))?;
            assert_eq!(n, 0);
            Ok(())
        })
    }

    fn list_ids(c: &Connection) -> Result<Vec<i64>> {
        Ok(list(c)?.into_iter().map(|m| m.id).collect())
    }

    #[test]
    fn decodes_bom_and_gbk() {
        assert_eq!(decode_file(b"\xEF\xBB\xBFhi"), "hi");
        let (gbk, _, _) = encoding_rs::GBK.encode("生锈");
        assert_eq!(decode_file(&gbk), "生锈");
    }
}
