//! 在线词典（SPEC 13）：Provider trait、注册表、缓存与限速。
//!
//! - API 模式：调用官方接口，把结构化结果渲染成和本地词典一样的 HTML 词条（可复制、可导出）。
//! - 网页模式：只给出原页面地址，由前端在独立只读窗口里打开（不抓取、不解析页面）。
//! - 查询时机：每次都查 / 本地无结果时才查 / 手动，全局默认、每部词典可覆盖。
//! - 缓存：`online_cache`，按天数过期；联网失败时回退到过期缓存并标注获取时间。

pub mod cache;
pub mod credentials;
mod freedict;
mod mw;
mod oxford;
mod web;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;
use url::Url;

use crate::db::{dicts, now_ms, Db};
use crate::dict::{DictMeta, RawEntry};
use crate::error::{Error, Result};
use crate::settings;

/// 同一部在线词典最多同时发出的请求数（简单限速，避免连按时打爆接口）
const PER_DICT_CONCURRENCY: usize = 2;
const USER_AGENT: &str = concat!("Lens/", env!("CARGO_PKG_VERSION"), " (desktop dictionary)");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OnlineMode {
    Api,
    Web,
}

/// 需要用户填写的凭据。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyKind {
    None,
    /// 单个 API Key
    Key,
    /// App ID + App Key
    IdAndKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    pub id: Option<String>,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    Always,
    LocalMiss,
    Manual,
}

impl Trigger {
    pub fn parse(s: &str) -> Self {
        match s {
            "always" => Self::Always,
            "manual" => Self::Manual,
            _ => Self::LocalMiss,
        }
    }
}

/// `dict.options`（JSON）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OnlineOptions {
    /// None = 跟随全局设置
    pub trigger: Option<Trigger>,
    /// 网页模式：含 `{word}` 的地址模板
    pub url_template: Option<String>,
    /// 牛津：production | sandbox
    pub endpoint: Option<String>,
}

impl OnlineOptions {
    pub fn parse(json: Option<&str>) -> Self {
        json.and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default()
    }
}

pub struct Ctx<'a> {
    pub client: &'a reqwest::Client,
    pub cred: Option<&'a Credential>,
}

#[async_trait]
pub trait OnlineProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn mode(&self) -> OnlineMode;
    fn key_kind(&self) -> KeyKind {
        KeyKind::None
    }
    /// API 模式：查词，返回渲染好的词条；未收录返回空
    async fn lookup(&self, ctx: &Ctx<'_>, word: &str) -> Result<Vec<RawEntry>>;
    /// 原页面地址（网页模式必有；API 模式可选，用于「在网站上查看」）
    fn page_url(&self, word: &str) -> Option<Url>;
    /// 允许经 lens:// 转发的远程音频所在主机
    fn media_hosts(&self) -> &'static [&'static str] {
        &[]
    }
}

/// 内置的在线词典目录（词典库「添加在线词典」列表）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub mode: OnlineMode,
    pub key_kind: KeyKind,
    pub category: &'static str,
    pub description: &'static str,
    /// 申请 Key 的页面
    pub key_url: Option<&'static str>,
}

pub const CATALOG: &[ProviderInfo] = &[
    ProviderInfo {
        id: freedict::ID,
        name: "Free Dictionary",
        mode: OnlineMode::Api,
        key_kind: KeyKind::None,
        category: "英英",
        description: "dictionaryapi.dev，免 Key；释义来自 Wiktionary（CC BY-SA）",
        key_url: None,
    },
    ProviderInfo {
        id: mw::ID,
        name: "Merriam-Webster",
        mode: OnlineMode::Api,
        key_kind: KeyKind::Key,
        category: "英英",
        description: "韦氏大学词典官方 API；需自行免费申请 Key，限非商业用途",
        key_url: Some("https://dictionaryapi.com/register/index"),
    },
    ProviderInfo {
        id: oxford::ID,
        name: "Oxford Dictionaries",
        mode: OnlineMode::Api,
        key_kind: KeyKind::IdAndKey,
        category: "英英",
        description: "牛津官方 API；需 App ID + App Key（付费；试用沙盒只能查 a 开头的词）",
        key_url: Some("https://developer.oxforddictionaries.com/"),
    },
    ProviderInfo {
        id: web::ID,
        name: "网页词典",
        mode: OnlineMode::Web,
        key_kind: KeyKind::None,
        category: "网页",
        description: "填一个含 {word} 的网址，在只读窗口里打开原页面；只能复制纯文本，不参与导出",
        key_url: None,
    },
];

pub fn catalog_entry(provider: &str) -> Option<&'static ProviderInfo> {
    CATALOG.iter().find(|p| p.id == provider)
}

fn make_provider(provider: &str, options: &OnlineOptions) -> Result<Arc<dyn OnlineProvider>> {
    Ok(match provider {
        freedict::ID => Arc::new(freedict::FreeDictionary),
        mw::ID => Arc::new(mw::MerriamWebster),
        oxford::ID => Arc::new(oxford::Oxford { sandbox: options.endpoint.as_deref() == Some("sandbox") }),
        web::ID => Arc::new(web::WebDict::new(options.url_template.as_deref().unwrap_or_default())?),
        other => return Err(Error::UnsupportedFormat(format!("在线词典 {other}"))),
    })
}

