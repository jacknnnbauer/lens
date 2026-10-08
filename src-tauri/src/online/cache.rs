//! `online_cache` 表：按（在线词典 id, 小写词）缓存渲染好的词条；未收录也缓存（空数组），避免反复请求。

use rusqlite::{params, Connection, OptionalExtension};

use crate::dict::RawEntry;
use crate::error::Result;

pub fn get(conn: &Connection, dict_id: &str, lemma: &str) -> Result<Option<(Vec<RawEntry>, i64)>> {
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT payload, fetched_at FROM online_cache WHERE provider = ?1 AND lemma = ?2",
            params![dict_id, lemma],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        // 解析不了的旧缓存当作没有
        Some((payload, at)) => Ok(serde_json::from_str(&payload).ok().map(|e| (e, at))),
        None => Ok(None),
    }
}

pub fn put(conn: &Connection, dict_id: &str, lemma: &str, entries: &[RawEntry], fetched_at: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO online_cache (provider, lemma, payload, fetched_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (provider, lemma) DO UPDATE SET payload = excluded.payload, fetched_at = excluded.fetched_at",
        params![dict_id, lemma, serde_json::to_string(entries)?, fetched_at],
    )?;
    Ok(())
}

pub fn delete_dict(conn: &Connection, dict_id: &str) -> Result<usize> {
    Ok(conn.execute("DELETE FROM online_cache WHERE provider = ?1", params![dict_id])?)
}

/// 删除早于 `before` 的缓存（启动时清理，保留一段过期缓存供离线回退）。
pub fn purge_before(conn: &Connection, before: i64) -> Result<usize> {
    Ok(conn.execute("DELETE FROM online_cache WHERE fetched_at < ?1", params![before])?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use crate::dict::EntryBody;

    #[test]
    fn roundtrip_overwrite_and_purge() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        migrations::apply(&mut conn)?;
        assert!(get(&conn, "d", "colour")?.is_none());
        let e = vec![RawEntry { headword: "colour".into(), body: EntryBody::Html("<b>x</b>".into()) }];
        put(&conn, "d", "colour", &e, 10)?;
        put(&conn, "d", "qwzx", &[], 10)?;
        assert_eq!(get(&conn, "d", "colour")?, Some((e.clone(), 10)));
        assert_eq!(get(&conn, "d", "qwzx")?, Some((Vec::new(), 10)));
        put(&conn, "d", "colour", &e, 20)?;
        assert_eq!(get(&conn, "d", "colour")?.map(|(_, at)| at), Some(20));
        assert_eq!(purge_before(&conn, 15)?, 1);
        assert_eq!(delete_dict(&conn, "d")?, 1);
        Ok(())
    }
}
