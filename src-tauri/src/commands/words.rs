use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::db::now_ms;
use crate::db::words::{self, TagCount, Word, WordFilter, WordPatch, WordbookStats};
use crate::dict::index::normalize_key;
use crate::error::Result;
use crate::query::normalize_query;

fn lemma_of(query: &str) -> String {
    normalize_key(&normalize_query(query))
}

/// 收藏（Ctrl+S）。没给语境句时自动带上这个词最近一次划词的原句。返回词 id。
#[tauri::command]
pub async fn save_word(
    state: State<'_, AppState>,
    query: String,
    brief: Option<String>,
    context_id: Option<i64>,
    tags: Option<Vec<String>>,
) -> Result<i64> {
    let lemma = lemma_of(&query);
    state.db.with_conn(|c| words::save(c, &lemma, brief.as_deref(), context_id, &tags.unwrap_or_default()))
}

/// 这个词是否已收藏，返回词 id。
#[tauri::command]
pub async fn word_id(state: State<'_, AppState>, query: String) -> Result<Option<i64>> {
    let lemma = lemma_of(&query);
    state.db.with_conn(|c| words::id_by_lemma(c, &lemma))
}

#[tauri::command]
pub async fn list_words(
    state: State<'_, AppState>,
    status: Option<String>,
    tag: Option<String>,
    search: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<Word>> {
    let f = WordFilter {
        status,
        tag,
        search: search.filter(|s| !s.trim().is_empty()),
        sort,
        id: None,
        limit: limit.unwrap_or(200),
        offset: offset.unwrap_or(0),
    };
    state.db.with_conn(|c| words::list(c, &f))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordPatchInput {
    pub brief: Option<String>,
    pub familiarity: Option<i64>,
    pub tags: Option<Vec<String>>,
}

#[tauri::command]
pub async fn update_word(state: State<'_, AppState>, id: i64, patch: WordPatchInput) -> Result<()> {
    let p = WordPatch { brief: patch.brief, familiarity: patch.familiarity, tags: patch.tags };
    state.db.with_conn(|c| words::update(c, id, &p))
}

#[tauri::command]
pub async fn delete_word(state: State<'_, AppState>, id: i64) -> Result<bool> {
    state.db.with_conn(|c| words::delete(c, id))
}

#[tauri::command]
pub async fn unlink_word_context(state: State<'_, AppState>, word_id: i64, context_id: i64) -> Result<()> {
    state.db.with_conn(|c| words::unlink_context(c, word_id, context_id))
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagCount>> {
    state.db.with_conn(words::tags)
}

#[tauri::command]
pub async fn wordbook_stats(state: State<'_, AppState>) -> Result<WordbookStats> {
    state.db.with_conn(|c| words::stats(c, now_ms()))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewItem {
    pub word: Word,
    /// 评分 1..4 各自的下次到期时间（毫秒时间戳）
    pub next_due: [i64; 4],
}

/// `day_start`：本地今天 0 点（毫秒），由前端给出，用来算「今天」的新词额度。
#[tauri::command]
pub async fn review_counts(state: State<'_, AppState>, day_start: i64) -> Result<crate::db::review::ReviewCounts> {
    state.db.with_conn(|c| crate::db::review::counts(c, now_ms(), day_start))
}

#[tauri::command]
pub async fn review_queue(state: State<'_, AppState>, day_start: i64, limit: Option<i64>) -> Result<Vec<ReviewItem>> {
    let now = now_ms();
    state.db.with_conn(|c| {
        let ids = crate::db::review::queue(c, now, day_start, limit.unwrap_or(200))?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(word) = words::get(c, id)? {
                out.push(ReviewItem { next_due: crate::db::review::preview(c, id, now)?, word });
            }
        }
        Ok(out)
    })
}

/// 评分（1 忘了 · 2 模糊 · 3 记得 · 4 简单），返回下次到期时间。
#[tauri::command]
pub async fn review_rate(state: State<'_, AppState>, id: i64, rating: u8) -> Result<i64> {
    state.db.with_conn(|c| crate::db::review::rate(c, id, rating, now_ms()))
}
