//! 内置英文词频排名（wordfreq 3.1，CC BY-SA 4.0；`scripts/build_freq.py` 生成）：
//! 4 万个原形，行号 = 排名。没导入 COCA 时，文章生词扫描和反查排序用它。

use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

const DATA: &[u8] = include_bytes!("../data/wordfreq_en.txt.gz");

pub struct Freq {
    rank: HashMap<String, u32>,
}

impl Freq {
    fn load() -> Self {
        let mut text = String::new();
        if let Err(e) = flate2::read::GzDecoder::new(DATA).read_to_string(&mut text) {
            tracing::warn!(error = %e, "词频表数据损坏");
        }
        let rank = text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .enumerate()
            .map(|(i, w)| (w.to_string(), i as u32 + 1))
            .collect();
        Self { rank }
    }

    /// 原形的排名（1 = 最常用）；不在前 4 万里为 None
    pub fn rank(&self, lemma: &str) -> Option<u32> {
        self.rank.get(lemma).copied()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.rank.len()
    }
}

/// 第一次用时解压（约 150 KB，几毫秒）
pub fn get() -> &'static Freq {
    static FREQ: OnceLock<Freq> = OnceLock::new();
    FREQ.get_or_init(Freq::load)
}

#[cfg(test)]
mod tests {
    #[test]
    fn common_words_rank_high() {
        let f = super::get();
        assert!(f.len() >= 30_000);
        assert_eq!(f.rank("the"), Some(1));
        let (house, abandon, obstinate) = (f.rank("house"), f.rank("abandon"), f.rank("obstinate"));
        assert!(house < abandon && abandon < obstinate, "{house:?} {abandon:?} {obstinate:?}");
        assert!(f.rank("abandoned").is_none(), "词形归到原形");
    }
}
