//! 内置的 Princeton WordNet 3.1（压缩后约 2.3 MB，随程序分发；许可声明在数据文件开头和 data/WORDNET-LICENSE）。
//! 没导入 COCA 的用户也能有：词性、近义词、派生关系、不规则变形。
//! 数据由 scripts/build_wordnet.py 生成，格式见那里的说明。

use std::collections::HashMap;
use std::io::Read;

use crate::error::{Error, Result};

static DATA: &[u8] = include_bytes!("../data/wordnet.tsv.gz");

pub const NOUN: u8 = 1;
pub const VERB: u8 = 2;
pub const ADJ: u8 = 4;
pub const ADV: u8 = 8;

fn pos_bit(p: &str) -> u8 {
    match p {
        "n" => NOUN,
        "v" => VERB,
        "a" => ADJ,
        "r" => ADV,
        _ => 0,
    }
}

/// 词性位 → 「n./v.」这样的标签（多个词性用 / 连起来）；没有为 None
pub fn pos_labels(bits: u8) -> Option<String> {
    let all: Vec<&str> = [NOUN, VERB, ADJ, ADV].iter().filter(|b| bits & **b != 0).filter_map(|b| pos_label(*b)).collect();
    (!all.is_empty()).then(|| all.join("/"))
}

/// 词性位 → n. / v. / adj. / adv.（只有一种时）
pub fn pos_label(bit: u8) -> Option<&'static str> {
    match bit {
        NOUN => Some("n."),
        VERB => Some("v."),
        ADJ => Some("adj."),
        ADV => Some("adv."),
        _ => None,
    }
}

pub struct WordNet {
    /// 同义词集：词性位 + 成员词
    synsets: Vec<(u8, Vec<String>)>,
    /// 词 → [(词性位, 义项编号按常用程度排序)]
    index: HashMap<String, Vec<(u8, Vec<u32>)>>,
    /// 派生关系（无向）
    derived: HashMap<String, Vec<String>>,
    /// (词性位, 变形) → 原形
    exceptions: HashMap<(u8, String), String>,
}

impl WordNet {
    pub fn load() -> Result<Self> {
        let mut text = String::new();
        flate2::read::GzDecoder::new(DATA)
            .read_to_string(&mut text)
            .map_err(|e| Error::Other(format!("WordNet 数据损坏：{e}")))?;
        Self::parse(&text)
    }

    fn parse(text: &str) -> Result<Self> {
        let mut wn = WordNet { synsets: Vec::new(), index: HashMap::new(), derived: HashMap::new(), exceptions: HashMap::new() };
        let word = |s: &str| s.replace('_', " ");
        for line in text.lines() {
            let mut f = line.split('\t');
            match f.next() {
                Some("S") => {
                    let (Some(p), Some(ws)) = (f.next(), f.next()) else { continue };
                    wn.synsets.push((pos_bit(p), ws.split('|').map(word).collect()));
                }
                Some("I") => {
                    let (Some(w), Some(p), Some(ids)) = (f.next(), f.next(), f.next()) else { continue };
                    let ids: Vec<u32> = ids.split(',').filter_map(|x| x.parse().ok()).collect();
                    wn.index.entry(word(w)).or_default().push((pos_bit(p), ids));
                }
                Some("D") => {
                    let (Some(a), Some(b)) = (f.next(), f.next()) else { continue };
                    let (a, b) = (word(a), word(b));
                    wn.derived.entry(a.clone()).or_default().push(b.clone());
                    wn.derived.entry(b).or_default().push(a);
                }
                Some("E") => {
                    let (Some(p), Some(form), Some(base)) = (f.next(), f.next(), f.next()) else { continue };
                    wn.exceptions.entry((pos_bit(p), word(form))).or_insert_with(|| word(base));
                }
                _ => {}
            }
        }
        if wn.synsets.is_empty() || wn.index.is_empty() {
            return Err(Error::Other("WordNet 数据为空".into()));
        }
        Ok(wn)
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// 这个词（小写原形）是不是 WordNet 里的词
    pub fn has(&self, word: &str) -> bool {
        self.index.contains_key(word)
    }

    /// 词性位的并集（NOUN | VERB …）；查不到为 0
    pub fn poses(&self, word: &str) -> u8 {
        self.index.get(word).map_or(0, |v| v.iter().fold(0, |acc, (p, _)| acc | p))
    }

    /// 只有一种词性时返回它
    pub fn single_pos(&self, word: &str) -> Option<&'static str> {
        pos_label(self.poses(word))
    }

