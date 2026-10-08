//! 全局状态：DictRegistry、OnlineRegistry、Db、Settings。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::{now_ms, Db};
use crate::dict::{import, DictRegistry};
use crate::protocol::ResourceCache;
use crate::query::Cancels;
use crate::error::Result;
use crate::online::{cache, OnlineRegistry};
use crate::settings::SettingsStore;

pub struct AppState {
    pub root: PathBuf,
    pub db: Db,
    pub settings: SettingsStore,
    pub dicts: DictRegistry,
    pub online: OnlineRegistry,
    pub res_cache: ResourceCache,
    pub lookups: Arc<Cancels>,
}

impl AppState {
    /// `root` = `%APPDATA%/Lens`
    pub fn init(root: &Path) -> Result<Self> {
        let index_dir = root.join("index");
        fs::create_dir_all(&index_dir)?;
        let db = Db::open(&root.join("lens.db"))?;
        let settings = SettingsStore::load(root.join("settings.json"))?;
        match import::sweep_orphans(&db, &index_dir) {
            Ok(0) => {}
            Ok(n) => tracing::info!(n, "已清理残留索引文件"),
            Err(e) => tracing::warn!(error = %e, "清理残留索引文件失败"),
        }
        let dicts = DictRegistry::new(index_dir);
        dicts.load_all(&db)?;
        let online = OnlineRegistry::new()?;
        online.load_all(&db)?;
        // 在线缓存保留 3 倍有效期：过期部分只用于离线回退
        let days = settings.get()?.online.cache_days;
        if days > 0 {
            match db.with_conn(|c| cache::purge_before(c, now_ms() - days * 3 * 86_400_000)) {
                Ok(0) => {}
                Ok(n) => tracing::info!(n, "已清理过期的在线缓存"),
                Err(e) => tracing::warn!(error = %e, "清理在线缓存失败"),
            }
        }
        Ok(Self {
            root: root.to_path_buf(),
            db,
            settings,
            dicts,
            online,
            res_cache: ResourceCache::default(),
            lookups: Arc::new(Cancels::default()),
        })
    }
}
