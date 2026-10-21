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

/// 扩展后的语境句最多这么长（UTF-16 码元）
const MAX_CONTEXT_UTF16: usize = 4000;

/// 弹窗里多带了前后句：更新已存的语境句。高亮区间是 UTF-16 下标，必须落在句子里。
#[tauri::command]
pub async fn extend_context(state: State<'_, AppState>, id: i64, sentence: String, hl_start: i64, hl_end: i64) -> Result<()> {
    let len = sentence.encode_utf16().count();
    let valid = len <= MAX_CONTEXT_UTF16 && 0 <= hl_start && hl_start < hl_end && usize::try_from(hl_end).is_ok_and(|e| e <= len);
    if !valid {
        return Err(crate::error::Error::Other("语境句或高亮位置无效".into()));
    }
    state.db.with_conn(|c| history::set_context_sentence(c, id, &sentence, hl_start, hl_end))
}
