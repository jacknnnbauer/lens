//! 义项图谱（M8，实验性）：从各词典的词条 HTML 里抽出义项，按词面相似度对齐成「同一个意思」的组。
//!
//! 抽取两条路：
//! - 结构化词典：class 为 d / def / dfn / definition / nataccep 等的元素就是一条释义（去掉其中的例句）；
//! - 纯文本词典：按 <br>/<p>/<div> 分行，以 `1.` `a)` `①` `1 =` 开头的行是一条义项。
//!
//! 对齐：中文按双字组、英文按词干（去停用词）求重叠系数，贪心归入最相近且尚无该词典成员的组。
//! 只是启发式，界面上标为「实验」。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

const MAX_SENSES_PER_DICT: usize = 20;
const MIN_CHARS: usize = 2;
const MAX_CHARS: usize = 400;
const LABEL_CHARS: usize = 140;
/// 相似度达到这个值才算同一个义项
const THRESHOLD: f64 = 0.5;
/// 两边都标了词性且不同：相似度打折（跨词性的「同义」多半是误配）
const POS_PENALTY: f64 = 0.5;

const DEF_CLASSES: &[&str] = &["d", "def", "defn", "dfn", "definition", "nataccep", "sense-def", "gloss"];
/// 释义元素里要跳过的部分：例句、编号
/// 释义元素里（或外层）要跳过的部分：例句、编号、习语与短语动词（单独成块，不算义项）
const SKIP_CLASSES: &[&str] = &[
    "x", "x-g", "xe", "xc", "example", "examples", "eg", "ex", "exa", "alp", "alp_a", "num", "z_n", "symbols-xsym", "id-g",
    "idm-g", "idmb", "idms", "pv-g", "pvb", "idiom", "idioms", "phrvb",
];
const POS_CLASSES: &[&str] = &["pos", "cixing"];
const BLOCK_TAGS: &[&str] = &[
    "br", "p", "div", "li", "dd", "dt", "tr", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "table", "section", "article",
    "blockquote",
];
const VOID_TAGS: &[&str] = &["br", "img", "link", "meta", "input", "hr", "source", "wbr", "area", "col", "embed", "track"];
const EN_STOP: &[&str] = &[
    "the", "and", "for", "that", "with", "from", "this", "which", "who", "whom", "are", "was", "were", "been", "being", "have",
    "has", "had", "into", "onto", "upon", "out", "off", "not", "any", "all", "such", "very", "more", "most", "some", "other",
    "its", "their", "them", "they", "you", "your", "his", "her", "one", "someone", "something", "somebody", "especially",
    "usually", "often", "used", "use", "esp", "etc", "kind", "type", "part", "make", "made", "way", "thing", "things", "person",
    "people", "can", "will", "may", "about", "than", "also", "when", "where", "what", "how", "being", "act", "state", "quality",
];
const CJK_STOP: &str = "的地得了着等或与和及之其某一个种使被把对在是有为于将而且以";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasInput {
    pub id: String,
    pub name: String,
    /// 该词典命中的各个词条 HTML
    pub entries: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sense {
    /// 词典在 `SenseAtlas::dicts` 里的下标
    pub dict: usize,
    /// 在该词典里的顺序（逐典视图按它排）
    pub index: usize,
    pub text: String,
    pub pos: Option<String>,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cluster {
    pub label: String,
    pub senses: Vec<Sense>,
    /// 参与的词典（下标，去重、按词典顺序）
    pub dicts: Vec<usize>,
    pub domain: Option<String>,
    /// 成员词性不一致时，各词典的词性
    pub pos_diff: Vec<(usize, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasDict {
    pub id: String,
    pub name: String,
    pub senses: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SenseAtlas {
    /// 抽出了义项的词典（按查词顺序）
    pub dicts: Vec<AtlasDict>,
    /// 共识多的在前
    pub clusters: Vec<Cluster>,
}

// —— HTML → 文本 ——

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        // 实体名很短：只在前 12 字节里找分号。按字符找，不能按字节切——
        // 「&」后面紧跟中文或 ī 这类多字节字符时，切在字符中间会 panic（义项图谱闪退）
        let end = tail.char_indices().take_while(|(i, _)| *i < 12).find(|(_, c)| *c == ';').map(|(i, _)| i);
        let decoded = end.and_then(|e| {
            let name = &tail[1..e];
            let ch = match name {
                "nbsp" | "ensp" | "emsp" | "thinsp" => Some(' '),
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "middot" => Some('·'),
                "mdash" => Some('—'),
                "ndash" => Some('–'),
                _ => {
                    let num = name.strip_prefix('#')?;
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code)
                }
            }?;
            Some((ch, e + 1))
        });
        match decoded {
            Some((ch, len)) => {
                out.push(ch);
                rest = &tail[len..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Tag<'a> {
    name: String,
    closing: bool,
    self_closing: bool,
    classes: Vec<&'a str>,
}

fn parse_tag(raw: &str) -> Option<Tag<'_>> {
    let closing = raw.starts_with('/');
    let body = raw.trim_start_matches('/');
    let name_end = body.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(body.len());
    let name = body[..name_end].to_ascii_lowercase();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let mut classes = Vec::new();
    if let Some(i) = body.find("class=") {
        let v = &body[i + 6..];
        let (q, v) = match v.chars().next() {
            Some(q @ ('"' | '\'')) => (Some(q), &v[1..]),
            _ => (None, v),
        };
        let end = match q {
            Some(q) => v.find(q).unwrap_or(v.len()),
            None => v.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(v.len()),
        };
        classes = v[..end].split_whitespace().collect();
    }
    Some(Tag { name, closing, self_closing: raw.ends_with('/'), classes })
}

struct Elem {
    name: String,
    def: bool,
    skip: bool,
    pos: bool,
}

#[derive(Debug, Default)]
struct Extracted {
    /// 结构化释义：(文本, 词性)
    defs: Vec<(String, Option<String>)>,
    /// 按块分行的纯文本
    lines: Vec<String>,
}

fn extract(html: &str) -> Extracted {
    let mut out = Extracted::default();
    let mut stack: Vec<Elem> = Vec::new();
    let mut line = String::new();
    let mut def_buf = String::new();
    let mut pos_buf = String::new();
    let mut cur_pos: Option<String> = None;
    let mut rest = html;

    let flush_line = |line: &mut String, lines: &mut Vec<String>| {
        let t = clean(&decode_entities(line));
        if !t.is_empty() {
            lines.push(t);
        }
        line.clear();
    };

    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            push_text(rest, &stack, &mut line, &mut def_buf, &mut pos_buf);
            break;
        };
        push_text(&rest[..lt], &stack, &mut line, &mut def_buf, &mut pos_buf);
        let after = &rest[lt + 1..];
        if let Some(c) = after.strip_prefix("!--") {
            rest = c.find("-->").map_or("", |e| &c[e + 3..]);
            continue;
        }
        let Some(gt) = after.find('>') else { break };
        let raw = after[..gt].trim();
        rest = &after[gt + 1..];
        let Some(tag) = parse_tag(raw) else { continue };

        if !tag.closing && (tag.name == "style" || tag.name == "script") {
            let close = format!("</{}", tag.name);
            rest = rest.to_ascii_lowercase().find(&close).map_or("", |e| {
                let r = &rest[e..];
                r.find('>').map_or("", |g| &r[g + 1..])
            });
            continue;
        }
        if BLOCK_TAGS.contains(&tag.name.as_str()) {
            flush_line(&mut line, &mut out.lines);
        }
        if tag.closing {
            if let Some(i) = stack.iter().rposition(|e| e.name == tag.name) {
                let popped: Vec<Elem> = stack.drain(i..).collect();
                for e in popped.into_iter().rev() {
                    if e.def && !stack.iter().any(|x| x.def) {
                        let t = clean(&decode_entities(&def_buf));
                        if !t.is_empty() {
                            out.defs.push((t, cur_pos.clone()));
                        }
                        def_buf.clear();
                    }
                    if e.pos && !stack.iter().any(|x| x.pos) {
                        cur_pos = normalize_pos(&clean(&decode_entities(&pos_buf))).or(cur_pos.take());
                        pos_buf.clear();
                    }
                }
            }
            continue;
        }
        if VOID_TAGS.contains(&tag.name.as_str()) || tag.self_closing {
            continue;
        }
        let has = |set: &[&str]| tag.classes.iter().any(|c| set.contains(c));
        stack.push(Elem { name: tag.name, def: has(DEF_CLASSES), skip: has(SKIP_CLASSES), pos: has(POS_CLASSES) });
    }
    flush_line(&mut line, &mut out.lines);
    out
}

fn push_text(text: &str, stack: &[Elem], line: &mut String, def_buf: &mut String, pos_buf: &mut String) {
    if text.is_empty() {
        return;
    }
    line.push_str(text);
    let skipping = stack.iter().any(|e| e.skip);
    if stack.iter().any(|e| e.def) && !skipping {
        def_buf.push_str(text);
    }
    if stack.iter().any(|e| e.pos) {
        pos_buf.push_str(text);
    }
}

// —— 词性、领域、编号 ——

fn normalize_pos(s: &str) -> Option<String> {
    // 取开头的字母词：`n[C]` → n，`[noun]` → noun，`transitive verb` 保留空格
    let t = s.trim_start_matches(|c: char| !c.is_ascii_alphabetic());
    let end = t.find(|c: char| !(c.is_ascii_alphabetic() || c == ' ')).unwrap_or(t.len());
    let t = t[..end].trim().to_ascii_lowercase();
    let t = t.as_str();
    let p = match t {
        "n" | "noun" | "nouns" => "n.",
        "v" | "vt" | "vi" | "verb" | "transitive verb" | "intransitive verb" => "v.",
        "adj" | "adjective" => "adj.",
        "adv" | "adverb" => "adv.",
        "prep" | "preposition" => "prep.",
        "conj" | "conjunction" => "conj.",
        "pron" | "pronoun" => "pron.",
        "int" | "interj" | "interjection" | "exclamation" => "int.",
        "abbr" | "abbreviation" => "abbr.",
        _ => return None,
    };
    Some(p.to_string())
}

/// 行首编号：`1.` `12)` `3、` `1 =` `①` `a.` `b)`；返回编号之后的正文。
fn strip_marker(line: &str) -> Option<&str> {
    let s = line.trim_start();
    let mut chars = s.char_indices();
    let (_, first) = chars.next()?;
    let rest = if ('\u{2460}'..='\u{2473}').contains(&first) {
        &s[first.len_utf8()..]
    } else if first == '(' {
        // `(1)` 小项
        let close = s.find(')')?;
        if close < 2 || !s[1..close].chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        &s[close + 1..]
    } else if first.is_ascii_digit() {
        let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let r = &s[end..];
        let r2 = r.trim_start();
        if let Some(x) = r.strip_prefix(['.', ')', '、', '．']) {
            x
        } else if r2.starts_with('=') || r2.starts_with(':') {
            r2
        } else {
            return None;
        }
    } else if first.is_ascii_lowercase() {
        let r = &s[1..];
        r.strip_prefix(['.', ')'])?
    } else {
        return None;
    };
    Some(rest.trim_start_matches(|c: char| c.is_whitespace() || c == ':' || c == '=' || c == '：'))
}

/// 罗马数字 + 词性的行（如 MW 的 `II. verb`）或单独的词性行（`[noun]`）
fn pos_line(line: &str) -> Option<String> {
    let t = line.trim();
    let t = t.trim_start_matches(|c: char| matches!(c, 'I' | 'V' | 'X') || c == '.' || c.is_whitespace());
    let t = t.split(['\\', '/']).next().unwrap_or(t);
    if t.chars().count() > 24 {
        return None;
    }
    normalize_pos(t.trim_matches(|c: char| c == '[' || c == ']' || c == '(' || c == ')' || c.is_whitespace()))
}

/// 领域标签：【计】【计算机】〈计〉[computing] (Computing)
fn domain_of(text: &str) -> Option<String> {
    for (open, close) in [('【', '】'), ('〈', '〉'), ('[', ']'), ('(', ')'), ('（', '）')] {
        let mut s = text;
        while let Some(i) = s.find(open) {
            let after = &s[i + open.len_utf8()..];
            let Some(j) = after.find(close) else { break };
            let inner = after[..j].trim();
            let n = inner.chars().count();
            let cjk = inner.chars().all(is_cjk);
            let en_domain = inner.chars().all(|c| c.is_ascii_alphabetic() || c == ' ')
                && inner.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && DOMAINS.contains(&inner.to_ascii_lowercase().as_str());
            let register = REGISTERS.iter().any(|r| inner.starts_with(r));
            if (cjk && (1..=4).contains(&n) && (open == '【' || open == '〈') && !register) || en_domain {
                return Some(inner.to_string());
            }
            s = &after[j..];
        }
    }
    None
}

/// 语体 / 地域标签，不是领域
const REGISTERS: &[&str] = &["尤", "英", "美", "澳", "非正式", "正式", "口", "书", "旧", "贬", "褒", "谑", "俚", "过时", "文"];

const DOMAINS: &[&str] = &[
    "computing", "computer", "computers", "finance", "financial", "law", "legal", "medicine", "medical", "biology", "chemistry",
    "physics", "mathematics", "maths", "math", "music", "sport", "sports", "business", "economics", "geology", "geography",
    "aviation", "nautical", "military", "botany", "zoology", "anatomy", "linguistics", "grammar", "philosophy", "religion",
    "technical",
];

fn is_cjk(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3400}'..='\u{4dbf}').contains(&c)
}

// —— 相似度 ——

#[derive(Debug, Default)]
pub(crate) struct Tokens {
    pub(crate) cjk: BTreeSet<String>,
    pub(crate) en: BTreeSet<String>,
}

pub(crate) fn stem(w: &str) -> String {
    for suf in ["ing", "ed", "es", "ly", "s"] {
        if let Some(b) = w.strip_suffix(suf) {
            if b.chars().count() >= 4 {
                return b.to_string();
            }
        }
    }
    w.to_string()
}

pub(crate) fn tokens(text: &str, headword: &str) -> Tokens {
    let mut t = Tokens::default();
    let head = headword.to_lowercase();
    let mut word = String::new();
    let mut run: Vec<char> = Vec::new();
    let flush_run = |run: &mut Vec<char>, t: &mut Tokens| {
        match run.len() {
            0 => {}
            1 => {
                t.cjk.insert(run[0].to_string());
            }
            _ => {
                for w in run.windows(2) {
                    t.cjk.insert(w.iter().collect());
                }
            }
        }
        run.clear();
    };
    let flush_word = |word: &mut String, t: &mut Tokens| {
        if word.len() >= 3 && !EN_STOP.contains(&word.as_str()) && *word != head {
            t.en.insert(stem(word));
        }
        word.clear();
    };
    for c in text.chars() {
        if is_cjk(c) {
            flush_word(&mut word, &mut t);
            if !CJK_STOP.contains(c) {
                run.push(c);
            } else {
                flush_run(&mut run, &mut t);
            }
        } else if c.is_ascii_alphabetic() {
            flush_run(&mut run, &mut t);
            word.push(c.to_ascii_lowercase());
        } else if matches!(c, '（' | '）' | '(' | ')') {
            // 括号里的字与外面连读：（河）岸 → 河岸
            flush_word(&mut word, &mut t);
        } else {
            flush_word(&mut word, &mut t);
            flush_run(&mut run, &mut t);
        }
    }
    flush_word(&mut word, &mut t);
    flush_run(&mut run, &mut t);
    t
}

fn overlap(a: &BTreeSet<String>, b: &BTreeSet<String>, min_shared: usize) -> f64 {
    let small = a.len().min(b.len());
    if small == 0 {
        return 0.0;
    }
    let shared = a.intersection(b).count();
    if shared < min_shared.min(small) {
        return 0.0;
    }
    shared as f64 / small as f64
}

/// 中文双字组重叠一个即可（译名短）；英文至少两个词干相同。
fn similarity(a: &Tokens, b: &Tokens) -> f64 {
    let en = if a.en.len().min(b.en.len()) >= 2 { overlap(&a.en, &b.en, 2) } else { 0.0 };
    overlap(&a.cjk, &b.cjk, 1).max(en)
}

fn sense_similarity(a: &(Sense, Tokens), b: &(Sense, Tokens)) -> f64 {
    let s = similarity(&a.1, &b.1);
    match (&a.0.pos, &b.0.pos) {
        (Some(x), Some(y)) if x != y => s * POS_PENALTY,
        _ => s,
    }
}

// —— 汇总 ——

fn trim_label(s: &str) -> String {
    if s.chars().count() <= LABEL_CHARS {
        return s.to_string();
    }
    let mut t: String = s.chars().take(LABEL_CHARS).collect();
    t.push('…');
    t
}

/// 一部词典的全部义项：优先结构化释义，没有时按编号行。
fn senses_of(entries: &[String], headword: &str) -> Vec<(String, Option<String>)> {
    let mut defs = Vec::new();
    let mut numbered = Vec::new();
    for html in entries {
        let ex = extract(html);
        defs.extend(ex.defs);
        let mut pos: Option<String> = None;
        for line in &ex.lines {
            if let Some(p) = pos_line(line) {
                pos = Some(p);
                continue;
            }
            // MW 的 `I. \ˈbaŋk\ noun`
            if let Some(p) = line.split_whitespace().last().and_then(normalize_pos) {
                if line.starts_with(['I', 'V', 'X']) && line.contains('.') {
                    pos = Some(p);
                    continue;
                }
            }
            if let Some(body) = strip_marker(line) {
                numbered.push((body.to_string(), pos.clone()));
            }
        }
    }
    let raw = if defs.is_empty() { numbered } else { defs };
    let mut seen = BTreeSet::new();
    raw.into_iter()
        .map(|(t, p)| (t.trim_matches(|c: char| c.is_whitespace() || c == ':' || c == ';').to_string(), p))
        .filter(|(t, _)| {
            let n = t.chars().count();
            (MIN_CHARS..=MAX_CHARS).contains(&n) && !t.eq_ignore_ascii_case(headword) && seen.insert(t.clone())
        })
        .take(MAX_SENSES_PER_DICT)
        .collect()
}

pub fn build(headword: &str, inputs: &[AtlasInput]) -> SenseAtlas {
    let mut dicts = Vec::new();
    let mut all: Vec<(Sense, Tokens)> = Vec::new();
    for input in inputs {
        let senses = senses_of(&input.entries, headword);
        if senses.is_empty() {
            continue;
        }
        let d = dicts.len();
        dicts.push(AtlasDict { id: input.id.clone(), name: input.name.clone(), senses: senses.len() });
        for (index, (text, pos)) in senses.into_iter().enumerate() {
            let tk = tokens(&text, headword);
            all.push((Sense { dict: d, index, domain: domain_of(&text), text, pos }, tk));
        }
    }

    // 贪心归组：(成员下标列表)
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in 0..all.len() {
        let d = all[i].0.dict;
        let best = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.iter().all(|&m| all[m].0.dict != d))
            .map(|(gi, g)| (gi, g.iter().map(|&m| sense_similarity(&all[i], &all[m])).fold(0.0, f64::max)))
            .filter(|(_, s)| *s >= THRESHOLD)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((gi, _)) => groups[gi].push(i),
            None => groups.push(vec![i]),
        }
    }

    let mut clusters: Vec<(usize, Cluster)> = groups
        .into_iter()
        .map(|g| {
            let first = g[0];
            let senses: Vec<Sense> = g.iter().map(|&m| all[m].0.clone()).collect();
            let mut ds: Vec<usize> = senses.iter().map(|s| s.dict).collect();
            ds.sort_unstable();
            ds.dedup();
            let poses: BTreeSet<&str> = senses.iter().filter_map(|s| s.pos.as_deref()).collect();
            let pos_diff = if poses.len() > 1 {
                senses.iter().filter_map(|s| s.pos.clone().map(|p| (s.dict, p))).collect()
            } else {
                Vec::new()
            };
            let domain = senses.iter().find_map(|s| s.domain.clone());
            (first, Cluster { label: trim_label(&senses[0].text), senses, dicts: ds, domain, pos_diff })
        })
        .collect();
    clusters.sort_by(|a, b| b.1.dicts.len().cmp(&a.1.dicts.len()).then(a.0.cmp(&b.0)));
    SenseAtlas { dicts, clusters: clusters.into_iter().map(|(_, c)| c).collect() }
}

