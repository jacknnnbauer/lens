//! AI 双语例句：调用 Claude API（Messages API，原始 HTTP；Rust 没有官方 SDK）或 DeepSeek（OpenAI 兼容接口）。
//! 提示里带上词典简释和 COCA 搭配词，让例句用真实、高频的搭配；Claude 用 JSON Schema 约束输出，
//! DeepSeek 用 JSON Output 模式（response_format = json_object，提示里给出格式示例）。
//! API Key 存在 Windows 凭据管理器（与在线词典的 Key 同一处），结果按（词, 模型）缓存在数据库里。

use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEEPSEEK_ENDPOINT: &str = "https://api.deepseek.com/chat/completions";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
    DeepSeek,
}

impl Provider {
    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("deepseek") {
            Provider::DeepSeek
        } else {
            Provider::Claude
        }
    }

    /// 凭据管理器里的条目名（`Lens/online/` 前缀下）
    pub fn key_id(self) -> &'static str {
        match self {
            Provider::Claude => "ai-anthropic",
            Provider::DeepSeek => "ai-deepseek",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
            Provider::DeepSeek => "DeepSeek",
        }
    }
}
const TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    pub en: String,
    pub zh: String,
    /// 例句用到的搭配（来自 COCA 或常见搭配）
    pub collocation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Examples {
    pub word: String,
    pub model: String,
    pub examples: Vec<Example>,
    /// 来自缓存时的生成时间
    pub cached_at: Option<i64>,
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "examples": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "en": { "type": "string" },
                        "zh": { "type": "string" },
                        "collocation": { "type": "string" }
                    },
                    "required": ["en", "zh", "collocation"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["examples"],
        "additionalProperties": false
    })
}

const SYSTEM: &str = "You write example sentences for a Chinese learner's English dictionary app. \
Sentences must sound like natural, contemporary American English that a native speaker would actually write or say. \
Build each sentence around a real, high-frequency collocation of the headword; prefer the COCA collocations provided, \
and never invent unusual word combinations. Vary the senses, structures and contexts (daily life, work, news, academic). \
Keep sentences 8–22 words. Give a faithful, natural Simplified Chinese translation of each sentence, \
not a word-by-word gloss. In `collocation`, write the exact collocation the sentence uses, e.g. \"generation gap\".";

pub fn prompt(word: &str, brief: Option<&str>, collocations: &str, count: usize) -> String {
    let mut p = format!("Headword: {word}\n");
    if let Some(b) = brief.filter(|b| !b.trim().is_empty()) {
        p.push_str(&format!("Dictionary gloss: {}\n", b.trim()));
    }
    if collocations.is_empty() {
        p.push_str("No COCA data for this word; use its most common collocations.\n");
    } else {
        p.push_str(&format!("COCA collocations (most frequent first):\n{collocations}\n"));
    }
    p.push_str(&format!("\nWrite {count} example sentences."));
    p
}

pub fn request_body(model: &str, word: &str, brief: Option<&str>, collocations: &str, count: usize) -> Value {
    json!({
        "model": model,
        "max_tokens": 4000,
        "system": SYSTEM,
        "messages": [{ "role": "user", "content": prompt(word, brief, collocations, count) }],
        "output_config": { "format": { "type": "json_schema", "schema": schema() } }
    })
}

// —— 用量与花费 ——

/// 一次调用用掉的 token（输入里缓存命中的部分单独算，价格低得多）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub cached: u64,
    pub output: u64,
}

