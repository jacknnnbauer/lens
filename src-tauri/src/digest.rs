//! 精读本（全书预习包第三期，「替代版」）：AI 通读全书，写出读完就可以不读原书的分析。
//!
//! 按《How to Read a Book》分析阅读的四问和读小说的规则组织：整体（一句话、一段话、三分钟讲解稿、结构）
//! → 人物与世界 → 逐章提要和书中原文 → 主题与评价 → 对话准备和细节自测。
//!
//! 分两步：
//! 1. 通读：正文按篇幅分成若干段（每段几章、约八千词）逐段发给模型，每章写提要、关键事件，挑出原文名段。
//!    原文必须逐字取自书：模型给的句子在本章正文里找得到才收，收的是书里的原文（不是模型抄写的版本），找不到就丢掉。
//! 2. 综合：把各章提要和收下的原文（带编号）发给模型，分四次写整体与人物、世界与主题、评价与谈资、问答与自测。
//!
//! 每一步的结果按书和模型存成文件，中途失败或停止，下次接着做，不重复花钱。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};

/// 改了提示或格式就升这个号，旧的缓存不再用
pub const VERSION: u32 = 1;
/// 每段最多这么多词、这么多章
pub const CHUNK_WORDS: usize = 8000;
pub const CHUNK_CHAPTERS: usize = 12;
/// 一章太长时只发前面这么多词
pub const MAX_CHAPTER_WORDS: usize = 24_000;
/// 原文最多这么多词
pub const MAX_QUOTE_WORDS: usize = 90;
/// 综合的四次调用
pub const SYNTH_STEPS: usize = 4;

// —— 分段 ——

/// 要通读的一章：书里的章节序号、标题、词数
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterRef {
    pub idx: i64,
    pub title: String,
    pub words: usize,
}

