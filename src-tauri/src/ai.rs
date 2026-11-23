//! AI 双语例句：调用 DeepSeek（OpenAI 兼容接口）。
//! 提示里带上词典简释和 COCA 搭配词，让例句用真实、高频的搭配；输出用 JSON Output 模式
//! （response_format = json_object，提示里给出格式示例）。
//! API Key 存在 Windows 凭据管理器（与在线词典的 Key 同一处），结果按（词, 模型）缓存在数据库里。

use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};

const ENDPOINT: &str = "https://api.deepseek.com/chat/completions";
/// 凭据管理器里的条目名（`Lens/online/` 前缀下）
pub const KEY_ID: &str = "ai-deepseek";
/// 服务商的名字（提示文字里用）
pub const NAME: &str = "DeepSeek";
/// 旧版存 Claude API Key 的条目名
const OLD_CLAUDE_KEY_ID: &str = "ai-anthropic";

/// 0.42.0 起不再用 Claude：启动时清掉旧版留在凭据管理器里的 Key（界面上已经没有地方能删它）。没有这一条时什么也不做
pub fn forget_old_claude_key() {
    if let Err(e) = crate::online::credentials::delete(OLD_CLAUDE_KEY_ID) {
        tracing::warn!(error = %e, "清除旧的 Claude API Key 失败");
    }
}

const TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Example {
    pub en: String,
    pub zh: String,
    /// 例句用到的搭配（来自 COCA 或常见搭配）
    #[serde(default)]
    pub collocation: String,
    /// 指定了句型时：这句是哪种从句（CLAUSES 里的 id）；旧缓存没有
    #[serde(default)]
    pub clause: String,
}

/// 可指定的 15 种从句：(id, 给模型看的英文名)
pub const CLAUSES: [(&str, &str); 15] = [
    ("subject", "subject clause (noun clause as subject)"),
    ("object", "object clause (noun clause as object)"),
    ("predicative", "predicative clause (noun clause after a linking verb)"),
    ("appositive", "appositive clause (that-clause explaining a noun such as fact, news, idea)"),
    ("restrictive", "restrictive (defining) relative clause"),
    ("nonrestrictive", "non-restrictive (non-defining) relative clause, set off by commas"),
    ("time", "adverbial clause of time"),
    ("place", "adverbial clause of place"),
    ("reason", "adverbial clause of reason"),
    ("condition", "conditional clause"),
    ("purpose", "adverbial clause of purpose"),
    ("result", "adverbial clause of result"),
    ("concession", "concessive clause"),
    ("comparison", "comparative clause"),
    ("manner", "adverbial clause of manner"),
];

/// 例句的长度与句型（来自设置）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    /// short | medium | long
    pub length: &'static str,
    /// 按 CLAUSES 顺序、去重后的句型 id
    pub clauses: Vec<String>,
}

impl Default for Style {
    fn default() -> Self {
        Self { length: "medium", clauses: Vec::new() }
    }
}

impl Style {
    /// 规整：未知长度当适中，未知句型丢掉，按清单顺序去重
    pub fn new(length: &str, clauses: &[String]) -> Self {
        let length = match length {
            "short" => "short",
            "long" => "long",
            _ => "medium",
        };
        let clauses = CLAUSES.iter().filter(|(id, _)| clauses.iter().any(|c| c == id)).map(|(id, _)| (*id).to_string()).collect();
        Self { length, clauses }
    }

    /// 缓存键：适中且不限句型为空串（和旧缓存一致）
    pub fn key(&self) -> String {
        if self.length == "medium" && self.clauses.is_empty() {
            return String::new();
        }
        format!("{}|{}", self.length, self.clauses.join(","))
    }

    fn words(&self) -> &'static str {
        match self.length {
            "short" => "5–10 words",
            "long" => "20–35 words",
            _ => "8–22 words",
        }
    }
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

const SYSTEM: &str = "You write example sentences for a Chinese learner's English dictionary app. \
Sentences must sound like natural, contemporary American English that a native speaker would actually write or say. \
Build each sentence around a real, high-frequency collocation of the headword; prefer the COCA collocations provided, \
and never invent unusual word combinations. Vary the senses, structures and contexts (daily life, work, news, academic). \
Give a faithful, natural Simplified Chinese translation of each sentence, \
not a word-by-word gloss. In `collocation`, write the exact collocation the sentence uses, e.g. \"generation gap\".";