#[cfg(test)]
mod tests {
    #[test]
    fn entities_next_to_multibyte_text_do_not_panic() {
        // 「&」后面是中文 / 带调号的拉丁字母：以前按字节切到字符中间会 panic
        assert_eq!(decode_entities("A&B 兰兰兰兰"), "A&B 兰兰兰兰");
        assert_eq!(decode_entities("&兰花兰花兰花"), "&兰花兰花兰花");
        assert_eq!(decode_entities("x &īīīīīī y"), "x &īīīīīī y");
        assert_eq!(decode_entities("a&nbsp;b &amp; 兰&#x4e2d;"), "a b & 兰中");
    }

    use super::*;

    fn input(id: &str, html: &str) -> AtlasInput {
        AtlasInput { id: id.into(), name: id.to_uppercase(), entries: vec![html.into()] }
    }

    #[test]
    fn extracts_structured_definitions_without_examples() {
        let html = r#"<style>.d{color:red}</style><span class="pos">n</span>
            <span class="n-g"><span class="z_n">1</span><span class="d">a place where money is kept <span class="chn">银行</span></span>
            <span class="x-g"><span class="x">She went to the bank.</span></span></span>
            <span class="n-g"><span class="d">the side of a river &amp; its slope <span class="chn">河岸</span></span></span>"#;
        let s = senses_of(&[html.to_string()], "bank");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].0, "a place where money is kept 银行");
        assert_eq!(s[0].1.as_deref(), Some("n."));
        assert_eq!(s[1].0, "the side of a river & its slope 河岸");
    }

    #[test]
    fn extracts_numbered_lines_and_pos_headers() {
        let html = "<b>bank</b><br><b>I.</b> \\ˈbaŋk\\ <i>noun</i><br>&nbsp;<b>1.</b> <b>:</b> the rising ground bordering a river<br>\
                    &nbsp;&nbsp;<b>a.</b> <b>:</b> a piled-up mass of cloud<br><b>II.</b><i>verb</i><br><b>1.</b> : to keep money in a bank<br>\
                    <p>[noun]</p><p><b>2</b> = <b>store, fund, reserve</b></p><p>① 河岸 ← 古诺尔斯语</p>";
        let s = senses_of(&[html.to_string()], "bank");
        let texts: Vec<&str> = s.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(
            texts,
            ["the rising ground bordering a river", "a piled-up mass of cloud", "to keep money in a bank", "store, fund, reserve", "河岸 ← 古诺尔斯语"]
        );
        let pos: Vec<Option<&str>> = s.iter().map(|x| x.1.as_deref()).collect();
        assert_eq!(pos, [Some("n."), Some("n."), Some("v."), Some("n."), Some("n.")]);
    }

    #[test]
    fn markers_and_domains() {
        assert_eq!(strip_marker("12. : text"), Some("text"));
        assert_eq!(strip_marker("b) text"), Some("text"));
        assert_eq!(strip_marker("③ 文本"), Some("文本"));
        assert_eq!(strip_marker("1 = save"), Some("save"));
        assert_eq!(strip_marker("1990 was a year"), None);
        assert_eq!(strip_marker("(2)  : motel"), Some("motel"));
        assert_eq!(strip_marker("(as of a hill) slope"), None);
        assert_eq!(pos_line("a."), None, "小项编号不是词性");
        assert_eq!(normalize_pos("n[C]").as_deref(), Some("n."));
        assert_eq!(normalize_pos("[transitive verb]").as_deref(), Some("v."));
        assert_eq!(strip_marker("bank"), None);
        assert_eq!(domain_of("【计】存储体"), Some("计".into()));
        assert_eq!(domain_of("(computing) a set of memory"), Some("computing".into()));
        assert_eq!(domain_of("(of a river) the side"), None);
        assert_eq!(domain_of("【尤英】拥有并使用"), None, "语体标签不算领域");
        assert_eq!(decode_entities("a&nbsp;b &#x4e2d;&#25991; &bogus; &"), "a b 中文 &bogus; &");
    }

    #[test]
    fn aligns_across_dictionaries() {
        let a = input(
            "a",
            r#"<span class="d">an institution that stores money and lends it <span class="chn">银行</span></span>
               <span class="d">the side of a river <span class="chn">河岸；河畔</span></span>
               <span class="d">(computing) a set of similar memory units <span class="chn">【计】存储体</span></span>"#,
        );
        let b = input(
            "b",
            r#"<div class="nataccep"><span class="alp_a">a)</span>a firm that stores and lends money <span class="chn">银行：</span></div>
               <div class="nataccep">land along the side of a river <span class="chn">（河）岸</span></div>"#,
        );
        let c = input("c", "<p>① 河岸 ← 古诺尔斯语</p><p>② 银行 ← 意大利语</p>");
        let empty = input("empty", "<p>only collocations here</p>");
        let atlas = build("bank", &[a, b, c, empty]);
        assert_eq!(atlas.dicts.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"], "抽不出义项的词典不计入");
        let sizes: Vec<usize> = atlas.clusters.iter().map(|c| c.dicts.len()).collect();
        assert_eq!(sizes, [3, 3, 1]);
        assert!(atlas.clusters[0].label.contains("money"));
        assert!(atlas.clusters[1].label.contains("river"));
        let unique = &atlas.clusters[2];
        assert_eq!((unique.dicts.as_slice(), unique.domain.as_deref()), (&[0usize][..], Some("计")), "【】标签优先");
    }

    #[test]
    fn reports_pos_differences() {
        let a = input("a", "<p>[noun]</p><p>1. 储蓄；存款</p>");
        // 跨词性只有几乎相同才归到一组
        let b = input("b", "<p>[verb]</p><p>1. 储蓄</p><p>2. 把钱存入银行</p>");
        let atlas = build("bank", &[a, b]);
        assert_eq!(atlas.clusters.len(), 2);
        assert_eq!(atlas.clusters[0].pos_diff, vec![(0, "n.".to_string()), (1, "v.".to_string())]);
    }

    /// 用真实词典样本调参：`LENS_ATLAS_DIR=<目录> cargo test atlas_samples -- --ignored --nocapture`
    /// 目录里每个 `<词>.json` 是 `[AtlasInput]`（商业词典内容，只留在本机，不进仓库）。
    #[test]
    #[ignore]
    fn atlas_samples() -> crate::error::Result<()> {
        let Ok(dir) = std::env::var("LENS_ATLAS_DIR") else { return Ok(()) };
        for e in std::fs::read_dir(dir)? {
            let path = e?.path();
            if path.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let word = path.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
            let inputs: Vec<AtlasInput> = serde_json::from_slice(&std::fs::read(&path)?)?;
            let a = build(&word, &inputs);
            let multi = a.clusters.iter().filter(|c| c.dicts.len() > 1).count();
            println!("
### {word}: {} dicts {:?}, {} clusters, {multi} shared", a.dicts.len(), a.dicts.iter().map(|d| d.senses).collect::<Vec<_>>(), a.clusters.len());
            for c in a.clusters.iter().filter(|c| c.dicts.len() > 1) {
                println!("  [{}]", c.dicts.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(","));
                for s in &c.senses {
                    println!("     {}{} {}", s.dict, s.pos.as_deref().map(|p| format!(" {p}")).unwrap_or_default(), s.text.chars().take(70).collect::<String>());
                }
            }
        }
        Ok(())
    }
}