/// 每百万 token 的美元价格：(输入, 缓存命中的输入, 输出)。
/// Claude 取自 platform.claude.com 定价页；DeepSeek 取自 api-docs.deepseek.com（高峰价，非高峰减半）。2026-10 核对。
fn price(model: &str) -> Option<(f64, f64, f64)> {
    Some(match model {
        "claude-opus-5-5" => (4.0, 0.20, 20.0),
        "claude-opus-4-8" | "claude-opus-4-7" | "claude-opus-4-6" | "claude-opus-5" => (5.0, 0.50, 25.0),
        "claude-sonnet-5-5" => (2.0, 0.10, 10.0),
        "claude-sonnet-5" => (2.0, 0.20, 10.0),
        "claude-haiku-4-5" => (1.0, 0.10, 5.0),
        "deepseek-flash" | "deepseek-v4-flash" => (0.30, 0.006, 1.20),
        "deepseek-v4-pro" => (1.32, 0.044, 3.96),
        _ => return None,
    })
}

/// DeepSeek 高峰时段：周一至周五 UTC 01:00–04:00、06:00–10:00；其余时间半价
pub fn deepseek_peak(ms: i64) -> bool {
    let secs = ms.div_euclid(1000);
    let day = secs.div_euclid(86_400);
    // 1970-01-01 是周四：0 = 周四 … 用 (day + 3) % 7 得到 0 = 周一
    let weekday = (day + 3).rem_euclid(7);
    let hour = secs.rem_euclid(86_400) / 3600;
    weekday < 5 && ((1..4).contains(&hour) || (6..10).contains(&hour))
}

/// 估算花费（美元）；不认识的模型返回 0
pub fn cost_usd(provider: Provider, model: &str, u: Usage, at_ms: i64) -> f64 {
    let Some((input, cached, output)) = price(model) else { return 0.0 };
    let factor = if provider == Provider::DeepSeek && !deepseek_peak(at_ms) { 0.5 } else { 1.0 };
    (u.input as f64 * input + u.cached as f64 * cached + u.output as f64 * output) / 1_000_000.0 * factor
}

/// 从响应里取 usage：Claude 是 input_tokens / cache_read_input_tokens / output_tokens，
/// DeepSeek（OpenAI 兼容）是 prompt_cache_miss_tokens / prompt_cache_hit_tokens / completion_tokens
pub fn usage_of(provider: Provider, v: &Value) -> Usage {
    let n = |p: &str| v.pointer(p).and_then(Value::as_u64).unwrap_or(0);
    match provider {
        Provider::Claude => Usage {
            input: n("/usage/input_tokens") + n("/usage/cache_creation_input_tokens"),
            cached: n("/usage/cache_read_input_tokens"),
            output: n("/usage/output_tokens"),
        },
        Provider::DeepSeek => {
            let hit = n("/usage/prompt_cache_hit_tokens");
            let miss = v.pointer("/usage/prompt_cache_miss_tokens").and_then(Value::as_u64).unwrap_or_else(|| n("/usage/prompt_tokens").saturating_sub(hit));
            Usage { input: miss, cached: hit, output: n("/usage/completion_tokens") }
        }
    }
}

/// DeepSeek 的 JSON Output 要求提示里出现 "json" 并给出格式示例
const JSON_FORMAT: &str = "Reply with JSON only, in exactly this format:\n\
{\"examples\": [{\"en\": \"English sentence\", \"zh\": \"中文翻译\", \"collocation\": \"generation gap\"}]}";

pub fn deepseek_request_body(model: &str, word: &str, brief: Option<&str>, collocations: &str, count: usize) -> Value {
    json!({
        "model": model,
        "max_tokens": 4000,
        "messages": [
            { "role": "system", "content": format!("{SYSTEM}\n\n{JSON_FORMAT}") },
            { "role": "user", "content": prompt(word, brief, collocations, count) }
        ],
        "response_format": { "type": "json_object" },
        "stream": false
    })
}

fn parse_examples(text: &str) -> Result<Vec<Example>> {
    #[derive(Deserialize)]
    struct Out {
        examples: Vec<Example>,
    }
    let out: Out = serde_json::from_str(text).map_err(|e| Error::Online(crate::tf!("无法解析模型输出：{}", "Could not parse the model output: {}", e)))?;
    Ok(out.examples.into_iter().filter(|e| !e.en.trim().is_empty()).collect())
}

