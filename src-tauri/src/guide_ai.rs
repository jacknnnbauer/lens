//! 读前导读的 AI 版：维基百科没有这本书、或用户想要时，用设置 › AI 例句里配好的 DeepSeek / Claude 写一份中文导读。
//!
//! - 发给模型的只有书名、作者、章节标题和开头一段正文（约两千字），让它说清楚自己知不知道这本书；
//!   不知道时只根据片段写开头的设定，不许编造情节和评价。
//! - 不剧透时不写情节走向和人物结局。
//! - 结果按书和模型存成文件，同一本书不重复花钱。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::wiki::{Article, BookGuide, Kind, Section};

/// 正文片段最多这么多字符
pub const EXCERPT_CHARS: usize = 6000;
/// 章节标题最多列这么多个
pub const MAX_PARTS: usize = 80;

pub struct Input<'a> {
    pub title: &'a str,
    pub author: Option<&'a str>,
    pub parts: &'a [String],
    pub excerpt: &'a str,
    pub spoilers: bool,
}

pub const SYSTEM: &str = "你是给中国英语学习者写英文原著读前导读的编辑。读者还没读这本书，要先了解背景再读原文。\
用简体中文写，平实准确，不夸张。只写你确实知道的事实：如果你并不了解这本书，就把 known 设为 false，\
只根据给出的章节标题和开头片段介绍开头的设定和读法，不要编造情节、人物结局、出版史或评价。";

/// 用户消息：书名、作者、章节、开头片段和剧透要求
pub fn prompt(i: &Input) -> String {
    let mut p = format!("书名：{}\n", i.title);
    if let Some(a) = i.author.filter(|a| !a.trim().is_empty()) {
        p.push_str(&format!("作者：{a}\n"));
    }
    if !i.parts.is_empty() {
        let list: Vec<&str> = i.parts.iter().take(MAX_PARTS).map(String::as_str).collect();
        p.push_str(&format!("章节标题：{}\n", list.join(" | ")));
    }
    if !i.excerpt.trim().is_empty() {
        p.push_str(&format!("\n开头片段：\n{}\n", i.excerpt.trim()));
    }
    p.push_str(if i.spoilers {
        "\n可以剧透：characters 介绍主要人物，plot 用三五段写完整的情节梗概（包括结局）。"
    } else {
        "\n不要剧透：characters 和 plot 留空字符串；其他各项也不要透露情节走向和人物结局。"
    });
    p.push_str("\n各项都写成一到三段，每段一百到两百字；不知道的项留空字符串。");
    p
}

pub fn schema() -> Value {
    let s = json!({ "type": "string" });
    json!({
        "type": "object",
        "properties": {
            "known": { "type": "boolean" },
            "overview": s, "author": s, "background": s, "characters": s, "plot": s,
            "themes": s, "reading_tips": s, "reception": s
        },
        "required": ["known", "overview", "author", "background", "characters", "plot", "themes", "reading_tips", "reception"],
        "additionalProperties": false
    })
}

/// DeepSeek 的 JSON 模式要在提示里给出格式
pub const JSON_FORMAT: &str = "只输出 JSON，格式：{\"known\": true, \"overview\": \"这本书讲什么、为什么值得读\", \"author\": \"作者简介\", \"background\": \"创作背景和时代\", \"characters\": \"\", \"plot\": \"\", \"themes\": \"主题与风格\", \"reading_tips\": \"读这本书的建议：难点、可以快读的部分\", \"reception\": \"评价与地位\"}";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Out {
    pub known: bool,
    pub overview: String,
    pub author: String,
    pub background: String,
    pub characters: String,
    pub plot: String,
    pub themes: String,
    pub reading_tips: String,
    pub reception: String,
}

pub fn parse(text: &str) -> Result<Out> {
    serde_json::from_str(text).map_err(|e| Error::Online(crate::tf!("无法解析模型输出：{}", "Could not parse the model output: {}", e)))
}

/// 模型的输出 → 和维基百科同样结构的导读（不剧透时丢掉情节和人物，以防模型没照做）
pub fn to_guide(out: &Out, title: &str, author: Option<&str>, model: &str, spoilers: bool) -> BookGuide {
    let mut sections = Vec::new();
    let mut push = |kind: Kind, heading: &str, text: &str| {
        if !text.trim().is_empty() {
            sections.push(Section { kind, heading: heading.to_string(), text: text.trim().to_string() });
        }
    };
    push(Kind::Lead, "", &out.overview);
    push(Kind::Background, "", &out.background);
    let hidden = !spoilers;
    if spoilers {
        push(Kind::Characters, "", &out.characters);
        push(Kind::Plot, "", &out.plot);
    }
    push(Kind::Themes, "", &out.themes);
    push(Kind::Other, crate::i18n::t("阅读建议", "Reading tips"), &out.reading_tips);
    push(Kind::Reception, "", &out.reception);
    let author_article = author.filter(|_| !out.author.trim().is_empty()).map(|a| Article {
        lang: "zh".into(),
        title: a.to_string(),
        url: String::new(),
        sections: vec![Section { kind: Kind::Lead, heading: String::new(), text: out.author.trim().to_string() }],
    });
    BookGuide {
        book: Article { lang: "zh".into(), title: title.to_string(), url: String::new(), sections },
        author: author_article,
        plot_hidden: hidden,
        grokipedia: String::new(),
        ai: Some(crate::wiki::AiNote { model: model.to_string(), known: out.known }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out() -> Out {
        Out {
            known: true,
            overview: "一部关于捕鲸的小说。".into(),
            author: "梅尔维尔是美国作家。".into(),
            background: "1851 年出版。".into(),
            characters: "亚哈船长……".into(),
            plot: "最后船沉了。".into(),
            themes: "执念。".into(),
            reading_tips: "鲸类学章节可以快读。".into(),
            reception: "".into(),
        }
    }

    #[test]
    fn spoilers_are_dropped_even_if_the_model_wrote_them() {
        let g = to_guide(&out(), "Moby-Dick", Some("Herman Melville"), "deepseek-flash", false);
        let kinds: Vec<Kind> = g.book.sections.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [Kind::Lead, Kind::Background, Kind::Themes, Kind::Other]);
        assert!(g.plot_hidden);
        assert_eq!(g.author.as_ref().map(|a| a.title.as_str()), Some("Herman Melville"));
        assert_eq!(g.ai.as_ref().map(|a| a.known), Some(true));
        let g = to_guide(&out(), "Moby-Dick", None, "m", true);
        assert!(g.book.sections.iter().any(|s| s.kind == Kind::Plot) && !g.plot_hidden);
        assert!(g.author.is_none(), "没有作者名时不放作者");
    }

    #[test]
    fn prompt_carries_the_spoiler_rule_and_excerpt() {
        let parts = vec!["Loomings".to_string(), "The Carpet-Bag".to_string()];
        let p = prompt(&Input { title: "Moby-Dick", author: Some("Herman Melville"), parts: &parts, excerpt: "Call me Ishmael.", spoilers: false });
        assert!(p.contains("书名：Moby-Dick") && p.contains("作者：Herman Melville") && p.contains("Loomings | The Carpet-Bag") && p.contains("Call me Ishmael."));
        assert!(p.contains("不要剧透"));
        let parsed = parse(r#"{"known": false, "overview": "x"}"#).expect("缺的项按空字符串");
        assert!(!parsed.known && parsed.plot.is_empty());
    }
}