#[derive(Clone)]
pub struct OnlineDict {
    pub meta: DictMeta,
    pub provider_id: String,
    pub provider: Arc<dyn OnlineProvider>,
    pub options: OnlineOptions,
    permits: Arc<Semaphore>,
}

impl OnlineDict {
    pub fn from_row(row: &dicts::DictRow) -> Result<Self> {
        let provider_id =
            row.provider.clone().ok_or_else(|| Error::InvalidDict(format!("在线词典 {} 缺少 provider", row.name)))?;
        let options = OnlineOptions::parse(row.options.as_deref());
        Ok(Self {
            meta: row.meta()?,
            provider: make_provider(&provider_id, &options)?,
            provider_id,
            options,
            permits: Arc::new(Semaphore::new(PER_DICT_CONCURRENCY)),
        })
    }

    pub fn id(&self) -> &str {
        &self.meta.id
    }

    pub fn trigger(&self, global: &settings::Online) -> Trigger {
        self.options.trigger.unwrap_or_else(|| Trigger::parse(&global.trigger))
    }
}

pub struct OnlineRegistry {
    dicts: RwLock<HashMap<String, OnlineDict>>,
    client: reqwest::Client,
}

impl OnlineRegistry {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| Error::Online(format!("无法初始化网络客户端：{e}")))?;
        Ok(Self { dicts: RwLock::new(HashMap::new()), client })
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// 启动时加载所有在线词典；单部失败只记日志。
    pub fn load_all(&self, db: &Db) -> Result<usize> {
        let rows = db.with_conn(dicts::list)?;
        let mut n = 0;
        for row in rows.iter().filter(|r| r.kind == "online") {
            match OnlineDict::from_row(row) {
                Ok(d) => {
                    self.insert(d)?;
                    n += 1;
                }
                Err(e) => tracing::warn!(dict = %row.name, error = %e, "在线词典加载失败，已跳过"),
            }
        }
        Ok(n)
    }

    pub fn insert(&self, dict: OnlineDict) -> Result<()> {
        self.dicts.write().map_err(|_| Error::LockPoisoned)?.insert(dict.meta.id.clone(), dict);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<OnlineDict>> {
        Ok(self.dicts.read().map_err(|_| Error::LockPoisoned)?.get(id).cloned())
    }

    pub fn remove(&self, id: &str) -> Result<Option<OnlineDict>> {
        Ok(self.dicts.write().map_err(|_| Error::LockPoisoned)?.remove(id))
    }

    pub fn has_provider(&self, provider: &str) -> Result<bool> {
        Ok(self.dicts.read().map_err(|_| Error::LockPoisoned)?.values().any(|d| d.provider_id == provider))
    }
}

/// 一次在线查询的结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// hit | miss | error
    pub status: &'static str,
    pub entries: Vec<RawEntry>,
    pub error: Option<String>,
    /// 结果来自缓存时的获取时间（毫秒时间戳）
    pub cached_at: Option<i64>,
    /// 缓存已过期，是联网失败后的回退
    pub stale: bool,
}

impl Outcome {
    fn from_entries(entries: Vec<RawEntry>, cached_at: Option<i64>, stale: bool, error: Option<String>) -> Self {
        let status = if entries.is_empty() { "miss" } else { "hit" };
        Self { status, entries, error, cached_at, stale }
    }

    fn error(msg: String) -> Self {
        Self { status: "error", entries: Vec::new(), error: Some(msg), cached_at: None, stale: false }
    }
}

fn cache_fresh(fetched_at: i64, now: i64, cache_days: i64) -> bool {
    cache_days < 0 || now - fetched_at < cache_days * 86_400_000
}

/// 查一部 API 模式的在线词典：新鲜缓存 → 联网（带超时）→ 失败时回退过期缓存。
pub async fn fetch(db: &Db, client: &reqwest::Client, dict: &OnlineDict, word: &str, cfg: &settings::Online) -> Outcome {
    let lemma = word.trim().to_lowercase();
    let cached = if cfg.cache_days != 0 {
        db.with_conn(|c| cache::get(c, dict.id(), &lemma)).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "读取在线缓存失败");
            None
        })
    } else {
        None
    };
    let now = now_ms();
    if let Some((entries, at)) = &cached {
        if cache_fresh(*at, now, cfg.cache_days) {
            return Outcome::from_entries(entries.clone(), Some(*at), false, None);
        }
    }

    let result = request(client, dict, word, cfg.timeout_ms).await;
    match result {
        Ok(entries) => {
            if cfg.cache_days != 0 {
                if let Err(e) = db.with_conn(|c| cache::put(c, dict.id(), &lemma, &entries, now)) {
                    tracing::warn!(error = %e, "写入在线缓存失败");
                }
            }
            Outcome::from_entries(entries, None, false, None)
        }
        Err(e) => {
            tracing::info!(dict = %dict.meta.name, provider = dict.provider.id(), error = %e, "在线查询失败");
            match cached {
                Some((entries, at)) => Outcome::from_entries(entries, Some(at), true, Some(e.to_string())),
                None => Outcome::error(e.to_string()),
            }
        }
    }
}

