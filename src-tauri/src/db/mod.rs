//! SQLite 存储：连接、迁移、仓储（history、words、contexts、tags…）。

pub mod articles;
pub mod batch;
pub mod dicts;
pub mod history;
pub mod migrations;
pub mod review;
pub mod words;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::{Error, Result};

/// 数据库里所有 `*_at` 字段统一用 Unix 毫秒。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        // 内存库会返回 "memory"，不当作错误
        let _mode: String =
            conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrations::apply(&mut conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.conn.lock().map_err(|_| Error::LockPoisoned)?;
        f(&conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_db_uses_wal_and_foreign_keys() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = Db::open(&dir.path().join("lens.db"))?;
        db.with_conn(|c| {
            let mode: String = c.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
            assert_eq!(mode.to_lowercase(), "wal");
            let fk: i64 = c.pragma_query_value(None, "foreign_keys", |r| r.get(0))?;
            assert_eq!(fk, 1);
            Ok(())
        })
    }

    #[test]
    fn cascade_delete_word_contexts() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            c.execute_batch(
                "INSERT INTO word (id, lemma, created_at) VALUES (1, 'run', 0);
                 INSERT INTO context (id, lemma, surface, sentence, hl_start, hl_end, method, created_at)
                   VALUES (1, 'run', 'ran', 'She ran home.', 4, 7, 'manual', 0);
                 INSERT INTO word_context VALUES (1, 1);
                 DELETE FROM word WHERE id = 1;",
            )?;
            let n: i64 = c.query_row("SELECT COUNT(*) FROM word_context", [], |r| r.get(0))?;
            assert_eq!(n, 0);
            Ok(())
        })
    }

    #[test]
    fn fts5_trigram_matches_chinese_substring() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO gloss_fts (dict_id, headword, text) VALUES ('d1', 'ubiquitous', '无处不在的；普遍存在的')",
                [],
            )?;
            let hw: String = c.query_row(
                "SELECT headword FROM gloss_fts WHERE gloss_fts MATCH '普遍存在'",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(hw, "ubiquitous");
            Ok(())
        })
    }
}
