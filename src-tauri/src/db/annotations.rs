//! 阅读批注：增删改查。定位信息（原文 + 前后文）由前端算，这里只存。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const COLORS: [&str; 4] = ["yellow", "green", "blue", "pink"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotation {
    pub id: i64,
    pub article_id: i64,
    pub chapter: i64,
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub color: String,
    pub note: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewAnnotation {
    pub article_id: i64,
    pub chapter: i64,
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub color: String,
    pub note: Option<String>,
    /// 撤销删除时带回原来的时间
    pub created_at: Option<i64>,
}

const COLS: &str = "id, article_id, chapter, quote, prefix, suffix, color, note, created_at, updated_at";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Annotation> {
    Ok(Annotation {
        id: r.get(0)?,
        article_id: r.get(1)?,
        chapter: r.get(2)?,
        quote: r.get(3)?,
        prefix: r.get(4)?,
        suffix: r.get(5)?,
        color: r.get(6)?,
        note: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

fn check_color(c: &str) -> Result<()> {
    if COLORS.contains(&c) {
        Ok(())
    } else {
        Err(Error::Other(format!("unknown color {c}")))
    }
}

/// 空白笔记存 NULL
fn clean_note(n: Option<&str>) -> Option<String> {
    n.map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

fn get(conn: &Connection, id: i64) -> Result<Option<Annotation>> {
    Ok(conn.query_row(&format!("SELECT {COLS} FROM annotation WHERE id = ?1"), params![id], row).optional()?)
}

pub fn list(conn: &Connection, article_id: i64) -> Result<Vec<Annotation>> {
    let mut st = conn.prepare(&format!("SELECT {COLS} FROM annotation WHERE article_id = ?1 ORDER BY chapter, created_at, id"))?;
    let rows = st.query_map(params![article_id], row)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn add(conn: &Connection, a: &NewAnnotation) -> Result<Annotation> {
    check_color(&a.color)?;
    let now = now_ms();
    let created = a.created_at.unwrap_or(now);
    conn.execute(
        "INSERT INTO annotation (article_id, chapter, quote, prefix, suffix, color, note, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![a.article_id, a.chapter, a.quote, a.prefix, a.suffix, a.color, clean_note(a.note.as_deref()), created, now],
    )?;
    get(conn, conn.last_insert_rowid())?.ok_or_else(|| Error::Other("annotation".into()))
}

/// `color` / `note` 为 None 表示不改；`note: Some(None)` 表示清掉笔记
pub fn update(conn: &Connection, id: i64, color: Option<&str>, note: Option<Option<&str>>) -> Result<Annotation> {
    if let Some(c) = color {
        check_color(c)?;
        conn.execute("UPDATE annotation SET color = ?2, updated_at = ?3 WHERE id = ?1", params![id, c, now_ms()])?;
    }
    if let Some(n) = note {
        conn.execute("UPDATE annotation SET note = ?2, updated_at = ?3 WHERE id = ?1", params![id, clean_note(n), now_ms()])?;
    }
    get(conn, id)?.ok_or_else(|| Error::Other(crate::i18n::t("这条批注已被删除", "This annotation has been deleted").into()))
}

/// 删掉，返回删掉的那条（给撤销用）
pub fn delete(conn: &Connection, id: i64) -> Result<Option<Annotation>> {
    let old = get(conn, id)?;
    conn.execute("DELETE FROM annotation WHERE id = ?1", params![id])?;
    Ok(old)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn new(article_id: i64, chapter: i64, quote: &str) -> NewAnnotation {
        NewAnnotation { article_id, chapter, quote: quote.into(), prefix: "before ".into(), suffix: " after".into(), color: "yellow".into(), note: None, created_at: None }
    }

    fn article(conn: &Connection) -> Result<i64> {
        conn.execute("INSERT INTO article (title, body, words, created_at, updated_at) VALUES ('t', 'b', 1, 0, 0)", [])?;
        Ok(conn.last_insert_rowid())
    }

    #[test]
    fn add_list_update_delete() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let id = article(c)?;
            let b = add(c, &new(id, 2, "second"))?;
            let a = add(c, &new(id, 1, "first"))?;
            assert_eq!(list(c, id)?.iter().map(|x| x.quote.as_str()).collect::<Vec<_>>(), ["first", "second"], "按章节排");
            let u = update(c, a.id, Some("green"), Some(Some("my note")))?;
            assert_eq!((u.color.as_str(), u.note.as_deref()), ("green", Some("my note")));
            let u = update(c, a.id, None, Some(None))?;
            assert_eq!((u.color.as_str(), u.note), ("green", None), "只清笔记，颜色不变");
            assert_eq!(delete(c, b.id)?.map(|x| x.quote), Some("second".into()));
            assert_eq!(delete(c, b.id)?, None);
            assert_eq!(list(c, id)?.len(), 1);
            Ok(())
        })
    }

    #[test]
    fn rejects_unknown_color() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let id = article(c)?;
            let mut n = new(id, 0, "x");
            n.color = "red".into();
            assert!(add(c, &n).is_err());
            let a = add(c, &new(id, 0, "x"))?;
            assert!(update(c, a.id, Some("purple"), None).is_err());
            Ok(())
        })
    }

    #[test]
    fn blank_note_is_null() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let id = article(c)?;
            let mut n = new(id, 0, "x");
            n.note = Some("   ".into());
            assert_eq!(add(c, &n)?.note, None);
            let a = add(c, &new(id, 0, "y"))?;
            assert_eq!(update(c, a.id, None, Some(Some(" \n ")))?.note, None);
            Ok(())
        })
    }

    #[test]
    fn restore_keeps_created_at() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let id = article(c)?;
            let mut n = new(id, 0, "x");
            n.created_at = Some(1_700_000_000_000);
            n.note = Some("kept".into());
            let a = add(c, &n)?;
            assert_eq!((a.created_at, a.note.as_deref()), (1_700_000_000_000, Some("kept")));
            Ok(())
        })
    }

    #[test]
    fn deleting_article_deletes_annotations() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let id = article(c)?;
            add(c, &new(id, 0, "x"))?;
            crate::db::articles::delete(c, id)?;
            assert_eq!(c.query_row("SELECT COUNT(*) FROM annotation", [], |r| r.get::<_, i64>(0))?, 0);
            Ok(())
        })
    }
}
