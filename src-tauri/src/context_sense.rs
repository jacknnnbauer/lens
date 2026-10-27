//! 语境释义（本地）：给一句话和其中的一个词，从各词典的义项里挑出这句话里最可能的那个，并说明依据。
//!
//! 不靠模型，三步：
//! 1. 候选：义项图谱（senses::build）已经把各词典的义项对齐成组，每组就是一个候选意思；
//! 2. 词性：看词尾和前后词（限定词、助动词、to、紧跟的宾语…）判断句中词性，排除词性不符的组；
//! 3. 搭配：句中实词（连同 COCA 近义词）与各组释义的词面重叠，加上 COCA 搭配表里是否有这个搭配。
//!
//! 只是启发式：词性和搭配都有把握时标「有把握」，否则标「初步判断」。

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::coca::Coca;
use crate::wordnet::WordNet;
use crate::senses::{self, AtlasInput, Cluster, SenseAtlas};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextInput {
    /// 词典里的词头（原形），如 address
    pub word: String,
    pub sentence: String,
    /// 所查的词在句中的 UTF-16 区间（与语境句的高亮一致）
    pub hl_start: usize,
    pub hl_end: usize,
    pub dicts: Vec<AtlasInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// 代表释义（义项图谱的组标签）
    pub label: String,
    pub pos: Option<String>,
    /// 这个意思出现在哪些词典（名称）
    pub dicts: Vec<String>,
    /// 第一本词典里的义项序号（从 1 起），界面上写「牛津 · 义项 2」
    pub first_dict: String,
    pub sense_no: usize,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSense {
    /// 句中原形（surface），如 addressed
    pub surface: String,
    /// 判断出的词性：n. / v. / adj. / adv.；判断不了为 None
    pub pos: Option<String>,
    pub best: Option<Candidate>,
    /// 备选（第 2、3 名）
    pub others: Vec<Candidate>,
    /// 判断依据，一条一句
    pub reasons: Vec<String>,
    /// 为什么不是其他意思（被排除的组的标签与原因）
    pub ruled_out: Vec<String>,
    /// 词性与搭配都支持、且领先明显
    pub confident: bool,
}

/// 以 -ly 结尾但不是副词的常见词（名词、形容词、动词）
const LY_NOT_ADVERB: &[&str] = &[
    "family", "supply", "reply", "apply", "ally", "rally", "belly", "jelly", "bully", "fly", "july", "italy", "assembly",
    "anomaly", "monopoly", "melancholy", "holy", "ugly", "friendly", "lovely", "likely", "unlikely", "early", "daily",
    "weekly", "monthly", "yearly", "only", "silly", "elderly", "costly", "lonely", "lively", "deadly", "orderly", "worldly",
    "timely", "manly", "womanly", "cowardly", "scholarly", "heavenly", "lowly", "comely", "curly", "burly", "surly", "oily",
    "chilly", "hilly", "jolly", "woolly", "smelly", "bubbly", "wobbly", "prickly", "ghostly", "kindly", "sickly", "homely",
];

const DETERMINERS: &[&str] = &[
    "the", "a", "an", "this", "that", "these", "those", "my", "your", "his", "her", "its", "our", "their", "some", "any", "no",
    "every", "each", "another", "such", "what", "which", "whose",
];
const MODALS: &[&str] = &[
    "will", "would", "can", "could", "shall", "should", "may", "might", "must", "do", "does", "did", "don't", "doesn't", "didn't",
    "won't", "can't", "cannot", "couldn't", "wouldn't", "shouldn't",
];
const HAVE: &[&str] = &["has", "have", "had", "having", "'ve", "'s", "'d"];
const BE: &[&str] = &["is", "are", "was", "were", "be", "been", "being", "am", "'re", "'m", "isn't", "aren't", "wasn't", "weren't"];
const SUBJECTS: &[&str] = &["i", "you", "we", "they", "he", "she", "it", "who", "which", "that"];
const OBJECT_STARTS: &[&str] = &[
    "the", "a", "an", "this", "that", "these", "those", "my", "your", "his", "her", "its", "our", "their", "them", "him", "me",
    "us", "it", "some", "any", "every", "each", "all",
];
const PREPOSITIONS: &[&str] = &[
    "of", "in", "on", "at", "for", "with", "by", "from", "about", "into", "over", "under", "between", "through", "during",
    "without", "within", "against", "among",
];
const DEGREE: &[&str] = &["very", "so", "too", "quite", "rather", "more", "most", "less", "least", "really", "extremely"];
/// 句中这些词不算「实词」，不参与搭配比对
const STOP: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "of", "in", "on", "at", "to", "for", "with", "by", "from", "about", "as", "is", "are",
    "was", "were", "be", "been", "being", "has", "have", "had", "do", "does", "did", "will", "would", "can", "could", "shall",
    "should", "may", "might", "must", "this", "that", "these", "those", "it", "its", "they", "them", "their", "he", "him", "his",
    "she", "her", "we", "us", "our", "you", "your", "i", "me", "my", "not", "no", "so", "if", "then", "than", "there", "here",
    "what", "which", "who", "whom", "when", "where", "why", "how", "all", "any", "some", "very", "just", "also", "into", "out",
    "up", "down", "over", "more", "most", "said", "says",
];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '-' || c == '’'
}

