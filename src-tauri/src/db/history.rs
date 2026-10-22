//! `history` / `context` 表的仓储（SPEC 6、M4）。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::now_ms;
use crate::error::Result;

/// 同一来源在这段时间内重复查同一个词，只更新时间、不新增一行（避免防抖中间态和反复查询刷屏）
pub const DEDUPE_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct NewContext<'a> {
    pub lemma: &'a str,
    pub surface: &'a str,
    pub sentence: &'a str,
    pub hl_start: i64,
    pub hl_end: i64,
    pub app: Option<&'a str>,
    pub window_title: Option<&'a str>,
    pub url: Option<&'a str>,
    pub method: &'a str,
}

pub fn insert_context(conn: &Connection, c: &NewContext) -> Result<i64> {
    conn.execute(
        "INSERT INTO context (lemma, surface, sentence, hl_start, hl_end, app, window_title, url, method, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![c.lemma, c.surface, c.sentence, c.hl_start, c.hl_end, c.app, c.window_title, c.url, c.method, now_ms()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 浏览器 URL 在弹窗显示后才取到，补写进已存的语境。
pub fn set_context_url(conn: &Connection, id: i64, url: &str) -> Result<()> {
    conn.execute("UPDATE context SET url = ?2 WHERE id = ?1 AND url IS NULL", params![id, url])?;
    Ok(())
}

/// 弹窗里点了「+ 前句 / 后句 +」：把扩展后的句子写回这条语境（历史、生词本都用它）。
pub fn set_context_sentence(conn: &Connection, id: i64, sentence: &str, hl_start: i64, hl_end: i64) -> Result<()> {
    conn.execute(
        "UPDATE context SET sentence = ?2, hl_start = ?3, hl_end = ?4 WHERE id = ?1",
        params![id, sentence, hl_start, hl_end],
    )?;
    Ok(())
}

/// 记一条历史；同一来源 DEDUPE_MS 内的同一个词合并为一条（更新时间，补上语境）。返回历史 id。
pub fn record(conn: &Connection, query: &str, lemma: &str, source: &str, context_id: Option<i64>, now: i64) -> Result<i64> {
    let recent: Option<i64> = conn
        .query_row(
            "SELECT id FROM history WHERE lemma = ?1 AND source = ?2 AND created_at >= ?3
             ORDER BY created_at DESC LIMIT 1",
            params![lemma, source, now - DEDUPE_MS],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = recent {
        conn.execute(
            "UPDATE history SET created_at = ?2, query = ?3, context_id = COALESCE(?4, context_id) WHERE id = ?1",
            params![id, now, query, context_id],
        )?;
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO history (query, lemma, source, context_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![query, lemma, source, context_id, now],
    )?;
    Ok(conn.last_insert_rowid())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRow {
    pub id: i64,
    pub sentence: String,
    pub hl_start: i64,
    pub hl_end: i64,
    pub app: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
    /// 捕获这句的时间（毫秒），导出时写在来源后面
    pub captured_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: i64,
    pub query: String,
    pub lemma: String,
    pub source: String,
    pub created_at: i64,
    pub context: Option<ContextRow>,
    /// 这个词一共查过几次（所有来源）
    pub lookups: i64,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryFilter {
    /// [from, to) 毫秒；None 表示不限
    pub from: Option<i64>,
    pub to: Option<i64>,
    /// 按词头或原文模糊匹配
    pub search: Option<String>,
    pub limit: i64,
    pub offset: i64,
}

pub fn list(conn: &Connection, f: &HistoryFilter) -> Result<Vec<HistoryItem>> {
    let like = f.search.as_deref().map(|s| format!("%{}%", s.trim().to_lowercase().replace('%', "\\%").replace('_', "\\_")));
    let mut stmt = conn.prepare(
        "SELECT h.id, h.query, h.lemma, h.source, h.created_at,
                c.id, c.sentence, c.hl_start, c.hl_end, c.app, c.window_title, c.url,
                (SELECT COUNT(*) FROM history h2 WHERE h2.lemma = h.lemma), c.created_at
         FROM history h LEFT JOIN context c ON c.id = h.context_id
         WHERE (?1 IS NULL OR h.created_at >= ?1)
           AND (?2 IS NULL OR h.created_at < ?2)
           AND (?3 IS NULL OR h.lemma LIKE ?3 ESCAPE '\\' OR lower(h.query) LIKE ?3 ESCAPE '\\')
         ORDER BY h.created_at DESC, h.id DESC
         LIMIT ?4 OFFSET ?5",
    )?;
    let rows = stmt.query_map(params![f.from, f.to, like, f.limit.clamp(1, 1000), f.offset.max(0)], |r| {
        let context = match r.get::<_, Option<i64>>(5)? {
            Some(id) => Some(ContextRow {
                id,
                sentence: r.get(6)?,
                hl_start: r.get(7)?,
                hl_end: r.get(8)?,
                app: r.get(9)?,
                window_title: r.get(10)?,
                url: r.get(11)?,
                captured_at: r.get(13)?,
            }),
            None => None,
        };
        Ok(HistoryItem {
            id: r.get(0)?,
            query: r.get(1)?,
            lemma: r.get(2)?,
            source: r.get(3)?,
            created_at: r.get(4)?,
            context,
            lookups: r.get(12)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn delete(conn: &Connection, ids: &[i64]) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut n = 0;
    for id in ids {
        n += tx.execute("DELETE FROM history WHERE id = ?1", params![id])?;
    }
    tx.commit()?;
    Ok(n)
}

/// 清空历史（可只清某个时间点之前的）。被历史引用、又没被生词本引用的语境一并删除。
pub fn clear(conn: &Connection, before: Option<i64>) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let n = tx.execute("DELETE FROM history WHERE ?1 IS NULL OR created_at < ?1", params![before])?;
    tx.execute(
        "DELETE FROM context WHERE id NOT IN (SELECT context_id FROM history WHERE context_id IS NOT NULL)
                                AND id NOT IN (SELECT context_id FROM word_context)",
        [],
    )?;
    tx.commit()?;
    Ok(n)
}

/// 某个词最近一次捕获的语境句。
/// 按 id 取一条语境句（主窗口查词页的「当前语境」用）
pub fn context_by_id(conn: &Connection, id: i64) -> Result<Option<ContextRow>> {
    Ok(conn
        .query_row(
            "SELECT id, sentence, hl_start, hl_end, app, window_title, url, created_at FROM context WHERE id = ?1",
            params![id],
            |r| {
                Ok(ContextRow {
                    id: r.get(0)?,
                    sentence: r.get(1)?,
                    hl_start: r.get(2)?,
                    hl_end: r.get(3)?,
                    app: r.get(4)?,
                    window_title: r.get(5)?,
                    url: r.get(6)?,
                    captured_at: r.get(7)?,
                })
            },
        )
        .optional()?)
}

pub fn latest_context(conn: &Connection, lemma: &str) -> Result<Option<ContextRow>> {
    Ok(conn
        .query_row(
            "SELECT id, sentence, hl_start, hl_end, app, window_title, url, created_at FROM context
             WHERE lemma = ?1 ORDER BY created_at DESC, id DESC LIMIT 1",
            params![lemma],
            |r| {
                Ok(ContextRow {
                    id: r.get(0)?,
                    sentence: r.get(1)?,
                    hl_start: r.get(2)?,
                    hl_end: r.get(3)?,
                    app: r.get(4)?,
                    window_title: r.get(5)?,
                    url: r.get(6)?,
                    captured_at: r.get(7)?,
                })
            },
        )
        .optional()?)
}

/// 某个词查过几次。
pub fn lookup_count(conn: &Connection, lemma: &str) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM history WHERE lemma = ?1", params![lemma], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn ctx<'a>(lemma: &'a str, sentence: &'a str) -> NewContext<'a> {
        NewContext {
            lemma,
            surface: lemma,
            sentence,
            hl_start: 0,
            hl_end: lemma.len() as i64,
            app: Some("msedge.exe"),
            window_title: Some("Article"),
            url: None,
            method: "uia",
        }
    }

    #[test]
    fn record_dedupes_within_window_per_source() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let a = record(c, "Ubiquitous", "ubiquitous", "main", None, 1_000)?;
            let b = record(c, "ubiquitous", "ubiquitous", "main", None, 1_000 + DEDUPE_MS - 1)?;
            assert_eq!(a, b, "窗口内合并");
            let p = record(c, "ubiquitous", "ubiquitous", "popup", None, 2_000)?;
            assert_ne!(a, p, "不同来源分开记");
            let later = record(c, "ubiquitous", "ubiquitous", "main", None, 1_000 + 2 * DEDUPE_MS + 5)?;
            assert_ne!(a, later, "超过窗口新起一条");
            assert_eq!(lookup_count(c, "ubiquitous")?, 3);
            Ok(())
        })
    }

    #[test]
    fn list_filters_joins_context_and_paginates() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let cid = insert_context(c, &ctx("ubiquitous", "They made tracking ubiquitous."))?;
            set_context_url(c, cid, "https://example.com/a")?;
            record(c, "ubiquitous", "ubiquitous", "popup", Some(cid), 10_000)?;
            record(c, "ran", "ran", "main", None, 20_000)?;
            record(c, "100%_done", "100%_done", "main", None, 30_000)?;

            let all = list(c, &HistoryFilter { limit: 50, ..Default::default() })?;
            assert_eq!(all.iter().map(|h| h.lemma.as_str()).collect::<Vec<_>>(), ["100%_done", "ran", "ubiquitous"]);
            let u = &all[2];
            let cx = u.context.as_ref().expect("context");
            assert_eq!(cx.sentence, "They made tracking ubiquitous.");
            assert_eq!(cx.url.as_deref(), Some("https://example.com/a"));
            assert_eq!(u.lookups, 1);

            assert_eq!(latest_context(c, "ubiquitous")?.map(|x| x.id), Some(cid));
            assert!(latest_context(c, "ran")?.is_none());
            let ranged = list(c, &HistoryFilter { from: Some(15_000), to: Some(25_000), limit: 50, ..Default::default() })?;
            assert_eq!(ranged.len(), 1);
            // % 和 _ 按字面匹配，不当通配符
            let s = list(c, &HistoryFilter { search: Some("%_".into()), limit: 50, ..Default::default() })?;
            assert_eq!(s.iter().map(|h| h.lemma.as_str()).collect::<Vec<_>>(), ["100%_done"]);
            let page = list(c, &HistoryFilter { limit: 1, offset: 1, ..Default::default() })?;
            assert_eq!(page[0].lemma, "ran");
            Ok(())
        })
    }

    #[test]
    fn clear_removes_orphan_contexts_but_keeps_wordbook_ones() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let keep = insert_context(c, &ctx("run", "She ran home."))?;
            let drop_ = insert_context(c, &ctx("ubiquitous", "x"))?;
            record(c, "run", "run", "popup", Some(keep), 1)?;
            record(c, "ubiquitous", "ubiquitous", "popup", Some(drop_), 2)?;
            c.execute("INSERT INTO word (id, lemma, created_at) VALUES (1, 'run', 0)", [])?;
            c.execute("INSERT INTO word_context VALUES (1, ?1)", [keep])?;

            assert_eq!(clear(c, None)?, 2);
            let left: Vec<i64> = c.prepare("SELECT id FROM context")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            assert_eq!(left, [keep]);
            Ok(())
        })
    }

    #[test]
    fn delete_specific_items() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let a = record(c, "a", "a", "main", None, 1)?;
            record(c, "b", "b", "main", None, 2)?;
            assert_eq!(delete(c, &[a, 999])?, 1);
            assert_eq!(list(c, &HistoryFilter { limit: 10, ..Default::default() })?.len(), 1);
            Ok(())
        })
    }
}
