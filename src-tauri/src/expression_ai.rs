//! 英文怎么说（DeepSeek）：给一个中文意思（或一句英文），按五种风格给出英文说法；可选对象。
//! 按 输入 + 对象描述 + 模型 缓存，同样的请求不再花钱。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// 五种风格，固定顺序
pub const STYLES: [&str; 5] = ["简单", "地道", "母语级", "幽默", "含蓄"];

/// 输入最长多少字（再长让用户挑一两句）
pub const MAX_CHARS: usize = 600;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub style: String,
    #[serde(default)]
    pub en: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expression {
    pub items: Vec<Item>,
}

pub const SYSTEM: &str = "你帮中国人把想说的意思用英文说出来。用户给的通常是一个中文意思：可能是一个短语（如「气死我了」）、一句话，或带着目的和顾虑的几句话。请意译：意思完整、语气合适比贴着中文字面重要，绝不要逐字直译。如果用户给的是英文，就把它改写成下面五种风格。\n\n五种风格（每种都要给，而且要有实际区别，不能只是换几个同义词）：\n- 简单：最基础的词和句型，英语一般的人也能看懂。\n- 地道：母语者日常最常用的自然说法。\n- 母语级：更有味道的地道表达，习语、短语动词都可以用。\n- 幽默：轻松、自嘲或俏皮，缓和气氛。\n- 含蓄：委婉、留余地，适合职场或敏感场合。\n\n每种给一句（或两三句）英文 en，再用一句简体中文 note 说明怎么处理的、适合什么场合，不超过 30 字。";

pub const JSON_FORMAT: &str = r#"只输出 JSON，格式（五项，style 依次为 简单、地道、母语级、幽默、含蓄）：
{"items": [{"style": "简单", "en": "英文说法", "note": "一句中文说明"}]}"#;

/// 用户消息：要表达的意思，加上对象（可选）
pub fn prompt(input: &str, audience: Option<&str>) -> String {
    let mut out = format!("要表达的意思：\n{}\n", input.trim());
    if let Some(a) = audience.map(str::trim).filter(|a| !a.is_empty()) {
        out.push_str(&format!("\n说话的对象：{a}\n五种说法都按这个对象调整用词、长度、礼貌程度和先说什么。\n"));
    }
    out
}

/// 解析模型输出：容忍 ```json 包裹；按 STYLES 排好；缺一种或英文为空算失败
pub fn parse(text: &str) -> Result<Expression> {
    let t = text.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    let bad = || Error::Online(crate::i18n::t("这次没写好，再试一次", "That didn't come out right. Please try again").into());
    let raw: Expression = serde_json::from_str(t).map_err(|_| bad())?;
    let mut items = Vec::with_capacity(STYLES.len());
    for style in STYLES {
        let it = raw.items.iter().find(|i| i.style.trim() == style).ok_or_else(bad)?;
        let en = it.en.trim().to_string();
        if en.is_empty() {
            return Err(bad());
        }
        items.push(Item { style: style.to_string(), en, note: it.note.trim().to_string() });
    }
    Ok(Expression { items })
}

pub fn cached(conn: &Connection, input: &str, audience: &str, model: &str) -> Result<Option<(Expression, i64)>> {
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT payload, created_at FROM expression WHERE input = ?1 AND audience = ?2 AND model = ?3",
            params![input, audience, model],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(p, at)| serde_json::from_str(&p).ok().map(|e| (e, at))))
}

pub fn store(conn: &Connection, input: &str, audience: &str, model: &str, e: &Expression, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO expression (input, audience, model, payload, created_at) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(input, audience, model) DO UPDATE SET payload = excluded.payload, created_at = excluded.created_at",
        params![input, audience, model, serde_json::to_string(e)?, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn reply(styles: &[&str]) -> String {
        let items: Vec<String> = styles.iter().map(|s| format!(r#"{{"style":"{s}","en":" {s} en ","note":" {s} 说明 "}}"#)).collect();
        format!(r#"{{"items":[{}]}}"#, items.join(","))
    }

    #[test]
    fn parses_five_styles_in_order() -> Result<()> {
        let e = parse(&reply(&["含蓄", "简单", "幽默", "地道", "母语级"]))?;
        assert_eq!(e.items.iter().map(|i| i.style.as_str()).collect::<Vec<_>>(), STYLES);
        assert_eq!((e.items[0].en.as_str(), e.items[0].note.as_str()), ("简单 en", "简单 说明"));
        Ok(())
    }

    #[test]
    fn rejects_missing_or_unknown_style() {
        assert!(parse(&reply(&["简单", "地道", "母语级", "幽默"])).is_err());
        assert!(parse(&reply(&["简单", "地道", "母语级", "幽默", "正式"])).is_err());
        assert!(parse("not json").is_err());
    }

    #[test]
    fn parses_fenced_reply() -> Result<()> {
        let e = parse(&format!("```json\n{}\n```", reply(&STYLES)))?;
        assert_eq!(e.items.len(), 5);
        Ok(())
    }

    #[test]
    fn prompt_mentions_audience_only_when_given() {
        assert!(!prompt("气死我了", None).contains("对象"));
        assert!(!prompt("气死我了", Some("  ")).contains("对象"));
        assert!(prompt("x", Some("约旦分包商：英语一般")).contains("约旦分包商：英语一般"));
    }

    #[test]
    fn cache_is_keyed_by_audience() -> Result<()> {
        let db = Db::open_in_memory()?;
        let e = parse(&reply(&STYLES))?;
        db.with_conn(|c| store(c, "气死我了", "", "m", &e, 1))?;
        assert!(db.with_conn(|c| cached(c, "气死我了", "约旦分包商", "m"))?.is_none());
        assert_eq!(db.with_conn(|c| cached(c, "气死我了", "", "m"))?.map(|(x, at)| (x.items.len(), at)), Some((5, 1)));
        Ok(())
    }
}
