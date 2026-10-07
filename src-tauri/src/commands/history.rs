use tauri::{State, Window};

use crate::app_state::AppState;
use crate::db::history::{self, HistoryFilter, HistoryItem};
use crate::db::now_ms;
use crate::dict::index::normalize_key;
use crate::error::Result;
use crate::query::normalize_query;

/// 记一条查词历史。来源取发起的窗口（main / popup）；同一来源 10 分钟内的同一个词合并。
#[tauri::command]
pub async fn record_history(
    window: Window,
    state: State<'_, AppState>,
    query: String,
    context_id: Option<i64>,
) -> Result<Option<i64>> {
    let lemma = normalize_key(&normalize_query(&query));
    if lemma.is_empty() {
        return Ok(None);
    }
    let source = window.label().to_string();
    let id = state.db.with_conn(|c| history::record(c, query.trim(), &lemma, &source, context_id, now_ms()))?;
    Ok(Some(id))
}

#[tauri::command]
pub async fn list_history(
    state: State<'_, AppState>,
    from: Option<i64>,
    to: Option<i64>,
    search: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<HistoryItem>> {
    let filter = HistoryFilter {
        from,
        to,
        search: search.filter(|s| !s.trim().is_empty()),
        limit: limit.unwrap_or(200),
        offset: offset.unwrap_or(0),
    };
    state.db.with_conn(|c| history::list(c, &filter))
}

#[tauri::command]
pub async fn delete_history(state: State<'_, AppState>, ids: Vec<i64>) -> Result<usize> {
    state.db.with_conn(|c| history::delete(c, &ids))
}

/// 清空历史；`before` 为毫秒时间戳时只清这之前的。
#[tauri::command]
pub async fn clear_history(state: State<'_, AppState>, before: Option<i64>) -> Result<usize> {
    state.db.with_conn(|c| history::clear(c, before))
}

#[tauri::command]
pub async fn lookup_count(state: State<'_, AppState>, query: String) -> Result<i64> {
    let lemma = normalize_key(&normalize_query(&query));
    state.db.with_conn(|c| history::lookup_count(c, &lemma))
}
