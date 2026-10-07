use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::db::dicts;
use crate::dict::import;
use crate::error::Result;

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
}

#[tauri::command]
pub async fn list_dicts(state: State<'_, AppState>) -> Result<Vec<DictInfo>> {
    let rows = state.db.with_conn(dicts::list)?;
    rows.into_iter()
        .map(|r| {
            Ok(DictInfo {
                loaded: state.dicts.get(&r.id)?.is_some(),
                index_state: r.index_state.as_str(),
                id: r.id,
                name: r.name,
                kind: r.kind,
                category: r.category,
                entry_count: r.entry_count,
                path: r.path,
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

#[tauri::command]
pub async fn remove_dict(state: State<'_, AppState>, id: String) -> Result<bool> {
    state.res_cache.evict_dict(&id);
    import::remove_dict(&state.db, &state.dicts, &id)
}
