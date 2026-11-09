//! 精读本（全书预习包第三期，「替代版」）：AI 通读全书，写出读完就可以不读原书的分析。
//!
//! 按《How to Read a Book》分析阅读的四问和读小说的规则组织：整体（一句话、一段话、三分钟讲解稿、结构）
//! → 人物与世界 → 逐章提要和书中原文 → 主题与评价 → 对话准备和细节自测。
//!
//! 分三步：
//! 1. 通读：正文按篇幅分成若干段（每段几章、约八千词）逐段发给模型，每章写连贯的复述、关键事件、阅读建议，
//!    挑出原文名句，并提名一个「名场面」（只给开头句和结尾句）。
//!    原文必须逐字取自书：模型给的句子在本章正文里找得到才收，收的是书里的原文（不是模型抄写的版本），找不到就丢掉；
//!    名场面按开头句、结尾句在书里定位，取中间的整段原文。
//! 2. 综合：把各章复述和收下的原文（带编号）发给模型，分四次写整体与人物、世界与主题、评价与谈资、问答与自测。
//! 3. 名场面：全书按重要程度选出最多十六段，分几次请模型写译文和前因后果。
//!    公版书（作者去世超过 70 年，卒年查维基百科）放整段原文和译文；其他书只放开头两三句。
//!
//! 每一步的结果按书和模型存成文件，中途失败或停止，下次接着做，不重复花钱。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};

/// 改了提示或格式就升这个号，旧的缓存不再用
pub const VERSION: u32 = 7;
/// 每段最多这么多词、这么多章
pub const CHUNK_WORDS: usize = 8000;
pub const CHUNK_CHAPTERS: usize = 8;
/// 一章太长时只发前面这么多词
pub const MAX_CHAPTER_WORDS: usize = 24_000;
/// 原文最多这么多词
pub const MAX_QUOTE_WORDS: usize = 90;
/// 综合的五次调用（第五次是图解）
pub const SYNTH_STEPS: usize = 5;
/// 名场面：全书最多几段、每次调用写几段（译文长，一次写多了会被截断）、分几次
pub const MAX_SCENES: usize = 16;
pub const SCENES_PER_STEP: usize = 4;
pub const SCENE_STEPS: usize = MAX_SCENES / SCENES_PER_STEP;
/// 名场面原文的长度范围（词）；非公版书只放开头这么多词
pub const SCENE_MIN_WORDS: usize = 60;
pub const SCENE_MAX_WORDS: usize = 700;
pub const SCENE_CLIP_WORDS: usize = 60;
/// 通读之后的步数：综合 + 名场面
pub const AFTER_READ_STEPS: usize = SYNTH_STEPS + SCENE_STEPS;

// —— 章：按书里的章号重新切 ——

/// 一行只有章号：「12」「XII」「Chapter 12」「CHAPTER XII」。返回章号
pub fn chapter_marker(line: &str) -> Option<u32> {
    let t = line.trim().trim_end_matches('.');
    let t = t.strip_prefix("Chapter ").or_else(|| t.strip_prefix("CHAPTER ")).unwrap_or(t).trim();
    if t.is_empty() || t.len() > 8 {
        return None;
    }
    // 扫描、转换留下的错：「1 O」「l2」——有数字时去掉空格，O 当 0、l 和 I 当 1
    let squeezed: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    if squeezed.chars().any(|c| c.is_ascii_digit()) && squeezed.chars().all(|c| c.is_ascii_digit() || "OoIl".contains(c)) {
        let fixed: String = squeezed.chars().map(|c| match c { 'O' | 'o' => '0', 'I' | 'l' => '1', c => c }).collect();
        return fixed.parse().ok().filter(|n| (1..=300).contains(n));
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        return t.parse().ok().filter(|n| (1..=300).contains(n));
    }
    // 罗马数字（只认大写，免得把单词 I、mix 当成章号）
    if t.chars().all(|c| "IVXLC".contains(c)) {
        let v = |c: char| match c {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            _ => 100,
        };
        let cs: Vec<u32> = t.chars().map(v).collect();
        let n = cs.iter().enumerate().map(|(i, &x)| if cs.get(i + 1).is_some_and(|&y| y > x) { -(x as i64) } else { x as i64 }).sum::<i64>();
        return u32::try_from(n).ok().filter(|n| (1..=300).contains(n));
    }
    None
}

fn words_of(s: &str) -> usize {
    s.split_whitespace().count()
}

/// 版权页、目录页这类很短的附加页
fn is_front_page(text: &str) -> bool {
    let l = text.to_lowercase();
    words_of(text) < 600 && (l.contains("isbn") || l.contains("all rights reserved") || l.contains("catalogue record") || l.contains("copyright ©"))
}

/// 默认范围里各部分（标题、纯文本）→ 要通读的章（标题、正文）。
///
/// 多数 epub 一个文件一章，原样用。有的书（尤其 Calibre 转出来的）把好几章塞进一个文件、章首只有一行数字「1」「2」：
/// 这时整本书连起来按章号重新切（章号要递增，最多跳三章，免得把正文里单独一行的数字当成章号），
/// 第一个章号之前的短内容（版权页）丢掉，跨文件的章接起来。至少有一个文件里出现两个以上章号才这样切。
pub fn build_units(files: &[(String, String)]) -> Vec<(String, String)> {
    let mut accepted: Vec<(usize, usize, u32)> = Vec::new(); // (文件, 行, 章号)
    let mut last: Option<u32> = None;
    for (fi, (_, text)) in files.iter().enumerate() {
        for (li, line) in text.lines().enumerate() {
            let Some(n) = chapter_marker(line) else { continue };
            let ok = match last {
                None => n <= 3,
                Some(l) => n > l && n <= l + 3,
            };
            if ok {
                accepted.push((fi, li, n));
                last = Some(n);
            }
        }
    }
    let per_file_max = (0..files.len()).map(|fi| accepted.iter().filter(|a| a.0 == fi).count()).max().unwrap_or(0);
    if accepted.len() < 3 || per_file_max < 2 {
        return files.iter().filter(|(_, t)| !is_front_page(t) && words_of(t) > 0).cloned().collect();
    }
    let mut out: Vec<(String, String)> = Vec::new();
    let mut cur: Option<(String, String)> = None;
    let mut pre = String::new();
    let mut k = 0;
    for (fi, (title, text)) in files.iter().enumerate() {
        for (li, line) in text.lines().enumerate() {
            if k < accepted.len() && accepted[k].0 == fi && accepted[k].1 == li {
                if let Some(c) = cur.take() {
                    out.push(c);
                } else if words_of(&pre) >= 500 && !is_front_page(&pre) {
                    // 第一章之前的长内容（序言）单独算一部分
                    out.push((title.clone(), std::mem::take(&mut pre)));
                }
                cur = Some((format!("Chapter {}", accepted[k].2), String::new()));
                k += 1;
                continue;
            }
            match cur.as_mut() {
                Some((_, body)) => {
                    body.push_str(line);
                    body.push('\n');
                }
                None => {
                    pre.push_str(line);
                    pre.push('\n');
                }
            }
        }
    }
    if let Some(c) = cur {
        out.push(c);
    }
    out
}

