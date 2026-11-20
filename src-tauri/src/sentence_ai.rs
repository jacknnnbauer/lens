//! 长难句拆解（DeepSeek）：提示、解析与校验、按句缓存。
//! 模型只摘出原文片段（不给字符位置），前端在原句里找位置。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Piece {
    pub text: String,
    #[serde(default)]
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub text: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub modifies: String,
    #[serde(default)]
    pub zh: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub title: String,
    #[serde(default)]
    pub explain: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Breakdown {
    pub translation: String,
    #[serde(default)]
    pub skeleton: Vec<Piece>,
    #[serde(default)]
    pub parts: Vec<Part>,
    #[serde(default)]
    pub points: Vec<Point>,
}

pub const SYSTEM: &str = "你是给中国英语学习者讲解长难句的老师。把用户给的英文句子拆开讲清楚：先找出主干（主谓宾 / 主系表），再按出现顺序列出各个从句和重要的短语（非谓语、介词短语、插入语、同位语等），说明类型、修饰或依附哪个词、这一部分的中文意思；然后给出通顺的整句中文译文；最后挑出 1 到 3 个中国学习者最容易卡住的难点（固定搭配、省略、倒装、指代、特殊句式等），每个用一两句话讲清楚。\n\n规则：\n1. 所有 text 字段必须是原句里一字不差的连续原文，不改写、不补词、不加省略号。\n2. skeleton 只放主干成分，role 用：主语、谓语、宾语、表语、宾补、间接宾语。\n3. parts 按在原句中出现的顺序排列；kind 写具体类型，如「定语从句」「宾语从句」「让步状语从句」「时间状语从句」「非谓语短语（现在分词）」「介词短语」「插入语」「同位语」；modifies 写它修饰或依附的原文词语，没有就留空。\n4. 只讲用户给的那句；前后句只用来判断指代，不要拆。\n5. 全部用简体中文讲解。";

pub const JSON_FORMAT: &str = r#"只输出 JSON，格式：
{"translation": "整句中文译文",
 "skeleton": [{"text": "原文片段", "role": "主语"}],
 "parts": [{"text": "原文片段", "kind": "定语从句", "modifies": "被修饰的原文词语", "zh": "这一部分的中文"}],
 "points": [{"title": "难点名称", "explain": "一两句讲解"}]}"#;

/// 用户消息：要拆的句子，加上前后句（仅供判断指代）
pub fn prompt(sentence: &str, before: Option<&str>, after: Option<&str>) -> String {
    let mut out = format!("要拆解的句子：\n{}\n", sentence.trim());
    let ctx: Vec<String> = [("前一句", before), ("后一句", after)]
        .into_iter()
        .filter_map(|(k, v)| v.map(str::trim).filter(|v| !v.is_empty()).map(|v| format!("{k}：{v}")))
        .collect();
    if !ctx.is_empty() {
        out.push_str(&format!("\n上下文（仅供参考，不要拆）：\n{}\n", ctx.join("\n")));
    }
    out
}

/// 解析模型输出：容忍 ```json 包裹和缺字段；去掉空片段，难点最多 3 条；没有译文算失败
pub fn parse(text: &str) -> Result<Breakdown> {
    let t = text.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    let bad = || Error::Online(crate::i18n::t("这次没拆好，再试一次", "That breakdown didn't come out right. Please try again").into());
    let mut b: Breakdown = serde_json::from_str(t).map_err(|_| bad())?;
    b.translation = b.translation.trim().to_string();
    if b.translation.is_empty() {
        return Err(bad());
    }
    b.skeleton.retain(|p| !p.text.trim().is_empty());
    b.parts.retain(|p| !p.text.trim().is_empty());
    b.points.retain(|p| !p.title.trim().is_empty());
    b.points.truncate(3);
    Ok(b)
}

pub fn cached(conn: &Connection, sentence: &str, model: &str) -> Result<Option<(Breakdown, i64)>> {
    let row: Option<(String, i64)> = conn
        .query_row("SELECT payload, created_at FROM sentence_breakdown WHERE sentence = ?1 AND model = ?2", params![sentence, model], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    Ok(row.and_then(|(p, at)| serde_json::from_str(&p).ok().map(|b| (b, at))))
}

pub fn store(conn: &Connection, sentence: &str, model: &str, b: &Breakdown, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO sentence_breakdown (sentence, model, payload, created_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(sentence, model) DO UPDATE SET payload = excluded.payload, created_at = excluded.created_at",
        params![sentence, model, serde_json::to_string(b)?, now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{"translation":"那是个让人人坐立不安的早晨。","skeleton":[{"text":"It","role":"主语"},{"text":"was","role":"谓语"},{"text":"the kind of morning","role":"表语"}],
"parts":[{"text":"that made everyone restless","kind":"定语从句","modifies":"morning","zh":"让每个人都坐立不安"}],
"points":[{"title":"make + 宾语 + 形容词","explain":"使……变得……"}]}"#;

    #[test]
    fn parses_full_reply() -> Result<()> {
        let b = parse(FULL)?;
        assert_eq!(b.skeleton.len(), 3);
        assert_eq!(b.parts[0].modifies, "morning");
        assert_eq!(b.points[0].title, "make + 宾语 + 形容词");
        Ok(())
    }

    #[test]
    fn parses_fenced_and_missing_fields() -> Result<()> {
        let b = parse("```json\n{\"translation\":\"译文\",\"skeleton\":[{\"text\":\"He\"}]}\n```")?;
        assert_eq!(b.translation, "译文");
        assert_eq!(b.skeleton[0].role, "");
        assert!(b.parts.is_empty() && b.points.is_empty());
        Ok(())
    }

    #[test]
    fn rejects_empty_translation() {
        assert!(parse(r#"{"translation":"  ","skeleton":[]}"#).is_err());
        assert!(parse("not json at all").is_err());
    }

    #[test]
    fn drops_blank_pieces_and_caps_points() -> Result<()> {
        let b = parse(
            r#"{"translation":"t","skeleton":[{"text":" ","role":"主语"},{"text":"was","role":"谓语"}],"parts":[{"text":"","kind":"x"}],
            "points":[{"title":"a"},{"title":"b"},{"title":"c"},{"title":"d"},{"title":" "}]}"#,
        )?;
        assert_eq!(b.skeleton.len(), 1);
        assert!(b.parts.is_empty());
        assert_eq!(b.points.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        Ok(())
    }

    #[test]
    fn prompt_marks_context_as_reference() {
        let p = prompt("The long one.", Some("Before it."), None);
        assert!(p.contains("The long one."));
        assert!(p.contains("Before it."));
        assert!(p.contains("仅供参考"));
        assert!(!prompt("Only.", None, None).contains("仅供参考"));
        assert!(!SYSTEM.is_empty() && JSON_FORMAT.contains("translation"));
    }

    #[test]
    fn cache_round_trip() -> Result<()> {
        let db = crate::db::Db::open_in_memory()?;
        db.with_conn(|c| {
            assert_eq!(cached(c, "S.", "m")?, None);
            let b = parse(FULL)?;
            store(c, "S.", "m", &b, 42)?;
            assert_eq!(cached(c, "S.", "m")?, Some((b.clone(), 42)));
            assert_eq!(cached(c, "S.", "other")?, None);
            store(c, "S.", "m", &b, 43)?;
            assert_eq!(cached(c, "S.", "m")?.map(|x| x.1), Some(43), "重新拆解覆盖旧的");
            Ok(())
        })
    }
}
