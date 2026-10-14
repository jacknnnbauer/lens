//! COCA 词频表与 AI 双语例句的命令（右侧栏、设置 › AI 例句）。

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::ai::{self, Examples};
use crate::app_state::AppState;
use crate::coca::{self, Coca, CocaEntry};
use crate::db::now_ms;
use crate::error::{Error, Result};
use crate::online::credentials;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CocaStatus {
    pub loaded: bool,
    pub entries: usize,
}

fn coca_of(state: &AppState) -> Option<Arc<Coca>> {
    state.coca.read().ok().and_then(|c| c.clone())
}

#[tauri::command]
pub async fn coca_status(state: State<'_, AppState>) -> Result<CocaStatus> {
    let c = coca_of(&state);
    Ok(CocaStatus { loaded: c.is_some(), entries: c.map_or(0, |c| c.len()) })
}

/// 导入用户自己解析好的 COCA 词表 JSON（复制到数据目录），立即生效。
#[tauri::command]
pub async fn coca_import(state: State<'_, AppState>, path: PathBuf) -> Result<CocaStatus> {
    let dest = state.root.join(coca::FILE);
    let loaded = tauri::async_runtime::spawn_blocking(move || -> Result<Coca> {
        coca::import(&path, &dest)?;
        Coca::load(&dest)
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))??;
    let entries = loaded.len();
    if let Ok(mut c) = state.coca.write() {
        *c = Some(Arc::new(loaded));
    }
    Ok(CocaStatus { loaded: true, entries })
}

#[tauri::command]
pub async fn coca_lookup(state: State<'_, AppState>, word: String) -> Result<Vec<CocaEntry>> {
    Ok(coca_of(&state).map(|c| c.lookup(&word)).unwrap_or_default())
}

#[tauri::command]
pub async fn ai_key_status() -> Result<bool> {
    Ok(credentials::read(ai::KEY_ID)?.is_some())
}

/// 保存或删除（传空）Claude API Key。
#[tauri::command]
pub async fn ai_set_key(key: Option<String>) -> Result<bool> {
    match key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        Some(k) => {
            credentials::write(ai::KEY_ID, &k)?;
            Ok(true)
        }
        None => {
            credentials::delete(ai::KEY_ID)?;
            Ok(false)
        }
    }
}

/// 双语例句：有缓存直接返回；`force` 时重新生成；`cached_only` 时没有缓存就返回 None（不花钱）。
#[tauri::command]
pub async fn ai_examples(
    state: State<'_, AppState>,
    word: String,
    brief: Option<String>,
    force: bool,
    cached_only: bool,
) -> Result<Option<Examples>> {
    let word = word.trim().to_lowercase();
    if word.is_empty() {
        return Ok(None);
    }
    let cfg = state.settings.get()?.ai;
    if !force {
        if let Some((examples, at)) = state.db.with_conn(|c| ai::cached(c, &word, &cfg.model))? {
            return Ok(Some(Examples { word, model: cfg.model, examples, cached_at: Some(at) }));
        }
    }
    if cached_only {
        return Ok(None);
    }
    let key = credentials::read(ai::KEY_ID)?.ok_or_else(|| {
        Error::Online(crate::i18n::t("还没有填写 Claude API Key（设置 › AI 例句）", "No Claude API key yet (Settings › AI examples)").into())
    })?;
    let hint = coca_of(&state).map(|c| coca::collocation_hint(&c.lookup(&word), 8)).unwrap_or_default();
    let body = ai::request_body(&cfg.model, &word, brief.as_deref(), &hint, cfg.count.clamp(1, 10));
    let examples = ai::generate(state.online.client(), &key, &body).await?;
    state.db.with_conn(|c| ai::store(c, &word, &cfg.model, &examples, now_ms()))?;
    Ok(Some(Examples { word, model: cfg.model, examples, cached_at: None }))
}

/// vocabulary.com 真实例句（一页 24 句）。缓存在有效期内直接用；联网失败时退回过期缓存并标注。
#[tauri::command]
pub async fn vocab_examples(state: State<'_, AppState>, word: String, domain: Option<String>, offset: Option<u32>, force: bool) -> Result<crate::vocab::Page> {
    use crate::vocab;
    let word = word.trim().to_string();
    if word.is_empty() {
        return Err(Error::Other(crate::i18n::t("没有要查的词", "No word to look up").into()));
    }
    let domain = vocab::normalize_domain(domain.as_deref().unwrap_or(""));
    let offset = offset.unwrap_or(0);
    let days = state.settings.get()?.online.cache_days;
    let now = now_ms();
    let cached = state.db.with_conn(|c| vocab::cached(c, &word, &domain, offset))?;
    let fresh = |at: i64| days < 0 || (days > 0 && now - at < days * 86_400_000);
    let page = |total, sentences, cached_at, stale| vocab::Page { word: word.clone(), domain: domain.clone(), offset, total, sentences, cached_at, stale };
    if let Some((total, sentences, at)) = &cached {
        if !force && fresh(*at) {
            return Ok(page(*total, sentences.clone(), Some(*at), false));
        }
    }
    match vocab::fetch(state.online.client(), &word, &domain, offset).await {
        Ok((total, sentences)) => {
            if days != 0 {
                state.db.with_conn(|c| vocab::store(c, &word, &domain, offset, total, &sentences, now))?;
            }
            Ok(page(total, sentences, None, false))
        }
        Err(e) => match cached {
            Some((total, sentences, at)) => Ok(page(total, sentences, Some(at), true)),
            None => Err(e),
        },
    }
}
