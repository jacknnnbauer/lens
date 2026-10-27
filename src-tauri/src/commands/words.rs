use serde::Deserialize;
use tauri::State;

use crate::app_state::AppState;
use crate::db::now_ms;
use crate::db::words::{self, TagCount, Word, WordFilter, WordPatch, WordbookStats};
use crate::error::Result;

fn lemma_of(state: &AppState, query: &str) -> String {
    crate::lemma::base_form(state, query)
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
    let lemma = lemma_of(&state, &query);
    state.db.with_conn(|c| words::save(c, &lemma, brief.as_deref(), context_id, &tags.unwrap_or_default()))
}

/// 这个词是否已收藏，返回词 id。
#[tauri::command]
pub async fn word_id(state: State<'_, AppState>, query: String) -> Result<Option<i64>> {
    let lemma = lemma_of(&state, &query);
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

/// 生词本按筛选全部取出（列表查询一次最多 1000 条，分页取完）
fn filtered_words(state: &AppState, status: Option<String>, tag: Option<String>, search: Option<String>) -> Result<Vec<Word>> {
    const PAGE: i64 = 1000;
    let mut f = WordFilter { status, tag, search: search.filter(|s| !s.trim().is_empty()), sort: Some("added".into()), id: None, limit: PAGE, offset: 0 };
    let mut list = Vec::new();
    loop {
        let page = state.db.with_conn(|c| words::list(c, &f))?;
        let n = page.len() as i64;
        list.extend(page);
        if n < PAGE {
            break;
        }
        f.offset += PAGE;
    }
    Ok(list)
}

fn require_absolute(path: &std::path::Path) -> Result<()> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(crate::error::Error::Other(
            crate::i18n::t("保存位置需要是完整路径（从盘符开始）", "The save location must be a full path, starting with the drive letter").into(),
        ))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnkiOptions {
    /// 牌组名
    pub deck: String,
    /// 识记卡（单词 → 释义）
    pub recall: bool,
    /// 语境卡（挖空的原句 → 单词）
    pub context: bool,
    /// 从本地词典取发音
    pub audio: bool,
    /// 发音偏好：us | uk
    pub accent: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnkiExportResult {
    pub words: usize,
    pub cards: usize,
    /// 带上了发音的词数
    pub audio: usize,
}

/// 导出 Anki 牌组包（.apkg，按当前筛选）：笔记类型、卡片模板、发音都打包在里面，双击就能导入。
#[tauri::command]
pub async fn export_anki_package(
    state: State<'_, AppState>,
    path: std::path::PathBuf,
    status: Option<String>,
    tag: Option<String>,
    search: Option<String>,
    options: AnkiOptions,
) -> Result<AnkiExportResult> {
    require_absolute(&path)?;
    let list = filtered_words(&state, status, tag, search)?;
    let mut dicts = crate::commands::lookup::locals(crate::commands::lookup::profile_sources(&state)?);
    let chosen = state.settings.get()?.wordbook.card_dict;
    dicts.sort_by_key(|d| {
        let m = d.source.meta();
        (m.id != chosen, crate::commands::lookup::is_reference_dict(&m.name))
    });
    let wordnet = state.wordnet();
    tauri::async_runtime::spawn_blocking(move || -> Result<AnkiExportResult> {
        let mut notes = Vec::with_capacity(list.len());
        let mut media: Vec<crate::anki_pkg::Media> = Vec::new();
        for w in &list {
            let pos = wordnet
                .as_ref()
                .and_then(|wn| crate::wordnet::pos_labels(wn.poses(&w.lemma)))
                .or_else(|| crate::gloss::pos_by_suffix(&w.lemma).map(str::to_string));
            let meaning = if w.brief.as_deref().is_some_and(|b| !b.trim().is_empty()) {
                None
            } else {
                dicts.iter().filter(|d| !crate::commands::lookup::is_reference_dict(&d.source.meta().name)).find_map(|d| {
                    d.source.lookup(&w.lemma).ok()?.iter().find_map(|e| {
                        let (crate::dict::EntryBody::Html(t) | crate::dict::EntryBody::Text(t)) = &e.body;
                        crate::gloss::chinese_gloss(t)
                    })
                })
            };
            let mut sound = String::new();
            if options.audio {
                if let Some(m) = crate::audio_pick::find(&dicts, &w.lemma, &options.accent) {
                    sound = format!("[sound:{}]", m.name);
                    if !media.iter().any(|x| x.name == m.name) {
                        media.push(m);
                    }
                }
            }
            notes.push(crate::wordbook_export::anki_note(w, pos, meaning, sound));
        }
        let deck = if options.deck.trim().is_empty() { crate::i18n::t("词镜 Lens 生词本", "Lens wordbook").to_string() } else { options.deck.trim().to_string() };
        let cards = crate::anki_pkg::write(&path, &deck, &notes, &media, crate::anki_pkg::Cards { recall: options.recall, context: options.context }, now_ms())?;
        Ok(AnkiExportResult { words: notes.len(), cards, audio: media.len() })
    })
    .await
    .map_err(|e| crate::error::Error::Other(e.to_string()))?
}

/// 导出生词本（按当前筛选）：format = anki（Anki 可直接导入的 .txt）| csv。返回导出的词数。
#[tauri::command]
pub async fn export_wordbook(
    state: State<'_, AppState>,
    path: std::path::PathBuf,
    format: String,
    status: Option<String>,
    tag: Option<String>,
    search: Option<String>,
) -> Result<usize> {
    require_absolute(&path)?;
    let list = filtered_words(&state, status, tag, search)?;
    let text = if format == "csv" { crate::wordbook_export::csv(&list) } else { crate::wordbook_export::anki(&list) };
    std::fs::write(&path, text)?;
    Ok(list.len())
}
