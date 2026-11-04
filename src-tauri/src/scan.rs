//! 文章生词扫描：把一篇英文切成词、还原成原形，按词频排名找出超过「词汇量」的词。
//!
//! - 排名用 COCA（导入过时）或内置词频表；两边都没有、WordNet 也不认识的（人名、拼写错误）不算。
//! - 句中大写开头的词当专有名词跳过；全大写的缩写跳过；缩约形式取前半（don't → do）。
//! - 已收藏的词标「已收藏」，用户标过「认识」的词标「认识」，其余超过词汇量的是生词。
//! - 每个词带上它第一次出现的那句话（收藏时当语境句）。偏移都是 UTF-16，前端直接用来高亮。

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::capture::sentence;

pub struct Lexicon<'a> {
    /// 原形的词频排名（1 = 最常用）
    pub rank: &'a dyn Fn(&str) -> Option<u32>,
    /// 是不是真词（WordNet 或词频表里有）
    pub is_word: &'a dyn Fn(&str) -> bool,
    /// 不规则变形 → 原形（went → go）
    pub irregular: &'a dyn Fn(&str) -> Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    New,
    Saved,
    Known,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanWord {
    pub lemma: String,
    /// 文中出现过的词形（小写，按出现顺序）
    pub forms: Vec<String>,
    pub count: u32,
    pub rank: Option<u32>,
    pub status: Status,
    /// 第一次出现的那句话与词在句中的 UTF-16 区间
    pub sentence: Option<String>,
    pub hl_start: usize,
    pub hl_end: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanToken {
    pub start: usize,
    pub end: usize,
    /// words 里的下标
    pub word: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub words: Vec<ScanWord>,
    pub tokens: Vec<ScanToken>,
    /// 计入的词次（不含专有名词、缩写、不认识的串）
    pub total: u32,
    /// 不同的原形数
    pub unique: u32,
    /// 认识的词次占比（生词以外的都算认识）
    pub coverage: f32,
}

/// 文本里的一个词：UTF-16 区间、原文
struct Raw<'t> {
    start16: usize,
    end16: usize,
    byte: usize,
    text: &'t str,
    /// 句首（前面是段首或句末标点）
    sentence_start: bool,
}

/// 连字符前的这些前缀不单独算词（non-essential、anti-inflation、re-elect）
const HYPHEN_PREFIXES: &[&str] = &["non", "anti", "pre", "post", "re", "co", "multi", "semi", "sub", "inter", "self", "ex", "pro", "mid", "neo", "pseudo", "quasi", "ultra"];

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’')
}

/// 切词：连续字母为一个词，中间可夹撇号（don't、O’Brien）；连字符当分隔
fn words(text: &str) -> Vec<Raw<'_>> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let (mut i, mut u16pos) = (0, 0usize);
    // 上一个非空白字符是不是句末标点（段首也算句首）
    let mut at_start = true;
    while i < chars.len() {
        let (b, c) = chars[i];
        if c.is_alphabetic() && c.is_ascii() {
            let (start_b, start16) = (b, u16pos);
            let mut j = i;
            let mut end16 = u16pos;
            while j < chars.len() {
                let ch = chars[j].1;
                let next_alpha = chars.get(j + 1).is_some_and(|(_, n)| n.is_ascii_alphabetic());
                if ch.is_ascii_alphabetic() || (is_apostrophe(ch) && next_alpha) {
                    end16 += ch.len_utf16();
                    j += 1;
                } else {
                    break;
                }
            }
            let end_b = chars.get(j).map_or(text.len(), |x| x.0);
            out.push(Raw { start16, end16, byte: start_b, text: &text[start_b..end_b], sentence_start: at_start });
            at_start = false;
            u16pos = end16;
            i = j;
            continue;
        }
        if matches!(c, '.' | '!' | '?' | '\n' | '。' | '！' | '？') {
            at_start = true;
        } else if !c.is_whitespace() && !matches!(c, '"' | '“' | '”' | '\'' | '‘' | '’' | '(' | ')' | '[' | ']' | '—' | '-' | '–' | '*' | '•') {
            at_start = false;
        }
        u16pos += c.len_utf16();
        i += 1;
    }
    out
}

