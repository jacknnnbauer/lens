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

/// 升级前留的备份最多几份
const BACKUPS_KEPT: usize = 3;

/// 要执行新的迁移时，先把数据库完整复制一份到 `backups/lens-v<旧版本>-<时间>.db`。
/// 新建的空库不备份；备份失败只记日志，不挡启动（迁移本身在事务里，失败会回滚）。
fn backup_before_upgrade(conn: &Connection, path: &Path) {
    let Ok(current) = migrations::current_version(conn) else { return };
    if current == 0 || current >= migrations::latest_version() {
        return;
    }
    let Some(dir) = path.parent().map(|p| p.join("backups")) else { return };
    let dest = dir.join(format!("lens-v{current}-{}.db", now_ms()));
    let done = std::fs::create_dir_all(&dir)
        .map_err(Error::from)
        .and_then(|()| conn.execute("VACUUM INTO ?1", [dest.to_string_lossy()]).map_err(Error::from));
    match done {
        Ok(_) => tracing::info!(from = current, to = migrations::latest_version(), file = %dest.display(), "升级数据库前已备份"),
        Err(e) => tracing::warn!(error = %e, "升级数据库前备份失败"),
    }
    prune_backups(&dir, BACKUPS_KEPT);
}

/// 只留最新的 `keep` 份升级备份
fn prune_backups(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("lens-v") && e.file_name().to_string_lossy().ends_with(".db"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, p) in files.into_iter().skip(keep) {
        if let Err(e) = std::fs::remove_file(&p) {
            tracing::warn!(file = %p.display(), error = %e, "删除旧的数据库备份失败");
        }
    }
}

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
        let conn = Connection::open(path)?;
        backup_before_upgrade(&conn, path);
        Self::init(conn)
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

#[cfg(test)]
mod backup_tests {
    use super::*;

    #[test]
    fn upgrading_an_old_database_keeps_a_backup() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("lens.db");
        let old = migrations::latest_version() - 1;
        {
            let mut c = Connection::open(&path)?;
            migrations::apply_upto(&mut c, old)?;
            c.execute("CREATE TABLE marker (x TEXT)", [])?;
            c.execute("INSERT INTO marker VALUES ('mine')", [])?;
        }
        let db = Db::open(&path)?;
        assert_eq!(db.with_conn(migrations::current_version)?, migrations::latest_version());
        let backups: Vec<_> = std::fs::read_dir(dir.path().join("backups"))?.flatten().map(|e| e.path()).collect();
        assert_eq!(backups.len(), 1);
        let name = backups[0].file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        assert!(name.starts_with(&format!("lens-v{old}-")), "{name}");
        // 备份是升级前的样子，数据完整
        let b = Connection::open(&backups[0])?;
        assert_eq!(migrations::current_version(&b)?, old);
        let x: String = b.query_row("SELECT x FROM marker", [], |r| r.get(0))?;
        assert_eq!(x, "mine");
        // 已是最新版本：再打开不再备份
        drop(db);
        Db::open(&path)?;
        assert_eq!(std::fs::read_dir(dir.path().join("backups"))?.count(), 1);
        Ok(())
    }

    #[test]
    fn new_database_is_not_backed_up() -> Result<()> {
        let dir = tempfile::tempdir()?;
        Db::open(&dir.path().join("lens.db"))?;
        assert!(!dir.path().join("backups").exists());
        Ok(())
    }

    #[test]
    fn only_the_newest_backups_are_kept() -> Result<()> {
        let dir = tempfile::tempdir()?;
        for i in 0..5 {
            std::fs::write(dir.path().join(format!("lens-v{i}-0.db")), b"x")?;
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::fs::write(dir.path().join("other.txt"), b"x")?;
        prune_backups(dir.path(), 3);
        let mut left: Vec<String> = std::fs::read_dir(dir.path())?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["lens-v2-0.db", "lens-v3-0.db", "lens-v4-0.db", "other.txt"]);
        Ok(())
    }
}
