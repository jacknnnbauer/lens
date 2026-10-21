use serde::Serialize;
use tauri::{AppHandle, State, Window};

use crate::app_state::AppState;
use crate::db::dicts;
use crate::dict::LoadedDict;
use crate::error::Result;
use crate::query::{self, LookupHandle, Source};

/// 当前场景配置组里启用且已加载的词典（本地 + 在线），按顺序。
fn profile_sources(state: &AppState) -> Result<Vec<Source>> {
    let profile = state.settings.get()?.active_profile;
    sources_for_profile(state, &profile)
}

/// 指定场景配置组里启用且已加载的词典（批量导出按开始时的配置组查）。
pub fn sources_for_profile(state: &AppState, profile: &str) -> Result<Vec<Source>> {
    let ids = state.db.with_conn(|c| dicts::enabled_for_profile(c, profile))?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(d) = state.dicts.get(&id)? {
            out.push(Source::Local(d));
        } else if let Some(d) = state.online.get(&id)? {
            out.push(Source::Online(d));
        }
    }
    Ok(out)
}

fn locals(sources: Vec<Source>) -> Vec<LoadedDict> {
    sources
        .into_iter()
        .filter_map(|s| match s {
            Source::Local(d) => Some(d),
            Source::Online(_) => None,
        })
        .collect()
}

/// 立即返回 requestId 和参与查询的词典；结果通过 `lookup:result` 逐部推送，最后推送 `lookup:done`。
/// 新查询会取消同一窗口之前未完成的查询；事件只推送给发起的窗口。
#[tauri::command]
pub async fn lookup(app: AppHandle, window: Window, state: State<'_, AppState>, query: String) -> Result<LookupHandle> {
    let keys = query::candidates(&query);
    let normalized = keys.last().cloned().unwrap_or_default();
    let request_id = uuid::Uuid::new_v4().to_string();
    let sources = if keys.is_empty() { Vec::new() } else { profile_sources(&state)? };

    let handle = LookupHandle {
        request_id: request_id.clone(),
        query,
        normalized,
        dicts: sources.iter().map(Source::brief).collect(),
    };

    let label = window.label().to_string();
    let cancels = state.lookups.clone();
    let flag = cancels.register(&request_id, &label)?;
    cancels.cancel_others(&request_id, &label)?;
    let cfg = state.settings.get()?.online;
    tauri::async_runtime::spawn(query::run(app, label, cancels, request_id, keys, sources, flag, cfg));
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
    let dicts = locals(profile_sources(&state)?);
    Ok(query::prefix_suggestions(&dicts, &prefix, limit.clamp(1, 50)))
}

/// 义项图谱（实验）：把各词典命中的词条对齐成义项组。前端在查词完成后把命中卡片的词条 HTML 传进来。
#[tauri::command]
pub async fn sense_atlas(word: String, dicts: Vec<crate::senses::AtlasInput>) -> Result<crate::senses::SenseAtlas> {
    tauri::async_runtime::spawn_blocking(move || crate::senses::build(&word, &dicts))
        .await
        .map_err(|e| crate::error::Error::Other(e.to_string()))
}

/// 搭配、词频类词典（COCA 搭配之类）：背面要的是释义，自动选择时排到最后
fn is_reference_dict(name: &str) -> bool {
    let n = name.to_lowercase();
    ["coca", "colloc", "搭配", "词频", "frequen", "同义", "thesaurus", "词根"].iter().any(|k| n.contains(k))
}

/// 语境卡、复习卡背面显示释义用：设置 › 生词本选了词典就先查那本；
/// 否则按当前配置组顺序查本地词典，搭配、词频类排在最后。
/// 只查本地，不发事件、不取消别的查询，也不记历史。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstEntry {
    pub dict_id: String,
    pub name: String,
    pub entries: Vec<crate::dict::RawEntry>,
}

#[tauri::command]
pub async fn first_entry(state: State<'_, AppState>, query: String) -> Result<Option<FirstEntry>> {
    let keys = query::candidates(&query);
    if keys.is_empty() {
        return Ok(None);
    }
    let mut dicts = locals(profile_sources(&state)?);
    // 稳定排序：先是设置里选的那本，然后是普通词典，搭配、词频类最后
    let chosen = state.settings.get()?.wordbook.card_dict;
    dicts.sort_by_key(|d| {
        let m = d.source.meta();
        (m.id != chosen, is_reference_dict(&m.name))
    });
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<FirstEntry>> {
        for d in &dicts {
            for k in &keys {
                let hits = match d.source.lookup(k) {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::warn!(dict = %d.source.meta().id, error = %e, "查词失败");
                        break;
                    }
                };
                if !hits.is_empty() {
                    let meta = d.source.meta();
                    return Ok(Some(FirstEntry { dict_id: meta.id.clone(), name: meta.name.clone(), entries: hits }));
                }
            }
        }
        Ok(None)
    })
    .await
    .map_err(|e| crate::error::Error::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::is_reference_dict;

    #[test]
    fn spots_collocation_and_frequency_dicts() {
        assert!(is_reference_dict("COCA2W"));
        assert!(is_reference_dict("牛津英语搭配词典"));
        assert!(!is_reference_dict("American Heritage Dictionary 5th"));
        assert!(!is_reference_dict("牛津高阶英汉双解词典 第 8 版"));
    }
}
