//! `dict` / `profile_dict` 表的仓储。

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::dict::{DictKind, DictMeta};
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexState {
    Pending,
    Building,
    Ready,
    Error,
}

impl IndexState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(Self::Pending),
            "building" => Ok(Self::Building),
            "ready" => Ok(Self::Ready),
            "error" => Ok(Self::Error),
            other => Err(Error::Other(format!("未知的索引状态：{other}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub path: Option<String>,
    pub category: Option<String>,
    pub entry_count: Option<i64>,
    pub index_path: Option<String>,
    pub index_state: IndexState,
    pub created_at: i64,
    /// 在线词典的 provider id（freedict / mw / oxford / web）
    pub provider: Option<String>,
    /// 每部词典的选项（JSON），见 migrations/0002_online.sql
    pub options: Option<String>,
}

impl DictRow {
    pub fn meta(&self) -> Result<DictMeta> {
        Ok(DictMeta {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: DictKind::parse(&self.kind)?,
            category: self.category.clone(),
            entry_count: self.entry_count.unwrap_or(0).max(0) as u64,
        })
    }

    fn from_row(r: &Row) -> rusqlite::Result<(Self, String)> {
        Ok((
            Self {
                id: r.get("id")?,
                name: r.get("name")?,
                kind: r.get("kind")?,
                path: r.get("path")?,
                category: r.get("category")?,
                entry_count: r.get("entry_count")?,
                index_path: r.get("index_path")?,
                index_state: IndexState::Pending,
                created_at: r.get("created_at")?,
                provider: r.get("provider")?,
                options: r.get("options")?,
            },
            r.get("index_state")?,
        ))
    }
}

const COLUMNS: &str =
    "id, name, kind, path, category, entry_count, index_path, index_state, created_at, provider, options";

/// 插入词典，并追加到每个场景配置组的末尾（默认启用）。
pub fn insert(conn: &Connection, row: &DictRow) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!("INSERT INTO dict ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"),
        params![
            row.id,
            row.name,
            row.kind,
            row.path,
            row.category,
            row.entry_count,
            row.index_path,
            row.index_state.as_str(),
            row.created_at,
            row.provider,
            row.options
        ],
    )?;
    tx.execute(
        "INSERT INTO profile_dict (profile_id, dict_id, sort)
         SELECT p.id, ?1, COALESCE((SELECT MAX(sort) + 1 FROM profile_dict pd WHERE pd.profile_id = p.id), 0)
         FROM profile p",
        params![row.id],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn list(conn: &Connection) -> Result<Vec<DictRow>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLUMNS} FROM dict ORDER BY created_at, rowid"))?;
    let rows = stmt.query_map([], DictRow::from_row)?;
    rows.map(|r| {
        let (mut row, state) = r?;
        row.index_state = IndexState::parse(&state)?;
        Ok(row)
    })
    .collect()
}

pub fn find_id_by_path(conn: &Connection, path: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT id FROM dict WHERE path = ?1", params![path], |r| r.get(0))
        .optional()?)
}

pub fn set_index_state(
    conn: &Connection,
    id: &str,
    state: IndexState,
    entry_count: Option<i64>,
) -> Result<()> {
    conn.execute(
        "UPDATE dict SET index_state = ?2, entry_count = COALESCE(?3, entry_count) WHERE id = ?1",
        params![id, state.as_str(), entry_count],
    )?;
    Ok(())
}

/// 场景配置组里启用的词典，按顺序。
pub fn enabled_for_profile(conn: &Connection, profile_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT dict_id FROM profile_dict WHERE profile_id = ?1 AND enabled = 1 ORDER BY sort, dict_id",
    )?;
    let ids = stmt.query_map(params![profile_id], |r| r.get(0))?;
    Ok(ids.collect::<rusqlite::Result<Vec<String>>>()?)
}

pub fn set_options(conn: &Connection, id: &str, options: &str) -> Result<()> {
    conn.execute("UPDATE dict SET options = ?2 WHERE id = ?1", params![id, options])?;
    Ok(())
}

pub fn rename(conn: &Connection, id: &str, name: &str) -> Result<()> {
    conn.execute("UPDATE dict SET name = ?2 WHERE id = ?1", params![id, name])?;
    Ok(())
}

/// 场景配置组里所有词典（含停用的）的先后顺序。
pub fn profile_order(conn: &Connection, profile_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT dict_id FROM profile_dict WHERE profile_id = ?1 ORDER BY sort, dict_id")?;
    let ids = stmt.query_map(params![profile_id], |r| r.get(0))?;
    Ok(ids.collect::<rusqlite::Result<Vec<String>>>()?)
}

/// 调整词典顺序：按 `ids` 的先后写进每个场景配置组（查词结果、导出都按这个顺序）。
/// 不在 `ids` 里的词典排在后面，保持原有相对顺序。
pub fn reorder(conn: &Connection, ids: &[String]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let profiles: Vec<String> = {
        let mut stmt = tx.prepare("SELECT id FROM profile")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for p in profiles {
        let mut order: Vec<String> = ids.to_vec();
        for id in profile_order(&tx, &p)? {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        let mut stmt = tx.prepare("UPDATE profile_dict SET sort = ?3 WHERE profile_id = ?1 AND dict_id = ?2")?;
        for (i, id) in order.iter().enumerate() {
            stmt.execute(params![p, id, i as i64])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// 删除词典（profile_dict 级联删除）。返回是否真的删掉了一行。
pub fn delete(conn: &Connection, id: &str) -> Result<bool> {
    Ok(conn.execute("DELETE FROM dict WHERE id = ?1", params![id])? > 0)
}

/// 场景配置组及其启用的词典数。
pub fn profiles(conn: &Connection) -> Result<Vec<(String, String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name,
                (SELECT COUNT(*) FROM profile_dict pd WHERE pd.profile_id = p.id AND pd.enabled = 1)
         FROM profile p ORDER BY p.sort",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn row(id: &str, at: i64) -> DictRow {
        DictRow {
            id: id.into(),
            name: id.into(),
            kind: "json".into(),
            path: None,
            category: None,
            entry_count: None,
            index_path: None,
            index_state: IndexState::Ready,
            created_at: at,
            provider: None,
            options: None,
        }
    }

    #[test]
    fn reorder_applies_to_every_profile() -> Result<()> {
        let mut conn = Connection::open_in_memory()?;
        migrations::apply(&mut conn)?;
        for (i, id) in ["a", "b", "c"].iter().enumerate() {
            insert(&conn, &row(id, i as i64))?;
        }
        assert_eq!(profile_order(&conn, "reading")?, ["a", "b", "c"]);
        reorder(&conn, &["c".to_string(), "a".to_string()])?;
        assert_eq!(profile_order(&conn, "reading")?, ["c", "a", "b"]);
        assert_eq!(profile_order(&conn, "coding")?, ["c", "a", "b"]);
        assert_eq!(enabled_for_profile(&conn, "translate")?, ["c", "a", "b"]);
        Ok(())
    }
}