/// 按篇幅把各章分段：一段不超过 CHUNK_WORDS 词、CHUNK_CHAPTERS 章；一章比这还长就单独一段
pub fn chunks(chapters: &[ChapterRef]) -> Vec<Vec<ChapterRef>> {
    let mut out: Vec<Vec<ChapterRef>> = Vec::new();
    let mut cur: Vec<ChapterRef> = Vec::new();
    let mut words = 0;
    for c in chapters {
        if !cur.is_empty() && (words + c.words > CHUNK_WORDS || cur.len() >= CHUNK_CHAPTERS) {
            out.push(std::mem::take(&mut cur));
            words = 0;
        }
        words += c.words;
        cur.push(c.clone());
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 一章的正文最多取 MAX_CHAPTER_WORDS 个词
pub fn clip_words(text: &str, max: usize) -> &str {
    let mut n = 0;
    let mut in_word = false;
    for (i, ch) in text.char_indices() {
        let w = !ch.is_whitespace();
        if w && !in_word {
            n += 1;
            if n > max {
                return &text[..i];
            }
        }
        in_word = w;
    }
    text
}

// —— 第一步：逐段通读 ——

pub const READ_SYSTEM: &str = "你是给中国读者写英文原著精读本的编辑。读者不会再读原书，要靠你的材料把这本书讲清楚、跟人聊得上。\
你会收到书的若干章原文（每章以 === [序号] 标题 === 开头）。逐章用简体中文写，准确、具体，写出人名、地点和发生了什么，不要空泛的评价。\
原文名段必须从给出的正文里逐字复制（大小写、标点都不改），不能凭记忆写，也不能改写或拼接。";

pub fn read_prompt(title: &str, author: Option<&str>, chapters: &[(ChapterRef, String)]) -> String {
    let mut p = format!("书名：{title}\n");
    if let Some(a) = author.filter(|a| !a.trim().is_empty()) {
        p.push_str(&format!("作者：{a}\n"));
    }
    p.push_str(&format!(
        "\n下面是 {} 章原文。chapters 里每章一项，idx 用标题前方括号里的序号：\n\
         - summary：提要，情节章节写三到六句（谁、做了什么、结果如何、有什么伏笔）；议论、知识性的题外话章节写一两句，并说明它在书里起什么作用\n\
         - kind：plot（推进情节）、digression（题外话、知识、议论）或 other\n\
         - events：这一章的关键事件，每条一句；没有就留空数组\n\
         - characters：这一章出场或被重点提到的人物（用原文的英文名）\n\
         - quotes：这一章最值得记住的原文 0 到 2 段（名句、关键情节、精彩描写），每段一到三句、不超过 {} 词，text 逐字复制原文，zh 是译文，why 一句话说明为什么重要\n\n",
        chapters.len(),
        MAX_QUOTE_WORDS
    ));
    for (c, text) in chapters {
        p.push_str(&format!("=== [{}] {} ===\n{}\n\n", c.idx, c.title, text.trim()));
    }
    p
}

pub fn read_schema() -> Value {
    let s = json!({ "type": "string" });
    let list = json!({ "type": "array", "items": { "type": "string" } });
    json!({
        "type": "object",
        "properties": {
            "chapters": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "idx": { "type": "integer" },
                        "summary": s, "kind": s, "events": list, "characters": list,
                        "quotes": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": { "text": s, "zh": s, "why": s },
                                "required": ["text", "zh", "why"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["idx", "summary", "kind", "events", "characters", "quotes"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["chapters"],
        "additionalProperties": false
    })
}

pub const READ_FORMAT: &str = "只输出 JSON，格式：{\"chapters\": [{\"idx\": 3, \"summary\": \"提要\", \"kind\": \"plot\", \"events\": [\"事件\"], \"characters\": [\"Ishmael\"], \"quotes\": [{\"text\": \"逐字复制的原文\", \"zh\": \"译文\", \"why\": \"为什么重要\"}]}]}";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawQuote {
    pub text: String,
    pub zh: String,
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawChapter {
    pub idx: i64,
    pub summary: String,
    pub kind: String,
    pub events: Vec<String>,
    pub characters: Vec<String>,
    pub quotes: Vec<RawQuote>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawRead {
    pub chapters: Vec<RawChapter>,
}

fn parse_err(e: serde_json::Error) -> Error {
    Error::Online(crate::tf!("无法解析模型输出：{}", "Could not parse the model output: {}", e))
}

pub fn parse_read(text: &str) -> Result<RawRead> {
    serde_json::from_str(text).map_err(parse_err)
}

// —— 原文核对 ——

/// 比较用的字符：弯引号、破折号统一，空白合并，忽略大小写
fn fold(c: char) -> Option<char> {
    Some(match c {
        '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' | '`' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{201F}' | '\u{2033}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' | '\u{2212}' => '-',
        '\u{00A0}' => ' ',
        c if c.is_whitespace() => ' ',
        c => c.to_lowercase().next().unwrap_or(c),
    })
}

/// 规整后的文本和每个字符在原文里的字节位置（连续空白只留一个）
fn normalize(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::with_capacity(text.len());
    let mut pos = Vec::with_capacity(text.len());
    for (i, c) in text.char_indices() {
        let Some(f) = fold(c) else { continue };
        if f == ' ' && (chars.is_empty() || chars.last() == Some(&' ')) {
            continue;
        }
        chars.push(f);
        pos.push(i);
    }
    while chars.last() == Some(&' ') {
        chars.pop();
        pos.pop();
    }
    (chars, pos)
}

fn find(hay: &[char], needle: &[char], from: usize) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (from..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

/// 在本章正文里找模型给的原文，返回书里的原文（找不到返回 None）。
/// 先整段找；找不到时用开头和结尾各一截定位（模型偶尔改了中间一个标点），中间长度要和模型给的差不多
pub fn locate(chapter: &str, quote: &str) -> Option<String> {
    let (q, _) = normalize(quote.trim().trim_matches(|c: char| c == '"' || c == '\u{201C}' || c == '\u{201D}'));
    let q: Vec<char> = q.into_iter().collect();
    if q.len() < 12 {
        return None;
    }
    let (hay, pos) = normalize(chapter);
    let span = |start: usize, end: usize| -> Option<String> {
        let a = pos[start];
        let b = if end < pos.len() { pos[end] } else { chapter.len() };
        // 结尾那个字符整个带上
        let s = chapter[a..b].trim();
        (!s.is_empty()).then(|| s.split_whitespace().collect::<Vec<_>>().join(" "))
    };
    if let Some(i) = find(&hay, &q, 0) {
        return span(i, i + q.len());
    }
    let k = 30.min(q.len() / 3);
    let (head, tail) = (&q[..k], &q[q.len() - k..]);
    let mut from = 0;
    while let Some(i) = find(&hay, head, from) {
        if let Some(j) = find(&hay, tail, i + k) {
            let len = j + k - i;
            if len * 10 <= q.len() * 12 && len * 10 >= q.len() * 8 {
                return span(i, j + k);
            }
        }
        from = i + 1;
    }
    None
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

// —— 通读结果（核对过的） ——

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    /// 全书编号（综合时引用、PDF 里标号用）
    pub id: u32,
    pub chapter: i64,
    /// 书里的原文
    pub text: String,
    pub zh: String,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterNote {
    pub idx: i64,
    pub title: String,
    pub summary: String,
    /// plot | digression | other
    pub kind: String,
    pub events: Vec<String>,
    pub characters: Vec<String>,
    /// 这一章收下的原文编号
    pub quotes: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkNotes {
    pub chapters: Vec<ChapterNote>,
    /// 编号先用段内的 0、1、2…，合并时重新编
    pub quotes: Vec<Quote>,
    /// 模型给了、但在正文里找不到的原文条数
    pub dropped: u32,
}

/// 模型的输出 → 核对过原文的一段笔记。模型漏掉的章补一条空提要，多出来的、序号不在这一段的丢掉
pub fn check_chunk(raw: &RawRead, chapters: &[(ChapterRef, String)]) -> ChunkNotes {
    let mut out = ChunkNotes::default();
    for (c, text) in chapters {
        let r = raw.chapters.iter().find(|r| r.idx == c.idx);
        let mut ids = Vec::new();
        for q in r.map(|r| r.quotes.as_slice()).unwrap_or_default().iter().take(2) {
            match locate(text, &q.text).filter(|t| word_count(t) <= MAX_QUOTE_WORDS + 10) {
                Some(t) if !out.quotes.iter().any(|x| x.text == t) => {
                    let id = out.quotes.len() as u32;
                    out.quotes.push(Quote { id, chapter: c.idx, text: t, zh: q.zh.trim().into(), why: q.why.trim().into() });
                    ids.push(id);
                }
                Some(_) => {}
                None => out.dropped += 1,
            }
        }
        let kind = match r.map(|r| r.kind.as_str()) {
            Some("plot") => "plot",
            Some("digression") => "digression",
            _ => "other",
        };
        out.chapters.push(ChapterNote {
            idx: c.idx,
            title: c.title.clone(),
            summary: r.map(|r| r.summary.trim().to_string()).unwrap_or_default(),
            kind: kind.into(),
            events: r.map(|r| clean(&r.events)).unwrap_or_default(),
            characters: r.map(|r| clean(&r.characters)).unwrap_or_default(),
            quotes: ids,
        });
    }
    out
}

fn clean(v: &[String]) -> Vec<String> {
    v.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// 各段笔记合成全书：原文从 1 起连续编号
pub fn merge(parts: Vec<ChunkNotes>) -> (Vec<ChapterNote>, Vec<Quote>, u32) {
    let (mut chapters, mut quotes, mut dropped) = (Vec::new(), Vec::new(), 0);
    for p in parts {
        let base = quotes.len() as u32 + 1;
        dropped += p.dropped;
        for mut c in p.chapters {
            c.quotes = c.quotes.iter().map(|q| q + base).collect();
            chapters.push(c);
        }
        for mut q in p.quotes {
            q.id += base;
            quotes.push(q);
        }
    }
    (chapters, quotes, dropped)
}

// —— 第二步：综合 ——

pub const SYNTH_SYSTEM: &str = "你是给中国读者写英文原著精读本的编辑。读者不会再读原书，读完你的材料要能跟人自如地聊这本书、讲给别人听，被问到细节也答得上来。\
你会收到全书的逐章提要和从书里摘下的原文（带编号 Q1、Q2…）。用简体中文写，具体、准确，多写人名、章节和细节，少写空话。\
人名第一次出现时写中文译名加英文原名，例如「亚哈（Ahab）」。章节用方括号里的序号引用。\
你自己对这本书、作者、出版和评价史的了解可以用，但只写确实知道的事实，拿不准的不写。需要引用原文时只用给出的编号，不要自己写英文原文。";

/// 综合时发给模型的材料：书名、作者、逐章提要、原文编号
pub fn notes_text(title: &str, author: Option<&str>, chapters: &[ChapterNote], quotes: &[Quote]) -> String {
    let mut p = format!("书名：{title}\n");
    if let Some(a) = author.filter(|a| !a.trim().is_empty()) {
        p.push_str(&format!("作者：{a}\n"));
    }
    p.push_str(&format!("共 {} 章。\n\n逐章提要：\n", chapters.len()));
    for c in chapters {
        let kind = if c.kind == "digression" { "（题外话）" } else { "" };
        p.push_str(&format!("[{}] {}{}：{}", c.idx, c.title, kind, c.summary));
        if !c.events.is_empty() {
            p.push_str(&format!(" 事件：{}。", c.events.join("；")));
        }
        if !c.characters.is_empty() {
            p.push_str(&format!(" 人物：{}。", c.characters.join(", ")));
        }
        p.push('\n');
    }
    p.push_str("\n原文：\n");
    for q in quotes {
        p.push_str(&format!("Q{} [{}] {}\n", q.id, q.chapter, q.text));
    }
    p
}

/// 第 `step` 次综合（0–3）要写什么
pub fn synth_task(step: usize) -> &'static str {
    match step {
        0 => "这次写「整体」和「人物」：\n\
              - one_line：一句话说清这本书讲什么（五十字以内）\n\
              - paragraph：一段话梗概，起因、经过、结局、意义（二百到三百字）\n\
              - talk：三分钟口语讲解稿，可以直接对别人说出口，有开场、主线、亮点和结尾（六百到九百字，分三到五段，用换行分段）\n\
              - structure：全书分成几大部分（三到七个），每部分 title、from（起始章序号）、to（结束章序号）、summary（这部分讲什么、起什么作用，两三句）\n\
              - characters：主要人物八到十五个，每人 name（中文名加英文名）、role（身份）、arc（经历和变化，三四句）、scenes（关键场景，带章序号）、meaning（象征或在书里的作用）、quote（最能代表他的一段原文编号，没有填 0）\n\
              - relations：人物关系，两三段，讲清谁和谁什么关系、谁和谁冲突",
        1 => "这次写「世界」「情节」和「主题」：\n\
              - background：时代背景和这个故事的世界（行业、地点、社会），读懂这本书需要知道的，两三段\n\
              - glossary：书里反复出现的专有名词和术语十到二十个，每个 term（英文）、zh（中文）、note（一句话解释）\n\
              - timeline：情节时间线，按发生先后列十五到三十个关键事件，每个 chapter（章序号）、event（一句话）\n\
              - motifs：关键意象和反复出现的东西五到十个，每个 name、meaning（含义，两三句）、chapters（出现的章序号数组）\n\
              - themes：主题四到七个，每个 name、text（论述，三到五句，要有书里的依据）、chapters（依据的章序号数组）\n\
              - technique：写作手法和叙述特点（叙述者、结构、文体、题外话为什么这么多等），两三段",
        2 => "这次写「评价与意义」和「谈资」：\n\
              - reception：出版和评价史，作者生平里和这本书有关的事，两三段\n\
              - criticism：常见的争议或批评三到五个，每个 claim（批评是什么）、pro（支持的理由）、con（反驳的理由）\n\
              - origins：真实原型、取材来源和对后世文化的影响，两三段\n\
              - misconceptions：关于这本书的常见误区四到八个，每个 wrong（常见说法）、right（实际情况）\n\
              - topics：五个可以和人展开讨论的话题，每个 topic、view（一个有依据的观点，两三句）、counter（有人会怎么反驳）\n\
              - golden：最值得记住、可以随口引用的原文八到十二段的编号\n\
              - further：延伸阅读三到六本（相关作品、同题材、研究），每本 title、author、why（和这本书比较着读能看出什么）",
        _ => "这次写「对话准备」和「细节自测」：\n\
              - faq：别人最可能问的二十个问题，每个 q、a（简洁但具体的回答，两到四句）、chapters（出处章序号数组）\n\
              - quiz：细节自测三十题，考人名、事件、物件、地点、数字等细节，每题 q、a（答案一句话）、chapter（出处章序号）",
    }
}

pub fn synth_prompt(notes: &str, step: usize) -> String {
    format!("{notes}\n{}\n\n各项都要写；某项实在没有可写的内容就留空。", synth_task(step))
}

pub fn synth_schema(step: usize) -> Value {
    let s = json!({ "type": "string" });
    let i = json!({ "type": "integer" });
    let ints = json!({ "type": "array", "items": { "type": "integer" } });
    let obj = |props: Value| {
        let req: Vec<String> = props.as_object().map(|m| m.keys().cloned().collect()).unwrap_or_default();
        json!({ "type": "object", "properties": props, "required": req, "additionalProperties": false })
    };
    let arr = |item: Value| json!({ "type": "array", "items": item });
    let props = match step {
        0 => json!({
            "one_line": s, "paragraph": s, "talk": s,
            "structure": arr(obj(json!({ "title": s, "from": i, "to": i, "summary": s }))),
            "characters": arr(obj(json!({ "name": s, "role": s, "arc": s, "scenes": s, "meaning": s, "quote": i }))),
            "relations": s
        }),
        1 => json!({
            "background": s,
            "glossary": arr(obj(json!({ "term": s, "zh": s, "note": s }))),
            "timeline": arr(obj(json!({ "chapter": i, "event": s }))),
            "motifs": arr(obj(json!({ "name": s, "meaning": s, "chapters": ints }))),
            "themes": arr(obj(json!({ "name": s, "text": s, "chapters": ints }))),
            "technique": s
        }),
        2 => json!({
            "reception": s,
            "criticism": arr(obj(json!({ "claim": s, "pro": s, "con": s }))),
            "origins": s,
            "misconceptions": arr(obj(json!({ "wrong": s, "right": s }))),
            "topics": arr(obj(json!({ "topic": s, "view": s, "counter": s }))),
            "golden": ints,
            "further": arr(obj(json!({ "title": s, "author": s, "why": s })))
        }),
        _ => json!({
            "faq": arr(obj(json!({ "q": s, "a": s, "chapters": ints }))),
            "quiz": arr(obj(json!({ "q": s, "a": s, "chapter": i })))
        }),
    };
    obj(props)
}

pub fn synth_format(step: usize) -> &'static str {
    match step {
        0 => "只输出 JSON，格式：{\"one_line\": \"\", \"paragraph\": \"\", \"talk\": \"\", \"structure\": [{\"title\": \"\", \"from\": 1, \"to\": 20, \"summary\": \"\"}], \"characters\": [{\"name\": \"\", \"role\": \"\", \"arc\": \"\", \"scenes\": \"\", \"meaning\": \"\", \"quote\": 0}], \"relations\": \"\"}",
        1 => "只输出 JSON，格式：{\"background\": \"\", \"glossary\": [{\"term\": \"\", \"zh\": \"\", \"note\": \"\"}], \"timeline\": [{\"chapter\": 1, \"event\": \"\"}], \"motifs\": [{\"name\": \"\", \"meaning\": \"\", \"chapters\": [1]}], \"themes\": [{\"name\": \"\", \"text\": \"\", \"chapters\": [1]}], \"technique\": \"\"}",
        2 => "只输出 JSON，格式：{\"reception\": \"\", \"criticism\": [{\"claim\": \"\", \"pro\": \"\", \"con\": \"\"}], \"origins\": \"\", \"misconceptions\": [{\"wrong\": \"\", \"right\": \"\"}], \"topics\": [{\"topic\": \"\", \"view\": \"\", \"counter\": \"\"}], \"golden\": [1], \"further\": [{\"title\": \"\", \"author\": \"\", \"why\": \"\"}]}",
        _ => "只输出 JSON，格式：{\"faq\": [{\"q\": \"\", \"a\": \"\", \"chapters\": [1]}], \"quiz\": [{\"q\": \"\", \"a\": \"\", \"chapter\": 1}]}",
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Part {
    pub title: String,
    pub from: i64,
    pub to: i64,
    pub summary: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Character {
    pub name: String,
    pub role: String,
    pub arc: String,
    pub scenes: String,
    pub meaning: String,
    pub quote: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Term {
    pub term: String,
    pub zh: String,
    pub note: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Event {
    pub chapter: i64,
    pub event: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Motif {
    pub name: String,
    pub meaning: String,
    pub chapters: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Theme {
    pub name: String,
    pub text: String,
    pub chapters: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Debate {
    pub claim: String,
    pub pro: String,
    pub con: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Myth {
    pub wrong: String,
    pub right: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Topic {
    pub topic: String,
    pub view: String,
    pub counter: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Book {
    pub title: String,
    pub author: String,
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Faq {
    pub q: String,
    pub a: String,
    pub chapters: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct QuizItem {
    pub q: String,
    pub a: String,
    pub chapter: i64,
}

/// 四次综合的输出合在一起（模型用 snake_case 写，前端拿到的是 camelCase）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Synthesis {
    #[serde(alias = "one_line")]
    pub one_line: String,
    pub paragraph: String,
    pub talk: String,
    pub structure: Vec<Part>,
    pub characters: Vec<Character>,
    pub relations: String,
    pub background: String,
    pub glossary: Vec<Term>,
    pub timeline: Vec<Event>,
    pub motifs: Vec<Motif>,
    pub themes: Vec<Theme>,
    pub technique: String,
    pub reception: String,
    pub criticism: Vec<Debate>,
    pub origins: String,
    pub misconceptions: Vec<Myth>,
    pub topics: Vec<Topic>,
    pub golden: Vec<u32>,
    pub further: Vec<Book>,
    pub faq: Vec<Faq>,
    pub quiz: Vec<QuizItem>,
}

/// 一次综合的输出（只有这一步的几项）并进总的结果
pub fn merge_synth(into: &mut Synthesis, text: &str) -> Result<()> {
    let s: Synthesis = serde_json::from_str(text).map_err(parse_err)?;
    let d = Synthesis::default();
    macro_rules! take {
        ($($f:ident),*) => { $( if s.$f != d.$f { into.$f = s.$f.clone(); } )* };
    }
    take!(one_line, paragraph, talk, structure, characters, relations, background, glossary, timeline, motifs, themes, technique, reception, criticism, origins, misconceptions, topics, golden, further, faq, quiz);
    Ok(())
}

/// 引用了不存在的原文编号就去掉，免得 PDF 里出现空引用
pub fn tidy(s: &mut Synthesis, quotes: &[Quote]) {
    let ok = |id: u32| quotes.iter().any(|q| q.id == id);
    for c in &mut s.characters {
        if !ok(c.quote) {
            c.quote = 0;
        }
    }
    let mut seen = std::collections::HashSet::new();
    s.golden.retain(|&id| ok(id) && seen.insert(id));
}

/// 完整的精读本：给前端排版
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Digest {
    pub model: String,
    pub chapters: Vec<ChapterNote>,
    pub quotes: Vec<Quote>,
    pub synthesis: Synthesis,
    /// 模型给了、但在书里找不到而丢掉的原文条数
    pub dropped: u32,
}

/// 估算的 token 数：英文一词约 1.35 token；中文输出一字约 0.7 token
pub fn estimate_tokens(total_words: usize, chapters: usize) -> (u64, u64) {
    let read_in = (total_words as f64 * 1.35) as u64 + 1500 * chapters.div_ceil(CHUNK_CHAPTERS).max(1) as u64;
    let read_out = 450 * chapters as u64;
    let notes = 220 * chapters as u64 + 60 * chapters as u64;
    let synth_in = notes * SYNTH_STEPS as u64;
    let synth_out = 5000 * SYNTH_STEPS as u64;
    (read_in + synth_in, read_out + synth_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(idx: i64, words: usize) -> ChapterRef {
        ChapterRef { idx, title: format!("Chapter {idx}"), words }
    }

    #[test]
    fn chunks_by_words_and_chapter_count() {
        let c: Vec<ChapterRef> = vec![ch(1, 3000), ch(2, 3000), ch(3, 3000), ch(4, 20000), ch(5, 100)];
        let idx: Vec<Vec<i64>> = chunks(&c).iter().map(|g| g.iter().map(|c| c.idx).collect()).collect();
        assert_eq!(idx, vec![vec![1, 2], vec![3], vec![4], vec![5]]);
        let many: Vec<ChapterRef> = (1..=30).map(|i| ch(i, 100)).collect();
        assert_eq!(chunks(&many).iter().map(Vec::len).collect::<Vec<_>>(), vec![12, 12, 6]);
    }

    #[test]
    fn clip_counts_words() {
        assert_eq!(clip_words("one two  three four", 2), "one two  ");
        assert_eq!(clip_words("one two", 5), "one two");
    }

    const TEXT: &str = "Call me Ishmael. Some years ago\u{2014}never mind how long precisely\u{2014}having little or no money in my purse, \
and nothing particular to interest me on shore, I thought I would sail about a little and see the watery part of the world.";

    #[test]
    fn locate_returns_the_book_text() {
        // 直引号、普通连字符、大小写不同也找得到，返回的是书里的写法
        let got = locate(TEXT, "some years ago-never mind how long precisely-having little or no money in my purse").unwrap();
        assert_eq!(got, "Some years ago\u{2014}never mind how long precisely\u{2014}having little or no money in my purse");
        // 换行和多余空格
        assert!(locate("I thought I would\n   sail about a little.", "I thought I would sail about a little.").is_some());
        // 中间改了一个词也认（开头结尾对得上、长度相当）
        let got = locate(TEXT, "and nothing particular to interest me on land, I thought I would sail about a little and see the watery part of the world.").unwrap();
        assert!(got.contains("on shore"), "{got}");
        // 编的句子不收
        assert!(locate(TEXT, "From hell's heart I stab at thee; for hate's sake I spit my last breath at thee.").is_none());
        assert!(locate(TEXT, "Call me").is_none(), "太短的不收");
    }

    #[test]
    fn check_chunk_keeps_verified_quotes_only() {
        let raw = RawRead {
            chapters: vec![RawChapter {
                idx: 1,
                summary: " 以实玛利决定出海。 ".into(),
                kind: "plot".into(),
                events: vec!["决定出海".into(), " ".into()],
                characters: vec!["Ishmael".into()],
                quotes: vec![
                    RawQuote { text: "\u{201C}Call me Ishmael.\u{201D} Some years ago".into(), zh: "叫我以实玛利。".into(), why: "开头".into() },
                    RawQuote { text: "This sentence is not in the book at all.".into(), zh: "".into(), why: "".into() },
                    RawQuote { text: "a third one is ignored".into(), zh: "".into(), why: "".into() },
                ],
            }],
        };
        let chapters = vec![(ch(1, 50), TEXT.to_string()), (ch(2, 10), "Nothing here.".to_string())];
        let n = check_chunk(&raw, &chapters);
        assert_eq!(n.chapters.len(), 2, "漏掉的章补上");
        assert_eq!(n.chapters[0].summary, "以实玛利决定出海。");
        assert_eq!(n.chapters[0].events, vec!["决定出海"]);
        assert_eq!(n.quotes.len(), 1);
        assert_eq!(n.quotes[0].text, "Call me Ishmael. Some years ago");
        assert_eq!(n.dropped, 1);
        assert_eq!(n.chapters[1].kind, "other");

        let (chs, qs, dropped) = merge(vec![n.clone(), n]);
        assert_eq!(qs.iter().map(|q| q.id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(chs[2].quotes, vec![2]);
        assert_eq!(dropped, 2);
    }

    #[test]
    fn synthesis_steps_merge_and_tidy() {
        let mut s = Synthesis::default();
        merge_synth(&mut s, r#"{"one_line":"一句话","structure":[{"title":"岸上","from":1,"to":22,"summary":"x"}],"characters":[{"name":"亚哈（Ahab）","quote":7},{"name":"x","quote":1}]}"#).unwrap();
        merge_synth(&mut s, r#"{"golden":[1,1,9],"quiz":[{"q":"?","a":"!","chapter":3}]}"#).unwrap();
        assert_eq!(s.one_line, "一句话");
        assert_eq!(s.structure[0].to, 22);
        assert_eq!(s.quiz.len(), 1);
        let quotes = vec![Quote { id: 1, chapter: 1, text: "Call me Ishmael.".into(), zh: String::new(), why: String::new() }];
        tidy(&mut s, &quotes);
        assert_eq!(s.characters[0].quote, 0);
        assert_eq!(s.characters[1].quote, 1);
        assert_eq!(s.golden, vec![1]);
        for step in 0..SYNTH_STEPS {
            let schema = synth_schema(step);
            assert!(schema["required"].as_array().unwrap().len() >= 2);
            assert!(synth_prompt("notes", step).contains("notes"));
        }
    }

    #[test]
    fn notes_text_lists_chapters_and_quotes() {
        let c = ChapterNote {
            idx: 36,
            title: "The Quarter-Deck".into(),
            summary: "亚哈宣布追杀白鲸。".into(),
            kind: "plot".into(),
            events: vec!["钉金币".into()],
            characters: vec!["Ahab".into()],
            quotes: vec![3],
        };
        let q = Quote { id: 3, chapter: 36, text: "Death to Moby Dick!".into(), zh: String::new(), why: String::new() };
        let t = notes_text("Moby-Dick", Some("Herman Melville"), &[c], &[q]);
        assert!(t.contains("[36] The Quarter-Deck：亚哈宣布追杀白鲸。 事件：钉金币。 人物：Ahab。"));
        assert!(t.contains("Q3 [36] Death to Moby Dick!"));
        let p = read_prompt("Moby-Dick", None, &[(ch(1, 5), "Call me Ishmael.".into())]);
        assert!(p.contains("=== [1] Chapter 1 ===\nCall me Ishmael."));
    }
}