/// 句子按词切开：(小写词, 在句中的 UTF-16 起点)
fn words_of(sentence: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut start = 0usize;
    let mut pos16 = 0usize;
    for c in sentence.chars() {
        if is_word_char(c) {
            if cur.is_empty() {
                start = pos16;
            }
            cur.push(if c == '’' { '\'' } else { c });
        } else if !cur.is_empty() {
            out.push((cur.to_lowercase(), start));
            cur.clear();
        }
        pos16 += c.len_utf16();
    }
    if !cur.is_empty() {
        out.push((cur.to_lowercase(), start));
    }
    out
}

/// 句中词性：返回 (词性, 理由)
fn guess_pos(words: &[(String, usize)], at: usize, headword: &str) -> (Option<&'static str>, String) {
    let w = words[at].0.as_str();
    let prev = at.checked_sub(1).map(|i| words[i].0.as_str());
    let next = words.get(at + 1).map(|x| x.0.as_str());
    let inflected_ed = w != headword && w.ends_with("ed");
    let inflected_ing = w != headword && w.ends_with("ing");
    let in_list = |list: &[&str], x: Option<&str>| x.is_some_and(|x| list.contains(&x));

    if w.len() > 4 && w.ends_with("ly") && !LY_NOT_ADVERB.contains(&w) {
        return (Some("adv."), format!("{w} 以 -ly 结尾，是副词"));
    }
    if prev == Some("to") && w == headword && in_list(OBJECT_STARTS, next) {
        return (Some("v."), format!("to {w} 后面接 {}，是动词不定式", next.unwrap_or_default()));
    }
    if in_list(MODALS, prev) {
        return (Some("v."), format!("前面是 {}，后面跟动词原形", prev.unwrap_or_default()));
    }
    if in_list(HAVE, prev) && (inflected_ed || w.ends_with("en")) {
        return (Some("v."), format!("{} {w} 是完成时，{w} 是动词", prev.unwrap_or_default()));
    }
    if in_list(BE, prev) && (inflected_ing || inflected_ed) {
        let what = if inflected_ing { "进行时" } else { "被动语态" };
        return (Some("v."), format!("{} {w} 是{what}，{w} 是动词", prev.unwrap_or_default()));
    }
    if in_list(DEGREE, prev) {
        return (Some("adj."), format!("前面是程度词 {}，多半是形容词", prev.unwrap_or_default()));
    }
    // 限定词 + 这个词 + 名词：a fair decision、the light box → 形容词（定语）
    let next_is_noun = next.is_some_and(|n| {
        n.len() > 2 && !STOP.contains(&n) && !n.ends_with("ed") && !n.ends_with("ing") && !n.ends_with("ly") && !n.ends_with('s')
    });
    if in_list(DETERMINERS, prev) && next_is_noun && !inflected_ed && !inflected_ing {
        return (Some("adj."), format!("夹在 {} 和 {} 之间，是形容词（修饰 {}）", prev.unwrap_or_default(), next.unwrap_or_default(), next.unwrap_or_default()));
    }
    if in_list(DETERMINERS, prev) {
        return (Some("n."), format!("前面是限定词 {}，是名词", prev.unwrap_or_default()));
    }
    // 所有格后面是名词：the speaker's address
    if prev.is_some_and(|p| p.len() > 2 && (p.ends_with("'s") || p.ends_with("s'"))) {
        return (Some("n."), format!("前面是所有格 {}，是名词", prev.unwrap_or_default()));
    }
    if in_list(PREPOSITIONS, prev) && !inflected_ing {
        return (Some("n."), format!("前面是介词 {}，是名词", prev.unwrap_or_default()));
    }
    if (inflected_ed || w == format!("{headword}s") || w == format!("{headword}es")) && in_list(OBJECT_STARTS, next) {
        return (Some("v."), format!("{w} 后面紧跟宾语（{} …），是动词", next.unwrap_or_default()));
    }
    if inflected_ed && !in_list(DETERMINERS, prev) && at > 0 {
        return (Some("v."), format!("{w} 是 -ed 形式、前面是主语，是动词过去式"));
    }
    // 原形后面紧跟宾语、前面不是限定词或介词：祈使句或动词（Please light the candle.）
    if in_list(OBJECT_STARTS, next) && !in_list(DETERMINERS, prev) && !in_list(PREPOSITIONS, prev) && !in_list(BE, prev) {
        return (Some("v."), format!("{w} 后面紧跟宾语（{} …），是动词", next.unwrap_or_default()));
    }
    if in_list(SUBJECTS, prev) {
        return (Some("v."), format!("前面是主语 {}，是动词", prev.unwrap_or_default()));
    }
    if in_list(BE, prev) {
        return (Some("adj."), format!("跟在 {} 后面作表语，多半是形容词或名词", prev.unwrap_or_default()));
    }
    (None, String::new())
}