// —— 章的编号 ——

/// 发给模型的章编号 = 书里的章节序号 + 1000。书里的章节序号常和书自己的章号差几位（前面有封面、前言），
/// 模型会把「[13] CHAPTER 1」里的 13 和 1 混着用；加 1000 后一眼就能区分
pub const ID_BASE: i64 = 1000;

pub fn cid(idx: i64) -> i64 {
    idx + ID_BASE
}

/// 模型给的章编号 → 书里的章节序号。≥ 1000 的按编号换算；小于 1000 的多半是书自己的章号，按标题里的数字找；都找不到返回 None
pub fn resolve(n: i64, chapters: &[(i64, String)]) -> Option<i64> {
    if n >= ID_BASE {
        let idx = n - ID_BASE;
        return chapters.iter().any(|(i, _)| *i == idx).then_some(idx);
    }
    chapters
        .iter()
        .find(|(_, t)| t.split(|c: char| !c.is_ascii_digit()).find(|w| !w.is_empty()).and_then(|w| w.parse::<i64>().ok()) == Some(n))
        .map(|(i, _)| *i)
}

/// 综合结果里所有的章编号换成书里的章节序号；对不上的丢掉（单个的填 0，排版时不显示）
pub fn resolve_synthesis(s: &mut Synthesis, chapters: &[(i64, String)]) {
    labels_in_text(s, chapters);
    let one = |n: i64| resolve(n, chapters).unwrap_or(0);
    let many = |v: &mut Vec<i64>| *v = v.iter().filter_map(|&n| resolve(n, chapters)).collect();
    for p in &mut s.structure {
        p.from = one(p.from);
        p.to = one(p.to);
    }
    for e in &mut s.timeline {
        e.chapter = one(e.chapter);
    }
    for m in &mut s.motifs {
        many(&mut m.chapters);
    }
    for t in &mut s.themes {
        many(&mut t.chapters);
    }
    for f in &mut s.faq {
        many(&mut f.chapters);
    }
    for q in &mut s.quiz {
        q.chapter = one(q.chapter);
    }
    let d = &mut s.diagrams;
    for e in &mut d.plot.events {
        e.chapter = one(e.chapter);
    }
    for x in &mut d.route.stops {
        x.chapter = one(x.chapter);
    }
    for m in &mut d.rhythm.marks {
        m.chapter = one(m.chapter);
    }
    d.rhythm.marks.retain(|m| m.chapter != 0);
    for x in &mut d.compare.items {
        x.chapter = one(x.chapter);
    }
    tidy_diagrams(d);
}

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
你会收到书的若干章原文（每章以 === [编号] 标题 === 开头，编号是四位数，和书里的章号不同）。逐章用简体中文写，准确、具体，写出人名、地点和发生了什么，不要空泛的评价。\
原文名段必须从给出的正文里逐字复制（大小写、标点都不改），不能凭记忆写，也不能改写或拼接。";

pub fn read_prompt(title: &str, author: Option<&str>, chapters: &[(ChapterRef, String)]) -> String {
    let mut p = format!("书名：{title}\n");
    if let Some(a) = author.filter(|a| !a.trim().is_empty()) {
        p.push_str(&format!("作者：{a}\n"));
    }
    p.push_str(&format!(
        "\n下面是 {} 章原文。chapters 里每章一项，idx 照抄标题前方括号里的四位数编号（不是书里的章号）：\n\
         - summary：复述。情节章节写成三百到六百字的连贯叙述，按发生顺序讲清谁、在哪里、做了什么、说了什么关键的话、结果如何、有什么伏笔，\
         读者不读原文也能像读过一样了解这一章；议论、知识性的题外话章节写一两句，说明讲了什么、在书里起什么作用\n\
         - kind：plot（推进情节）、digression（题外话、知识、议论）或 other\n\
         - events：这一章的关键事件，每条一句；没有就留空数组\n\
         - characters：这一章出场或被重点提到的人物（用原文的英文名）\n\
         - advice：如果读者只想读一小部分原文，这一章怎么读：read（值得读原文，要严格：只给全书最关键、最精彩的约十分之一到七分之一的章，\
         一段里通常只有零到一章）、normal、skim（可以略读）、skip（可以跳过，看复述就够：大多数知识性、议论性的题外话和过渡章）\n\
         - advice_why：一句话说明理由\n\
         - tension：这一章的紧张程度 1 到 5（1 平静或题外话，3 有冲突，5 全书高潮）\n\
         - quotes：这一章最值得记住的原文 0 到 2 段（名句、关键情节、精彩描写），每段一到三句、不超过 {} 词，text 逐字复制原文，zh 是译文，why 一句话说明为什么重要\n\
         - scene：这一章如果有全书级别的名场面（最精彩、最关键、最常被引用的一段，一两百到五六百词），给出 title（中文场景名）、\
         start（这段的第一句，逐字复制）、end（这段的最后一句，逐字复制）、why（为什么是名场面）、importance（1 到 5，5 是全书最重要的几段）；\
         没有就全部留空、importance 填 0。整本书只有十几段名场面，不要每章都给\n\n",
        chapters.len(),
        MAX_QUOTE_WORDS
    ));
    for (c, text) in chapters {
        p.push_str(&format!("=== [{}] {} ===\n{}\n\n", cid(c.idx), c.title, text.trim()));
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
                        "summary": s, "kind": s, "events": list, "characters": list, "advice": s, "advice_why": s, "tension": { "type": "integer" },
                        "scene": {
                            "type": "object",
                            "properties": { "title": s, "start": s, "end": s, "why": s, "importance": { "type": "integer" } },
                            "required": ["title", "start", "end", "why", "importance"],
                            "additionalProperties": false
                        },
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
                    "required": ["idx", "summary", "kind", "events", "characters", "advice", "advice_why", "tension", "quotes", "scene"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["chapters"],
        "additionalProperties": false
    })
}

