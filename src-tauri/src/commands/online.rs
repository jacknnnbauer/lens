//! 在线词典（SPEC 13）：添加 / 修改 / Key / 手动查询 / 网页模式窗口。

use std::time::Instant;

use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Manager, State, Url, WebviewUrl, WebviewWindowBuilder};

use crate::app_state::AppState;
use crate::db::{dicts, now_ms};
use crate::error::{Error, Result};
use crate::online::{self, cache, credentials, OnlineDict, OnlineOptions, ProviderInfo};
use crate::query::LookupResultEvent;

pub const WEB_WINDOW: &str = "webdict";

#[tauri::command]
pub async fn list_online_providers() -> Result<Vec<ProviderInfo>> {
    Ok(online::CATALOG.to_vec())
}

/// 添加一部在线词典（追加到各场景配置组末尾）。API 词典每种只能加一部；网页词典可加多部。
#[tauri::command]
pub async fn add_online_dict(
    state: State<'_, AppState>,
    provider: String,
    name: Option<String>,
    url_template: Option<String>,
) -> Result<String> {
    let info = online::catalog_entry(&provider).ok_or_else(|| Error::UnsupportedFormat(format!("在线词典 {provider}")))?;
    if info.mode == online::OnlineMode::Api && state.online.has_provider(&provider)? {
        return Err(Error::AlreadyImported(info.name.to_string()));
    }
    let options = OnlineOptions { url_template: url_template.filter(|t| !t.trim().is_empty()), ..Default::default() };
    let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).unwrap_or_else(|| info.name.to_string());
    let row = dicts::DictRow {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        kind: "online".into(),
        path: None,
        category: Some(info.category.to_string()),
        entry_count: None,
        index_path: None,
        index_state: dicts::IndexState::Ready,
        created_at: now_ms(),
        provider: Some(provider),
        options: Some(serde_json::to_string(&options)?),
    };
    // 先校验（网页模板无效时直接报错，不写库）
    let dict = OnlineDict::from_row(&row)?;
    state.db.with_conn(|c| dicts::insert(c, &row))?;
    state.online.insert(dict)?;
    tracing::info!(provider = ?row.provider, "已添加在线词典");
    Ok(row.id)
}

/// 改名、改查询时机、网页模板、牛津端点。
#[tauri::command]
pub async fn update_online_dict(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    options: OnlineOptions,
) -> Result<()> {
    let rows = state.db.with_conn(dicts::list)?;
    let mut row = rows
        .into_iter()
        .find(|r| r.id == id && r.kind == "online")
        .ok_or_else(|| Error::Other("找不到这部在线词典".into()))?;
    if let Some(n) = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        row.name = n;
    }
    let old = OnlineOptions::parse(row.options.as_deref());
    // 模板 / 端点变了，旧缓存不再对应
    let source_changed = old.url_template != options.url_template || old.endpoint != options.endpoint;
    row.options = Some(serde_json::to_string(&options)?);
    let dict = OnlineDict::from_row(&row)?;
    state.db.with_conn(|c| {
        dicts::rename(c, &row.id, &row.name)?;
        dicts::set_options(c, &row.id, row.options.as_deref().unwrap_or("{}"))
    })?;
    if source_changed {
        state.db.with_conn(|c| cache::delete_dict(c, &id))?;
    }
    state.online.insert(dict)
}

/// 保存或清除 API Key（Windows 凭据管理器）。`key` 为空表示清除。返回现在是否有 Key。
#[tauri::command]
pub async fn set_online_key(state: State<'_, AppState>, id: String, app_id: Option<String>, key: Option<String>) -> Result<bool> {
    let dict = state.online.get(&id)?.ok_or_else(|| Error::Other("找不到这部在线词典".into()))?;
    let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    match key {
        None => {
            credentials::delete(&id)?;
            Ok(false)
        }
        Some(k) => {
            let app_id = match dict.provider.key_kind() {
                online::KeyKind::IdAndKey => Some(
                    app_id.map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).ok_or_else(|| Error::Online("请同时填写 App ID".into()))?,
                ),
                _ => None,
            };
            credentials::write(&id, &credentials::join(app_id.as_deref(), &k))?;
            // Key 换了，之前因 Key 无效而缓存的「未收录」也作废
            state.db.with_conn(|c| cache::delete_dict(c, &id))?;
            Ok(true)
        }
    }
}

pub fn has_key(id: &str) -> bool {
    credentials::read(id).ok().flatten().is_some()
}

/// 手动查询一部在线词典（查询时机为「手动」时，用户点卡片上的按钮）。
#[tauri::command]
pub async fn lookup_online(state: State<'_, AppState>, request_id: String, id: String, query: String) -> Result<LookupResultEvent> {
    let dict = state.online.get(&id)?.ok_or_else(|| Error::Other("找不到这部在线词典".into()))?;
    let word = crate::query::normalize_query(&query);
    let cfg = state.settings.get()?.online;
    let t = Instant::now();
    let outcome = online::fetch(&state.db, state.online.client(), &dict, &word, &cfg).await;
    Ok(LookupResultEvent::from_outcome(&request_id, &id, outcome, t.elapsed().as_secs_f64() * 1000.0))
}

/// 在只读窗口里打开网页词典（SPEC 13 网页模式）：独立数据目录、禁弹窗、禁下载，只允许 http(s)。
/// 窗口常驻复用：已打开时直接跳转。
#[tauri::command]
pub async fn open_web_dict(app: AppHandle, state: State<'_, AppState>, url: String, title: Option<String>) -> Result<()> {
    let url = Url::parse(&url).map_err(|_| Error::Online("网址无效".into()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Online("只能打开 http(s) 网址".into()));
    }
    let title = format!("{} · 网页词典", title.unwrap_or_else(|| url.host_str().unwrap_or_default().to_string()));
    if let Some(w) = app.get_webview_window(WEB_WINDOW) {
        w.navigate(url).map_err(|e| Error::Other(e.to_string()))?;
        let _ = w.set_title(&title);
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, WEB_WINDOW, WebviewUrl::External(url))
        .title(title)
        .inner_size(960.0, 800.0)
        .data_directory(state.root.join("webdict"))
        .on_navigation(|u| matches!(u.scheme(), "http" | "https"))
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .on_page_load(|w, p| {
            if p.event() == PageLoadEvent::Finished {
                tracing::debug!(url = %p.url(), window = w.label(), "网页词典加载完成");
            }
        })
        .build()
        .map_err(|e| Error::Other(format!("无法打开网页词典窗口：{e}")))?;
    Ok(())
}