/// 义项图谱里每一组都标了词性、且全是同一个时返回它（有没标词性的组就不下结论：table 的动词义项可能恰好没标）
fn single_pos(atlas: &SenseAtlas) -> Option<&'static str> {
    let mut seen: Option<&'static str> = None;
    for c in &atlas.clusters {
        let p = norm_pos(&cluster_pos(c)?)?;
        match seen {
            None => seen = Some(p),
            Some(q) if q == p => {}
            Some(_) => return None,
        }
    }
    seen
}

/// COCA 里这个词只有一种（我们关心的）词性
fn coca_single_pos(coca: &Coca, headword: &str) -> Option<&'static str> {
    let poses: BTreeSet<&'static str> = coca.lookup(headword).iter().filter_map(|e| coca_pos(&e.pos)).collect();
    if poses.len() == 1 {
        poses.into_iter().next()
    } else {
        None
    }
}

fn norm_pos(p: &str) -> Option<&'static str> {
    match p {
        "n." => Some("n."),
        "v." => Some("v."),
        "adj." => Some("adj."),
        "adv." => Some("adv."),
        _ => None,
    }
}

/// COCA 的单字母词性 → n. / v. / adj. / adv.
fn coca_pos(p: &str) -> Option<&'static str> {
    match p {
        "n" => Some("n."),
        "v" => Some("v."),
        "j" => Some("adj."),
        "r" => Some("adv."),
        _ => None,
    }
}

