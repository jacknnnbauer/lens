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
    state.coca()
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

/// 这个服务商（claude | deepseek）是否已保存 API Key。
#[tauri::command]
pub async fn ai_key_status(provider: Option<String>) -> Result<bool> {
    let p = ai::Provider::parse(provider.as_deref().unwrap_or("claude"));
    Ok(credentials::read(p.key_id())?.is_some())
}

/// 保存或删除（传空）某个服务商的 API Key。
#[tauri::command]
pub async fn ai_set_key(provider: Option<String>, key: Option<String>) -> Result<bool> {
    let p = ai::Provider::parse(provider.as_deref().unwrap_or("claude"));
    match key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty()) {
        Some(k) => {
            credentials::write(p.key_id(), &k)?;
            Ok(true)
        }
        None => {
            credentials::delete(p.key_id())?;
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
    let provider = cfg.provider();
    let model = cfg.active_model().to_string();
    if !force {
        if let Some((examples, at)) = state.db.with_conn(|c| ai::cached(c, &word, &model))? {
            return Ok(Some(Examples { word, model, examples, cached_at: Some(at) }));
        }
    }
    if cached_only {
        return Ok(None);
    }
    let key = credentials::read(provider.key_id())?.ok_or_else(|| {
        Error::Online(crate::tf!(
            "还没有填写 {} API Key（设置 › AI 例句）",
            "No {} API key yet (Settings › AI examples)",
            provider.name()
        ))
    })?;
    let hint = coca_of(&state).map(|c| coca::collocation_hint(&c.lookup(&word), 8)).unwrap_or_default();
    let body = ai::body_for(provider, &model, &word, brief.as_deref(), &hint, cfg.count.clamp(1, 10));
    let (examples, usage) = ai::generate(state.online.client(), provider, &key, &body).await?;
    let now = now_ms();
    state.db.with_conn(|c| {
        ai::store(c, &word, &model, &examples, now)?;
        ai::record(c, provider, &model, &word, usage, now)?;
        Ok(())
    })?;
    Ok(Some(Examples { word, model, examples, cached_at: None }))
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
    let online = state.settings.get()?.online;
    if !online.vocab_examples {
        return Err(Error::Other(crate::i18n::t("已在设置 › 在线词典里关闭 vocabulary.com 例句", "vocabulary.com examples are turned off in Settings › Online dictionaries").into()));
    }
    let days = online.cache_days;
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

/// AI 用量汇总（设置 › AI 例句）。month_start：本月 1 日 0 点（本地时间，毫秒）。
#[tauri::command]
pub async fn ai_usage(state: State<'_, AppState>, month_start: i64) -> Result<ai::UsageSummary> {
    state.db.with_conn(|c| ai::summary(c, month_start))
}

#[tauri::command]
pub async fn ai_usage_clear(state: State<'_, AppState>) -> Result<()> {
    state.db.with_conn(ai::clear_usage)
}