/// 解析 OpenAI 兼容的 chat/completions 响应（DeepSeek）
pub fn parse_chat_response(v: &Value) -> Result<Vec<Example>> {
    let choice = v.pointer("/choices/0").ok_or_else(|| Error::Online(crate::i18n::t("模型没有返回内容", "The model returned no content").into()))?;
    if choice.get("finish_reason").and_then(Value::as_str) == Some("length") {
        return Err(Error::Online(crate::i18n::t("输出被截断，请重试", "The output was cut off. Please try again").into()));
    }
    let text = choice
        .pointer("/message/content")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        // JSON Output 模式偶尔返回空内容（官方文档注明），提示重试
        .ok_or_else(|| Error::Online(crate::i18n::t("模型返回了空内容，请重试", "The model returned an empty reply. Please try again").into()))?;
    parse_examples(text)
}

/// 解析 Messages API 的响应：先看 stop_reason，再取第一个 text 块里的 JSON。
pub fn parse_response(v: &Value) -> Result<Vec<Example>> {
    match v.get("stop_reason").and_then(Value::as_str) {
        Some("refusal") => return Err(Error::Online(crate::i18n::t("模型拒绝了这个请求", "The model declined this request").into())),
        Some("max_tokens") => return Err(Error::Online(crate::i18n::t("输出被截断，请重试", "The output was cut off. Please try again").into())),
        _ => {}
    }
    let text = v
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| blocks.iter().find(|b| b.get("type").and_then(Value::as_str) == Some("text")))
        .and_then(|b| b.get("text"))
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Online(crate::i18n::t("模型没有返回内容", "The model returned no content").into()))?;
    parse_examples(text)
}

fn http_error(provider: Provider, status: u16, body: &str) -> Error {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let msg = match status {
        401 => crate::i18n::t("API Key 无效，请在设置 › AI 例句里重新填写", "The API key is invalid. Re-enter it in Settings › AI examples").to_string(),
        402 => crate::i18n::t("账户余额不足，请先充值", "Insufficient balance. Please top up your account").to_string(),
        403 => crate::tf!("没有权限：{}", "Permission denied: {}", detail),
        404 => crate::tf!("找不到模型：{}", "Model not found: {}", detail),
        429 => crate::i18n::t("请求太频繁或额度不足，稍后再试", "Rate limited or out of credit. Try again later").to_string(),
        500..=599 => crate::tf!("{} 服务暂时不可用，稍后再试", "The {} service is temporarily unavailable. Try again later", provider.name()),
        _ => crate::tf!("请求失败（{}）：{}", "Request failed ({}): {}", status, detail),
    };
    Error::Online(msg)
}

/// 这个服务商的请求体
pub fn body_for(provider: Provider, model: &str, word: &str, brief: Option<&str>, collocations: &str, count: usize) -> Value {
    match provider {
        Provider::Claude => request_body(model, word, brief, collocations, count),
        Provider::DeepSeek => deepseek_request_body(model, word, brief, collocations, count),
    }
}

