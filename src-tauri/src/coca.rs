//! COCA 2 万词频表（Davies & Gardner《A Frequency Dictionary of Contemporary American English》的词表部分）。
//! 数据由用户从自己的 PDF 解析成 JSON 后导入，复制到数据目录的 `coca20k.json`；不随程序分发、不进仓库。
//!
//! 每个词条：排名、词元、词性、频次、分布度、语体（S 口语 / F 小说 / M 杂志 / N 报纸 / A 学术）、
//! 近义词组、按词性分组的搭配词。搭配词里的 `•` 标出词头的位置：`•gap` = generation gap，`boom•` = boom generation。

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const FILE: &str = "coca20k.json";
/// 少于这个数不像一份完整的词表
const MIN_ENTRIES: usize = 1000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CocaEntry {
    pub rank: u32,
    pub lemma: String,
    /// 原表的单字母词性：n v j(形容词) r(副词) i(介词) c(连词) …
    pub pos: String,
    #[serde(default)]
    pub freq: u64,
    #[serde(default)]
    pub disp: f64,
    #[serde(default)]
    pub register: String,
    #[serde(default)]
    pub synonyms: Vec<String>,
    /// 分组名 adj / noun / verb / misc / all → 逗号分隔的搭配词
    #[serde(default)]
    pub collocates: BTreeMap<String, String>,
}

pub struct Coca {
    by_lemma: HashMap<String, Vec<CocaEntry>>,
    len: usize,
}

impl Coca {
    pub(crate) fn from_entries(entries: Vec<CocaEntry>) -> Result<Self> {
        if entries.len() < MIN_ENTRIES {
            return Err(Error::Other(crate::tf!(
                "只有 {} 个词条，不像完整的 COCA 词频表",
                "Only {} entries — this doesn't look like the full COCA list",
                entries.len()
            )));
        }
        let len = entries.len();
        let mut by_lemma: HashMap<String, Vec<CocaEntry>> = HashMap::new();
        for e in entries {
            by_lemma.entry(e.lemma.to_lowercase()).or_default().push(e);
        }
        for v in by_lemma.values_mut() {
            v.sort_by_key(|e| e.rank);
        }
        Ok(Self { by_lemma, len })
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let entries: Vec<CocaEntry> = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Other(crate::tf!("COCA 词表格式不对：{}", "The COCA list isn't in the expected format: {}", e)))?;
        Self::from_entries(entries)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// 查一个词：先按原样，再按词形还原（went → go、generations → generation）。同一词元的不同词性都返回。
    /// 全部原形（小写），词汇网络找易混词用
    pub fn lemmas(&self) -> Vec<String> {
        self.by_lemma.keys().cloned().collect()
    }

    /// 这个词的近义词（各组拆开、去掉短语）
    pub fn synonyms_of(&self, word: &str) -> Vec<String> {
        self.lookup(word)
            .iter()
            .flat_map(|e| e.synonyms.iter())
            .flat_map(|g| g.split(','))
            .map(|x| x.trim().to_lowercase())
            .filter(|x| !x.is_empty() && !x.contains(' '))
            .collect()
    }

    /// 这个词（小写）是不是表里的原形
    pub fn has_lemma(&self, lower: &str) -> bool {
        self.by_lemma.contains_key(lower)
    }

    pub fn lookup(&self, word: &str) -> Vec<CocaEntry> {
        let w = word.trim().to_lowercase();
        if let Some(v) = self.by_lemma.get(&w) {
            return v.clone();
        }
        let lemma = crate::batch::parse::lemmatize(&w, &|c: &str| self.by_lemma.contains_key(&c.to_lowercase()));
        self.by_lemma.get(&lemma.to_lowercase()).cloned().unwrap_or_default()
    }
}

/// 校验后复制到数据目录（先写临时文件再改名），返回词条数。
pub fn import(src: &Path, dest: &Path) -> Result<usize> {
    let coca = Coca::load(src)?;
    let tmp = dest.with_extension("json.part");
    std::fs::copy(src, &tmp)?;
    std::fs::rename(&tmp, dest)?;
    Ok(coca.len())
}

/// 搭配词展开成可读的短语：`•gap` → `generation gap`，`boom•` → `boom generation`，没有 • 的原样返回。
pub fn expand(lemma: &str, collocate: &str) -> String {
    let c = collocate.trim();
    if let Some(rest) = c.strip_prefix('•') {
        format!("{lemma} {}", rest.trim())
    } else if let Some(rest) = c.strip_suffix('•') {
        format!("{} {lemma}", rest.trim())
    } else {
        c.to_string()
    }
}

/// 给大模型的提示：每组前几个搭配，已展开成短语
pub fn collocation_hint(entries: &[CocaEntry], per_group: usize) -> String {
    let mut out = Vec::new();
    for e in entries {
        for (group, list) in &e.collocates {
            let items: Vec<String> = list.split(',').map(str::trim).filter(|s| !s.is_empty()).take(per_group).map(|c| expand(&e.lemma, c)).collect();
            if !items.is_empty() {
                out.push(format!("{group} ({}): {}", e.pos, items.join(", ")));
            }
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rank: u32, lemma: &str, pos: &str) -> CocaEntry {
        CocaEntry {
            rank,
            lemma: lemma.into(),
            pos: pos.into(),
            freq: 100,
            disp: 0.9,
            register: String::new(),
            synonyms: vec![],
            collocates: BTreeMap::from([("noun".to_string(), "•gap, boom•, power".to_string())]),
        }
    }

    fn sample() -> Result<Coca> {
        let mut v: Vec<CocaEntry> = (0..MIN_ENTRIES as u32).map(|i| entry(i + 10, &format!("w{i}"), "n")).collect();
        v.push(entry(2, "generation", "n"));
        v.push(entry(1, "go", "v"));
        v.push(entry(5, "light", "j"));
        v.push(entry(3, "light", "n"));
        Coca::from_entries(v)
    }

    #[test]
    fn lookup_handles_case_inflection_and_multiple_pos() -> Result<()> {
        let c = sample()?;
        assert_eq!(c.lookup("Generation")[0].rank, 2);
        assert_eq!(c.lookup("generations")[0].lemma, "generation");
        assert_eq!(c.lookup("went")[0].lemma, "go");
        assert_eq!(c.lookup("light").iter().map(|e| e.rank).collect::<Vec<_>>(), [3, 5], "按排名排");
        assert!(c.lookup("zzzz").is_empty());
        Ok(())
    }

    #[test]
    fn expands_bullets() {
        assert_eq!(expand("generation", "•gap"), "generation gap");
        assert_eq!(expand("generation", "boom•"), "boom generation");
        assert_eq!(expand("generation", "power"), "power");
        let hint = collocation_hint(&[entry(1, "generation", "n")], 2);
        assert_eq!(hint, "noun (n): generation gap, boom generation");
    }

    #[test]
    fn rejects_small_files() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = dir.path().join("x.json");
        std::fs::write(&p, serde_json::to_vec(&vec![entry(1, "a", "n")])?)?;
        assert!(import(&p, &dir.path().join(FILE)).is_err());
        Ok(())
    }
}
