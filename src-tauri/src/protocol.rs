//! `lens://` 自定义协议（SPEC 9.3）：`lens://res/<dictId>/<词典内路径>` → 词典资源。
//! Windows 上 WebView2 实际访问的是 `http://lens.localhost/res/<dictId>/<路径>`。
//! 带 LRU 缓存，总量上限 64MB。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use percent_encoding::percent_decode_str;
use tauri::http::{header, Response, StatusCode};
use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::audio;
use crate::online;
use crate::error::{Error, Result};

pub const SCHEME: &str = "lens";
const CACHE_LIMIT: usize = 64 * 1024 * 1024;
/// 单个资源超过这个大小不进缓存（大音频、大图）
const CACHE_ITEM_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Default)]
struct CacheInner {
    items: HashMap<String, (Arc<Vec<u8>>, u64)>,
    bytes: usize,
    tick: u64,
}

#[derive(Default)]
pub struct ResourceCache(Mutex<CacheInner>);

impl ResourceCache {
    fn get(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        let mut c = self.0.lock().ok()?;
        c.tick += 1;
        let tick = c.tick;
        c.items.get_mut(key).map(|(v, t)| {
            *t = tick;
            v.clone()
        })
    }

    fn put(&self, key: String, value: Arc<Vec<u8>>) {
        if value.len() > CACHE_ITEM_LIMIT {
            return;
        }
        let Ok(mut c) = self.0.lock() else { return };
        c.tick += 1;
        let tick = c.tick;
        c.bytes += value.len();
        if let Some((old, _)) = c.items.insert(key, (value, tick)) {
            c.bytes -= old.len();
        }
        while c.bytes > CACHE_LIMIT {
            let Some(oldest) = c.items.iter().min_by_key(|(_, (_, t))| *t).map(|(k, _)| k.clone()) else { break };
            if let Some((v, _)) = c.items.remove(&oldest) {
                c.bytes -= v.len();
            }
        }
    }

    /// 词典被删除时清掉它的缓存。
    pub fn evict_dict(&self, dict_id: &str) {
        let Ok(mut c) = self.0.lock() else { return };
        let prefix = format!("{dict_id}\0");
        let keys: Vec<String> = c.items.keys().filter(|k| k.starts_with(&prefix)).cloned().collect();
        for k in keys {
            if let Some((v, _)) = c.items.remove(&k) {
                c.bytes -= v.len();
            }
        }
    }
}

fn is_spx(path: &str) -> bool {
    path.rsplit('.').next().is_some_and(|e| e.eq_ignore_ascii_case("spx"))
}

/// 响应的 MIME；.spx 已转码为 WAV。
pub fn mime_for(path: &str) -> &'static str {
    if is_spx(path) {
        return "audio/wav";
    }
    let ext = path.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "m4a" => "audio/mp4",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}

/// 解析 `/res/<dictId>/<路径>`，各段做 percent 解码。
pub fn parse_path(uri_path: &str) -> Option<(String, String)> {
    let mut parts = uri_path.trim_start_matches('/').splitn(3, '/');
    if parts.next()? != "res" {
        return None;
    }
    let decode = |s: &str| percent_decode_str(s).decode_utf8_lossy().into_owned();
    let id = decode(parts.next()?);
    let path = decode(parts.next()?);
    (!id.is_empty() && !path.is_empty()).then_some((id, path))
}

fn load(app: &AppHandle, dict_id: &str, path: &str) -> Result<Option<Arc<Vec<u8>>>> {
    let state = app.state::<AppState>();
    let key = format!("{dict_id}\0{}", path.replace('\\', "/").to_lowercase());
    if let Some(hit) = state.res_cache.get(&key) {
        return Ok(Some(hit));
    }
    let bytes = match state.dicts.get(dict_id)? {
        // vocabulary.com 的词典包不带音频：本地没有时从 audio.vocab.com 取（并存到本地，下次离线可用）
        Some(dict) => match dict.source.resource(path)? {
            Some(b) => Some(b),
            None if crate::vocab::is_audio_path(path) => crate::vocab::audio(state.online.client(), &state.root, path).unwrap_or_else(|e| {
                tracing::debug!(path, error = %e, "vocabulary.com 发音下载失败");
                None
            }),
            None => None,
        },
        None => match state.online.get(dict_id)? {
            Some(dict) => remote_media(state.online.client(), &dict, path)?,
            None => None,
        },
    };
    let Some(bytes) = bytes else { return Ok(None) };
    // WebView2 不支持 Speex，转成 WAV（缓存的是转码结果）
    let bytes = if is_spx(path) { audio::spx_to_wav(&bytes)? } else { bytes };
    let bytes = Arc::new(bytes);
    state.res_cache.put(key, bytes.clone());
    Ok(Some(bytes))
}