/// 按服务商发送请求（body 来自 body_for）并解析；同时返回这次用掉的 token。
pub async fn generate(client: &reqwest::Client, provider: Provider, key: &str, body: &Value) -> Result<(Vec<Example>, Usage)> {
    let req = match provider {
        Provider::Claude => client.post(ENDPOINT).header("x-api-key", key).header("anthropic-version", API_VERSION),
        Provider::DeepSeek => client.post(DEEPSEEK_ENDPOINT).bearer_auth(key),
    }
    .json(body);
    let resp = req
        .header("content-type", "application/json")
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| Error::Online(crate::tf!("连不上 {} API：{}", "Could not reach the {} API: {}", provider.name(), e)))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| Error::Online(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(http_error(provider, status, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| Error::Online(e.to_string()))?;
    let usage = usage_of(provider, &v);
    let examples = match provider {
        Provider::Claude => parse_response(&v),
        Provider::DeepSeek => parse_chat_response(&v),
    }?;
    Ok((examples, usage))
}

/// 通用的 JSON 输出调用（读前导读用）：`system`、`user` 两段提示，Claude 按 schema 约束，DeepSeek 用 JSON 模式
/// （`json_format` 是给 DeepSeek 的格式示例）。返回模型输出的 JSON 文本和用量
#[allow(clippy::too_many_arguments)]
pub async fn complete_json(
    client: &reqwest::Client,
    provider: Provider,
    key: &str,
    model: &str,
    system: &str,
    user: &str,
    schema: Value,
    json_format: &str,
    max_tokens: u32,
    timeout: Duration,
) -> Result<(String, Usage)> {
    let (req, body) = match provider {
        Provider::Claude => (
            client.post(ENDPOINT).header("x-api-key", key).header("anthropic-version", API_VERSION),
            json!({
                "model": model,
                "max_tokens": max_tokens,
                "system": system,
                "messages": [{ "role": "user", "content": user }],
                "output_config": { "format": { "type": "json_schema", "schema": schema } }
            }),
        ),
        Provider::DeepSeek => (
            client.post(DEEPSEEK_ENDPOINT).bearer_auth(key),
            json!({
                "model": model,
                "max_tokens": max_tokens,
                "messages": [
                    { "role": "system", "content": format!("{system}\n\n{json_format}") },
                    { "role": "user", "content": user }
                ],
                "response_format": { "type": "json_object" },
                "stream": false
            }),
        ),
    };
    let resp = req
        .json(&body)
        .header("content-type", "application/json")
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| Error::Online(crate::tf!("连不上 {} API：{}", "Could not reach the {} API: {}", provider.name(), e)))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| Error::Online(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(http_error(provider, status, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| Error::Online(e.to_string()))?;
    let usage = usage_of(provider, &v);
    let truncated = match provider {
        Provider::Claude => v.get("stop_reason").and_then(Value::as_str) == Some("max_tokens"),
        Provider::DeepSeek => v.pointer("/choices/0/finish_reason").and_then(Value::as_str) == Some("length"),
    };
    if truncated {
        return Err(Error::Online(crate::i18n::t("输出被截断，请重试", "The output was cut off. Please try again").into()));
    }
    if provider == Provider::Claude && v.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
        return Err(Error::Online(crate::i18n::t("模型拒绝了这个请求", "The model declined this request").into()));
    }
    let out = match provider {
        Provider::Claude => v
            .get("content")
            .and_then(Value::as_array)
            .and_then(|blocks| blocks.iter().find(|b| b.get("type").and_then(Value::as_str) == Some("text")))
            .and_then(|b| b.get("text"))
            .and_then(Value::as_str),
        Provider::DeepSeek => v.pointer("/choices/0/message/content").and_then(Value::as_str),
    }
    .filter(|t| !t.trim().is_empty())
    .ok_or_else(|| Error::Online(crate::i18n::t("模型返回了空内容，请重试", "The model returned an empty reply. Please try again").into()))?;
    Ok((out.to_string(), usage))
}

// —— 用量记账（迁移 0006 的 ai_usage 表）——

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub model: String,
    pub calls: u64,
    pub input: u64,
    pub cached: u64,
    pub output: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    /// 全部（按模型）
    pub models: Vec<ModelUsage>,
    pub total: ModelUsage,
    /// 本月（since 之后）
    pub month: ModelUsage,
    /// 第一条记录的时间
    pub since: Option<i64>,
}

pub fn record(conn: &Connection, provider: Provider, model: &str, word: &str, u: Usage, now: i64) -> Result<f64> {
    let cost = cost_usd(provider, model, u, now);
    conn.execute(
        "INSERT INTO ai_usage (provider, model, word, input_tokens, cached_tokens, output_tokens, cost_usd, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![provider.name().to_lowercase(), model, word, u.input as i64, u.cached as i64, u.output as i64, cost, now],
    )?;
    Ok(cost)
}

