//! 词典引擎（SPEC 7）：`DictSource` trait、`DictRegistry`。

pub mod import;
pub mod index;
pub mod json;
pub mod mdx;
pub mod stardict;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::Serialize;

use crate::db::{dicts, Db};
use crate::error::{Error, Result};
use index::DictIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DictKind {
    Mdx,
    Stardict,
    Json,
    Online,
}

impl DictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mdx => "mdx",
            Self::Stardict => "stardict",
            Self::Json => "json",
            Self::Online => "online",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "mdx" => Ok(Self::Mdx),
            "stardict" => Ok(Self::Stardict),
            "json" => Ok(Self::Json),
            "online" => Ok(Self::Online),
            other => Err(Error::UnsupportedFormat(other.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictMeta {
    pub id: String,
    pub name: String,
    pub kind: DictKind,
    pub category: Option<String>,
    pub entry_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "content", rename_all = "lowercase")]
pub enum EntryBody {
    Html(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawEntry {
    pub headword: String,
    pub body: EntryBody,
}

pub trait DictSource: Send + Sync {
    fn meta(&self) -> &DictMeta;
    /// 精确查词条，返回原始 HTML（或纯文本），可能多条（同形异义）
    fn lookup(&self, key: &str) -> Result<Vec<RawEntry>>;
    /// 读取资源（mdd 里的图片/音频/CSS），路径为词典内相对路径
    fn resource(&self, path: &str) -> Result<Option<Vec<u8>>>;
}

/// 已加载的词典：数据源 + 它的 FST（前缀 / 模糊查询直接用索引，不经过数据源）。
#[derive(Clone)]
pub struct LoadedDict {
    pub source: Arc<dyn DictSource>,
    pub index: Arc<DictIndex>,
}

/// 按数据库里的一行打开词典（索引必须已建好）。
pub fn open_dict(row: &dicts::DictRow, index_dir: &Path) -> Result<LoadedDict> {
    let path = row
        .path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| Error::InvalidDict(format!("词典 {} 缺少文件路径", row.name)))?;
    let (fst_path, dup_path) = index::index_paths(index_dir, &row.id);
    let index = Arc::new(DictIndex::open(&fst_path, &dup_path)?);
    let meta = row.meta()?;
    let source: Arc<dyn DictSource> = match meta.kind {
        DictKind::Json => Arc::new(json::JsonDict::open(meta, &path, index.clone())?),
        DictKind::Stardict => {
            Arc::new(stardict::StarDict::open(meta, &path, index_dir, index.clone())?)
        }
        other => return Err(Error::UnsupportedFormat(other.as_str().to_string())),
    };
    Ok(LoadedDict { source, index })
}

pub struct DictRegistry {
    index_dir: PathBuf,
    dicts: RwLock<HashMap<String, LoadedDict>>,
}

impl DictRegistry {
    pub fn new(index_dir: PathBuf) -> Self {
        Self { index_dir, dicts: RwLock::new(HashMap::new()) }
    }

    pub fn index_dir(&self) -> &Path {
        &self.index_dir
    }

    /// 启动时加载所有索引就绪的词典；单部失败只记日志，不影响其他词典。返回成功加载的数量。
    pub fn load_all(&self, db: &Db) -> Result<usize> {
        let rows = db.with_conn(dicts::list)?;
        let mut loaded = 0;
        for row in rows.iter().filter(|r| r.index_state == dicts::IndexState::Ready) {
            match open_dict(row, &self.index_dir) {
                Ok(d) => {
                    self.insert(d)?;
                    loaded += 1;
                }
                Err(e) => tracing::warn!(dict = %row.name, error = %e, "词典加载失败，已跳过"),
            }
        }
        Ok(loaded)
    }

    pub fn insert(&self, dict: LoadedDict) -> Result<()> {
        let id = dict.source.meta().id.clone();
        self.dicts.write().map_err(|_| Error::LockPoisoned)?.insert(id, dict);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<LoadedDict>> {
        Ok(self.dicts.read().map_err(|_| Error::LockPoisoned)?.get(id).cloned())
    }

    pub fn len(&self) -> Result<usize> {
        Ok(self.dicts.read().map_err(|_| Error::LockPoisoned)?.len())
    }
}
