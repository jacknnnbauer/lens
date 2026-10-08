//! `batch` / `batch_item` 表（SPEC 12.3，断点续传）。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::Result;

/// 条目状态：pending | querying | rendering | done | failed | skipped
pub const PENDING: &str = "pending";
pub const QUERYING: &str = "querying";
pub const RENDERING: &str = "rendering";
pub const DONE: &str = "done";
pub const FAILED: &str = "failed";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRow {
    pub id: String,
    pub request_json: String,
    /// running | paused | cancelled | done
    pub state: String,
    pub out_dir: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRow {
    pub seq: i64,
    pub lemma: String,
    pub state: String,
    pub note: Option<String>,
    /// 生成的文件名
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub total: i64,
    pub pending: i64,
    pub querying: i64,
    pub rendering: i64,
    pub done: i64,
    pub failed: i64,
    /// 完成但有提示（某部词典失败、文件名被规范…）
    pub noted: i64,
    pub files: i64,
}

pub fn create(conn: &Connection, id: &str, request_json: &str, out_dir: &str, words: &[String], now: i64) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO batch (id, request_json, state, out_dir, created_at, updated_at) VALUES (?1, ?2, 'running', ?3, ?4, ?4)",
        params![id, request_json, out_dir, now],
    )?;
    {
        let mut stmt = tx.prepare("INSERT INTO batch_item (batch_id, seq, lemma, state) VALUES (?1, ?2, ?3, 'pending')")?;
        for (i, w) in words.iter().enumerate() {
            stmt.execute(params![id, i as i64, w])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<BatchRow> {
    Ok(BatchRow {
        id: r.get(0)?,
        request_json: r.get(1)?,
        state: r.get(2)?,
        out_dir: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<BatchRow>> {
    Ok(conn
        .query_row("SELECT id, request_json, state, out_dir, created_at, updated_at FROM batch WHERE id = ?1", params![id], row)
        .optional()?)
}

/// 未完成的批次（运行中被中断的、暂停的），新的在前。
pub fn unfinished(conn: &Connection) -> Result<Vec<BatchRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, request_json, state, out_dir, created_at, updated_at FROM batch
         WHERE state IN ('running', 'paused') ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn set_state(conn: &Connection, id: &str, state: &str, now: i64) -> Result<()> {
    conn.execute("UPDATE batch SET state = ?2, updated_at = ?3 WHERE id = ?1", params![id, state, now])?;
    Ok(())
}

/// 启动或续传前：中途停下的条目（查词中 / 渲染中）退回待处理。
pub fn reset_in_flight(conn: &Connection, id: &str) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE batch_item SET state = 'pending' WHERE batch_id = ?1 AND state IN ('querying', 'rendering')",
        params![id],
    )?)
}

/// 重试失败项：退回待处理并清掉原因。
pub fn reset_failed(conn: &Connection, id: &str) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE batch_item SET state = 'pending', note = NULL WHERE batch_id = ?1 AND state = 'failed'",
        params![id],
    )?)
}

