//! 全局状态：DictRegistry、Db、Settings。

use std::fs;
use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::dict::DictRegistry;
use crate::error::Result;
use crate::settings::SettingsStore;

pub struct AppState {
    pub root: PathBuf,
    pub db: Db,
    pub settings: SettingsStore,
    pub dicts: DictRegistry,
}

impl AppState {
    /// `root` = `%APPDATA%/Lens`
    pub fn init(root: &Path) -> Result<Self> {
        let index_dir = root.join("index");
        fs::create_dir_all(&index_dir)?;
        let db = Db::open(&root.join("lens.db"))?;
        let settings = SettingsStore::load(root.join("settings.json"))?;
        let dicts = DictRegistry::new(index_dir);
        dicts.load_all(&db)?;
        Ok(Self { root: root.to_path_buf(), db, settings, dicts })
    }
}
