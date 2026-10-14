//! 手写迁移：按 `PRAGMA user_version` 记录已执行到的版本，每个迁移在一个事务里执行。
//! 已发布的迁移文件不得修改；新增字段写新的 `NNNN_*.sql` 并追加到 `MIGRATIONS`。

use rusqlite::Connection;

use crate::error::{Error, Result};

const MIGRATIONS: &[(i32, &str)] = &[
    (1, include_str!("../../migrations/0001_init.sql")),
    (2, include_str!("../../migrations/0002_online.sql")),
    (3, include_str!("../../migrations/0003_review.sql")),
    (4, include_str!("../../migrations/0004_ai.sql")),
];

pub fn latest_version() -> i32 {
    MIGRATIONS.last().map_or(0, |(v, _)| *v)
}

pub fn current_version(conn: &Connection) -> Result<i32> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}

pub fn apply(conn: &mut Connection) -> Result<()> {
    let current = current_version(conn)?;
    let latest = latest_version();
    if current > latest {
        return Err(Error::DbTooNew { found: current, supported: latest });
    }
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "已执行数据库迁移");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_strictly_increase_from_one() {
        for (i, (v, _)) in MIGRATIONS.iter().enumerate() {
            assert_eq!(*v, i as i32 + 1);
        }
    }

    #[test]
    fn apply_is_idempotent() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        apply(&mut conn)?;
        apply(&mut conn)?;
        assert_eq!(current_version(&conn)?, latest_version());
        let profiles: i64 = conn.query_row("SELECT COUNT(*) FROM profile", [], |r| r.get(0))?;
        assert_eq!(profiles, 3);
        Ok(())
    }

    #[test]
    fn rejects_newer_database() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "user_version", latest_version() + 1)?;
        assert!(matches!(apply(&mut conn), Err(Error::DbTooNew { .. })));
        Ok(())
    }
}