pub const READ_FORMAT: &str = "只输出 JSON，格式：{\"chapters\": [{\"idx\": 3, \"summary\": \"提要\", \"kind\": \"plot\", \"events\": [\"事件\"], \"characters\": [\"Ishmael\"], \"advice\": \"read\", \"advice_why\": \"理由\", \"tension\": 3, \"quotes\": [{\"text\": \"逐字复制的原文\", \"zh\": \"译文\", \"why\": \"为什么重要\"}], \"scene\": {\"title\": \"\", \"start\": \"\", \"end\": \"\", \"why\": \"\", \"importance\": 0}}]}";

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
    pub advice: String,
    pub advice_why: String,
    pub tension: u8,
    pub quotes: Vec<RawQuote>,
    pub scene: RawScene,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawScene {
    pub title: String,
    pub start: String,
    pub end: String,
    pub why: String,
    pub importance: u8,
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

/// 在本章正文里找模型给的原文，返回书里的原文（找不到返回 None）
pub fn locate(chapter: &str, quote: &str) -> Option<String> {
    locate_span(chapter, quote).map(|(a, b)| squash(&chapter[a..b]))
}

/// 空白合并成一个空格
fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 在本章正文里找模型给的原文，返回在正文里的字节范围。
/// 先整段找；找不到时用开头和结尾各一截定位（模型偶尔改了中间一个标点），中间长度要和模型给的差不多
pub fn locate_span(chapter: &str, quote: &str) -> Option<(usize, usize)> {
    let (q, _) = normalize(quote.trim().trim_matches(|c: char| c == '"' || c == '\u{201C}' || c == '\u{201D}'));
    let q: Vec<char> = q.into_iter().collect();
    if q.len() < 12 {
        return None;
    }
    let (hay, pos) = normalize(chapter);
    let span = |start: usize, end: usize| -> Option<(usize, usize)> {
        let a = pos[start];
        let b = if end < pos.len() { pos[end] } else { chapter.len() };
        // 去掉两头的空白
        let s = &chapter[a..b];
        let a2 = a + (s.len() - s.trim_start().len());
        let b2 = b - (s.len() - s.trim_end().len());
        (b2 > a2).then_some((a2, b2))
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

// —— 名场面 ——

/// 按开头句、结尾句在本章里取出一整段原文；太短、太长（超过的截到句末）或找不到时返回 None
pub fn locate_scene(chapter: &str, start: &str, end: &str) -> Option<String> {
    if start.trim().is_empty() || end.trim().is_empty() {
        return None;
    }
    let (a, a_end) = locate_span(chapter, start)?;
    // 结尾句从开头句之后找
    let (_, b) = locate_span(&chapter[a_end..], end).map(|(x, y)| (a_end + x, a_end + y))?;
    // 保留原书的分段
    let text = chapter[a..b].lines().map(squash).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n");
    let n = word_count(&text);
    if n < SCENE_MIN_WORDS {
        return None;
    }
    Some(if n > SCENE_MAX_WORDS { clip_to_sentence(&text, SCENE_MAX_WORDS) } else { text })
}

/// 取前 `max` 个词，再退到最后一个句末（句末之后的引号一起带上）
pub fn clip_to_sentence(text: &str, max: usize) -> String {
    let cut = clip_words(text, max).trim_end();
    let cut = cut.trim_end_matches(|c: char| c.is_whitespace());
    if cut.len() == text.trim_end().len() {
        return cut.to_string();
    }
    let ends: Vec<usize> = cut
        .char_indices()
        .filter(|&(_, c)| matches!(c, '.' | '!' | '?'))
        .map(|(i, c)| {
            let mut j = i + c.len_utf8();
            for q in cut[j..].chars() {
                if matches!(q, '"' | '\u{201D}' | '\'' | '\u{2019}') {
                    j += q.len_utf8();
                } else {
                    break;
                }
            }
            j
        })
        .collect();
    match ends.last() {
        // 至少留下一半，否则直接截词加省略号
        Some(&j) if word_count(&cut[..j]) * 2 >= word_count(cut) => cut[..j].to_string(),
        _ => format!("{cut} …"),
    }
}

/// 名场面：编号、出处、原文（非公版书只有开头）、译文、前因后果
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: u32,
    pub chapter: i64,
    pub title: String,
    pub text: String,
    pub why: String,
    pub importance: u8,
    /// 第三步写的译文、前因后果
    pub zh: String,
    pub context: String,
    /// 非公版书：只放了开头
    pub clipped: bool,
}

/// 全书的名场面候选里按重要程度选出最多 MAX_SCENES 段，按章节先后排、从 1 编号
pub fn select_scenes(chapters: &[ChapterNote]) -> Vec<Scene> {
    let mut all: Vec<Scene> = chapters.iter().filter_map(|c| c.scene.clone()).filter(|s| s.importance > 0).collect();
    all.sort_by(|a, b| b.importance.cmp(&a.importance).then(a.chapter.cmp(&b.chapter)));
    all.truncate(MAX_SCENES);
    all.sort_by_key(|s| s.chapter);
    for (i, s) in all.iter_mut().enumerate() {
        s.id = i as u32 + 1;
    }
    all
}

/// 非公版书只放开头几句
pub fn clip_scene(s: &mut Scene) {
    s.text = clip_to_sentence(&s.text, SCENE_CLIP_WORDS);
    s.clipped = true;
}

pub const SCENE_SYSTEM: &str = "你是给中国读者写英文原著精读本的编辑。下面是书里的几个名场面原文，读者会读原文，你帮他读懂。用简体中文写。";

/// 第 k 次名场面调用：这几段的原文和所在章的复述
pub fn scene_prompt(title: &str, author: Option<&str>, scenes: &[Scene], chapters: &[ChapterNote]) -> String {
    let mut p = format!("书名：{title}\n");
    if let Some(a) = author.filter(|a| !a.trim().is_empty()) {
        p.push_str(&format!("作者：{a}\n"));
    }
    p.push_str("\n对每个名场面写：id（照抄）、zh（原文的完整中文译文，忠实、通顺，不删减；按原文分段，段与段之间用换行分开）、context（前因后果：这一幕之前发生了什么、之后怎样，一百到二百字）、why（为什么是名场面、读的时候注意什么，一两句）。\n\n");
    for s in scenes {
        let summary = chapters.iter().find(|c| c.idx == s.chapter).map(|c| c.summary.as_str()).unwrap_or_default();
        let title = chapters.iter().find(|c| c.idx == s.chapter).map(|c| c.title.as_str()).unwrap_or_default();
        p.push_str(&format!("=== 名场面 {}：{}（{}）===\n这一章的复述：{}\n原文：\n{}\n\n", s.id, s.title, title, summary, s.text));
    }
    p
}

pub fn scene_schema() -> Value {
    let s = json!({ "type": "string" });
    json!({
        "type": "object",
        "properties": {
            "scenes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "id": { "type": "integer" }, "zh": s, "context": s, "why": s },
                    "required": ["id", "zh", "context", "why"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["scenes"],
        "additionalProperties": false
    })
}

pub const SCENE_FORMAT: &str = "只输出 JSON，格式：{\"scenes\": [{\"id\": 1, \"zh\": \"译文\", \"context\": \"前因后果\", \"why\": \"为什么是名场面\"}]}";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawSceneNote {
    pub id: u32,
    pub zh: String,
    pub context: String,
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RawScenes {
    pub scenes: Vec<RawSceneNote>,
}

pub fn parse_scenes(text: &str) -> Result<RawScenes> {
    serde_json::from_str(text).map_err(parse_err)
}

/// 第三步的译文、前因后果写回名场面（模型给的 why 更完整时替换通读时的）
pub fn apply_scene_notes(scenes: &mut [Scene], notes: &RawScenes) {
    for n in &notes.scenes {
        if let Some(s) = scenes.iter_mut().find(|s| s.id == n.id) {
            s.zh = n.zh.trim().to_string();
            s.context = n.context.trim().to_string();
            if !n.why.trim().is_empty() {
                s.why = n.why.trim().to_string();
            }
        }
    }
}

// —— 公版判断 ——

/// 维基百科作者条目开头的生卒年：「Herman Melville (born Melvill; August 1, 1819 – September 28, 1891)」→ 1891。
/// 只认第一对括号里的两个年份；只有出生年（还在世）或认不出时返回 None
pub fn death_year(lead: &str) -> Option<i32> {
    let open = lead.find('(')?;
    let close = open + lead[open..].find(')')?;
    let inner = &lead[open + 1..close];
    let years: Vec<i32> = inner
        .split(|c: char| !c.is_ascii_digit())
        .filter(|w| w.len() == 4)
        .filter_map(|w| w.parse().ok())
        .filter(|y| (1000..=2100).contains(y))
        .collect();
    (years.len() >= 2).then(|| *years.last().unwrap_or(&0)).filter(|&d| d >= years[0])
}

/// 作者去世超过 70 年（多数国家的版权期限是作者终身加 70 年，到期后的下一年进入公有领域）
pub fn is_public_domain(died: Option<i32>, this_year: i32) -> bool {
    died.is_some_and(|d| d + 70 < this_year)
}

/// 毫秒时间戳 → 公历年份（UTC）
pub fn year_of_ms(ms: i64) -> i32 {
    // Howard Hinnant 的 civil_from_days
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2)) as i32
}

/// 公版判断的结果（存成文件，同一本书只查一次）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rights {
    pub author_died: Option<i32>,
    pub public_domain: bool,
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
    /// 阅读建议：read | normal | skim | skip，和理由
    #[serde(default)]
    pub advice: String,
    #[serde(default)]
    pub advice_why: String,
    /// 紧张程度 1–5（全书节奏图用）；旧缓存没有时为 0
    #[serde(default)]
    pub tension: u8,
    /// 这一章收下的原文编号
    pub quotes: Vec<u32>,
    /// 这一章提名的名场面（核对过原文）；全书再从中选
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<Scene>,
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
        // 模型偶尔不加 1000，照写书里的章节序号也认
        let r = raw.chapters.iter().find(|r| r.idx == cid(c.idx)).or_else(|| raw.chapters.iter().find(|r| r.idx == c.idx));
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
        let advice = r.map(|r| r.advice.as_str()).filter(|a| matches!(*a, "read" | "skim" | "skip")).unwrap_or("normal");
        let scene = r.map(|r| &r.scene).filter(|s| s.importance > 0).and_then(|s| {
            let found = locate_scene(text, &s.start, &s.end);
            if found.is_none() {
                out.dropped += 1;
            }
            found.map(|t| Scene { chapter: c.idx, title: s.title.trim().into(), text: t, why: s.why.trim().into(), importance: s.importance.min(5), ..Scene::default() })
        });
        out.chapters.push(ChapterNote {
            idx: c.idx,
            title: c.title.clone(),
            summary: r.map(|r| r.summary.trim().to_string()).unwrap_or_default(),
            kind: kind.into(),
            events: r.map(|r| clean(&r.events)).unwrap_or_default(),
            characters: r.map(|r| clean(&r.characters)).unwrap_or_default(),
            advice: advice.into(),
            advice_why: r.map(|r| r.advice_why.trim().to_string()).unwrap_or_default(),
            tension: r.map(|r| r.tension.clamp(1, 5)).unwrap_or(1),
            quotes: ids,
            scene,
        });
    }
    out
}

fn clean(v: &[String]) -> Vec<String> {
    v.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// 「值得读原文」全书最多约五分之一（至少三章）：模型是一段一段判断的，段多章少时会给太多。
/// 超出的按紧张程度留最高的（一样高时留靠前的），其余改成 normal
pub fn cap_must_read(chapters: &mut [ChapterNote]) {
    let max = (chapters.len() / 5).max(3);
    let mut reads: Vec<usize> = (0..chapters.len()).filter(|&i| chapters[i].advice == "read").collect();
    if reads.len() <= max {
        return;
    }
    reads.sort_by(|&a, &b| chapters[b].tension.cmp(&chapters[a].tension).then(a.cmp(&b)));
    for &i in &reads[max..] {
        chapters[i].advice = "normal".into();
    }
}

/// 一段拆成几份分别通读后再拼回一段（段内的原文编号接着排）
pub fn join_chunks(parts: Vec<ChunkNotes>) -> ChunkNotes {
    let mut out = ChunkNotes::default();
    for p in parts {
        let base = out.quotes.len() as u32;
        out.dropped += p.dropped;
        for mut c in p.chapters {
            c.quotes = c.quotes.iter().map(|q| q + base).collect();
            out.chapters.push(c);
        }
        for mut q in p.quotes {
            q.id += base;
            out.quotes.push(q);
        }
    }
    out
}

/// 单独一章仍然写不下时加的要求
pub const SHORTER: &str = "\n这一章很长，输出有长度限制：复述控制在四百字以内，events 最多八条，quotes 最多一段。";

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
你会收到全书的逐章复述和从书里摘下的原文（带编号 Q1、Q2…）。用简体中文写，具体、准确，多写人名、章节和细节，少写空话。\
人名第一次出现时写中文译名加英文原名，例如「亚哈（Ahab）」。凡是要填章的地方，都填章前方括号里的四位数编号（不是书里的章号）。\
你自己对这本书、作者、出版和评价史的了解可以用，但只写确实知道的事实，拿不准的不写。需要引用原文时只用给出的编号，不要自己写英文原文。\
文字里提到某一章时写「第 N 章」（书里的章号），不要把四位编号写进文字。";

/// 综合时发给的逐章复述最多这么多字（约六万 token，DeepSeek 的上下文放得下）
pub const NOTES_CHARS: usize = 80_000;

/// 综合时发给模型的材料：书名、作者、逐章复述、原文编号
pub fn notes_text(title: &str, author: Option<&str>, chapters: &[ChapterNote], quotes: &[Quote]) -> String {
    let mut p = format!("书名：{title}\n");
    if let Some(a) = author.filter(|a| !a.trim().is_empty()) {
        p.push_str(&format!("作者：{a}\n"));
    }
    p.push_str(&format!("共 {} 章。\n\n逐章复述：\n", chapters.len()));
    // 全书复述合计最多约 NOTES_CHARS 字，章多时每章截短
    let cap = (NOTES_CHARS / chapters.len().max(1)).max(150);
    for c in chapters {
        let kind = if c.kind == "digression" { "（题外话）" } else { "" };
        let summary: String = if c.summary.chars().count() > cap { c.summary.chars().take(cap).collect::<String>() + "…" } else { c.summary.clone() };
        p.push_str(&format!("[{}] {}{}：{}", cid(c.idx), c.title, kind, summary));
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
        p.push_str(&format!("Q{} [{}] {}\n", q.id, cid(q.chapter), q.text));
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
        4 => "这次画「图解」：从下面的图示类型里挑出最能帮读者看懂这本书的三到六种（不适合的不画，比如没有旅程就不画路线图，没有家族线就不画家谱），\
              把选中的写进 chosen（kind 是类型名，why 一句话说明这张图帮读者看懂什么），并填好对应的数据；没选的类型数据留空：\n\
              - relations 人物关系图：center 主角的 id；groups 分组（id、label，如船长、大副、岸上），五到八组；\
              people 十到二十人（id 用英文小写、name 中文名、en 英文名、group 分组 id）；links 关系二十到三十条，每个人至少有一条，尽量能顺着关系连到主角（a、b 是人物 id，kind 是 conflict 对立冲突 / bond 友谊亲情救助 / rank 上下级雇佣 / fate 预言警告宿命，label 四到八个字）\n\
              - plot 故事进展与伏笔图：events 十五到二十八个关键事件（id 如 e1、chapter 章编号、text 二十字以内）；\
              links 伏笔→应验或前因→后果的连线六到十二条（from、to 是事件 id，kind 是 fate 伏笔应验 / cause 前因后果，label 八字以内）\n\
              - tree 组织或等级树（船、军队、宫廷、公司里谁管谁）：nodes 每个 id、parent（上级的 id，最上面的填空字符串）、name、role（职位或一句说明）、special（不公开或特殊的关系填 true）\n\
              - family 家谱：格式同 tree，parent 填父亲或母亲的 id，role 写和谁结婚、第几代等\n\
              - route 地点路线图（旅程、航海、远征）：stops 五到九站，每站 place、chapter（章编号）、what（在那里发生的事，十二字以内）\n\
              - rhythm 全书节奏图（每章情节还是题外话、紧张程度，程序按逐章复述自动画）：marks 三到六个高潮点，每个 chapter、label（六字以内）\n\
              - compare 对照图（几个人物、阵营、事件按同样几项横向比较，如九次海上相遇、几位求婚者）：title；items 三到十二项，\
              每项 name、en（英文名，没有留空）、chapter、badge（一个四字以内的标签，如「见过白鲸」）、tone（bad 凶兆伤亡 / warn 警告 / good 喜剧反衬 / neutral）、text（二十五字以内）\n\
              - causal 因果链或论证结构（历史、社科、议论性的书）：nodes 五到十二个（id、text 十五字以内、kind 是 claim 论点 / reason 理由 / evidence 证据 / cause 原因 / effect 结果）；links（from、to、label）\n\
              - concepts 概念关系图（哲学、科普书的核心概念）：nodes 五到十个（id、text 概念名、kind 填 concept）；links（from、to、label 说明关系）",
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
        4 => {
            let link = |a: &str, b: &str| obj(json!({ a: s, b: s, "kind": s, "label": s }));
            let tree = obj(json!({ "nodes": arr(obj(json!({ "id": s, "parent": s, "name": s, "role": s, "special": { "type": "boolean" } }))) }));
            let graph = obj(json!({ "nodes": arr(obj(json!({ "id": s, "text": s, "kind": s }))), "links": arr(obj(json!({ "from": s, "to": s, "label": s }))) }));
            json!({
                "diagrams": obj(json!({
                    "chosen": arr(obj(json!({ "kind": s, "why": s }))),
                    "relations": obj(json!({
                        "center": s,
                        "groups": arr(obj(json!({ "id": s, "label": s }))),
                        "people": arr(obj(json!({ "id": s, "name": s, "en": s, "group": s }))),
                        "links": arr(link("a", "b"))
                    })),
                    "plot": obj(json!({ "events": arr(obj(json!({ "id": s, "chapter": i, "text": s }))), "links": arr(link("from", "to")) })),
                    "tree": tree.clone(),
                    "family": tree,
                    "route": obj(json!({ "stops": arr(obj(json!({ "place": s, "chapter": i, "what": s }))) })),
                    "rhythm": obj(json!({ "marks": arr(obj(json!({ "chapter": i, "label": s }))) })),
                    "compare": obj(json!({ "title": s, "items": arr(obj(json!({ "name": s, "en": s, "chapter": i, "badge": s, "tone": s, "text": s }))) })),
                    "causal": graph.clone(),
                    "concepts": graph
                }))
            })
        }
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
        4 => "只输出 JSON，格式：{\"diagrams\": {\"chosen\": [{\"kind\": \"relations\", \"why\": \"\"}], \"relations\": {\"center\": \"\", \"groups\": [{\"id\": \"\", \"label\": \"\"}], \"people\": [{\"id\": \"\", \"name\": \"\", \"en\": \"\", \"group\": \"\"}], \"links\": [{\"a\": \"\", \"b\": \"\", \"kind\": \"bond\", \"label\": \"\"}]}, \"plot\": {\"events\": [{\"id\": \"e1\", \"chapter\": 1001, \"text\": \"\"}], \"links\": [{\"from\": \"e1\", \"to\": \"e2\", \"kind\": \"fate\", \"label\": \"\"}]}, \"tree\": {\"nodes\": [{\"id\": \"\", \"parent\": \"\", \"name\": \"\", \"role\": \"\", \"special\": false}]}, \"family\": {\"nodes\": []}, \"route\": {\"stops\": [{\"place\": \"\", \"chapter\": 1001, \"what\": \"\"}]}, \"rhythm\": {\"marks\": [{\"chapter\": 1001, \"label\": \"\"}]}, \"compare\": {\"title\": \"\", \"items\": [{\"name\": \"\", \"en\": \"\", \"chapter\": 1001, \"badge\": \"\", \"tone\": \"bad\", \"text\": \"\"}]}, \"causal\": {\"nodes\": [], \"links\": []}, \"concepts\": {\"nodes\": [], \"links\": []}}}",
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
    pub diagrams: Diagrams,
}

// —— 图解 ——

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chosen {
    pub kind: String,
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub en: String,
    pub group: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Link {
    #[serde(alias = "from")]
    pub a: String,
    #[serde(alias = "to")]
    pub b: String,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelGraph {
    pub center: String,
    pub groups: Vec<Group>,
    pub people: Vec<Person>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlotEvent {
    pub id: String,
    pub chapter: i64,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlotMap {
    pub events: Vec<PlotEvent>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeNode {
    pub id: String,
    pub parent: String,
    pub name: String,
    pub role: String,
    pub special: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tree {
    pub nodes: Vec<TreeNode>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stop {
    pub place: String,
    pub chapter: i64,
    pub what: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Route {
    pub stops: Vec<Stop>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mark {
    pub chapter: i64,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rhythm {
    pub marks: Vec<Mark>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompareItem {
    pub name: String,
    pub en: String,
    pub chapter: i64,
    pub badge: String,
    pub tone: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Compare {
    pub title: String,
    pub items: Vec<CompareItem>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphNode {
    pub id: String,
    pub text: String,
    pub kind: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub links: Vec<Link>,
}

/// AI 挑的图示和各自的数据（没选的留空）
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Diagrams {
    pub chosen: Vec<Chosen>,
    pub relations: RelGraph,
    pub plot: PlotMap,
    pub tree: Tree,
    pub family: Tree,
    pub route: Route,
    pub rhythm: Rhythm,
    pub compare: Compare,
    pub causal: Graph,
    pub concepts: Graph,
}

/// 图示的类型名（AI 偶尔写别的名字，不认识的丢掉）
pub const DIAGRAM_KINDS: [&str; 9] = ["relations", "plot", "tree", "family", "route", "rhythm", "compare", "causal", "concepts"];

/// 只留认识的、有数据的图；重复的去掉，最多六张
pub fn tidy_diagrams(d: &mut Diagrams) {
    let has = |d: &Diagrams, k: &str| match k {
        "relations" => d.relations.people.len() >= 2,
        "plot" => d.plot.events.len() >= 3,
        "tree" => d.tree.nodes.len() >= 2,
        "family" => d.family.nodes.len() >= 2,
        "route" => d.route.stops.len() >= 2,
        "rhythm" => true,
        "compare" => d.compare.items.len() >= 2,
        "causal" => d.causal.nodes.len() >= 2,
        "concepts" => d.concepts.nodes.len() >= 2,
        _ => false,
    };
    let mut seen = std::collections::HashSet::new();
    let chosen: Vec<Chosen> = d
        .chosen
        .iter()
        .filter(|c| DIAGRAM_KINDS.contains(&c.kind.as_str()) && has(d, &c.kind) && seen.insert(c.kind.clone()))
        .take(6)
        .cloned()
        .collect();
    d.chosen = chosen;
}

/// 一次综合的输出（只有这一步的几项）并进总的结果
///
/// 逐项解析：DeepSeek 的 JSON 模式不强制格式，模型偶尔多写一项、把列表写成一段话，只丢掉格式不对的那一项（图解按每种图分别丢）
pub fn merge_synth(into: &mut Synthesis, text: &str) -> Result<()> {
    let v: Value = serde_json::from_str(text).map_err(parse_err)?;
    let Some(obj) = v.as_object() else {
        return Err(Error::Online(crate::i18n::t("模型输出的不是 JSON 对象", "The model output is not a JSON object").into()));
    };
    let d = Synthesis::default();
    let mut take = |s: Synthesis| {
        macro_rules! take {
            ($($f:ident),*) => { $( if s.$f != d.$f { into.$f = s.$f.clone(); } )* };
        }
        take!(one_line, paragraph, talk, structure, characters, relations, background, glossary, timeline, motifs, themes, technique, reception, criticism, origins, misconceptions, topics, golden, further, faq, quiz);
        // 图解：已有的几种图不被这一项里没有的图覆盖掉
        if s.diagrams != d.diagrams {
            let (n, o) = (&s.diagrams, &mut into.diagrams);
            if !n.chosen.is_empty() {
                o.chosen = n.chosen.clone();
            }
            macro_rules! sub {
                ($($f:ident),*) => { $( if n.$f != Default::default() { o.$f = n.$f.clone(); } )* };
            }
            sub!(relations, plot, tree, family, route, rhythm, compare, causal, concepts);
        }
    };
    for (k, val) in obj {
        match serde_json::from_value::<Synthesis>(json!({ k: val })) {
            Ok(s) => take(s),
            Err(e) if k == "diagrams" => {
                tracing::warn!(error = %e, "精读本：图解有格式不对的部分，逐种解析");
                for (sk, sv) in val.as_object().into_iter().flatten() {
                    match serde_json::from_value::<Synthesis>(json!({ "diagrams": { sk: sv } })) {
                        Ok(s) => take(s),
                        Err(e) => tracing::warn!(key = %sk, error = %e, "精读本：丢掉格式不对的一种图"),
                    }
                }
            }
            Err(e) => tracing::warn!(key = %k, error = %e, "精读本：丢掉格式不对的一项"),
        }
    }
    Ok(())
}

/// 文字里出现的四位章编号（模型偶尔把「[1013]」照抄进正文）换成「第 N 章」或章名
pub fn ids_to_labels(text: &str, chapters: &[(i64, String)]) -> String {
    let label = |n: i64| -> Option<String> {
        let idx = n - ID_BASE;
        let (_, title) = chapters.iter().find(|(i, _)| *i == idx)?;
        let num = title.split(|c: char| !c.is_ascii_digit()).find(|w| !w.is_empty());
        Some(match num {
            Some(d) => format!("第 {d} 章"),
            None => title.clone(),
        })
    };
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let run = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        if run == 4 && (i == 0 || !b[i - 1].is_ascii_digit()) {
            if let Some(l) = text[i..i + 4].parse::<i64>().ok().filter(|&n| n > ID_BASE).and_then(label) {
                out.push_str(&l);
                i += 4;
                continue;
            }
        }
        if run > 0 {
            out.push_str(&text[i..i + run]);
            i += run;
            continue;
        }
        let ch = text[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// 综合结果里所有文字中的四位章编号换成章名
fn labels_in_text(s: &mut Synthesis, chapters: &[(i64, String)]) {
    fn walk(v: &mut Value, chapters: &[(i64, String)]) {
        match v {
            Value::String(t) => *t = ids_to_labels(t, chapters),
            Value::Array(a) => a.iter_mut().for_each(|x| walk(x, chapters)),
            Value::Object(o) => o.values_mut().for_each(|x| walk(x, chapters)),
            _ => {}
        }
    }
    if let Ok(mut v) = serde_json::to_value(&*s) {
        walk(&mut v, chapters);
        if let Ok(fixed) = serde_json::from_value(v) {
            *s = fixed;
        }
    }
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
    pub scenes: Vec<Scene>,
    pub synthesis: Synthesis,
    /// 作者卒年（查维基百科）、是不是公版书：公版书的名场面放整段原文和译文
    pub author_died: Option<i32>,
    pub public_domain: bool,
    /// 模型给了、但在书里找不到而丢掉的原文条数
    pub dropped: u32,
}

/// 估算的 token 数：英文一词约 1.35 token；中文输出一字约 0.7 token
pub fn estimate_tokens(total_words: usize, chapters: usize) -> (u64, u64) {
    let read_in = (total_words as f64 * 1.35) as u64 + 2000 * chapters.div_ceil(CHUNK_CHAPTERS).max(1) as u64;
    // 情节章的复述三百到六百字，加上事件、原文、名场面的开头结尾（实测《白鲸》每章约 1500 token）
    let read_out = 1500 * chapters as u64;
    let notes = ((NOTES_CHARS as u64).min(450 * chapters as u64)) * 7 / 10 + 120 * chapters as u64;
    let synth_in = notes * SYNTH_STEPS as u64;
    let synth_out = 8000 * SYNTH_STEPS as u64;
    // 名场面：每段原文约 450 词，译文和前因后果约 1000 token
    let scenes = MAX_SCENES.min(chapters) as u64;
    let scene_in = scenes * (600 + 400);
    let scene_out = scenes * 1500;
    (read_in + synth_in + scene_in, read_out + synth_out + scene_out)
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
        let many: Vec<ChapterRef> = (1..=20).map(|i| ch(i, 100)).collect();
        assert_eq!(chunks(&many).iter().map(Vec::len).collect::<Vec<_>>(), vec![8, 8, 4]);
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
                advice: "read".into(),
                advice_why: "开头".into(),
                tension: 3,
                scene: RawScene::default(),
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
        assert_eq!((n.chapters[0].advice.as_str(), n.chapters[1].advice.as_str()), ("read", "normal"));

        let joined = join_chunks(vec![n.clone(), n.clone()]);
        assert_eq!(joined.quotes.iter().map(|q| q.id).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(joined.chapters[2].quotes, vec![1]);
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
            assert!(!schema["required"].as_array().unwrap().is_empty());
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
            advice: "read".into(),
            advice_why: String::new(),
            tension: 5,
            quotes: vec![3],
            scene: None,
        };
        let q = Quote { id: 3, chapter: 36, text: "Death to Moby Dick!".into(), zh: String::new(), why: String::new() };
        let t = notes_text("Moby-Dick", Some("Herman Melville"), &[c], &[q]);
        assert!(t.contains("[1036] The Quarter-Deck：亚哈宣布追杀白鲸。 事件：钉金币。 人物：Ahab。"));
        assert!(t.contains("逐章复述"));
        assert!(t.contains("Q3 [1036] Death to Moby Dick!"));
        let p = read_prompt("Moby-Dick", None, &[(ch(1, 5), "Call me Ishmael.".into())]);
        assert!(p.contains("=== [1001] Chapter 1 ===\nCall me Ishmael."));
        let chs = vec![(10, "ETYMOLOGY".to_string()), (13, "CHAPTER 1 Loomings".to_string()), (145, "CHAPTER 133 The Chase".to_string())];
        assert_eq!(resolve(1013, &chs), Some(13));
        assert_eq!(resolve(133, &chs), Some(145), "模型写了书里的章号");
        assert_eq!(resolve(1999, &chs), None);
        let mut s = Synthesis { timeline: vec![Event { chapter: 1145, event: String::new() }], themes: vec![Theme { chapters: vec![1013, 999, 1], ..Theme::default() }], ..Synthesis::default() };
        resolve_synthesis(&mut s, &chs);
        assert_eq!(s.timeline[0].chapter, 145);
        assert_eq!(s.themes[0].chapters, vec![13, 13], "1 是书里的第 1 章，999 对不上丢掉");
        let mut s = Synthesis::default();
        merge_synth(&mut s, r#"{"diagrams":{"chosen":[{"kind":"relations","why":"x"},{"kind":"route","why":"y"},{"kind":"pie","why":"z"},{"kind":"relations","why":"dup"},{"kind":"tree","why":"empty"}],
            "relations":{"center":"ahab","people":[{"id":"ahab","name":"亚哈"},{"id":"moby","name":"白鲸"}],"links":[{"a":"ahab","b":"moby","kind":"conflict","label":"复仇"}]},
            "route":{"stops":[{"place":"南塔克特","chapter":1013,"what":"起航"},{"place":"太平洋","chapter":133,"what":"沉船"}]}}}"#).unwrap();
        resolve_synthesis(&mut s, &chs);
        let kinds: Vec<&str> = s.diagrams.chosen.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, ["relations", "route"], "不认识的、重复的、没数据的去掉");
        // 多写了一项、格式不对：只丢那一项
        let mut s2 = Synthesis::default();
        merge_synth(&mut s2, r#"{"technique":"手法","structure":"本该是列表","diagrams":{"chosen":[{"kind":"route","why":""}],"route":{"stops":[{"place":"罗马","chapter":1013,"what":""}]},"plot":"坏的"}}"#).unwrap();
        assert_eq!(s2.technique, "手法");
        assert!(s2.structure.is_empty());
        assert_eq!(s2.diagrams.route.stops.len(), 1, "图解里坏的那种丢掉，其他保留");
        assert_eq!(ids_to_labels("岸上（1013—1145）与 [1010]，1851 年", &chs), "岸上（第 1 章—第 133 章）与 [ETYMOLOGY]，1851 年");
        assert_eq!(s.diagrams.route.stops.iter().map(|x| x.chapter).collect::<Vec<_>>(), vec![13, 145]);
        for step in 0..SYNTH_STEPS {
            assert!(synth_schema(step)["properties"].as_object().is_some_and(|m| !m.is_empty()), "step {step}");
        }
    }

    fn long_text() -> String {
        let mut t = String::from("The ship lay quiet in the harbour. ");
        for i in 0..40 {
            t.push_str(&format!("Sentence number {i} tells a little more of the story here. "));
        }
        t.push_str("And then the whale rose from the deep. Nobody spoke for a while.");
        t
    }

    #[test]
    fn scenes_are_cut_from_the_book_between_start_and_end() {
        let t = long_text();
        let s = locate_scene(&t, "Sentence number 2 tells a little more of the story here.", "And then the whale rose from the deep.").unwrap();
        let p = locate_scene(&t.replace("Sentence number 20 ", "\n  Sentence number 20 "), "Sentence number 2 tells a little more of the story here.", "And then the whale rose from the deep.").unwrap();
        assert_eq!(p.lines().count(), 2, "保留分段");
        assert!(p.lines().nth(1).unwrap().starts_with("Sentence number 20"));
        assert!(s.starts_with("Sentence number 2 tells") && s.ends_with("rose from the deep."), "{s}");
        // 结尾句找不到、太短都不收
        assert!(locate_scene(&t, "Sentence number 2 tells a little more of the story here.", "This sentence is not there at all, really.").is_none());
        assert!(locate_scene(&t, "Sentence number 2 tells a little more of the story here.", "Sentence number 3 tells a little more of the story here.").is_none());
        // 截短退到句末
        let c = clip_to_sentence(&t, 20);
        assert!(c.ends_with('.') && word_count(&c) <= 20, "{c}");
    }

    #[test]
    fn scenes_are_picked_by_importance_then_ordered_by_chapter() {
        let note = |idx: i64, importance: u8| ChapterNote {
            idx,
            title: String::new(),
            summary: String::new(),
            kind: "plot".into(),
            events: vec![],
            characters: vec![],
            advice: "normal".into(),
            advice_why: String::new(),
            tension: 2,
            quotes: vec![],
            scene: Some(Scene { chapter: idx, importance, text: "x".into(), ..Scene::default() }),
        };
        let chapters: Vec<ChapterNote> = (1..=30).map(|i| note(i, if i % 3 == 0 { 5 } else { 2 })).collect();
        let picked = select_scenes(&chapters);
        assert_eq!(picked.len(), MAX_SCENES);
        assert!(picked.windows(2).all(|w| w[0].chapter < w[1].chapter));
        assert_eq!(picked.iter().filter(|s| s.importance == 5).count(), 10, "最重要的都在");
        assert_eq!(picked[0].id, 1);
        let mut s = Scene { text: long_text(), ..Scene::default() };
        clip_scene(&mut s);
        assert!(s.clipped && word_count(&s.text) <= SCENE_CLIP_WORDS);
        let mut list = vec![Scene { id: 2, why: "旧".into(), ..Scene::default() }];
        apply_scene_notes(&mut list, &parse_scenes(r#"{"scenes":[{"id":2,"zh":"译文","context":"前因","why":""}]}"#).unwrap());
        assert_eq!((list[0].zh.as_str(), list[0].context.as_str(), list[0].why.as_str()), ("译文", "前因", "旧"));
    }

    #[test]
    fn must_read_is_capped_by_tension() {
        let note = |idx: i64, tension: u8| ChapterNote {
            idx,
            title: String::new(),
            summary: String::new(),
            kind: "plot".into(),
            events: vec![],
            characters: vec![],
            advice: "read".into(),
            advice_why: String::new(),
            tension,
            quotes: vec![],
            scene: None,
        };
        let mut chs: Vec<ChapterNote> = (1..=20).map(|i| note(i, if i == 7 || i == 19 { 5 } else { 2 })).collect();
        cap_must_read(&mut chs);
        let kept: Vec<i64> = chs.iter().filter(|c| c.advice == "read").map(|c| c.idx).collect();
        assert_eq!(kept, vec![1, 7, 19, 2].into_iter().collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn chapters_are_split_by_bare_numbers() {
        assert_eq!(chapter_marker(" 12 "), Some(12));
        assert_eq!(chapter_marker("XIV"), Some(14));
        assert_eq!(chapter_marker("Chapter 3."), Some(3));
        assert_eq!(chapter_marker("CHAPTER 1 Loomings"), None);
        assert_eq!(chapter_marker("I"), Some(1));
        assert_eq!(chapter_marker("1956"), None);
        assert_eq!(chapter_marker("1 O"), Some(10), "OCR 把 0 认成 O");
        assert_eq!(chapter_marker("Oh"), None);
        let body = |s: &str| format!("{s} ").repeat(200);
        let files = vec![
            ("Published by Vintage".to_string(), "Published by Vintage. ISBN 0 09 928287 9. A CIP catalogue record.".to_string()),
            ("Published by Vintage".to_string(), format!("1\n{}\n2\n{}\n‘Yes.’\n4\n{}", body("one"), body("two"), body("four"))),
            ("Tom Ripley".to_string(), format!("{}\n5\n{}\n7\n{}\n1956\n", body("still four"), body("five"), body("seven"))),
        ];
        let units = build_units(&files);
        let titles: Vec<&str> = units.iter().map(|u| u.0.as_str()).collect();
        assert_eq!(titles, ["Chapter 1", "Chapter 2", "Chapter 4", "Chapter 5", "Chapter 7"], "版权页丢掉，跳号也认");
        assert!(units[2].1.contains("still four"), "跨文件的章接上");
        assert!(units[4].1.contains("1956"), "不递增的数字是正文");
        // 一个文件一章的书原样用
        let moby = vec![("CHAPTER 1 Loomings".to_string(), "CHAPTER 1\nLoomings\nCall me Ishmael.".to_string()), ("CHAPTER 2 The Carpet-Bag".to_string(), "CHAPTER 2\nThe Carpet-Bag\nI stuffed.".to_string()), ("CHAPTER 3".to_string(), "CHAPTER 3\nx".to_string())];
        assert_eq!(build_units(&moby).iter().map(|u| u.0.as_str()).collect::<Vec<_>>(), ["CHAPTER 1 Loomings", "CHAPTER 2 The Carpet-Bag", "CHAPTER 3"]);
    }

    #[test]
    fn public_domain_from_the_wikipedia_lead() {
        let melville = "Herman Melville (born Melvill; August 1, 1819 – September 28, 1891) was an American novelist.";
        assert_eq!(death_year(melville), Some(1891));
        assert!(is_public_domain(death_year(melville), 2026));
        assert_eq!(death_year("Robert M. Pirsig (September 6, 1928 – April 24, 2017) was an American writer."), Some(2017));
        assert!(!is_public_domain(Some(2017), 2026));
        assert_eq!(death_year("Haruki Murakami (born January 12, 1949) is a Japanese writer."), None);
        assert!(!is_public_domain(None, 2026), "查不到按非公版");
        assert!(is_public_domain(Some(1955), 2026) && !is_public_domain(Some(1956), 2026));
        assert_eq!(year_of_ms(1_793_000_000_000), 2026);
        assert_eq!(year_of_ms(0), 1970);
        assert_eq!(year_of_ms(1_735_689_599_000), 2024, "2024-12-31 23:59:59");
    }
}
