use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::db::dicts;
use crate::dict::import;
use crate::error::Result;
use crate::online::{self, cache, credentials, KeyKind, OnlineMode, OnlineOptions};

pub const EVENT_INDEX_PROGRESS: &str = "dict:index-progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub category: Option<String>,
    pub entry_count: Option<i64>,
    pub path: Option<String>,
    pub index_state: &'static str,
    /// 是否已成功加载、可以查词
    pub loaded: bool,
    /// 本地词典的文件不在记录的位置（挪走、删掉、换了盘符）
    pub missing: bool,
    /// 在线词典：provider id、模式、需要的凭据、是否已填 Key、选项
    pub provider: Option<String>,
    pub mode: Option<OnlineMode>,
    pub key_kind: Option<KeyKind>,
    pub has_key: bool,
    pub options: Option<OnlineOptions>,
    /// 启用这部词典的场景配置组 id
    pub profiles: Vec<String>,
}

/// 词典列表，按当前场景配置组里的顺序。
#[tauri::command]
pub async fn list_dicts(state: State<'_, AppState>) -> Result<Vec<DictInfo>> {
    let mut rows = state.db.with_conn(dicts::list)?;
    let profile = state.settings.get()?.active_profile;
    let order = state.db.with_conn(|c| dicts::profile_order(c, &profile))?;
    rows.sort_by_key(|r| order.iter().position(|id| id == &r.id).unwrap_or(usize::MAX));
    let pairs = state.db.with_conn(dicts::enabled_pairs)?;
    rows.into_iter()
        .map(|r| {
            let profiles = pairs.iter().filter(|(d, _)| d == &r.id).map(|(_, p)| p.clone()).collect();
            let online = if r.kind == "online" { state.online.get(&r.id)? } else { None };
            let info = r.provider.as_deref().and_then(online::catalog_entry);
            Ok(DictInfo {
                loaded: state.dicts.get(&r.id)?.is_some() || online.is_some(),
                missing: r.kind != "online" && r.path.as_deref().is_some_and(|p| !std::path::Path::new(p).exists()),
                mode: info.map(|i| i.mode),
                key_kind: info.map(|i| i.key_kind),
                has_key: info.is_some_and(|i| i.key_kind != KeyKind::None) && super::online::has_key(&r.id),
                options: (r.kind == "online").then(|| OnlineOptions::parse(r.options.as_deref())),
                provider: r.provider,
                index_state: r.index_state.as_str(),
                id: r.id,
                name: r.name,
                kind: r.kind,
                category: r.category,
                entry_count: r.entry_count,
                path: r.path,
                profiles,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexProgress {
    pub path: String,
    /// 本批第几个（从 0 开始）/ 共几个
    pub index: usize,
    pub total: usize,
    /// start | done | error
    pub phase: &'static str,
    pub dict_id: Option<String>,
    pub name: Option<String>,
    pub error: Option<String>,
}

/// 后台依次导入；每个文件开始、完成或失败时推送 `dict:index-progress`。立即返回。
#[tauri::command]
pub async fn import_dicts(app: AppHandle, paths: Vec<String>) -> Result<()> {
    tauri::async_runtime::spawn(async move {
        let total = paths.len();
        for (index, path) in paths.into_iter().enumerate() {
            let progress = |phase, dict_id, name, error| IndexProgress {
                path: path.clone(),
                index,
                total,
                phase,
                dict_id,
                name,
                error,
            };
            tracing::info!(path = %path, index, total, "开始导入词典");
            let _ = app.emit(EVENT_INDEX_PROGRESS, progress("start", None, None, None));
            let (app2, p) = (app.clone(), path.clone());
            let res = tokio::task::spawn_blocking(move || {
                let state = app2.state::<AppState>();
                import::import_dict(&state.db, &state.dicts, std::path::Path::new(&p))
            })
            .await;
            let event = match res {
                Ok(Ok(meta)) => progress("done", Some(meta.id), Some(meta.name), None),
                Ok(Err(e)) => {
                    tracing::warn!(path = %path, error = %e, "导入词典失败");
                    progress("error", None, None, Some(e.to_string()))
                }
                Err(e) => {
                    tracing::error!(path = %path, error = %e, "导入任务 panic");
                    progress("error", None, None, Some("导入时发生内部错误".into()))
                }
            };
            let _ = app.emit(EVENT_INDEX_PROGRESS, event);
        }
    });
    Ok(())
}

/// 词典文件挪了位置：重新指定文件，不用重建索引
#[tauri::command]
pub async fn relink_dict(app: AppHandle, id: String, path: String) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state.res_cache.evict_dict(&id);
        import::relink_dict(&state.db, &state.dicts, &id, std::path::Path::new(&path)).map(|_| ())
    })
    .await
    .map_err(|e| crate::error::Error::Other(e.to_string()))?
}

#[tauri::command]
pub async fn remove_dict(state: State<'_, AppState>, id: String) -> Result<bool> {
    state.res_cache.evict_dict(&id);
    if state.online.remove(&id)?.is_some() {
        // 在线词典：连同缓存与凭据一起删
        state.db.with_conn(|c| cache::delete_dict(c, &id))?;
        if let Err(e) = credentials::delete(&id) {
            tracing::warn!(error = %e, "删除在线词典凭据失败");
        }
        return state.db.with_conn(|c| dicts::delete(c, &id));
    }
    import::remove_dict(&state.db, &state.dicts, &id)
}

/// 调整词典顺序（词典库里上移 / 下移）。
#[tauri::command]
pub async fn reorder_dicts(state: State<'_, AppState>, ids: Vec<String>) -> Result<()> {
    state.db.with_conn(|c| dicts::reorder(c, &ids))
}

/// 在某个场景配置组里启用 / 停用一部词典（设置 › 场景配置组）。
#[tauri::command]
pub async fn set_profile_dict(state: State<'_, AppState>, profile: String, dict: String, enabled: bool) -> Result<bool> {
    state.db.with_conn(|c| dicts::set_profile_enabled(c, &profile, &dict, enabled))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub dict_count: i64,
}

#[tauri::command]
pub async fn list_profiles(state: State<'_, AppState>) -> Result<Vec<ProfileInfo>> {
    let rows = state.db.with_conn(dicts::profiles)?;
    Ok(rows.into_iter().map(|(id, name, dict_count)| ProfileInfo { id, name, dict_count }).collect())
}
