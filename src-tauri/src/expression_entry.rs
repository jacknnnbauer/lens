//! 英文怎么说 · 更多说法（DeepSeek）：一个短的中文说法在英语里有哪些讲法——
//! 按强度或场景分组、每条带中文语感，最后一段小对话和一个值得整块记的表达。
//! 按 输入 + 模型 缓存（和对象无关），同样的请求不再花钱。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// 组数、每组条数、全篇条数、对话句数的上下限（多了截掉，少了算没写好）
pub const MAX_GROUPS: usize = 4;
pub const MIN_GROUPS: usize = 2;
pub const MAX_ITEMS: usize = 6;
pub const MAX_TOTAL: usize = 18;
pub const MAX_DIALOGUE: usize = 4;
pub const MIN_DIALOGUE: usize = 2;
/// 只认这两个标签，其余丢掉
pub const TAGS: [&str; 2] = ["粗口", "正式"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryItem {
    #[serde(default)]
    pub en: String,
    #[serde(default)]
    pub zh: String,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub star: bool,
    #[serde(default)]
    pub when: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryGroup {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub items: Vec<EntryItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogLine {
    #[serde(default)]
    pub who: String,
    #[serde(default)]
    pub en: String,
    #[serde(default)]
    pub zh: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(default)]
    pub groups: Vec<EntryGroup>,
    #[serde(default)]
    pub dialogue: Vec<DialogLine>,
    #[serde(default)]
    pub tip: String,
}

pub const SYSTEM: &str = "你帮中国的英语学习者弄懂：一个中文说法（如「气死我了」「真服了」「随便吧」）在英语里有哪些地道讲法。\n\n要求：\n1. 按情绪强度或使用场景分 2–4 组（通常 3 组）。组名用很短的中文口语，比如「烦死了」「气炸了」「无奈又恼火」，按这个说法本身来起，不要套模板。\n2. 每组 3–6 条，全篇不超过 18 条。en 是母语者真会说的完整说法；zh 写这句的中文语感（意译，像中国人会怎么说），不要逐字直译。\n3. 只有粗口（如 pissed、crap、hell）才把 tag 写成「粗口」，只有明显正式、书面才写「正式」，其余 tag 留空。\n4. 全篇挑 3–5 条美式日常最常用的，star 设为 true。\n5. 只有需要说明场景时才写 when，以「适合」开头，不超过 15 字。\n6. dialogue：2–4 句 A、B 轮流的小对话，自然地用上前面的几个说法，每句带 zh 翻译。\n7. tip：一个最值得整块记住的表达，加半句说明，不超过 30 字。\n8. 不要编造出处，不要解释语法。";

pub const JSON_FORMAT: &str = r#"只输出 JSON，格式：
{"groups": [{"name": "组名", "items": [{"en": "英文说法", "zh": "中文语感", "tag": "", "star": false, "when": ""}]}],
 "dialogue": [{"who": "A", "en": "英文", "zh": "中文"}],
 "tip": "值得整块记的表达"}"#;

/// 用户消息：要讲的中文说法
pub fn prompt(input: &str) -> String {
    format!("中文说法：\n{}\n", input.trim())
}

/// 解析模型输出：容忍 ```json 包裹；去掉空条目和空组，多了截掉，少了算失败；不认识的标签丢掉
pub fn parse(text: &str) -> Result<Entry> {
    let t = text.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    let bad = || Error::Online(crate::i18n::t("这次没写好，再试一次", "That didn't come out right. Please try again").into());
    let raw: Entry = serde_json::from_str(t).map_err(|_| bad())?;

    let mut groups = Vec::new();
    let mut total = 0;
    for g in raw.groups {
        if groups.len() == MAX_GROUPS || total == MAX_TOTAL {
            break;
        }
        let items: Vec<EntryItem> = g
            .items
            .into_iter()
            .filter(|i| !i.en.trim().is_empty())
            .take(MAX_ITEMS.min(MAX_TOTAL - total))
            .map(|i| {
                let tag = i.tag.trim();
                EntryItem {
                    en: i.en.trim().to_string(),
                    zh: i.zh.trim().to_string(),
                    tag: if TAGS.contains(&tag) { tag.to_string() } else { String::new() },
                    star: i.star,
                    when: i.when.trim().to_string(),
                }
            })
            .collect();
        if items.is_empty() {
            continue;
        }
        total += items.len();
        groups.push(EntryGroup { name: g.name.trim().to_string(), items });
    }

    let dialogue: Vec<DialogLine> = raw
        .dialogue
        .into_iter()
        .filter(|l| !l.en.trim().is_empty())
        .take(MAX_DIALOGUE)
        .map(|l| DialogLine { who: l.who.trim().to_string(), en: l.en.trim().to_string(), zh: l.zh.trim().to_string() })
        .collect();

    if groups.len() < MIN_GROUPS || dialogue.len() < MIN_DIALOGUE {
        return Err(bad());
    }
    Ok(Entry { groups, dialogue, tip: raw.tip.trim().to_string() })
}

pub fn cached(conn: &Connection, input: &str, model: &str) -> Result<Option<(Entry, i64)>> {
    let row: Option<(String, i64)> = conn
        .query_row(
            "SELECT payload, created_at FROM expression_entry WHERE input = ?1 AND model = ?2",
            params![input, model],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(p, at)| serde_json::from_str(&p).ok().map(|e| (e, at))))
}

pub fn store(conn: &Connection, input: &str, model: &str, e: &Entry, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO expression_entry (input, model, payload, created_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(input, model) DO UPDATE SET payload = excluded.payload, created_at = excluded.created_at",
        params![input, model, serde_json::to_string(e)?, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use serde_json::json;

    fn item(en: &str) -> serde_json::Value {
        json!({ "en": format!(" {en} "), "zh": format!(" {en} 语感 "), "tag": "", "star": false, "when": "" })
    }

    fn group(name: &str, n: usize) -> serde_json::Value {
        json!({ "name": format!(" {name} "), "items": (0..n).map(|i| item(&format!("{name}{i}"))).collect::<Vec<_>>() })
    }

    fn lines(n: usize) -> Vec<serde_json::Value> {
        (0..n).map(|i| json!({ "who": if i % 2 == 0 { "A" } else { "B" }, "en": format!(" line {i} "), "zh": format!(" 第 {i} 句 ") })).collect()
    }

    fn reply(groups: Vec<serde_json::Value>, dialogue: Vec<serde_json::Value>) -> String {
        json!({ "groups": groups, "dialogue": dialogue, "tip": " don't even get me started " }).to_string()
    }

    #[test]
    fn parses_groups_dialogue_tip() -> Result<()> {
        let mut g = group("烦", 3);
        g["items"][0]["star"] = json!(true);
        g["items"][1]["when"] = json!(" 适合反复被惹 ");
        let e = parse(&reply(vec![g, group("气", 3), group("无奈", 3)], lines(2)))?;
        assert_eq!(e.groups.len(), 3);
        assert_eq!(e.groups[0].name, "烦");
        assert_eq!((e.groups[0].items[0].en.as_str(), e.groups[0].items[0].zh.as_str()), ("烦0", "烦0 语感"));
        assert!(e.groups[0].items[0].star && !e.groups[0].items[1].star);
        assert_eq!(e.groups[0].items[1].when, "适合反复被惹");
        assert_eq!(e.dialogue.len(), 2);
        assert_eq!((e.dialogue[1].who.as_str(), e.dialogue[1].en.as_str(), e.dialogue[1].zh.as_str()), ("B", "line 1", "第 1 句"));
        assert_eq!(e.tip, "don't even get me started");
        Ok(())
    }

    #[test]
    fn parse_accepts_fenced_reply() -> Result<()> {
        let e = parse(&format!("```json\n{}\n```", reply(vec![group("a", 3), group("b", 3)], lines(2))))?;
        assert_eq!(e.groups.len(), 2);
        Ok(())
    }

    #[test]
    fn parse_truncates_extras() -> Result<()> {
        let e = parse(&reply(vec![group("a", 8), group("b", 5), group("c", 5), group("d", 5), group("e", 5)], lines(6)))?;
        assert_eq!(e.groups.len(), 4);
        assert_eq!(e.groups[0].items.len(), 6);
        assert_eq!(e.groups.iter().map(|g| g.items.len()).sum::<usize>(), 18);
        assert_eq!(e.dialogue.len(), 4);
        Ok(())
    }

    #[test]
    fn parse_drops_unknown_tag() -> Result<()> {
        let mut g = group("a", 3);
        g["items"][0]["tag"] = json!("俚语");
        g["items"][1]["tag"] = json!(" 正式 ");
        g["items"][2]["tag"] = json!("粗口");
        let e = parse(&reply(vec![g, group("b", 3)], lines(2)))?;
        let tags: Vec<&str> = e.groups[0].items.iter().map(|i| i.tag.as_str()).collect();
        assert_eq!(tags, ["", "正式", "粗口"]);
        Ok(())
    }

    #[test]
    fn parse_rejects_too_little() {
        assert!(parse(&reply(vec![group("a", 3)], lines(2))).is_err());
        assert!(parse(&reply(vec![group("a", 3), group("b", 3)], lines(0))).is_err());
        assert!(parse(&reply(vec![group("a", 3), group("b", 3)], lines(1))).is_err());
        assert!(parse("not json").is_err());
    }

    #[test]
    fn empty_items_and_groups_are_dropped() -> Result<()> {
        let mut empty = group("空", 2);
        empty["items"][0]["en"] = json!("  ");
        empty["items"][1]["en"] = json!("");
        let mut partly = group("半", 3);
        partly["items"][1]["en"] = json!(" ");
        // 只剩 1 个有内容的组：不够
        assert!(parse(&reply(vec![empty.clone(), group("a", 3)], lines(2))).is_err());
        let e = parse(&reply(vec![empty, partly, group("a", 3)], lines(2)))?;
        assert_eq!(e.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["半", "a"]);
        assert_eq!(e.groups[0].items.len(), 2);
        Ok(())
    }

    #[test]
    fn cache_is_keyed_by_input_and_model() -> Result<()> {
        let db = Db::open_in_memory()?;
        let e = parse(&reply(vec![group("a", 3), group("b", 3)], lines(2)))?;
        db.with_conn(|c| store(c, "气死我了", "m", &e, 7))?;
        assert!(db.with_conn(|c| cached(c, "气死我了", "other"))?.is_none());
        assert!(db.with_conn(|c| cached(c, "真服了", "m"))?.is_none());
        assert_eq!(db.with_conn(|c| cached(c, "气死我了", "m"))?, Some((e, 7)));
        Ok(())
    }
}
