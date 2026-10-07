//! 全局状态：DictRegistry（M1）、Db、Settings。

use std::fs;
use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::error::Result;
use crate::settings::SettingsStore;

pub struct AppState {
    pub root: PathBuf,
    pub db: Db,
    pub settings: SettingsStore,
}

impl AppState {
    /// `root` = `%APPDATA%/Lens`
    pub fn init(root: &Path) -> Result<Self> {
        fs::create_dir_all(root.join("index"))?;
        let db = Db::open(&root.join("lens.db"))?;
        let settings = SettingsStore::load(root.join("settings.json"))?;
        Ok(Self { root: root.to_path_buf(), db, settings })
    }
}