/// 在线词典的发音：只转发 provider 声明的主机（本函数在 spawn_blocking 线程里调用）。
fn remote_media(client: &reqwest::Client, dict: &online::OnlineDict, path: &str) -> Result<Option<Vec<u8>>> {
    let Some(url) = online::remote_media_url(dict, path) else { return Ok(None) };
    tauri::async_runtime::block_on(async {
        let res = client
            .get(url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| Error::Online(format!("下载发音失败：{e}")))?;
        if !res.status().is_success() {
            return Ok(None);
        }
        let bytes = res.bytes().await.map_err(|e| Error::Online(format!("下载发音失败：{e}")))?;
        Ok(Some(bytes.to_vec()))
    })
}

fn response(status: StatusCode, mime: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    let mut res = Response::new(body);
    *res.status_mut() = status;
    let h = res.headers_mut();
    if let Ok(v) = mime.parse() {
        h.insert(header::CONTENT_TYPE, v);
    }
    // 词典 CSS 里的字体等跨源加载需要
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, header::HeaderValue::from_static("*"));
    h.insert(header::CACHE_CONTROL, header::HeaderValue::from_static("max-age=3600"));
    res
}

/// 同步处理（调用方放在阻塞线程里）。
pub fn handle(app: &AppHandle, uri_path: &str) -> Response<Vec<u8>> {
    let Some((dict_id, path)) = parse_path(uri_path) else {
        return response(StatusCode::BAD_REQUEST, "text/plain", b"bad path".to_vec());
    };
    match load(app, &dict_id, &path) {
        Ok(Some(bytes)) => response(StatusCode::OK, mime_for(&path), bytes.as_ref().clone()),
        Ok(None) => response(StatusCode::NOT_FOUND, "text/plain", Vec::new()),
        Err(e) => {
            tracing::warn!(dict = %dict_id, path = %path, error = %e, "读取词典资源失败");
            let status = match e {
                Error::LockPoisoned => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::UNPROCESSABLE_ENTITY,
            };
            response(status, "text/plain", Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_encoded_paths() {
        assert_eq!(
            parse_path("/res/abc-123/img/%E4%B8%AD%20a.png"),
            Some(("abc-123".into(), "img/中 a.png".into()))
        );
        assert_eq!(parse_path("/res/abc/"), None);
        assert_eq!(parse_path("/other/abc/x.css"), None);
    }

    #[test]
    fn cache_evicts_least_recently_used() {
        let c = ResourceCache::default();
        let big = Arc::new(vec![0u8; CACHE_ITEM_LIMIT]);
        for i in 0..16 {
            c.put(format!("d\0{i}"), big.clone());
        }
        // 16 × 4MB = 64MB 恰好装满；再访问 0 号，然后插入新项，应淘汰 1 号
        assert!(c.get("d\u{0}0").is_some());
        c.put("d\0new".into(), big.clone());
        assert!(c.get("d\u{0}1").is_none());
        assert!(c.get("d\u{0}0").is_some());
        c.evict_dict("d");
        assert!(c.get("d\u{0}0").is_none());
    }

    #[test]
    fn mime_types() {
        assert_eq!(mime_for("a/B.CSS"), "text/css; charset=utf-8");
        assert_eq!(mime_for("uk/x.SPX"), "audio/wav");
        assert_eq!(mime_for("x.bin"), "application/octet-stream");
    }
}
