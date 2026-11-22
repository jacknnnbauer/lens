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

/// 是否已保存 DeepSeek API Key。
#[tauri::command]
pub async fn ai_key_status() -> Result<bool> {
    Ok(credentials::read(ai::KEY_ID)?.is_some())
}

/// 保存或删除（传空）DeepSeek API Key。
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

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTest {
    pub provider: &'static str,
    pub model: String,
    pub ms: u64,
}

/// 设置 › AI 例句的「测试连接」：用当前模型发一句最短的请求
#[tauri::command]
pub async fn ai_test(state: State<'_, AppState>) -> Result<AiTest> {
    let model = state.settings.get()?.ai.deepseek_model;
    let key = credentials::read(ai::KEY_ID)?.ok_or_else(|| {
        crate::error::Error::Online(crate::tf!("还没有填写 {} API Key", "No {} API key yet", ai::NAME))
    })?;
    let t = std::time::Instant::now();
    let usage = ai::ping(state.online.client(), &key, &model).await?;
    let ms = t.elapsed().as_millis() as u64;
    let label = crate::i18n::t("（测试连接）", "(connection test)");
    state.db.with_conn(|c| ai::record(c, &model, label, usage, crate::db::now_ms()).map(|_| ()))?;
    Ok(AiTest { provider: ai::NAME, model, ms })
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
    let model = cfg.deepseek_model.clone();
    if !force {
        if let Some((examples, at)) = state.db.with_conn(|c| ai::cached(c, &word, &model))? {
            return Ok(Some(Examples { word, model, examples, cached_at: Some(at) }));
        }
    }
    if cached_only {
        return Ok(None);
    }
    let key = credentials::read(ai::KEY_ID)?.ok_or_else(|| {
        Error::Online(crate::tf!(
            "还没有填写 {} API Key（设置 › AI 例句）",
            "No {} API key yet (Settings › AI examples)",
            ai::NAME
        ))
    })?;
    let hint = coca_of(&state).map(|c| coca::collocation_hint(&c.lookup(&word), 8)).unwrap_or_default();
    let body = ai::request_body(&model, &word, brief.as_deref(), &hint, cfg.count.clamp(1, 10));
    let (examples, usage) = ai::generate(state.online.client(), &key, &body).await?;
    let now = now_ms();
    state.db.with_conn(|c| {
        ai::store(c, &word, &model, &examples, now)?;
        ai::record(c, &model, &word, usage, now)?;
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

// —— 长难句拆解 ——

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakdownResult {
    pub breakdown: crate::sentence_ai::Breakdown,
    pub model: String,
    /// 从缓存取的：当时拆的时间；刚拆的为 None
    pub cached_at: Option<i64>,
    /// 这次请求花了多少（美元）；缓存为 0
    pub cost_usd: f64,
}

/// 拆一句长难句：先查缓存（force 时跳过），没有再问 DeepSeek
#[tauri::command]
pub async fn sentence_breakdown(state: State<'_, AppState>, sentence: String, before: Option<String>, after: Option<String>, force: bool) -> Result<BreakdownResult> {
    use crate::sentence_ai;
    let sentence = sentence.split_whitespace().collect::<Vec<_>>().join(" ");
    if sentence.is_empty() {
        return Err(Error::Other("sentence".into()));
    }
    let model = state.settings.get()?.ai.deepseek_model;
    if !force {
        if let Some((breakdown, at)) = state.db.with_conn(|c| sentence_ai::cached(c, &sentence, &model))? {
            return Ok(BreakdownResult { breakdown, model, cached_at: Some(at), cost_usd: 0.0 });
        }
    }
    let key = credentials::read(ai::KEY_ID)?.ok_or_else(|| {
        Error::Online(crate::tf!("还没有填写 {} API Key（设置 › AI 服务）", "No {} API key yet (Settings › AI)", ai::NAME))
    })?;
    let user = sentence_ai::prompt(&sentence, before.as_deref(), after.as_deref());
    let (text, usage) = ai::complete_json(state.online.client(), &key, &model, sentence_ai::SYSTEM, &user, sentence_ai::JSON_FORMAT, 6000, std::time::Duration::from_secs(90)).await?;
    let breakdown = sentence_ai::parse(&text)?;
    let now = now_ms();
    let label: String = sentence.chars().take(40).collect();
    let cost_usd = state.db.with_conn(|c| {
        sentence_ai::store(c, &sentence, &model, &breakdown, now)?;
        ai::record(c, &model, &label, usage, now)
    })?;
    Ok(BreakdownResult { breakdown, model, cached_at: None, cost_usd })
}

// —— 英文怎么说 ——

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionResult {
    pub expression: crate::expression_ai::Expression,
    pub model: String,
    /// 从缓存取的：当时生成的时间；刚生成的为 None
    pub cached_at: Option<i64>,
    /// 这次请求花了多少（美元）；缓存为 0
    pub cost_usd: f64,
}

/// 规整输入：合并空白；空的、太长的直接报错
fn expression_input(input: &str) -> Result<String> {
    let input = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if input.is_empty() {
        return Err(Error::Other("input".into()));
    }
    if input.chars().count() > crate::expression_ai::MAX_CHARS {
        return Err(Error::Online(crate::i18n::t("太长了，挑一两句再试", "That's too long. Pick a sentence or two").into()));
    }
    Ok(input)
}

/// 只查缓存（卡片出现时用，不联网、不花钱）
#[tauri::command]
pub async fn expression_cached(state: State<'_, AppState>, input: String, audience: Option<String>) -> Result<Option<ExpressionResult>> {
    use crate::expression_ai;
    let Ok(input) = expression_input(&input) else { return Ok(None) };
    let audience = audience.unwrap_or_default();
    let model = state.settings.get()?.ai.deepseek_model;
    Ok(state
        .db
        .with_conn(|c| expression_ai::cached(c, &input, audience.trim(), &model))?
        .map(|(expression, at)| ExpressionResult { expression, model, cached_at: Some(at), cost_usd: 0.0 }))
}

/// 生成五种说法：先查缓存（force 时跳过），没有再问 DeepSeek
#[tauri::command]
pub async fn expression_generate(state: State<'_, AppState>, input: String, audience: Option<String>, force: bool) -> Result<ExpressionResult> {
    use crate::expression_ai;
    let input = expression_input(&input)?;
    let audience = audience.unwrap_or_default().trim().to_string();
    let model = state.settings.get()?.ai.deepseek_model;
    if !force {
        if let Some((expression, at)) = state.db.with_conn(|c| expression_ai::cached(c, &input, &audience, &model))? {
            return Ok(ExpressionResult { expression, model, cached_at: Some(at), cost_usd: 0.0 });
        }
    }
    let key = credentials::read(ai::KEY_ID)?.ok_or_else(|| {
        Error::Online(crate::tf!("还没有填写 {} API Key（设置 › AI 服务）", "No {} API key yet (Settings › AI)", ai::NAME))
    })?;
    let user = expression_ai::prompt(&input, Some(&audience));
    let (text, usage) = ai::complete_json(state.online.client(), &key, &model, expression_ai::SYSTEM, &user, expression_ai::JSON_FORMAT, 3000, std::time::Duration::from_secs(60)).await?;
    let expression = expression_ai::parse(&text)?;
    let now = now_ms();
    let label: String = input.chars().take(40).collect();
    let cost_usd = state.db.with_conn(|c| {
        expression_ai::store(c, &input, &audience, &model, &expression, now)?;
        ai::record(c, &model, &label, usage, now)
    })?;
    Ok(ExpressionResult { expression, model, cached_at: None, cost_usd })
}

// —— 英文怎么说 · 更多说法 ——

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryResult {
    pub entry: crate::expression_entry::Entry,
    pub model: String,
    /// 从缓存取的：当时生成的时间；刚生成的为 None
    pub cached_at: Option<i64>,
    /// 这次请求花了多少（美元）；缓存为 0
    pub cost_usd: f64,
}

/// 只查缓存（不联网、不花钱）
#[tauri::command]
pub async fn expression_entry_cached(state: State<'_, AppState>, input: String) -> Result<Option<EntryResult>> {
    use crate::expression_entry;
    let Ok(input) = expression_input(&input) else { return Ok(None) };
    let model = state.settings.get()?.ai.deepseek_model;
    Ok(state
        .db
        .with_conn(|c| expression_entry::cached(c, &input, &model))?
        .map(|(entry, at)| EntryResult { entry, model, cached_at: Some(at), cost_usd: 0.0 }))
}

/// 生成「更多说法」词条：先查缓存（force 时跳过），没有再问 DeepSeek
#[tauri::command]
pub async fn expression_entry_generate(state: State<'_, AppState>, input: String, force: bool) -> Result<EntryResult> {
    use crate::expression_entry;
    let input = expression_input(&input)?;
    let model = state.settings.get()?.ai.deepseek_model;
    if !force {
        if let Some((entry, at)) = state.db.with_conn(|c| expression_entry::cached(c, &input, &model))? {
            return Ok(EntryResult { entry, model, cached_at: Some(at), cost_usd: 0.0 });
        }
    }
    let key = credentials::read(ai::KEY_ID)?.ok_or_else(|| {
        Error::Online(crate::tf!("还没有填写 {} API Key（设置 › AI 服务）", "No {} API key yet (Settings › AI)", ai::NAME))
    })?;
    let user = expression_entry::prompt(&input);
    let (text, usage) = ai::complete_json(state.online.client(), &key, &model, expression_entry::SYSTEM, &user, expression_entry::JSON_FORMAT, 3000, std::time::Duration::from_secs(60)).await?;
    let entry = expression_entry::parse(&text)?;
    let now = now_ms();
    let label = format!("更多说法 · {}", input.chars().take(40).collect::<String>());
    let cost_usd = state.db.with_conn(|c| {
        expression_entry::store(c, &input, &model, &entry, now)?;
        ai::record(c, &model, &label, usage, now)
    })?;
    Ok(EntryResult { entry, model, cached_at: None, cost_usd })
}
