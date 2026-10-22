//! 词汇网络（离线）：一个词炸开成四组相关词——派生词、同根词、易混词、近义词。
//!
//! - 派生词：去掉词尾得到词干（convulse → convuls），在本地词典的词头索引里按前缀找，
//!   剩下的部分必须能拆成常见后缀（-ion、-ive、-ive+ly…），才算同一个词族；
//! - 同根词：再去掉前缀得到词根（con|vuls），给词根换上常见前缀（re-、a-、e-…）去索引里找；
//! - 易混词：COCA 词表里拼写只差一两个字母的词，加上一张常见易混词对照表；
//! - 近义词：COCA 的近义词组。
//!
//! 全是启发式，不联网。

use serde::Serialize;

/// 后缀（长的在前），用来求词干、检查派生词
const SUFFIXES: &[&str] = &[
    "ationally", "izations", "ization", "isation", "ational", "fulness", "lessness", "iveness", "ousness", "ically", "ations",
    "ation", "itions", "ition", "ements", "ement", "iness", "ments", "ment", "ness", "ances", "ences", "ance", "ence", "ancy",
    "ency", "ables", "able", "ible", "ably", "ibly", "ical", "ally", "ious", "eous", "ously", "ous", "ively", "ive", "ities", "ity",
    "ants", "ant", "ents", "ent", "isms", "ism", "ists", "ist", "izes", "ized", "izing", "ize", "ises", "ised", "ising", "ise",
    "ifies", "ified", "ify", "ful", "less", "ions", "ion", "ures", "ure", "eries", "ery", "ary", "ory", "ily", "ier", "iest",
    "ied", "ies", "ers", "er", "ors", "or", "ees", "ee", "ings", "ing", "ed", "es", "al", "ly", "ic", "ics", "s", "e", "y",
];
/// 前缀（长的在前），用来求词根、拼同根词
const PREFIXES: &[&str] = &[
    "counter", "inter", "trans", "super", "under", "over", "anti", "auto", "mis", "non", "out", "pre", "pro", "re", "de", "dis",
    "con", "com", "col", "cor", "co", "ex", "e", "in", "im", "il", "ir", "un", "sub", "sup", "per", "ob", "op", "ad", "ab", "ac",
    "af", "ag", "al", "ap", "ar", "as", "at", "a", "en", "em", "di", "se",
];
/// 常见易混词（双向）
const CONFUSABLE_PAIRS: &[(&str, &str)] = &[
    ("affect", "effect"), ("accept", "except"), ("adapt", "adopt"), ("advice", "advise"), ("allude", "elude"),
    ("altogether", "all together"), ("allusion", "illusion"), ("amoral", "immoral"), ("averse", "adverse"), ("bare", "bear"),
    ("breath", "breathe"), ("capital", "capitol"), ("censor", "censure"), ("cite", "site"), ("climatic", "climactic"),
    ("complement", "compliment"), ("comprise", "compose"), ("conscience", "conscious"), ("continual", "continuous"),
    ("council", "counsel"), ("credible", "credulous"), ("defuse", "diffuse"), ("desert", "dessert"), ("device", "devise"),
    ("disinterested", "uninterested"), ("discreet", "discrete"), ("economic", "economical"), ("elicit", "illicit"),
    ("emigrate", "immigrate"), ("eminent", "imminent"), ("ensure", "insure"), ("envelop", "envelope"), ("farther", "further"),
    ("flaunt", "flout"), ("historic", "historical"), ("imply", "infer"), ("ingenious", "ingenuous"), ("lay", "lie"),
    ("lose", "loose"), ("moral", "morale"), ("personal", "personnel"), ("precede", "proceed"), ("prescribe", "proscribe"),
    ("principal", "principle"), ("raise", "rise"), ("respectfully", "respectively"), ("sensible", "sensitive"),
    ("stationary", "stationery"), ("than", "then"), ("tortuous", "torturous"), ("waive", "wave"), ("weather", "whether"),
    ("adverse", "averse"), ("alternate", "alternative"), ("assure", "ensure"), ("compulsive", "compulsory"),
    ("contemptible", "contemptuous"), ("deprecate", "depreciate"), ("expand", "expend"), ("illegible", "ineligible"),
    ("incredible", "incredulous"), ("industrial", "industrious"), ("judicial", "judicious"), ("loath", "loathe"),
    ("persecute", "prosecute"), ("perspective", "prospective"), ("quiet", "quite"), ("regretful", "regrettable"),
    ("respectable", "respectful"), ("sociable", "social"), ("subconscious", "unconscious"), ("successful", "successive"),
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyGroup {
    /// derived | root | confusable | synonym
    pub kind: &'static str,
    pub words: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordFamily {
    pub word: String,
    pub stem: String,
    /// 去掉前缀后的词根（找不到前缀时为 None）
    pub root: Option<String>,
    pub groups: Vec<FamilyGroup>,
}

const MAX_PER_GROUP: usize = 10;

/// 去掉一个后缀（剩下至少 4 个字母）；ss 结尾的不当复数去 s（address、class）
fn strip_suffix(w: &str) -> Option<&str> {
    SUFFIXES.iter().find_map(|s| {
        let rest = w.strip_suffix(s)?;
        if rest.chars().count() < 4 || (*s == "s" && w.ends_with("ss")) || (*s == "es" && w.ends_with("ses")) {
            return None;
        }
        Some(rest)
    })
}

/// 词干：最多去两层后缀（nationalities → nationality → national）；
/// 第二层只去两个字母以上的真后缀，不再去单个 s / e / y（convulse → convuls，不能再变成 convul）
pub fn stem_of(word: &str) -> String {
    let Some(first) = strip_suffix(word) else { return word.to_string() };
    let removed = word.len() - first.len();
    match strip_suffix(first) {
        Some(second) if removed >= 2 && first.len() - second.len() >= 2 => second.to_string(),
        _ => first.to_string(),
    }
}

/// rest 能不能全部拆成后缀（ion、ive+ly、i+ness…）
fn all_suffixes(rest: &str) -> bool {
    if rest.is_empty() {
        return true;
    }
    // y → i：happy → happ|iness
    SUFFIXES.iter().any(|s| rest.starts_with(s) && all_suffixes(&rest[s.len()..]))
        || rest.strip_prefix('i').is_some_and(|r| !r.is_empty() && all_suffixes(r))
}

/// 去掉前缀后的词根（至少 3 个字母）
pub fn root_of(stem: &str) -> Option<(&'static str, String)> {
    PREFIXES.iter().find_map(|p| {
        let r = stem.strip_prefix(p)?;
        (r.chars().count() >= 3 && r.chars().any(|c| "aeiouy".contains(c))).then(|| (*p, r.to_string()))
    })
}

/// 去掉组里其他词的词形变化：有 avulse 时去掉 avulsed / avulses / avulsing，有 convulsion 时去掉 convulsions
fn drop_inflections(list: &mut Vec<String>) {
    let all = list.clone();
    let has = |b: &str| all.iter().any(|x| x == b || x.strip_suffix('e') == Some(b));
    list.retain(|w| {
        !["ing", "ed", "es", "d", "s"].iter().any(|x| w.strip_suffix(x).is_some_and(|b| b.len() >= 3 && has(b)))
    });
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// prefix_search(前缀, 上限)：本地词典词头里以它开头的词（小写）；lemmas：COCA 原形表；synonyms：COCA 近义词
pub fn build(word: &str, prefix_search: &dyn Fn(&str, usize) -> Vec<String>, lemmas: &[String], synonyms: &[String]) -> WordFamily {
    let word = word.trim().to_lowercase();
    let clean = |w: &str| w.chars().all(|c| c.is_ascii_lowercase() || c == '-') && w.len() <= 20;
    let derived_of = |stem: &str| -> Vec<String> {
        prefix_search(stem, 200)
            .into_iter()
            .filter(|c| c != &word && clean(c) && c.strip_prefix(stem).is_some_and(all_suffixes))
            .collect()
    };
    // 词干切得太狠时（deceive → dece）找不到派生词：退回只去掉末尾 e 的词干（deceiv → deceiver）
    let mut stem = stem_of(&word);
    let mut derived = derived_of(&stem);
    if derived.is_empty() {
        let soft = word.strip_suffix('e').unwrap_or(&word).to_string();
        if soft != stem {
            derived = derived_of(&soft);
            stem = soft;
        }
    }
    derived.sort_by_key(|w| (w.len(), w.clone()));
    derived.dedup();
    // 词形变化不算新词：convulse 的 convulses / convulsed / convulsing；convulsion 的复数 convulsions
    let inflections: Vec<String> = ["s", "es", "d", "ed", "ing"]
        .iter()
        .flat_map(|x| [format!("{word}{x}"), format!("{stem}{x}")])
        .collect();
    derived.retain(|w| !inflections.contains(w));
    drop_inflections(&mut derived);
    derived.truncate(MAX_PER_GROUP);

    // 同根词：换前缀
    let root = root_of(&stem);
    let mut same_root: Vec<String> = Vec::new();
    if let Some((own, r)) = &root {
        for p in PREFIXES.iter().chain(std::iter::once(&"")) {
            if p == own {
                continue;
            }
            for c in prefix_search(&format!("{p}{r}"), 8) {
                let rest = &c[p.len() + r.len()..];
                if clean(&c) && c != word && !c.starts_with(&stem) && all_suffixes(rest) && !same_root.contains(&c) {
                    same_root.push(c);
                }
            }
            if same_root.len() >= MAX_PER_GROUP * 2 {
                break;
            }
        }
        same_root.sort_by_key(|w| (w.len(), w.clone()));
        drop_inflections(&mut same_root);
        same_root.truncate(MAX_PER_GROUP);
    }

    // 易混词：对照表 + COCA 里拼写只差一点的
    let mut confusable: Vec<String> = CONFUSABLE_PAIRS
        .iter()
        .filter_map(|(a, b)| if *a == word { Some(b.to_string()) } else if *b == word { Some(a.to_string()) } else { None })
        .collect();
    if word.len() >= 4 {
        let mut near: Vec<(usize, &String)> = lemmas
            .iter()
            .filter(|l| l.as_str() != word && l.len().abs_diff(word.len()) <= 1 && clean(l))
            .filter_map(|l| {
                let d = edit_distance(l, &word);
                // 一个字母之差；或长词里、长度相同、首字母相同的两个字母之差（principal / principle）
                (d == 1 || (d == 2 && word.len() >= 7 && l.len() == word.len() && l.chars().next() == word.chars().next())).then_some((d, l))
            })
            .filter(|(_, l)| !derived.contains(l) && !l.starts_with(&stem))
            .collect();
        near.sort();
        for (_, l) in near.into_iter().take(6) {
            if !confusable.contains(l) {
                confusable.push(l.clone());
            }
        }
    }
    confusable.truncate(MAX_PER_GROUP);

    let mut syn: Vec<String> = Vec::new();
    for s in synonyms {
        let s = s.trim().to_lowercase();
        if !s.is_empty() && s != word && !syn.contains(&s) {
            syn.push(s);
        }
    }
    syn.truncate(MAX_PER_GROUP);

    let groups = [("derived", derived), ("root", same_root), ("confusable", confusable), ("synonym", syn)]
        .into_iter()
        .filter(|(_, w)| !w.is_empty())
        .map(|(kind, words)| FamilyGroup { kind, words })
        .collect();
    WordFamily { word, stem, root: root.map(|(_, r)| r), groups }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Vec<String> {
        [
            "convulse", "convulsed", "convulsion", "convulsions", "convulsive", "convulsively", "convulsant", "convulsing",
            "revulsion", "revulsive", "avulsion", "evulsion", "address", "addressee", "addressable", "redress", "dress",
            "affect", "effect", "affection", "affected", "convent",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    fn search(prefix: &str, limit: usize) -> Vec<String> {
        index().into_iter().filter(|w| w.starts_with(prefix)).take(limit).collect()
    }

    #[test]
    fn stems_and_roots() {
        assert_eq!(stem_of("convulse"), "convuls");
        assert_eq!(stem_of("convulsively"), "convuls");
        assert_eq!(stem_of("address"), "address");
        assert_eq!(root_of("convuls"), Some(("con", "vuls".to_string())));
        assert!(all_suffixes("ively") && all_suffixes("ion") && all_suffixes("iness"));
        assert!(!all_suffixes("xyz"));
    }

    #[test]
    fn convulse_family() {
        let lemmas: Vec<String> = ["effect", "affect", "affix", "defect", "convert"].iter().map(|s| s.to_string()).collect();
        let f = build("convulse", &search, &lemmas, &["shake".into(), "agitate".into()]);
        let g = |k: &str| f.groups.iter().find(|g| g.kind == k).map(|g| g.words.clone()).unwrap_or_default();
        let d = g("derived");
        assert!(d.contains(&"convulsion".to_string()) && d.contains(&"convulsive".to_string()) && d.contains(&"convulsively".to_string()), "{d:?}");
        assert!(!d.contains(&"convent".to_string()));
        for w in ["convulsed", "convulsing", "convulsions"] {
            assert!(!d.contains(&w.to_string()), "词形变化不算：{w}");
        }
        let r = g("root");
        assert!(r.contains(&"revulsion".to_string()) && r.contains(&"avulsion".to_string()), "{r:?}");
        assert_eq!(g("synonym"), vec!["shake", "agitate"]);
    }

    #[test]
    fn inflections_are_dropped() {
        let mut v: Vec<String> = ["avulse", "avulsed", "avulses", "avulsing", "avulsion", "avulsions", "revulsion"].iter().map(|s| s.to_string()).collect();
        drop_inflections(&mut v);
        assert_eq!(v, vec!["avulse", "avulsion", "revulsion"]);
    }

    #[test]
    fn confusables_from_table_and_spelling() {
        let lemmas: Vec<String> = ["effect", "affect", "affix", "defect"].iter().map(|s| s.to_string()).collect();
        let f = build("affect", &search, &lemmas, &[]);
        let c = f.groups.iter().find(|g| g.kind == "confusable").map(|g| g.words.clone()).unwrap_or_default();
        assert_eq!(c.first().map(String::as_str), Some("effect"));
    }
}