pub fn pending_items(conn: &Connection, id: &str) -> Result<Vec<(i64, String)>> {
    let mut stmt = conn.prepare("SELECT seq, lemma FROM batch_item WHERE batch_id = ?1 AND state = 'pending' ORDER BY seq")?;
    let rows = stmt.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn set_item(conn: &Connection, id: &str, seq: i64, state: &str, note: Option<&str>, files: Option<&[String]>) -> Result<()> {
    let files = files.map(serde_json::to_string).transpose()?;
    conn.execute(
        "UPDATE batch_item SET state = ?3, note = COALESCE(?4, note), files = COALESCE(?5, files) WHERE batch_id = ?1 AND seq = ?2",
        params![id, seq, state, note, files],
    )?;
    Ok(())
}

fn item_row(r: &rusqlite::Row) -> rusqlite::Result<ItemRow> {
    let files: Option<String> = r.get(4)?;
    Ok(ItemRow {
        seq: r.get(0)?,
        lemma: r.get(1)?,
        state: r.get(2)?,
        note: r.get(3)?,
        files: files.and_then(|f| serde_json::from_str(&f).ok()).unwrap_or_default(),
    })
}

/// 条目列表。filter：all | active（查词中 / 渲染中）| noted（完成但有提示）| failed
pub fn items(conn: &Connection, id: &str, filter: &str, offset: i64, limit: i64) -> Result<Vec<ItemRow>> {
    let cond = match filter {
        "active" => "AND state IN ('querying', 'rendering')",
        "noted" => "AND state = 'done' AND note IS NOT NULL",
        "failed" => "AND state = 'failed'",
        _ => "",
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT seq, lemma, state, note, files FROM batch_item WHERE batch_id = ?1 {cond} ORDER BY seq LIMIT ?2 OFFSET ?3"
    ))?;
    let rows = stmt.query_map(params![id, limit, offset], item_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn items_by_seq(conn: &Connection, id: &str, seqs: &[i64]) -> Result<Vec<ItemRow>> {
    let mut stmt = conn.prepare("SELECT seq, lemma, state, note, files FROM batch_item WHERE batch_id = ?1 AND seq = ?2")?;
    let mut out = Vec::with_capacity(seqs.len());
    for s in seqs {
        if let Some(r) = stmt.query_row(params![id, s], item_row).optional()? {
            out.push(r);
        }
    }
    Ok(out)
}

pub fn all_items(conn: &Connection, id: &str) -> Result<Vec<ItemRow>> {
    items(conn, id, "all", 0, i64::MAX)
}

pub fn counts(conn: &Connection, id: &str) -> Result<Counts> {
    let mut stmt = conn.prepare("SELECT state, note IS NOT NULL, files FROM batch_item WHERE batch_id = ?1")?;
    let mut c = Counts::default();
    let rows = stmt.query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?, r.get::<_, Option<String>>(2)?)))?;
    for row in rows {
        let (state, noted, files) = row?;
        c.total += 1;
        match state.as_str() {
            PENDING => c.pending += 1,
            QUERYING => c.querying += 1,
            RENDERING => c.rendering += 1,
            DONE => {
                c.done += 1;
                if noted {
                    c.noted += 1;
                }
            }
            FAILED => c.failed += 1,
            _ => {}
        }
        if let Some(f) = files.and_then(|f| serde_json::from_str::<Vec<String>>(&f).ok()) {
            c.files += f.len() as i64;
        }
    }
    Ok(c)
}

pub fn delete(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM batch WHERE id = ?1", params![id])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    #[test]
    fn lifecycle() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        migrations::apply(&mut conn)?;
        conn.execute_batch("PRAGMA foreign_keys = ON")?;
        let words: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        create(&conn, "B", "{}", "D:/out", &words, 1)?;
        assert_eq!(unfinished(&conn)?.len(), 1);
        set_item(&conn, "B", 0, DONE, None, Some(&["a.pdf".to_string()]))?;
        set_item(&conn, "B", 1, QUERYING, None, None)?;
        set_item(&conn, "B", 2, FAILED, Some("无结果"), None)?;
        let c = counts(&conn, "B")?;
        assert_eq!((c.total, c.done, c.querying, c.failed, c.files), (3, 1, 1, 1, 1));
        assert_eq!(reset_in_flight(&conn, "B")?, 1);
        assert_eq!(pending_items(&conn, "B")?, vec![(1, "b".to_string())]);
        assert_eq!(items(&conn, "B", "failed", 0, 10)?[0].note.as_deref(), Some("无结果"));
        assert_eq!(reset_failed(&conn, "B")?, 1);
        assert_eq!(pending_items(&conn, "B")?.len(), 2);
        set_state(&conn, "B", "done", 2)?;
        assert!(unfinished(&conn)?.is_empty());
        assert!(delete(&conn, "B")?);
        let left: i64 = conn.query_row("SELECT COUNT(*) FROM batch_item", [], |r| r.get(0))?;
        assert_eq!(left, 0, "条目随批次级联删除");
        Ok(())
    }
}
