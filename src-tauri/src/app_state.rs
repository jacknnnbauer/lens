//! 全局状态：DictRegistry、OnlineRegistry、Db、Settings。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

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
    /// 全文索引（反查、正文搜索）的建索引队列
    pub fulltext: crate::commands::fulltext::Jobs,
    /// COCA 词频表（导入过才有）。启动时在后台读，读完前用 `coca()` 取会稍等
    pub coca: std::sync::RwLock<Option<Arc<crate::coca::Coca>>>,
    coca_pending: (Mutex<bool>, Condvar),
    /// 内置 WordNet：启动时在后台解压，读完前用 `wordnet()` 取会稍等
    wordnet: std::sync::RwLock<Option<Arc<crate::wordnet::WordNet>>>,
    wordnet_pending: (Mutex<bool>, Condvar),
}

impl AppState {
    /// `root` = `%APPDATA%/Lens`
    pub fn init(root: &Path) -> Result<Self> {
        let index_dir = root.join("index");
        fs::create_dir_all(&index_dir)?;
        let db = Db::open(&root.join("lens.db"))?;
        let settings = SettingsStore::load(root.join("settings.json"))?;
        crate::i18n::set_language(&settings.get()?.appearance.language);
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
        // 上次退出时还在跑的批量导出：标记为暂停，由用户决定是否继续
        if let Err(e) = db.with_conn(|c| {
            c.execute("UPDATE batch SET state = 'paused' WHERE state = 'running'", [])?;
            Ok(())
        }) {
            tracing::warn!(error = %e, "整理未完成的批量导出失败");
        }
        Ok(Self {
            coca: std::sync::RwLock::new(None),
            coca_pending: (Mutex::new(root.join(crate::coca::FILE).exists()), Condvar::new()),
            wordnet: std::sync::RwLock::new(None),
            wordnet_pending: (Mutex::new(true), Condvar::new()),
            root: root.to_path_buf(),
            db,
            settings,
            dicts,
            online,
            res_cache: ResourceCache::default(),
            lookups: Arc::new(Cancels::default()),
            fulltext: Default::default(),
        })
    }

    /// COCA 词频表：导入过就读进来（9 MB 的 JSON，放在启动后的后台线程里，不拖慢窗口出现）；读不了只记日志
    pub fn load_coca(&self) {
        let path = self.root.join(crate::coca::FILE);
        if path.exists() {
            match crate::coca::Coca::load(&path) {
                Ok(c) => {
                    tracing::info!(entries = c.len(), "已加载 COCA 词频表");
                    if let Ok(mut slot) = self.coca.write() {
                        *slot = Some(Arc::new(c));
                    }
                }
                Err(e) => tracing::warn!(error = %e, "COCA 词频表读取失败"),
            }
        }
        let (pending, done) = &self.coca_pending;
        if let Ok(mut p) = pending.lock() {
            *p = false;
        }
        done.notify_all();
    }

    /// 内置 WordNet（约 2 MB 压缩数据）：启动后在后台解压；坏了只记日志
    pub fn load_wordnet(&self) {
        match crate::wordnet::WordNet::load() {
            Ok(w) => {
                tracing::info!(words = w.len(), "已加载 WordNet");
                if let Ok(mut slot) = self.wordnet.write() {
                    *slot = Some(Arc::new(w));
                }
            }
            Err(e) => tracing::warn!(error = %e, "WordNet 读取失败"),
        }
        let (pending, done) = &self.wordnet_pending;
        if let Ok(mut p) = pending.lock() {
            *p = false;
        }
        done.notify_all();
    }

    /// 内置 WordNet；启动时还在解压就等它（最多几百毫秒）
    pub fn wordnet(&self) -> Option<Arc<crate::wordnet::WordNet>> {
        let (pending, done) = &self.wordnet_pending;
        if let Ok(p) = pending.lock() {
            let _ = done.wait_timeout_while(p, std::time::Duration::from_secs(5), |p| *p);
        }
        self.wordnet.read().ok().and_then(|w| w.clone())
    }

    /// 当前的 COCA 词频表；启动时还在读就等它读完（最多几百毫秒）
    pub fn coca(&self) -> Option<Arc<crate::coca::Coca>> {
        let (pending, done) = &self.coca_pending;
        if let Ok(p) = pending.lock() {
            let _ = done.wait_timeout_while(p, std::time::Duration::from_secs(5), |p| *p);
        }
        self.coca.read().ok().and_then(|c| c.clone())
    }
}
