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