async fn request(client: &reqwest::Client, dict: &OnlineDict, word: &str, timeout_ms: u64) -> Result<Vec<RawEntry>> {
    let cred = match dict.provider.key_kind() {
        KeyKind::None => None,
        kind => {
            let c = credentials::read(dict.id())?
                .ok_or_else(|| Error::Online("还没有填写 API Key（词典库 › 在线词典）".into()))?;
            Some(credentials::split(&c, kind)?)
        }
    };
    let _permit = dict.permits.acquire().await.map_err(|_| Error::Online("在线词典已关闭".into()))?;
    let ctx = Ctx { client, cred: cred.as_ref() };
    match tokio::time::timeout(Duration::from_millis(timeout_ms.max(500)), dict.provider.lookup(&ctx, word)).await {
        Ok(r) => r,
        Err(_) => Err(Error::Online(format!("请求超时（{} 秒）", timeout_ms.max(500) / 1000))),
    }
}

/// 发 GET 请求取 JSON；404 视为未收录（None）。
pub(crate) async fn get_json(req: reqwest::RequestBuilder) -> Result<Option<serde_json::Value>> {
    let res = req.send().await.map_err(net_error)?;
    let status = res.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(Error::Online("API Key 无效或没有权限".into()));
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(Error::Online("请求过于频繁，已被接口限流".into()));
    }
    if !status.is_success() {
        return Err(Error::Online(format!("服务器返回 {}", status.as_u16())));
    }
    let text = res.text().await.map_err(net_error)?;
    match serde_json::from_str(&text) {
        Ok(v) => Ok(Some(v)),
        // 韦氏 Key 无效时返回 200 + 纯文本
        Err(_) if text.to_ascii_lowercase().contains("invalid api key") => Err(Error::Online("API Key 无效".into())),
        Err(e) => Err(Error::Online(format!("返回内容无法解析：{e}"))),
    }
}

fn net_error(e: reqwest::Error) -> Error {
    if e.is_timeout() {
        Error::Online("请求超时".into())
    } else if e.is_connect() {
        Error::Online("无法连接到服务器（离线或被拦截）".into())
    } else {
        Error::Online(format!("网络错误：{e}"))
    }
}

/// 远程音频：`sound://remote/<host>/<path>` 经 lens:// 转发，只允许 provider 声明的主机。
pub fn remote_media_url(dict: &OnlineDict, path: &str) -> Option<Url> {
    let rest = path.strip_prefix("remote/")?;
    let host = rest.split('/').next()?;
    if !dict.provider.media_hosts().contains(&host) {
        return None;
    }
    Url::parse(&format!("https://{rest}")).ok()
}

pub(crate) fn sound_link(url: &str) -> String {
    match url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        Some(rest) => format!(r#" <a class="audio" href="sound://remote/{}" title="发音">🔊</a>"#, esc(rest)),
        None => String::new(),
    }
}

pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// 在线词条共用的样式：颜色只用主题变量。
pub(crate) const STYLE: &str = "<style>\
.lens-online .hw{font-weight:600;font-size:1.25em}\
.lens-online .phon{margin-left:.5em;color:var(--sub)}\
.lens-online .audio{margin-left:.3em;text-decoration:none}\
.lens-online .pos{margin:.7em 0 .2em;font-style:italic;color:var(--sub)}\
.lens-online ol{margin:.2em 0;padding-left:1.5em}\
.lens-online li{margin:.25em 0}\
.lens-online .x{display:block;color:var(--sub);font-style:italic}\
.lens-online .syn{display:block;font-size:.9em;color:var(--sub)}\
.lens-online .source{margin:.9em 0 0;font-size:12px;color:var(--faint)}\
</style>";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_parse_tolerates_garbage() {
        assert_eq!(OnlineOptions::parse(None), OnlineOptions::default());
        assert_eq!(OnlineOptions::parse(Some("{not json")), OnlineOptions::default());
        let o = OnlineOptions::parse(Some(r#"{"trigger":"manual","urlTemplate":"https://x/{word}"}"#));
        assert_eq!(o.trigger, Some(Trigger::Manual));
        assert_eq!(o.url_template.as_deref(), Some("https://x/{word}"));
    }

    #[test]
    fn cache_freshness() {
        let day = 86_400_000;
        assert!(cache_fresh(0, day - 1, 1));
        assert!(!cache_fresh(0, day, 1));
        assert!(cache_fresh(0, 1000 * day, -1));
    }

    #[test]
    fn sound_link_only_for_http() {
        assert!(sound_link("https://media.example.com/a.mp3").contains(r#"href="sound://remote/media.example.com/a.mp3""#));
        assert_eq!(sound_link("javascript:alert(1)"), "");
    }
}
