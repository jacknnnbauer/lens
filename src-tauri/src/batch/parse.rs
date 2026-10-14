//! 批量导出的输入整理（SPEC 12.1）与文件名清洗（SPEC 12.3）。
//!
//! - 分隔符：换行 , ; ， ； 、 Tab；去首尾标点；短语保留整体
//! - 按原形去重（Ran → run）：先查常见不规则变化表，再按词尾规则还原，还原结果须是本地词典里的词头
//! - 非法条目（含路径字符、长度 > 64、纯数字）单列，不进任务；超出上限的单列

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::query::normalize_query;

const MAX_ITEM_CHARS: usize = 64;
const MAX_FILE_STEM: usize = 120;
const SEPARATORS: &[char] = &['\n', '\r', ',', ';', '，', '；', '、', '\t'];

/// 常见不规则变化（屈折形 → 原形）。只收高频词，查不到的交给词尾规则。
const IRREGULAR: &[(&str, &str)] = &[
    ("am", "be"), ("is", "be"), ("are", "be"), ("was", "be"), ("were", "be"), ("been", "be"), ("being", "be"),
    ("has", "have"), ("had", "have"), ("does", "do"), ("did", "do"), ("done", "do"),
    ("ran", "run"), ("went", "go"), ("gone", "go"), ("came", "come"), ("saw", "see"), ("seen", "see"),
    ("took", "take"), ("taken", "take"), ("gave", "give"), ("given", "give"), ("got", "get"), ("gotten", "get"),
    ("made", "make"), ("knew", "know"), ("known", "know"), ("thought", "think"), ("told", "tell"),
    ("said", "say"), ("found", "find"), ("left", "leave"), ("felt", "feel"), ("brought", "bring"),
    ("bought", "buy"), ("caught", "catch"), ("taught", "teach"), ("fought", "fight"), ("sought", "seek"),
    ("began", "begin"), ("begun", "begin"), ("broke", "break"), ("broken", "break"), ("chose", "choose"),
    ("chosen", "choose"), ("drew", "draw"), ("drawn", "draw"), ("drank", "drink"), ("drunk", "drink"),
    ("drove", "drive"), ("driven", "drive"), ("ate", "eat"), ("eaten", "eat"), ("fell", "fall"),
    ("fallen", "fall"), ("flew", "fly"), ("flown", "fly"), ("forgot", "forget"), ("forgotten", "forget"),
    ("froze", "freeze"), ("frozen", "freeze"), ("grew", "grow"), ("grown", "grow"), ("hid", "hide"),
    ("hidden", "hide"), ("held", "hold"), ("kept", "keep"), ("laid", "lay"), ("led", "lead"), ("lay", "lie"),
    ("lain", "lie"), ("lost", "lose"), ("meant", "mean"), ("met", "meet"), ("paid", "pay"), ("rode", "ride"),
    ("ridden", "ride"), ("rang", "ring"), ("rung", "ring"), ("rose", "rise"), ("risen", "rise"),
    ("sold", "sell"), ("sent", "send"), ("shook", "shake"), ("shaken", "shake"), ("shone", "shine"),
    ("shot", "shoot"), ("sang", "sing"), ("sung", "sing"), ("sank", "sink"), ("sunk", "sink"), ("sat", "sit"),
    ("slept", "sleep"), ("spoke", "speak"), ("spoken", "speak"), ("spent", "spend"), ("stood", "stand"),
    ("stole", "steal"), ("stolen", "steal"), ("struck", "strike"), ("swam", "swim"), ("swum", "swim"),
    ("threw", "throw"), ("thrown", "throw"), ("understood", "understand"), ("woke", "wake"),
    ("woken", "wake"), ("wore", "wear"), ("worn", "wear"), ("won", "win"), ("wrote", "write"),
    ("written", "write"), ("built", "build"), ("dealt", "deal"), ("dug", "dig"), ("fed", "feed"),
    ("fled", "flee"), ("hung", "hang"), ("heard", "hear"), ("lent", "lend"), ("lit", "light"),
    ("slid", "slide"), ("spun", "spin"), ("stuck", "stick"), ("stung", "sting"), ("swept", "sweep"),
    ("swung", "swing"), ("tore", "tear"), ("torn", "tear"), ("wept", "weep"), ("wound", "wind"),
    ("better", "good"), ("best", "good"), ("worse", "bad"), ("worst", "bad"),
    ("children", "child"), ("men", "man"), ("women", "woman"), ("feet", "foot"), ("teeth", "tooth"),
    ("geese", "goose"), ("mice", "mouse"), ("people", "person"), ("lives", "life"), ("knives", "knife"),
    ("wives", "wife"), ("leaves", "leaf"), ("halves", "half"), ("wolves", "wolf"), ("analyses", "analysis"),
    ("crises", "crisis"), ("phenomena", "phenomenon"), ("criteria", "criterion"), ("data", "datum"),
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invalid {
    pub text: String,
    pub reason: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Merged {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseOutput {
    /// 实际进入任务的词（原形、去重后，保持首次出现的顺序）
    pub words: Vec<String>,
    /// 识别到的条目数（去重前）
    pub total: usize,
    /// 因重复或同原形被合并的条目
    pub merged: Vec<Merged>,
    pub invalid: Vec<Invalid>,
    /// 超出单批上限而没有进入任务的词
    pub over_limit: Vec<String>,
}

fn invalid_reason(item: &str) -> Option<&'static str> {
    if item.chars().count() > MAX_ITEM_CHARS {
        return Some("超过 64 个字符");
    }
    if item.chars().any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()) {
        return Some("含路径或特殊字符");
    }
    if item.chars().all(|c| c.is_ascii_digit() || c.is_whitespace() || c == '.') {
        return Some("纯数字");
    }
    None
}

/// 单词的原形：不规则表 → 原样存在 → 词尾规则（还原结果须存在）。短语原样返回。
pub fn lemmatize(word: &str, exists: &dyn Fn(&str) -> bool) -> String {
    if word.contains(' ') {
        return word.to_string();
    }
    let lower = word.to_lowercase();
    if let Some((_, lemma)) = IRREGULAR.iter().find(|(form, _)| *form == lower) {
        return (*lemma).to_string();
    }
    if exists(word) {
        return word.to_string();
    }
    let w = lower.as_str();
    let mut candidates: Vec<String> = Vec::new();
    let strip = |suffix: &str| w.strip_suffix(suffix).filter(|s| s.chars().count() >= 2).map(str::to_string);
    if let Some(s) = strip("ies") {
        candidates.push(format!("{s}y"));
    }
    for suffix in ["es", "s"] {
        if let Some(s) = strip(suffix) {
            candidates.push(s);
        }
    }
    if let Some(s) = strip("ied") {
        candidates.push(format!("{s}y"));
    }
    for suffix in ["ed", "ing", "er", "est"] {
        if let Some(s) = strip(suffix) {
            candidates.push(format!("{s}e"));
            candidates.push(s.clone());
            // 双写辅音：stopped → stop
            let chars: Vec<char> = s.chars().collect();
            if chars.len() >= 3 && chars[chars.len() - 1] == chars[chars.len() - 2] {
                candidates.push(chars[..chars.len() - 1].iter().collect());
            }
        }
    }
    candidates.into_iter().find(|c| exists(c)).unwrap_or_else(|| word.to_string())
}

/// 解析粘贴的词表（也用于整理从历史、生词本选出的词）。
/// 列表符号：从 COCA 搭配表、笔记里粘过来的词常带 `event•`、`• apple`、`1. apple`、`- apple`
const BULLETS: &[char] = &['•', '·', '●', '○', '◦', '▪', '■', '□', '◆', '◇', '★', '☆', '►', '▶', '→', '*', '-', '–', '—'];

fn strip_list_marks(raw: &str) -> &str {
    let s = raw.trim().trim_matches(|c: char| BULLETS.contains(&c) || c.is_whitespace());
    // 行首编号 `12.` `3)` `4、`
    let digits = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if digits > 0 && digits < s.len() {
        if let Some(rest) = s[digits..].strip_prefix(['.', ')', '、', '．']) {
            return rest.trim();
        }
    }
    s
}

pub fn parse_list(text: &str, max_words: usize, exists: &dyn Fn(&str) -> bool) -> ParseOutput {
    let mut out = ParseOutput::default();
    // 原形（小写）→ 保留下来的写法
    let mut seen: HashMap<String, String> = HashMap::new();
    for raw in text.split(SEPARATORS) {
        let item = normalize_query(strip_list_marks(raw));
        if item.is_empty() {
            continue;
        }
        out.total += 1;
        if let Some(reason) = invalid_reason(&item) {
            out.invalid.push(Invalid { text: item, reason });
            continue;
        }
        let lemma = lemmatize(&item, exists);
        let key = lemma.to_lowercase();
        if let Some(kept) = seen.get(&key) {
            out.merged.push(Merged { from: item, to: kept.clone() });
            continue;
        }
        seen.insert(key, lemma.clone());
        if lemma.to_lowercase() != item.to_lowercase() {
            out.merged.push(Merged { from: item, to: lemma.clone() });
        }
        if out.words.len() >= max_words {
            out.over_limit.push(lemma);
        } else {
            out.words.push(lemma);
        }
    }
    out
}

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Windows 安全的文件名主干（不含扩展名）：去变音（résumé → resume）、去非法字符与控制字符、
/// 去首尾空格和点、避开保留名、长度 ≤ 120。空结果返回 `word`。
pub fn safe_file_stem(word: &str) -> String {
    let ascii_folded: String = word.nfkd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect();
    let cleaned: String = ascii_folded
        .chars()
        .map(|c| if matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() { '_' } else { c })
        .collect();
    let mut stem: String = cleaned.trim_matches(|c: char| c == ' ' || c == '.').chars().take(MAX_FILE_STEM).collect();
    stem = stem.trim_end_matches([' ', '.']).to_string();
    if stem.is_empty() {
        return "word".to_string();
    }
    let base = stem.split('.').next().unwrap_or_default().to_ascii_uppercase();
    if RESERVED.contains(&base.as_str()) {
        stem.push('_');
    }
    stem
}

/// 在已用名字之外取一个不冲突的主干：重名加 `-2`、`-3`（不区分大小写）。
pub fn unique_stem(stem: &str, used: &mut HashSet<String>) -> String {
    let mut candidate = stem.to_string();
    let mut n = 2;
    while used.contains(&candidate.to_lowercase()) {
        candidate = format!("{stem}-{n}");
        n += 1;
    }
    used.insert(candidate.to_lowercase());
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(word: &str) -> bool {
        ["run", "stop", "study", "box", "make", "colour", "take into account", "ubiquitous", "Paris"].iter().any(|w| w.eq_ignore_ascii_case(word))
    }

    #[test]
    fn strips_bullets_and_numbering() {
        let out = parse_list("event•\n• exhibition\n12. apple\n- pear\n3) plum\n1990", 100, &|_| true);
        assert_eq!(out.words, ["event", "exhibition", "apple", "pear", "plum"]);
        assert_eq!(out.invalid.len(), 1, "纯数字仍然算无效");
    }

    #[test]
    fn splits_on_all_separators_and_keeps_phrases() {
        let out = parse_list("ubiquitous\nephemeral, pervasive；serendipity、x\ttake into account", 100, &dict);
        assert_eq!(out.words, ["ubiquitous", "ephemeral", "pervasive", "serendipity", "x", "take into account"]);
        assert_eq!(out.total, 6);
    }

    #[test]
    fn dedupes_by_lemma_and_case() {
        let out = parse_list("run\nRan\nRUN\nstopped\nstudies\nboxes\n“colour,”", 100, &dict);
        assert_eq!(out.words, ["run", "stop", "study", "box", "colour"]);
        assert!(out.merged.contains(&Merged { from: "Ran".into(), to: "run".into() }));
        assert!(out.merged.contains(&Merged { from: "stopped".into(), to: "stop".into() }));
        assert_eq!(out.merged.iter().filter(|m| m.to == "run").count(), 2);
    }

    #[test]
    fn unknown_words_are_kept_as_is() {
        assert_eq!(lemmatize("blorbing", &dict), "blorbing");
        assert_eq!(lemmatize("Paris", &dict), "Paris");
    }

    #[test]
    fn invalid_and_over_limit() {
        let long = "a".repeat(65);
        let out = parse_list(&format!("C:\\temp?\n12345\n{long}\nfine\nok\nthird"), 2, &dict);
        assert_eq!(out.words, ["fine", "ok"]);
        assert_eq!(out.over_limit, ["third"]);
        assert_eq!(out.invalid.iter().map(|i| i.reason).collect::<Vec<_>>(), ["含路径或特殊字符", "纯数字", "超过 64 个字符"]);
    }

    #[test]
    fn file_stems() {
        assert_eq!(safe_file_stem("résumé"), "resume");
        assert_eq!(safe_file_stem(" ..con.. "), "con_");
        assert_eq!(safe_file_stem("a/b?c"), "a_b_c");
        assert_eq!(safe_file_stem("COM1.txt"), "COM1.txt_");
        assert_eq!(safe_file_stem("..."), "word");
        assert_eq!(safe_file_stem(&"x".repeat(200)).len(), 120);
        assert_eq!(safe_file_stem("take into account"), "take into account");
    }

    #[test]
    fn unique_stems_case_insensitive() {
        let mut used = HashSet::new();
        assert_eq!(unique_stem("Run", &mut used), "Run");
        assert_eq!(unique_stem("run", &mut used), "run-2");
        assert_eq!(unique_stem("run", &mut used), "run-3");
    }
}