/// 一组义项的词性：成员里最多的那个
fn cluster_pos(c: &Cluster) -> Option<String> {
    let mut count: HashMap<&str, usize> = HashMap::new();
    for s in &c.senses {
        if let Some(p) = s.pos.as_deref() {
            *count.entry(p).or_default() += 1;
        }
    }
    count.into_iter().max_by_key(|(_, n)| *n).map(|(p, _)| p.to_string())
}

/// 搭配表里的词（去掉 • 标记），小写
fn collocates_of(coca: Option<&Coca>, headword: &str, pos: Option<&str>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(coca) = coca else { return out };
    for e in coca.lookup(headword) {
        if pos.is_some() && coca_pos(&e.pos) != pos {
            continue;
        }
        for list in e.collocates.values() {
            for c in list.split(',') {
                let c = c.trim().trim_matches('•').trim().to_lowercase();
                if !c.is_empty() && !c.contains(' ') {
                    out.insert(c);
                }
            }
        }
    }
    out
}

/// coca、wordnet 都可以没有：COCA 给近义词和搭配，WordNet（内置）在没有 COCA 时补上近义词和词性
pub fn analyze(input: &ContextInput, coca: Option<&Coca>, wordnet: Option<&WordNet>) -> ContextSense {
    let headword = input.word.trim().to_lowercase();
    let words = words_of(&input.sentence);
    let at = words.iter().position(|(_, s)| *s >= input.hl_start && *s < input.hl_end.max(input.hl_start + 1)).or_else(|| {
        words.iter().position(|(w, _)| w.starts_with(&headword) || headword.starts_with(w.as_str()))
    });
    let surface = at.map(|i| words[i].0.clone()).unwrap_or_else(|| headword.clone());
    // 句首、后面紧跟逗号：Significantly, the vote …
    let at_sentence_start = at == Some(0) && {
        let end16 = at.and_then(|i| words.get(i)).map_or(0, |(w, s)| s + w.encode_utf16().count());
        input.sentence.encode_utf16().skip(end16).take(2).any(|u| u == u16::from(b','))
    };
    // 同义词词典、搭配、词频类词典只列词，不是释义，不当候选
    let defining: Vec<AtlasInput> = input.dicts.iter().filter(|d| !crate::commands::lookup::is_reference_dict(&d.name)).cloned().collect();
    let atlas: SenseAtlas = senses::build(&headword, if defining.is_empty() { &input.dicts } else { &defining });

    let (guessed, guess_reason) = match at {
        Some(i) => guess_pos(&words, i, &headword),
        None => (None, String::new()),
    };
    // 这个词只有一种词性（词典义项全是副词，或 COCA 里只作副词）：直接用，句中位置的猜测让位
    let only_pos = single_pos(&atlas)
        .or_else(|| coca.and_then(|c| coca_single_pos(c, &headword)))
        .or_else(|| wordnet.and_then(|wn| wn.single_pos(&headword)));
    // 句中是变形（tabled、running）时，词形本身就是证据，不让「只有一种词性」盖过它（table 在 COCA 里只列了名词）
    let inflected = surface != headword && !surface.ends_with("ly");
    let (pos, pos_reason) = match (only_pos, guessed) {
        (Some(only), Some(g)) if only == g => (Some(g), guess_reason),
        (Some(_), Some(g)) if inflected => (Some(g), guess_reason),
        (Some(only), _) => (Some(only), format!("{surface} 在词典里只有{}的用法", pos_name(only))),
        (None, g) => (g, guess_reason),
    };

    // 句中实词：紧跟在后面的（宾语、介词宾语）最能说明意思，前面的主语次之，远处的更弱
    let mut context: Vec<(String, f64)> = Vec::new();
    for (i, (w, _)) in words.iter().enumerate() {
        if Some(i) == at || STOP.contains(&w.as_str()) || w.len() < 3 || w.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let weight = match at {
            Some(a) if i > a && i - a <= 4 => 1.0,
            // 动词前面的主语很少决定动词的意思（company addressed …），权重压低
            Some(a) if i < a && a - i <= 3 => if pos == Some("v.") { 0.3 } else { 0.5 },
            _ => 0.4,
        };
        context.push((w.clone(), weight));
    }
    // 与目标词同词性的搭配组：目标是动词时看「常和 issue 一起用的动词」（address, deal, resolve…）
    let group = match pos {
        Some("v.") => Some("verb"),
        Some("n.") => Some("noun"),
        Some("adj.") => Some("adj"),
        Some("adv.") => Some("adv"),
        _ => None,
    };
    // 句中实词的 COCA 近义词（issue → matter, question…）和常见搭配词（issue → address, deal, resolve…）
    let mut synonyms: HashMap<String, (String, f64)> = HashMap::new();
    let mut partners: HashMap<String, (String, f64)> = HashMap::new();
    if let Some(coca) = coca {
        for (w, weight) in &context {
            let entries = coca.lookup(w);
            let has_noun = entries.iter().any(|e| e.pos == "n");
            for e in entries {
                let syn_ok = !has_noun || e.pos == "n";
                for group in e.synonyms.iter().filter(|_| syn_ok) {
                    for s in group.split(',').map(|x| x.trim().to_lowercase()) {
                        if !s.is_empty() && !s.contains(' ') && s != headword {
                            synonyms.entry(senses::stem(&s)).or_insert((format!("{w}\u{1}{s}"), weight * 0.7));
                        }
                    }
                }
                for (g, list) in &e.collocates {
                    // 只看与目标词同词性的那组；词性判断不了时各组都看，但打折
                    let factor = match group {
                        Some(want) if g == want => 1.0,
                        Some(_) => continue,
                        None => 0.5,
                    };
                    for c in list.split(',').map(|x| x.trim().trim_matches('•').trim().to_lowercase()) {
                        if !c.is_empty() && !c.contains(' ') && c != headword {
                            partners.entry(senses::stem(&c)).or_insert((format!("{w}\u{1}{c}"), weight * factor));
                        }
                    }
                }
            }
        }
    } else if let Some(wn) = wordnet {
        // 没有 COCA：句中实词的近义词从 WordNet 取（能当名词的只看名词义项，和上面一样）
        for (w, weight) in &context {
            let pos = if wn.poses(w) & crate::wordnet::NOUN != 0 { crate::wordnet::NOUN } else { 0 };
            for s in wn.synonyms(w, pos, 12) {
                if s != headword {
                    synonyms.entry(senses::stem(&s)).or_insert((format!("{w}\u{1}{s}"), weight * 0.7));
                }
            }
        }
    }
    let colloc = collocates_of(coca, &headword, pos);

    let mut scored: Vec<(f64, usize, Vec<String>)> = Vec::new();
    for (ci, c) in atlas.clusters.iter().enumerate() {
        let text: String = c.senses.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        let tk = senses::tokens(&text, &headword);
        let cpos = cluster_pos(c);
        let mut score = 0.0;
        let mut why = Vec::new();
        match (pos, cpos.as_deref()) {
            (Some(p), Some(cp)) if p == cp => score += 3.0,
            (Some(_), Some(_)) => score -= 4.0,
            _ => {}
        }
        for (w, weight) in &context {
            if tk.en.contains(&senses::stem(w)) {
                score += 2.0 * weight;
                why.push((weight * 2.0, format!("句中的 {w} 正好出现在这个释义里")));
            }
        }
        // 键里存「句中词\u{1}原词」，说明里显示原词而不是词干
        let split = |k: &str| -> (String, String) {
            let mut it = k.splitn(2, '\u{1}');
            (it.next().unwrap_or_default().to_string(), it.next().unwrap_or_default().to_string())
        };
        for t in &tk.en {
            if let Some((key, weight)) = synonyms.get(t) {
                let (from, word) = split(key);
                score += 1.5 * weight;
                why.push((1.5 * weight, format!("句中的 {from} 与释义里的 {word} 意思相近（COCA 近义词）")));
            } else if let Some((key, weight)) = partners.get(t) {
                let (from, word) = split(key);
                score += 1.2 * weight;
                why.push((1.2 * weight, format!("{from} 常和 {word} 连用（COCA 搭配），这个释义说的正是 {word}")));
            }
        }
        // 习语、短语：释义开头就带着这个词（be makin' bank、address yourself to…）——单独查一个词时多半不是它
        if c.senses.iter().all(|s| is_phrase_sense(&s.text, &headword)) {
            score -= 2.5;
        }
        // 「it is significant that…」这类句子副词的义项：只有在句首、后面跟逗号时才是它（Significantly, …）
        let sentence_adverb = c.senses.iter().all(|s| {
            let t = s.text.trim_start().to_lowercase();
            t.starts_with("it is ") || t.starts_with("it was ") || t.starts_with("used to say") || t.starts_with("used for saying")
        });
        if sentence_adverb && !at_sentence_start {
            score -= 2.0;
        }
        // 共识：几本词典都有这个意思；先列的义项更常用
        score += (0.15 * c.dicts.len() as f64).min(0.6);
        let first_index = c.senses.iter().map(|s| s.index).min().unwrap_or(0);
        score -= 0.05 * first_index as f64;
        // 依据按分量排，最有力的在前
        why.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut why: Vec<String> = why.into_iter().map(|(_, t)| t).collect();
        why.dedup();
        scored.push((score, ci, why));
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));

    let candidate = |ci: usize, score: f64| -> Candidate {
        let c = &atlas.clusters[ci];
        let first = c.senses.iter().min_by_key(|s| s.dict).cloned();
        // 显示组里最完整、最干净的那条释义（有的词典只写一个词，如 candle；
        // vocabulary.com 的正文带着「Definitions of …」「Primary Meanings of …」这类标题，去掉）
        let label = c
            .senses
            .iter()
            .map(|s| clean_label(&s.text))
            .filter(|t| !t.is_empty())
            .max_by_key(|t| (!t.contains("Antonyms") && !t.contains("Synonyms"), t.chars().count().min(120)))
            .map(|t| if t.chars().count() > 140 { format!("{}…", t.chars().take(140).collect::<String>()) } else { t })
            .unwrap_or_else(|| c.label.clone());
        Candidate {
            label,
            pos: cluster_pos(c),
            dicts: c.dicts.iter().filter_map(|&d| atlas.dicts.get(d).map(|x| x.name.clone())).collect(),
            first_dict: first.as_ref().and_then(|s| atlas.dicts.get(s.dict)).map(|d| d.name.clone()).unwrap_or_default(),
            sense_no: first.map_or(1, |s| s.index + 1),
            score,
        }
    };

    let mut reasons = Vec::new();
    if !pos_reason.is_empty() {
        reasons.push(pos_reason);
    }
    // 句中实词出现在 COCA 搭配表里：address + issue
    let hits: Vec<&str> = context.iter().map(|(w, _)| w.as_str()).filter(|w| colloc.contains(*w) || colloc.contains(&senses::stem(w))).collect();
    if let Some(h) = hits.first() {
        reasons.push(format!("{headword} + {h} 是 COCA 里的常见搭配"));
    }
    let best = scored.first().map(|(s, ci, why)| {
        reasons.extend(why.iter().take(3).cloned());
        candidate(*ci, *s)
    });
    let others: Vec<Candidate> = scored.iter().skip(1).take(2).map(|(s, ci, _)| candidate(*ci, *s)).collect();

    let mut ruled_out = Vec::new();
    if let Some(p) = pos {
        let mut seen = BTreeSet::new();
        for c in &atlas.clusters {
            if let Some(cp) = cluster_pos(c) {
                if cp != p && seen.insert(cp.clone()) && ruled_out.len() < 3 {
                    ruled_out.push(format!("不是{}的意思（如「{}」）：句中是{}", pos_name(&cp), short(&c.label), pos_name(p)));
                }
            }
        }
    }

    let margin = match (scored.first(), scored.get(1)) {
        (Some(a), Some(b)) => a.0 - b.0,
        (Some(_), None) => 9.0,
        _ => 0.0,
    };
    let evidence = scored.first().is_some_and(|(_, _, why)| !why.is_empty()) || !hits.is_empty();
    let confident = pos.is_some() && evidence && margin >= 1.0;
    ContextSense { surface, pos: pos.map(str::to_string), best, others, reasons, ruled_out, confident }
}