/// 缩约形式取前半：don't → do，can't → can，won't → will，it's → it
fn strip_contraction(lower: &str) -> String {
    let Some(p) = lower.find(is_apostrophe) else { return lower.to_string() };
    let (head, tail) = (&lower[..p], &lower[p..]);
    let tail: String = tail.chars().skip(1).collect();
    if tail == "t" {
        return match head {
            "can" | "ca" => "can".into(),
            "won" | "wo" => "will".into(),
            "shan" | "sha" => "shall".into(),
            h => h.strip_suffix('n').unwrap_or(h).to_string(),
        };
    }
    head.to_string()
}

/// 原形：不规则变形表 → 词本身和按词尾规则得到的候选里，是真词且最常用的那个
/// （wages 的候选 wag、wage 都是词，取 wage；species 自己在词表里排名更靠前，就不拆）
pub fn lemma(lower: &str, lx: &Lexicon) -> String {
    if let Some(base) = (lx.irregular)(lower) {
        return base;
    }
    let options: Vec<String> = std::iter::once(lower.to_string())
        .chain(crate::batch::parse::suffix_candidates(lower).into_iter().filter(|c| c != lower))
        .filter(|c| (lx.is_word)(c))
        .collect();
    // 同样常用时取候选原形（COCA 查 eroded 得到的就是 erode 那一行）
    options.into_iter().min_by_key(|c| ((lx.rank)(c).unwrap_or(u32::MAX), c == lower)).unwrap_or_else(|| lower.to_string())
}

pub fn scan(text: &str, level: u32, lx: &Lexicon, saved: &HashSet<String>, known: &HashSet<String>) -> ScanResult {
    let mut words_out: Vec<ScanWord> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut tokens = Vec::new();
    let mut lemma_cache: HashMap<String, String> = HashMap::new();
    let (mut total, mut new_tokens) = (0u32, 0u32);
    let mut unique: HashSet<String> = HashSet::new();

    for w in words(text) {
        let first_upper = w.text.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        let all_upper = w.text.chars().filter(|c| c.is_ascii_alphabetic()).all(|c| c.is_ascii_uppercase());
        // 缩写（NASA、UK）；句中大写开头的专有名词（London、Biden）
        if (all_upper && w.text.len() >= 2) || (first_upper && !w.sentence_start) {
            continue;
        }
        let lower = strip_contraction(&w.text.to_lowercase());
        if lower.len() < 2 && lower != "a" && lower != "i" {
            continue;
        }
        if text[w.byte + w.text.len()..].starts_with('-') && HYPHEN_PREFIXES.contains(&lower.as_str()) {
            continue;
        }
        let lem = lemma_cache.entry(lower.clone()).or_insert_with(|| lemma(&lower, lx)).clone();
        let rank = (lx.rank)(&lem);
        if rank.is_none() && !(lx.is_word)(&lem) {
            continue;
        }
        total += 1;
        unique.insert(lem.clone());
        let status = if saved.contains(&lem) {
            Some(Status::Saved)
        } else if known.contains(&lem) {
            Some(Status::Known)
        } else if rank.is_none_or(|r| r > level) {
            Some(Status::New)
        } else {
            None
        };
        let Some(status) = status else { continue };
        if status == Status::New {
            new_tokens += 1;
        }
        let idx = match index.get(&lem) {
            Some(&i) => i,
            None => {
                let (sentence, hl_start, hl_end) = sentence_of(text, &w);
                words_out.push(ScanWord { lemma: lem.clone(), forms: Vec::new(), count: 0, rank, status, sentence, hl_start, hl_end });
                index.insert(lem.clone(), words_out.len() - 1);
                words_out.len() - 1
            }
        };
        let sw = &mut words_out[idx];
        sw.count += 1;
        if !sw.forms.contains(&lower) {
            sw.forms.push(lower);
        }
        tokens.push(ScanToken { start: w.start16, end: w.end16, word: idx });
    }
    let coverage = if total == 0 { 1.0 } else { 1.0 - new_tokens as f32 / total as f32 };
    ScanResult { words: words_out, tokens, total, unique: unique.len() as u32, coverage }
}