    /// 近义词：按义项常用程度依次取各同义词集的其他成员（只要单词，不要短语），去重。
    /// pos 为 0 时不限词性
    pub fn synonyms(&self, word: &str, pos: u8, limit: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let Some(senses) = self.index.get(word) else { return out };
        // 各词性的义项轮流取：常用义项的近义词排在前面
        let lists: Vec<&Vec<u32>> = senses.iter().filter(|(p, _)| pos == 0 || p & pos != 0).map(|(_, ids)| ids).collect();
        let longest = lists.iter().map(|l| l.len()).max().unwrap_or(0);
        for i in 0..longest {
            for ids in &lists {
                let Some(&id) = ids.get(i) else { continue };
                let Some((_, members)) = self.synsets.get(id as usize) else { continue };
                for m in members {
                    if m != word && !m.contains(' ') && !out.contains(m) {
                        out.push(m.clone());
                        if out.len() >= limit {
                            return out;
                        }
                    }
                }
            }
        }
        out
    }

    /// 派生关系：直接相连的词，再走一步（rustily → rusty → rust）；走第二步时要求开头三个字母相同，免得跑远
    pub fn derived(&self, word: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let Some(first) = self.derived.get(word) else { return out };
        let start: String = word.chars().take(3).collect();
        for a in first {
            if a != word && !out.contains(a) {
                out.push(a.clone());
            }
        }
        for a in first {
            for b in self.derived.get(a).into_iter().flatten() {
                if b != word && b.starts_with(&start) && !out.contains(b) {
                    out.push(b.clone());
                }
            }
        }
        out
    }

    /// 不规则变形的原形（went → go、mice → mouse、better → good）
    pub fn irregular(&self, form: &str) -> Option<&str> {
        [VERB, NOUN, ADJ, ADV].iter().find_map(|p| self.exceptions.get(&(*p, form.to_string())).map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_data_loads_and_answers() -> Result<()> {
        let wn = WordNet::load()?;
        assert!(wn.len() > 100_000);
        assert_eq!(wn.single_pos("significantly"), Some("adv."));
        assert_eq!(wn.poses("rust") & (NOUN | VERB), NOUN | VERB, "rust 能当名词、动词（也能当颜色形容词）");
        assert!(wn.derived("rusty").contains(&"rust".to_string()));
        assert!(wn.synonyms("rusty", ADJ, 10).iter().any(|s| s == "rust" || s == "hoary"), "{:?}", wn.synonyms("rusty", ADJ, 10));
        assert_eq!(wn.irregular("went"), Some("go"));
        assert_eq!(wn.irregular("mice"), Some("mouse"));
        assert!(wn.has("address"));
        // 分词形容词也收成词（addressed adj.）：原形判断时要让给更短的 address（见 lemma::base_form）
        assert_eq!(wn.poses("addressed"), ADJ);
        Ok(())
    }

    #[test]
    fn parses_lines() -> Result<()> {
        let wn = WordNet::parse("# licence\nS\tn\trust|rust_brown\nS\tv\trust|corrode\nI\trust\tn\t0\nI\trust\tv\t1\nD\trust\trusty\nE\tv\twent\tgo\n")?;
        assert_eq!(wn.poses("rust") & (NOUN | VERB), NOUN | VERB, "rust 能当名词、动词（也能当颜色形容词）");
        assert_eq!(wn.synonyms("rust", 0, 5), vec!["corrode"]);
        assert_eq!(wn.derived("rusty"), vec!["rust"]);
        assert_eq!(wn.irregular("went"), Some("go"));
        Ok(())
    }
}
