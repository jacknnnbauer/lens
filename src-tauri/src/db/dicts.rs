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
            },
            r.get("index_state")?,
        ))
    }
}

const COLUMNS: &str =
    "id, name, kind, path, category, entry_count, index_path, index_state, created_at";

/// 插入词典，并追加到每个场景配置组的末尾（默认启用）。
pub fn insert(conn: &Connection, row: &DictRow) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        &format!("INSERT INTO dict ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"),
        params![
            row.id,
            row.name,
            row.kind,
            row.path,
            row.category,
            row.entry_count,
            row.index_path,
            row.index_state.as_str(),
            row.created_at
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