fn sum_row(model: String, r: &rusqlite::Row<'_>) -> rusqlite::Result<ModelUsage> {
    Ok(ModelUsage {
        model,
        calls: r.get::<_, i64>(0)?.max(0) as u64,
        input: r.get::<_, i64>(1)?.max(0) as u64,
        cached: r.get::<_, i64>(2)?.max(0) as u64,
        output: r.get::<_, i64>(3)?.max(0) as u64,
        cost_usd: r.get(4)?,
    })
}

const SUMS: &str = "COUNT(*), COALESCE(SUM(input_tokens), 0), COALESCE(SUM(cached_tokens), 0), COALESCE(SUM(output_tokens), 0), COALESCE(SUM(cost_usd), 0.0)";

/// month_start：本月 1 日 0 点（毫秒，前端按本地时间算好传进来）
pub fn summary(conn: &Connection, month_start: i64) -> Result<UsageSummary> {
    let mut st = conn.prepare(&format!("SELECT {SUMS}, model FROM ai_usage GROUP BY model ORDER BY SUM(cost_usd) DESC"))?;
    let models = st
        .query_map([], |r| {
            let m: String = r.get(5)?;
            sum_row(m, r)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let total = conn.query_row(&format!("SELECT {SUMS} FROM ai_usage"), [], |r| sum_row(String::new(), r))?;
    let month = conn.query_row(&format!("SELECT {SUMS} FROM ai_usage WHERE created_at >= ?1"), params![month_start], |r| sum_row(String::new(), r))?;
    let since = conn.query_row("SELECT MIN(created_at) FROM ai_usage", [], |r| r.get::<_, Option<i64>>(0))?;
    Ok(UsageSummary { models, total, month, since })
}

pub fn clear_usage(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM ai_usage", [])?;
    Ok(())
}

// —— 缓存（迁移 0004 的 ai_examples 表）——

pub fn cached(conn: &Connection, word: &str, model: &str) -> Result<Option<(Vec<Example>, i64)>> {
    let row: Option<(String, i64)> = conn
        .query_row("SELECT payload, created_at FROM ai_examples WHERE word = ?1 AND model = ?2", params![word, model], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    Ok(row.and_then(|(p, t)| serde_json::from_str(&p).ok().map(|v| (v, t))))
}

pub fn store(conn: &Connection, word: &str, model: &str, examples: &[Example], now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO ai_examples (word, model, payload, created_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (word, model) DO UPDATE SET payload = excluded.payload, created_at = excluded.created_at",
        params![word, model, serde_json::to_string(examples)?, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn builds_request_with_schema_and_collocations() {
        let b = request_body("claude-opus-4-8", "generation", Some("一代人"), "noun (n): generation gap", 5);
        assert_eq!(b["model"], "claude-opus-4-8");
        assert_eq!(b["output_config"]["format"]["type"], "json_schema");
        assert_eq!(b["output_config"]["format"]["schema"]["additionalProperties"], false);
        let p = b["messages"][0]["content"].as_str().unwrap_or_default();
        assert!(p.contains("generation gap") && p.contains("一代人") && p.contains("Write 5"));
        assert!(b.get("temperature").is_none(), "Opus 4.8 不接受采样参数");
    }

    #[test]
    fn parses_responses_and_errors() -> Result<()> {
        let ok = json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": "{\"examples\":[{\"en\":\"Mind the generation gap.\",\"zh\":\"注意代沟。\",\"collocation\":\"generation gap\"}]}"}]});
        let ex = parse_response(&ok)?;
        assert_eq!(ex[0].zh, "注意代沟。");
        assert!(parse_response(&json!({"stop_reason": "refusal", "content": []})).is_err());
        assert!(parse_response(&json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": "not json"}]})).is_err());
        assert!(matches!(http_error(Provider::Claude, 401, ""), Error::Online(m) if m.contains("API Key")));
        Ok(())
    }

    #[test]
    fn deepseek_request_and_response() -> Result<()> {
        let b = deepseek_request_body("deepseek-flash", "generation", None, "", 3);
        assert_eq!(b["response_format"]["type"], "json_object");
        let sys = b["messages"][0]["content"].as_str().unwrap_or_default();
        assert!(sys.to_lowercase().contains("json") && sys.contains("\"examples\""), "JSON Output 要求提示里有 json 和格式示例");
        let ok = json!({"choices": [{"finish_reason": "stop", "message": {"content": "{\"examples\":[{\"en\":\"Mind the gap.\",\"zh\":\"小心空隙。\",\"collocation\":\"mind the gap\"}]}"}}]});
        assert_eq!(parse_chat_response(&ok)?[0].zh, "小心空隙。");
        assert!(parse_chat_response(&json!({"choices": [{"finish_reason": "stop", "message": {"content": ""}}]})).is_err());
        assert!(parse_chat_response(&json!({"choices": [{"finish_reason": "length", "message": {"content": "{"}}]})).is_err());
        assert_eq!(Provider::parse("DeepSeek"), Provider::DeepSeek);
        assert_eq!(Provider::parse("whatever"), Provider::Claude);
        Ok(())
    }

    #[test]
    fn usage_and_cost() -> Result<()> {
        let claude = json!({"usage": {"input_tokens": 1000, "cache_read_input_tokens": 0, "output_tokens": 2000}});
        let u = usage_of(Provider::Claude, &claude);
        assert_eq!(u, Usage { input: 1000, cached: 0, output: 2000 });
        // Opus 4.8：1000 × $5/M + 2000 × $25/M = 0.005 + 0.05
        assert!((cost_usd(Provider::Claude, "claude-opus-4-8", u, 0) - 0.055).abs() < 1e-9);
        let ds = json!({"usage": {"prompt_tokens": 900, "prompt_cache_hit_tokens": 100, "prompt_cache_miss_tokens": 800, "completion_tokens": 500}});
        let u = usage_of(Provider::DeepSeek, &ds);
        assert_eq!(u, Usage { input: 800, cached: 100, output: 500 });
        // 2026-10-21 是周三，UTC 07:00 高峰；同日 UTC 12:00 非高峰半价
        let peak = 1_792_566_000_000; // 2026-10-21T07:00:00Z
        assert!(deepseek_peak(peak));
        assert!(!deepseek_peak(peak + 5 * 3_600_000));
        let full = cost_usd(Provider::DeepSeek, "deepseek-flash", u, peak);
        assert!((cost_usd(Provider::DeepSeek, "deepseek-flash", u, peak + 5 * 3_600_000) * 2.0 - full).abs() < 1e-12);
        assert_eq!(cost_usd(Provider::Claude, "unknown-model", u, 0), 0.0);

        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            record(c, Provider::Claude, "claude-opus-4-8", "a", Usage { input: 1000, cached: 0, output: 2000 }, 10)?;
            record(c, Provider::Claude, "claude-opus-4-8", "b", Usage { input: 1000, cached: 0, output: 2000 }, 100)?;
            let s = summary(c, 50)?;
            assert_eq!((s.total.calls, s.month.calls, s.models.len()), (2, 1, 1));
            assert!((s.total.cost_usd - 0.11).abs() < 1e-9);
            Ok(())
        })
    }

    #[test]
    fn caches_per_word_and_model() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let ex = vec![Example { en: "a".into(), zh: "甲".into(), collocation: "x".into() }];
            store(c, "word", "m1", &ex, 1)?;
            store(c, "word", "m1", &ex, 2)?;
            assert_eq!(cached(c, "word", "m1")?.map(|(_, t)| t), Some(2));
            assert!(cached(c, "word", "m2")?.is_none());
            Ok(())
        })
    }
}
