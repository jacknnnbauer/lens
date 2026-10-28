//! `article` 表：阅读里存的文章（Markdown 原文）。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::now_ms;
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
        },
        body,
    ))
}

const COLS: &str = "id, title, source_path, words, progress, created_at, updated_at, opened_at, 0, body";

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

pub fn set_progress(conn: &Connection, id: i64, progress: f64) -> Result<()> {
    conn.execute("UPDATE article SET progress = ?2 WHERE id = ?1", params![id, progress.clamp(0.0, 1.0)])?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM article WHERE id = ?1", params![id])? > 0)
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
            assert!(delete(c, b)?);
            assert_eq!(list(c)?.len(), 1);
            assert!(create(c, None, "  \n", None).is_err());
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