/// 词所在的那句话（在它所在的段落里断句）
fn sentence_of(text: &str, w: &Raw) -> (Option<String>, usize, usize) {
    let para_start = text[..w.byte].rfind('\n').map_or(0, |p| p + 1);
    let para_end = text[w.byte..].find('\n').map_or(text.len(), |p| w.byte + p);
    let para = &text[para_start..para_end];
    // 只数段落这一小段：每个生词都要算一次，从头数的话长文本会很慢
    let before16 = w.start16 - text[para_start..w.byte].encode_utf16().count();
    match sentence::extract(para, (w.start16 - before16, w.end16 - before16)) {
        Some(s) => (Some(s.sentence), s.hl_start, s.hl_end),
        None => (None, 0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexicon_ranks() -> HashMap<&'static str, u32> {
        [("the", 1), ("be", 2), ("government", 300), ("do", 20), ("not", 15), ("want", 80), ("to", 3), ("it", 10), ("a", 4),
         ("problem", 400), ("in", 6), ("tackle", 4000), ("ubiquitous", 11000), ("say", 30), ("on", 9), ("go", 25), ("i", 5), ("wage", 2500), ("wag", 12000), ("wait", 300), ("waite", 30000)]
            .into_iter()
            .collect()
    }

    fn run(text: &str, level: u32, saved: &[&str], known: &[&str]) -> ScanResult {
        let ranks = lexicon_ranks();
        let wn: HashSet<&str> = ["obstinate", "tackle", "ubiquitous", "government", "problem", "go"].into_iter().collect();
        let rank = |w: &str| ranks.get(w).copied();
        let is_word = |w: &str| ranks.contains_key(w) || wn.contains(w);
        let irregular = |w: &str| (w == "went").then(|| "go".to_string());
        let lx = Lexicon { rank: &rank, is_word: &is_word, irregular: &irregular };
        let s: HashSet<String> = saved.iter().map(|x| x.to_string()).collect();
        let k: HashSet<String> = known.iter().map(|x| x.to_string()).collect();
        scan(text, level, &lx, &s, &k)
    }

    #[test]
    fn finds_words_beyond_level_with_sentence() {
        let text = "Biden said the government tackled it.\nObstinate problems are ubiquitous; NASA didn't tackle them.";
        let r = run(text, 3000, &["ubiquitous"], &[]);
        let got: Vec<(&str, Status, u32)> = r.words.iter().map(|w| (w.lemma.as_str(), w.status, w.count)).collect();
        assert_eq!(got, vec![("tackle", Status::New, 2), ("obstinate", Status::New, 1), ("ubiquitous", Status::Saved, 1)]);
        let tackle = &r.words[0];
        assert_eq!(tackle.forms, vec!["tackled", "tackle"]);
        assert_eq!(tackle.sentence.as_deref(), Some("Biden said the government tackled it."));
        assert_eq!((tackle.hl_start, tackle.hl_end), (26, 33));
        // 句首大写的 Obstinate 照常算；句中的 Biden、全大写的 NASA 跳过；didn't → do
        let t = &r.tokens[1];
        assert_eq!(&text.encode_utf16().collect::<Vec<_>>()[t.start..t.end], "Obstinate".encode_utf16().collect::<Vec<_>>().as_slice());
        assert!(r.words.iter().all(|w| w.lemma != "biden" && w.lemma != "nasa"));
    }

    #[test]
    fn known_words_and_coverage() {
        let r = run("I went to tackle it.", 3000, &[], &["tackle"]);
        assert_eq!(r.words[0].status, Status::Known);
        assert_eq!(r.total, 5);
        assert!((r.coverage - 1.0).abs() < 1e-6, "标了认识的不算生词");
        let r = run("I went to tackle it.", 3000, &[], &[]);
        assert!((r.coverage - 0.8).abs() < 1e-6);
    }

    #[test]
    fn picks_the_most_common_base() {
        let r = run("Wages fell while we kept waiting.", 100, &[], &[]);
        let lemmas: Vec<&str> = r.words.iter().map(|w| w.lemma.as_str()).collect();
        assert!(lemmas.contains(&"wage") && lemmas.contains(&"wait"), "{lemmas:?}");
    }

    #[test]
    fn hyphen_prefixes_are_not_words() {
        let r = run("Turn off non-essential alerts.", 100, &[], &[]);
        assert!(r.words.iter().all(|w| w.lemma != "non"));
    }

    #[test]
    fn contractions() {
        assert_eq!(strip_contraction("don't"), "do");
        assert_eq!(strip_contraction("can’t"), "can");
        assert_eq!(strip_contraction("won't"), "will");
        assert_eq!(strip_contraction("government's"), "government");
    }
}
