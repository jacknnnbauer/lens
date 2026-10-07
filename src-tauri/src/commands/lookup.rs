use tauri::{AppHandle, State};

use crate::app_state::AppState;
use crate::db::dicts;
use crate::dict::LoadedDict;
use crate::error::Result;
use crate::query::{self, DictBrief, LookupHandle};

/// 当前场景配置组里启用且已加载的词典，按顺序。
fn profile_dicts(state: &AppState) -> Result<Vec<LoadedDict>> {
    let profile = state.settings.get()?.active_profile;
    let ids = state.db.with_conn(|c| dicts::enabled_for_profile(c, &profile))?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(d) = state.dicts.get(&id)? {
            out.push(d);
        }
    }
    Ok(out)
}

/// 立即返回 requestId 和参与查询的词典；结果通过 `lookup:result` 逐部推送，最后推送 `lookup:done`。
/// 新查询会取消同一来源之前未完成的查询。
#[tauri::command]
pub async fn lookup(app: AppHandle, state: State<'_, AppState>, query: String) -> Result<LookupHandle> {
    let keys = query::candidates(&query);
    let normalized = keys.last().cloned().unwrap_or_default();
    let request_id = uuid::Uuid::new_v4().to_string();
    let dicts = if keys.is_empty() { Vec::new() } else { profile_dicts(&state)? };

    let handle = LookupHandle {
        request_id: request_id.clone(),
        query,
        normalized,
        dicts: dicts
            .iter()
            .map(|d| DictBrief { id: d.source.meta().id.clone(), name: d.source.meta().name.clone() })
            .collect(),
    };

    let cancels = state.lookups.clone();
    let flag = cancels.register(&request_id)?;
    cancels.cancel_all_except(&request_id)?;
    tauri::async_runtime::spawn(query::run(app, cancels, request_id, keys, dicts, flag));
    Ok(handle)
}

#[tauri::command]
pub async fn cancel_lookup(state: State<'_, AppState>, request_id: String) -> Result<()> {
    state.lookups.cancel(&request_id)
}

#[tauri::command]
pub async fn suggest(state: State<'_, AppState>, prefix: String, limit: usize) -> Result<Vec<String>> {
    let prefix = query::normalize_query(&prefix);
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    let dicts = profile_dicts(&state)?;
    Ok(query::prefix_suggestions(&dicts, &prefix, limit.clamp(1, 50)))
}