/// 去掉释义开头的「Definitions of X」「Primary Meanings of X」和紧跟的序号、词性缩写
fn clean_label(text: &str) -> String {
    let mut t = text.trim();
    for head in ["Definitions of ", "Primary Meanings of ", "Full Definitions of "] {
        if let Some(rest) = t.strip_prefix(head) {
            // 跳过标题里的词本身，以及「1」「adv」「n」这类粘在一起的序号 / 词性
            let rest = rest.trim_start_matches(|c: char| c.is_alphabetic());
            let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c.is_whitespace());
            let rest = ["adv", "adj", "n", "v"].iter().find_map(|p| rest.strip_prefix(p)).unwrap_or(rest);
            t = rest.trim_start();
            break;
        }
    }
    t.to_string()
}

/// 义项是不是习语 / 短语：英文部分开头几个词里就出现了这个词本身（或其变形）
fn is_phrase_sense(text: &str, headword: &str) -> bool {
    let head = headword.to_lowercase();
    let stem: String = head.chars().take(head.chars().count().saturating_sub(1).max(3)).collect();
    text.split(|c: char| !(c.is_ascii_alphabetic() || c == '\'' || c == '’'))
        .filter(|w| !w.is_empty())
        .take(4)
        .any(|w| w.to_lowercase().starts_with(&stem))
}