pub fn prompt(word: &str, brief: Option<&str>, collocations: &str, count: usize, style: &Style) -> String {
    let mut p = format!("Headword: {word}\n");
    if let Some(b) = brief.filter(|b| !b.trim().is_empty()) {
        p.push_str(&format!("Dictionary gloss: {}\n", b.trim()));
    }
    if collocations.is_empty() {
        p.push_str("No COCA data for this word; use its most common collocations.\n");
    } else {
        p.push_str(&format!("COCA collocations (most frequent first):\n{collocations}\n"));
    }
    p.push_str(&format!("\nEach sentence should be {} long.", style.words()));
    if style.clauses.is_empty() {
        p.push_str(&format!("\nWrite {count} example sentences. Leave `clause` empty."));
    } else {
        p.push_str(&format!(
            "\nWrite {} example sentences, exactly one for each clause type below, in this order. \
Each sentence must really contain that type of clause, used correctly, and must use the headword. \
Set `clause` to the id before the colon.\n",
            style.clauses.len()
        ));
        for id in &style.clauses {
            let name = CLAUSES.iter().find(|(c, _)| c == id).map(|(_, n)| *n).unwrap_or_default();
            p.push_str(&format!("- {id}: {name}\n"));
        }
    }
    p
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
/// 取自 api-docs.deepseek.com（高峰价，非高峰减半）。2026-10 核对。
fn price(model: &str) -> Option<(f64, f64, f64)> {
    Some(match model {
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
pub fn cost_usd(model: &str, u: Usage, at_ms: i64) -> f64 {
    let Some((input, cached, output)) = price(model) else { return 0.0 };
    let factor = if deepseek_peak(at_ms) { 1.0 } else { 0.5 };
    (u.input as f64 * input + u.cached as f64 * cached + u.output as f64 * output) / 1_000_000.0 * factor
}

/// 从响应里取 usage：prompt_cache_miss_tokens / prompt_cache_hit_tokens / completion_tokens
pub fn usage_of(v: &Value) -> Usage {
    let n = |p: &str| v.pointer(p).and_then(Value::as_u64).unwrap_or(0);
    let hit = n("/usage/prompt_cache_hit_tokens");
    let miss = v.pointer("/usage/prompt_cache_miss_tokens").and_then(Value::as_u64).unwrap_or_else(|| n("/usage/prompt_tokens").saturating_sub(hit));
    Usage { input: miss, cached: hit, output: n("/usage/completion_tokens") }
}

/// DeepSeek 的 JSON Output 要求提示里出现 "json" 并给出格式示例
const JSON_FORMAT: &str = "Reply with JSON only, in exactly this format:\n\
{\"examples\": [{\"en\": \"English sentence\", \"zh\": \"中文翻译\", \"collocation\": \"generation gap\", \"clause\": \"\"}]}";

pub fn request_body(model: &str, word: &str, brief: Option<&str>, collocations: &str, count: usize, style: &Style) -> Value {
    json!({
        "model": model,
        "max_tokens": 4000,
        "messages": [
            { "role": "system", "content": format!("{SYSTEM}\n\n{JSON_FORMAT}") },
            { "role": "user", "content": prompt(word, brief, collocations, count, style) }
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

/// 解析 OpenAI 兼容的 chat/completions 响应（正式调用走 settle；这里留给测试检查解析规则）
#[cfg(test)]
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

fn http_error(status: u16, body: &str) -> Error {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string))
        // 没有标准的错误信息时带上返回内容的开头，方便排查
        .unwrap_or_else(|| body.trim().chars().take(200).collect());
    let msg = match status {
        401 => crate::i18n::t("API Key 无效，请在设置 › AI 例句里重新填写", "The API key is invalid. Re-enter it in Settings › AI examples").to_string(),
        402 => crate::i18n::t("账户余额不足，请先充值", "Insufficient balance. Please top up your account").to_string(),
        403 => crate::tf!("没有权限：{}", "Permission denied: {}", detail),
        404 => crate::tf!("找不到模型：{}", "Model not found: {}", detail),
        429 => crate::i18n::t("请求太频繁或额度不足，稍后再试", "Rate limited or out of credit. Try again later").to_string(),
        500..=599 => crate::tf!("{} 服务暂时不可用，稍后再试", "The {} service is temporarily unavailable. Try again later", NAME),
        _ => crate::tf!("请求失败（{}）：{}", "Request failed ({}): {}", status, detail),
    };
    Error::Online(msg)
}

/// 发给 DeepSeek 的请求：认证头和 JSON 正文。`.json()` 已经带了 Content-Type，不能再加一个——
/// 重复的 Content-Type 会被 DeepSeek 拒绝（415）
pub fn request(client: &reqwest::Client, key: &str, body: &Value) -> reqwest::RequestBuilder {
    client.post(ENDPOINT).bearer_auth(key).json(body)
}

/// 连通性测试：用当前模型发一句最短的请求（输出最多 16 个 token），确认网络、API Key、模型名都没问题。返回用量
pub async fn ping(client: &reqwest::Client, key: &str, model: &str) -> Result<Usage> {
    let body = json!({
        "model": model,
        "max_tokens": 16,
        "messages": [{ "role": "user", "content": "Reply with the single word OK." }],
        "stream": false
    });
    let resp = request(client, key, &body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| Error::Online(crate::tf!("连不上 {} API：{}", "Could not reach the {} API: {}", NAME, e)))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| Error::Online(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(http_error(status, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| Error::Online(e.to_string()))?;
    Ok(usage_of(&v))
}

/// 发送请求（body 来自 request_body）并解析。用量当场记账（失败的调用也记，见 settle）
pub async fn generate(client: &reqwest::Client, db: &crate::db::Db, key: &str, label: &str, body: &Value) -> Result<Vec<Example>> {
    let resp = request(client, key, body)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| Error::Online(crate::tf!("连不上 {} API：{}", "Could not reach the {} API: {}", NAME, e)))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| Error::Online(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(http_error(status, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| Error::Online(e.to_string()))?;
    let model = body["model"].as_str().unwrap_or_default().to_string();
    let (out, _) = db.with_conn(|c| settle(c, &model, label, &v, crate::db::now_ms()))?;
    parse_examples(&out)
}

/// 模型已经回话（钱已经花了）：先把这次用量记进 ai_usage，再检查有没有截断、是不是空内容。
/// 这样截断、空内容、后面解析失败的调用也都算进用量统计。返回模型输出的文本和这次花费（美元）
pub fn settle(conn: &Connection, model: &str, label: &str, v: &Value, now: i64) -> Result<(String, f64)> {
    let cost = record(conn, model, label, usage_of(v), now)?;
    if v.pointer("/choices/0/finish_reason").and_then(Value::as_str) == Some("length") {
        return Err(truncated_error());
    }
    let out = v
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        // JSON Output 模式偶尔返回空内容（官方文档注明），提示重试
        .ok_or_else(|| Error::Online(crate::i18n::t("模型返回了空内容，请重试", "The model returned an empty reply. Please try again").into()))?;
    Ok((out.to_string(), cost))
}

fn truncated_error() -> Error {
    Error::Online(crate::i18n::t("输出被截断，请重试", "The output was cut off. Please try again").into())
}

/// 是不是「输出超过 max_tokens 被截断」：调用方可以把请求拆小再试
pub fn is_truncated(e: &Error) -> bool {
    matches!((e, &truncated_error()), (Error::Online(a), Error::Online(b)) if a == b)
}

/// 通用的 JSON 输出调用（读前导读、精读本、拆解、英文怎么说用）：`system`、`user` 两段提示，JSON 模式输出
/// （`json_format` 是提示里要给的格式示例）。用量以 `label` 当场记账（失败也记）。返回模型输出的 JSON 文本和这次花费（美元）
#[allow(clippy::too_many_arguments)]
pub async fn complete_json(
    client: &reqwest::Client,
    db: &crate::db::Db,
    key: &str,
    model: &str,
    label: &str,
    system: &str,
    user: &str,
    json_format: &str,
    max_tokens: u32,
    timeout: Duration,
) -> Result<(String, f64)> {
    let body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": [
            { "role": "system", "content": format!("{system}\n\n{json_format}") },
            { "role": "user", "content": user }
        ],
        "response_format": { "type": "json_object" },
        "stream": false
    });
    let resp = request(client, key, &body)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| Error::Online(crate::tf!("连不上 {} API：{}", "Could not reach the {} API: {}", NAME, e)))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| Error::Online(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(http_error(status, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| Error::Online(e.to_string()))?;
    db.with_conn(|c| settle(c, model, label, &v, crate::db::now_ms()))
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

pub fn record(conn: &Connection, model: &str, word: &str, u: Usage, now: i64) -> Result<f64> {
    let cost = cost_usd(model, u, now);
    conn.execute(
        "INSERT INTO ai_usage (provider, model, word, input_tokens, cached_tokens, output_tokens, cost_usd, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![NAME.to_lowercase(), model, word, u.input as i64, u.cached as i64, u.output as i64, cost, now],
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

pub fn cached(conn: &Connection, word: &str, model: &str, style: &str) -> Result<Option<(Vec<Example>, i64)>> {
    let row: Option<(String, i64)> = conn
        .query_row("SELECT payload, created_at FROM ai_examples WHERE word = ?1 AND model = ?2 AND style = ?3", params![word, model, style], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    Ok(row.and_then(|(p, t)| serde_json::from_str(&p).ok().map(|v| (v, t))))
}

pub fn store(conn: &Connection, word: &str, model: &str, style: &str, examples: &[Example], now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO ai_examples (word, model, style, payload, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (word, model, style) DO UPDATE SET payload = excluded.payload, created_at = excluded.created_at",
        params![word, model, style, serde_json::to_string(examples)?, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn request_carries_one_content_type() {
        // 两个 Content-Type 会被 DeepSeek 拒绝（415）
        let c = reqwest::Client::new();
        let r = request(&c, "k", &json!({})).build().unwrap();
        assert_eq!(r.headers().get_all("content-type").iter().count(), 1);
    }

    #[test]
    fn request_and_response() -> Result<()> {
        let b = request_body("deepseek-flash", "generation", Some("一代人"), "noun (n): generation gap", 3, &Style::default());
        assert_eq!(b["response_format"]["type"], "json_object");
        let sys = b["messages"][0]["content"].as_str().unwrap_or_default();
        assert!(sys.to_lowercase().contains("json") && sys.contains("\"examples\""), "JSON Output 要求提示里有 json 和格式示例");
        let p = b["messages"][1]["content"].as_str().unwrap_or_default();
        assert!(p.contains("generation gap") && p.contains("一代人") && p.contains("Write 3"));
        let ok = json!({"choices": [{"finish_reason": "stop", "message": {"content": "{\"examples\":[{\"en\":\"Mind the gap.\",\"zh\":\"小心空隙。\",\"collocation\":\"mind the gap\"}]}"}}]});
        assert_eq!(parse_chat_response(&ok)?[0].zh, "小心空隙。");
        assert!(parse_chat_response(&json!({"choices": [{"finish_reason": "stop", "message": {"content": ""}}]})).is_err());
        assert!(parse_chat_response(&json!({"choices": [{"finish_reason": "length", "message": {"content": "{"}}]})).is_err());
        assert!(parse_chat_response(&json!({"choices": [{"finish_reason": "stop", "message": {"content": "not json"}}]})).is_err());
        assert!(matches!(http_error(401, ""), Error::Online(m) if m.contains("API Key")));
        Ok(())
    }

    #[test]
    fn usage_and_cost() -> Result<()> {
        let ds = json!({"usage": {"prompt_tokens": 900, "prompt_cache_hit_tokens": 100, "prompt_cache_miss_tokens": 800, "completion_tokens": 500}});
        let u = usage_of(&ds);
        assert_eq!(u, Usage { input: 800, cached: 100, output: 500 });
        // 2026-10-21 是周三，UTC 07:00 高峰；同日 UTC 12:00 非高峰半价
        let peak = 1_792_566_000_000; // 2026-10-21T07:00:00Z
        assert!(deepseek_peak(peak));
        assert!(!deepseek_peak(peak + 5 * 3_600_000));
        let full = cost_usd("deepseek-flash", u, peak);
        assert!((cost_usd("deepseek-flash", u, peak + 5 * 3_600_000) * 2.0 - full).abs() < 1e-12);
        assert_eq!(cost_usd("unknown-model", u, 0), 0.0);

        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let one = Usage { input: 1_000_000, cached: 0, output: 0 };
            record(c, "deepseek-flash", "a", one, peak - 100)?;
            record(c, "deepseek-flash", "b", one, peak)?;
            let s = summary(c, peak - 50)?;
            assert_eq!((s.total.calls, s.month.calls, s.models.len()), (2, 1, 1));
            assert!((s.total.cost_usd - 0.60).abs() < 1e-9);
            Ok(())
        })
    }

    #[test]
    fn caches_per_word_and_model() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let ex = vec![Example { en: "a".into(), zh: "甲".into(), collocation: "x".into(), clause: String::new() }];
            store(c, "word", "m1", "", &ex, 1)?;
            store(c, "word", "m1", "", &ex, 2)?;
            assert_eq!(cached(c, "word", "m1", "")?.map(|(_, t)| t), Some(2));
            assert!(cached(c, "word", "m2", "")?.is_none());
            Ok(())
        })
    }

    #[test]
    fn style_key_is_empty_for_default_and_normalized() {
        assert_eq!(Style::default().key(), "");
        assert_eq!(Style::new("medium", &[]).key(), "");
        // 未知长度当适中；未知句型丢掉；按清单顺序、去重
        assert_eq!(Style::new("huge", &["bogus".into()]).key(), "");
        let s = Style::new("short", &["time".into(), "subject".into(), "time".into(), "x".into()]);
        assert_eq!(s.clauses, ["subject", "time"]);
        assert_eq!(s.key(), "short|subject,time");
        assert_eq!(Style::new("long", &[]).key(), "long|");
        assert_eq!(CLAUSES.len(), 15);
    }

    #[test]
    fn prompt_has_length_and_clauses() {
        let p = prompt("abroad", None, "", 5, &Style::default());
        assert!(p.contains("8–22 words") && p.contains("Write 5"));
        let p = prompt("abroad", None, "", 5, &Style::new("short", &[]));
        assert!(p.contains("5–10 words"));
        let p = prompt("abroad", None, "", 5, &Style::new("long", &["subject".into(), "concession".into()]));
        assert!(p.contains("20–35 words"));
        // 有句型时每种一句：句数 = 句型数，不再按 count
        assert!(p.contains("Write 2") && !p.contains("Write 5"));
        assert!(p.contains("subject") && p.contains("concession") && p.contains("subject clause"));
        let b = request_body("m", "abroad", None, "", 5, &Style::new("short", &["time".into()]));
        let sys = b["messages"][0]["content"].as_str().unwrap_or_default();
        assert!(sys.contains("\"clause\""), "格式示例里要有 clause 字段");
        assert!(!sys.contains("8–22"), "长度不再写死在系统提示里");
    }

    #[test]
    fn cache_is_keyed_by_style_and_old_payload_reads() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let ex = vec![Example { en: "a".into(), zh: "甲".into(), collocation: String::new(), clause: "subject".into() }];
            store(c, "abroad", "m", "short|subject", &ex, 1)?;
            assert!(cached(c, "abroad", "m", "")?.is_none());
            assert_eq!(cached(c, "abroad", "m", "short|subject")?.map(|(v, _)| v[0].clause.clone()), Some("subject".into()));
            // 旧版存的 payload 没有 clause 字段
            c.execute("INSERT INTO ai_examples (word, model, style, payload, created_at) VALUES ('old', 'm', '', '[{\"en\":\"x\",\"zh\":\"y\",\"collocation\":\"z\"}]', 5)", [])?;
            assert_eq!(cached(c, "old", "m", "")?.map(|(v, _)| v[0].clause.clone()), Some(String::new()));
            Ok(())
        })
    }

    #[test]
    fn migration_0019_keeps_old_rows() -> Result<()> {
        let c = Connection::open_in_memory()?;
        c.execute_batch(include_str!("../migrations/0004_ai.sql"))?;
        c.execute("INSERT INTO ai_examples (word, model, payload, created_at) VALUES ('w', 'm', '[]', 3)", [])?;
        c.execute_batch(include_str!("../migrations/0019_ai_example_style.sql"))?;
        assert_eq!(cached(&c, "w", "m", "")?.map(|(_, t)| t), Some(3));
        store(&c, "w", "m", "long|", &[], 4)?;
        assert_eq!(cached(&c, "w", "m", "")?.map(|(_, t)| t), Some(3), "不同样式各存一份");
        Ok(())
    }

    #[test]
    fn failed_calls_are_still_charged() -> Result<()> {
        let db = Db::open_in_memory()?;
        let usage = json!({"prompt_tokens": 100, "prompt_cache_hit_tokens": 0, "prompt_cache_miss_tokens": 100, "completion_tokens": 3000});
        let rows = |c: &Connection| -> Result<i64> { Ok(c.query_row("SELECT COUNT(*) FROM ai_usage", [], |r| r.get(0))?) };
        db.with_conn(|c| {
            // 输出被截断：钱已经花了，要记账，同时报「截断」让调用方拆小重试
            let cut = json!({"choices": [{"finish_reason": "length", "message": {"content": "{"}}], "usage": usage});
            let e = settle(c, "deepseek-flash", "长句", &cut, 1).unwrap_err();
            assert!(is_truncated(&e));
            assert_eq!(rows(c)?, 1);
            // 返回空内容：同样记账
            let empty = json!({"choices": [{"finish_reason": "stop", "message": {"content": " "}}], "usage": usage});
            assert!(settle(c, "deepseek-flash", "空", &empty, 2).is_err());
            assert_eq!(rows(c)?, 2);
            // 正常：返回内容和花费
            let ok = json!({"choices": [{"finish_reason": "stop", "message": {"content": "{\"a\":1}"}}], "usage": usage});
            let (text, cost) = settle(c, "deepseek-flash", "好", &ok, 3)?;
            assert_eq!(text, "{\"a\":1}");
            assert!(cost > 0.0);
            assert_eq!(rows(c)?, 3);
            Ok(())
        })
    }
}
