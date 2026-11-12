//! 文章生词扫描的命令。业务逻辑在 `crate::scan`。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::db::history::{insert_context, NewContext};
use crate::db::words;
use crate::error::{Error, Result};

/// 一次最多扫这么多字符（约 25 万词，电子书最长的一章也够）
const MAX_CHARS: usize = 1_500_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResponse {
    #[serde(flatten)]
    pub result: crate::scan::ScanResult,
    /// 排名用的词频表：coca | builtin
    pub source: &'static str,
}

/// 扫描一篇文章：`level` = 词汇量（排名在这之后的算生词）
#[tauri::command]
pub async fn scan_text(state: State<'_, AppState>, text: String, level: u32) -> Result<ScanResponse> {
    if text.chars().count() > MAX_CHARS {
        return Err(Error::Other(crate::tf!("文章太长（超过 {} 字），请分段扫描", "The text is too long (over {} characters) — scan it in parts", MAX_CHARS)));
    }
    let saved: HashSet<String> = state.db.with_conn(words::all_lemmas)?.into_iter().map(|(_, l)| l).collect();
    let known: HashSet<String> = state.db.with_conn(words::known_lemmas)?.into_iter().collect();
    let (coca, wordnet) = (state.coca(), state.wordnet());
    tauri::async_runtime::spawn_blocking(move || {
        let result = crate::scan::with_lexicon(coca.as_deref(), wordnet.as_deref(), |lx| crate::scan::scan(&text, level, lx, &saved, &known));
        ScanResponse { result, source: if coca.is_some() { "coca" } else { "builtin" } }
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))
}

/// 标记 / 取消「认识」：认识的词以后扫描不再当生词
#[tauri::command]
pub async fn set_known_words(state: State<'_, AppState>, lemmas: Vec<String>, known: bool) -> Result<()> {
    state.db.with_conn(|c| words::set_known(c, &lemmas, known))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSave {
    pub lemma: String,
    pub surface: String,
    pub sentence: Option<String>,
    pub hl_start: i64,
    pub hl_end: i64,
    pub brief: Option<String>,
}

/// 把扫描出的生词收进生词本，文中的原句当语境句。返回收藏的词数
#[tauri::command]
pub async fn save_scanned_words(state: State<'_, AppState>, items: Vec<ScanSave>, title: Option<String>, tags: Option<Vec<String>>) -> Result<usize> {
    let title = title.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let tags = tags.unwrap_or_default();
    state.db.with_conn(|c| {
        let mut n = 0;
        for it in &items {
            let ctx = match it.sentence.as_deref().filter(|s| !s.trim().is_empty()) {
                Some(s) => Some(insert_context(
                    c,
                    &NewContext {
                        lemma: &it.lemma,
                        surface: &it.surface,
                        sentence: s,
                        hl_start: it.hl_start,
                        hl_end: it.hl_end,
                        app: None,
                        window_title: Some(title.as_deref().unwrap_or(crate::i18n::t("文章生词扫描", "Article scan"))),
                        url: None,
                        method: "manual",
                    },
                )?),
                None => None,
            };
            words::save(c, &it.lemma, it.brief.as_deref(), ctx, &tags)?;
            n += 1;
        }
        Ok(n)
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownImport {
    /// 词表里认出来的词（原形、去重）
    pub found: usize,
    /// 其中原来不在「认识」里的
    pub added: usize,
    /// 导入后「认识」一共多少词
    pub total: usize,
}

/// 导入用户认识的词表（txt / csv，每行一个词最常见）：都标成「认识」，以后扫描不再当生词
#[tauri::command]
pub async fn import_known_words(state: State<'_, AppState>, path: std::path::PathBuf) -> Result<KnownImport> {
    let bytes = std::fs::read(&path)?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err(Error::Other(crate::i18n::t("词表太大（超过 20 MB）", "The word list is too large (over 20 MB)").into()));
    }
    let text = crate::db::articles::decode_file(&bytes);
    let (coca, wordnet) = (state.coca(), state.wordnet());
    let lemmas = tauri::async_runtime::spawn_blocking(move || crate::scan::with_lexicon(coca.as_deref(), wordnet.as_deref(), |lx| crate::scan::known_list(&text, lx)))
        .await
        .map_err(|e| Error::Other(e.to_string()))?;
    if lemmas.is_empty() {
        return Err(Error::Other(crate::i18n::t("这个文件里没有认出英文单词", "No English words were found in this file").into()));
    }
    let before: HashSet<String> = state.db.with_conn(words::known_lemmas)?.into_iter().collect();
    state.db.with_conn(|c| words::set_known(c, &lemmas, true))?;
    let added = lemmas.iter().filter(|l| !before.contains(*l)).count();
    Ok(KnownImport { found: lemmas.len(), added, total: before.len() + added })
}

/// 「认识」一共多少词
#[tauri::command]
pub async fn known_words_count(state: State<'_, AppState>) -> Result<usize> {
    Ok(state.db.with_conn(words::known_lemmas)?.len())
}

/// 清空「认识」（导错了词表时用）
#[tauri::command]
pub async fn clear_known_words(state: State<'_, AppState>) -> Result<()> {
    state.db.with_conn(|c| {
        c.execute("DELETE FROM known_word", [])?;
        Ok(())
    })
}