fn pos_name(p: &str) -> &'static str {
    match p {
        "n." => "名词",
        "v." => "动词",
        "adj." => "形容词",
        "adv." => "副词",
        _ => "其他词性",
    }
}

fn short(s: &str) -> String {
    let t: String = s.chars().take(24).collect();
    if s.chars().count() > 24 {
        format!("{t}…")
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(id: &str, html: &str) -> AtlasInput {
        AtlasInput { id: id.into(), name: id.to_uppercase(), entries: vec![html.into()] }
    }

    fn input(sentence: &str, target: &str, dicts: Vec<AtlasInput>) -> ContextInput {
        let start = sentence.find(target).unwrap_or(0);
        ContextInput {
            word: "address".into(),
            sentence: sentence.into(),
            hl_start: sentence[..start].encode_utf16().count(),
            hl_end: sentence[..start + target.len()].encode_utf16().count(),
            dicts,
        }
    }

    fn coca() -> Coca {
        let e = |lemma: &str, pos: &str, syn: &[&str], col: &[(&str, &str)]| crate::coca::CocaEntry {
            rank: 1,
            lemma: lemma.into(),
            pos: pos.into(),
            freq: 1,
            disp: 0.9,
            register: String::new(),
            synonyms: syn.iter().map(|x| x.to_string()).collect(),
            collocates: col.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        };
        let mut v: Vec<_> = (0..1000).map(|i| e(&format!("w{i}"), "n", &[], &[])).collect();
        v.push(e("issue", "n", &["subject, matter, question"], &[("verb", "address•, raise, deal•, discuss, resolve")]));
        v.push(e("address", "v", &["tackle, deal with, take in hand"], &[("noun", "•issue, •problem, question, •concern")]));
        Coca::from_entries(v).expect("样本 COCA")
    }

    fn address_dict() -> Vec<AtlasInput> {
        let oxford = r#"<span class="pos">noun</span><span class="def">details of where somebody lives or works and where letters can be sent 地址</span>
            <span class="def">a formal speech that is made in front of an audience 演说</span>
            <span class="pos">verb</span><span class="def">to write on an envelope the name and address of the person you are sending it to 写地址</span>
            <span class="def">to think about a problem or a situation and decide how you are going to deal with it 处理；对付</span>
            <span class="def">to say something directly to somebody 向…说话</span>"#;
        vec![dict("ox", oxford)]
    }

    #[test]
    fn picks_deal_with_for_address_the_issue() {
        let c = coca();
        let r = analyze(&input("The company addressed the issue.", "addressed", address_dict()), Some(&c), None);
        assert_eq!(r.pos.as_deref(), Some("v."));
        let best = r.best.clone().expect("有候选");
        assert!(best.label.contains("deal with") || best.label.contains("problem"), "{best:?}");
        assert!(r.ruled_out.iter().any(|x| x.contains("名词")), "{:?}", r.ruled_out);
        assert!(r.reasons.iter().any(|x| x.contains("address + issue")), "{:?}", r.reasons);
        assert!(r.confident, "{r:?}");
    }

    #[test]
    fn noun_after_determiner() {
        let r = analyze(&input("Please write your address on the form.", "address", address_dict()), None, None);
        assert_eq!(r.pos.as_deref(), Some("n."));
        assert!(r.best.is_some_and(|b| b.pos.as_deref() == Some("n.")));
    }

    #[test]
    fn adverbs_are_adverbs() {
        let w = words_of("with significantly enhanced capabilities");
        assert_eq!(guess_pos(&w, 1, "significantly").0, Some("adv."), "词头本身带 -ly 也是副词");
        let w = words_of("my family is large");
        assert_ne!(guess_pos(&w, 1, "family").0, Some("adv."));
        let mw = r#"<span class="pos">adverb</span><span class="def">in a significant manner; to a significant degree</span>
            <span class="def">it is significant</span>"#;
        let s = "DeepSeek has been updated, with significantly enhanced capabilities.";
        let start = s.find("significantly").unwrap_or(0);
        let r = analyze(
            &ContextInput {
                word: "significantly".into(),
                sentence: s.into(),
                hl_start: start,
                hl_end: start + "significantly".len(),
                dicts: vec![dict("mw", mw)],
            },
            None,
            None,
        );
        assert_eq!(r.pos.as_deref(), Some("adv."));
        assert!(r.best.is_some_and(|b| b.label.contains("significant degree") || b.label.contains("manner")), "不在句首，不选「it is significant」");
    }

    #[test]
    fn inflected_form_beats_single_pos() {
        let html = r#"<span class="pos">noun</span><span class="def">a piece of furniture with a flat top</span>
            <span class="def">a list of facts or numbers arranged in rows</span>"#;
        let s = "They tabled the motion.";
        let start = s.find("tabled").unwrap_or(0);
        let r = analyze(
            &ContextInput { word: "table".into(), sentence: s.into(), hl_start: start, hl_end: start + 6, dicts: vec![dict("x", html)] },
            None,
            None,
        );
        assert_eq!(r.pos.as_deref(), Some("v."), "tabled + 宾语是动词，即使词典只抽出了名词义项");
    }

    #[test]
    fn labels_are_cleaned() {
        assert_eq!(clean_label("Definitions of significantly1advin a significant manner"), "in a significant manner");
        assert_eq!(clean_label("Primary Meanings of table1.na piece of furniture"), "a piece of furniture");
        assert_eq!(clean_label("to deal with"), "to deal with");
    }

    #[test]
    fn pos_rules() {
        let w = words_of("She will address the crowd and has addressed letters.");
        assert_eq!(guess_pos(&w, 2, "address").0, Some("v."));
        assert_eq!(guess_pos(&w, 7, "address").0, Some("v."));
        let w = words_of("The company addressed the issue.");
        assert_eq!(guess_pos(&w, 2, "address").0, Some("v."), "前面隔一个词的 the 不能让它变成名词");
        let w = words_of("The speaker's address was moving.");
        assert_eq!(guess_pos(&w, 2, "address").0, Some("n."));
    }
}
