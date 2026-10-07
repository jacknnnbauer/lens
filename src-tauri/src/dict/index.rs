//! FST 索引（SPEC 7.2）：归一化词头 → 记录位置（u64）。
//!
//! - `<id>.fst`：`fst::Map`，启动时 mmap 加载，不把整部词典读进内存。
//! - `<id>.dup`：冲突表。同一个归一化键对应多条记录时，FST 里存 `DUP_FLAG | 冲突表下标`。
//!
//! 记录位置的含义由各词典格式自己定义（JSON：条目下标；mdx：块序号 + 块内偏移；StarDict：offset/size）。

use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use fst::automaton::{Levenshtein, Str};
use fst::{Automaton, IntoStreamer, Map, MapBuilder, Streamer};
use memmap2::Mmap;
use unicode_normalization::UnicodeNormalization;

use crate::error::{Error, Result};

/// 最高位置 1 表示值是冲突表下标；记录位置本身不得占用最高位。
const DUP_FLAG: u64 = 1 << 63;
const DUP_MAGIC: &[u8; 8] = b"LENSDUP1";
/// 模糊查询只对长度 ≥ 4 的输入启用（SPEC 7.2）。
pub const MIN_FUZZY_CHARS: usize = 4;
/// 模糊查询先收集这么多候选再按编辑距离排序，避免短词 + 距离 2 时扫出海量结果。
const FUZZY_SCAN_CAP: usize = 500;

/// 词头归一化：trim、NFC、小写。查询端必须用同一个函数。
pub fn normalize_key(s: &str) -> String {
    s.trim().nfc().collect::<String>().to_lowercase()
}

pub fn index_paths(dir: &Path, dict_id: &str) -> (PathBuf, PathBuf) {
    (dir.join(format!("{dict_id}.fst")), dir.join(format!("{dict_id}.dup")))
}

#[derive(Default)]
pub struct IndexBuilder {
    pairs: Vec<(String, u64)>,
}

impl IndexBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// 加入一个键；空键忽略。`value` 不得占用最高位。
    pub fn add(&mut self, key: &str, value: u64) -> Result<()> {
        if value & DUP_FLAG != 0 {
            return Err(Error::InvalidDict(format!("记录位置超出范围：{value}")));
        }
        let key = normalize_key(key);
        if !key.is_empty() {
            self.pairs.push((key, value));
        }
        Ok(())
    }

    /// 写出 `<id>.fst` 与 `<id>.dup`（先写 `.tmp` 再改名），返回不同键的个数。
    pub fn write(mut self, fst_path: &Path, dup_path: &Path) -> Result<usize> {
        // 稳定排序：同键内保持插入顺序（同形异义按词典原顺序显示）
        self.pairs.sort_by(|a, b| a.0.cmp(&b.0));

        let fst_tmp = fst_path.with_extension("fst.tmp");
        let mut builder = MapBuilder::new(BufWriter::new(File::create(&fst_tmp)?))?;
        let mut dups: Vec<Vec<u64>> = Vec::new();
        let mut keys = 0usize;

        let mut i = 0;
        while i < self.pairs.len() {
            let key = &self.pairs[i].0;
            let mut j = i + 1;
            while j < self.pairs.len() && self.pairs[j].0 == *key {
                j += 1;
            }
            // 同键下重复的值（如别名归一化后与词头相同）只保留第一次出现
            let mut values: Vec<u64> = Vec::with_capacity(j - i);
            for (_, v) in &self.pairs[i..j] {
                if !values.contains(v) {
                    values.push(*v);
                }
            }
            let value = if let [single] = values[..] {
                single
            } else {
                dups.push(values);
                DUP_FLAG | (dups.len() as u64 - 1)
            };
            builder.insert(key, value)?;
            keys += 1;
            i = j;
        }

        let mut w = builder.into_inner()?;
        w.flush()?;
        drop(w);

        let dup_tmp = dup_path.with_extension("dup.tmp");
        write_dups(&dup_tmp, &dups)?;
        fs::rename(&fst_tmp, fst_path)?;
        fs::rename(&dup_tmp, dup_path)?;
        Ok(keys)
    }
}

fn write_dups(path: &Path, dups: &[Vec<u64>]) -> Result<()> {
    let mut w = BufWriter::new(File::create(path)?);
    w.write_all(DUP_MAGIC)?;
    w.write_all(&(dups.len() as u64).to_le_bytes())?;
    for group in dups {
        w.write_all(&(group.len() as u64).to_le_bytes())?;
        for v in group {
            w.write_all(&v.to_le_bytes())?;
        }
    }
    w.flush()?;
    Ok(())
}

fn read_dups(path: &Path) -> Result<Vec<Vec<u64>>> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    let bad = || Error::InvalidDict(format!("冲突表文件损坏：{}", path.display()));
    let mut words = bytes.strip_prefix(DUP_MAGIC.as_slice()).ok_or_else(bad)?.chunks_exact(8);
    if !words.remainder().is_empty() {
        return Err(bad());
    }
    let mut next = || -> Result<u64> {
        let w = words.next().ok_or_else(bad)?;
        Ok(u64::from_le_bytes(w.try_into().map_err(|_| bad())?))
    };
    let groups = next()?;
    let mut dups = Vec::new();
    for _ in 0..groups {
        let len = next()?;
        dups.push((0..len).map(|_| next()).collect::<Result<Vec<_>>>()?);
    }
    Ok(dups)
}

pub struct DictIndex {
    map: Map<Mmap>,
    dups: Vec<Vec<u64>>,
}

impl DictIndex {
    pub fn open(fst_path: &Path, dup_path: &Path) -> Result<Self> {
        let file = File::open(fst_path)?;
        // SAFETY: 索引文件由本程序独占生成，运行期间不会被截断或改写；
        // 重建索引时写新文件后改名，不原地修改已映射的文件。
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self { map: Map::new(mmap)?, dups: read_dups(dup_path)? })
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// 精确查：输入会先归一化。
    pub fn get(&self, key: &str) -> Vec<u64> {
        self.map.get(normalize_key(key)).map_or_else(Vec::new, |v| self.expand(v))
    }

    /// 前缀查询，按字典序返回最多 `limit` 个（归一化键, 记录位置）。
    pub fn prefix(&self, prefix: &str, limit: usize) -> Vec<(String, Vec<u64>)> {
        let prefix = normalize_key(prefix);
        if prefix.is_empty() || limit == 0 {
            return Vec::new();
        }
        let mut stream = self.map.search(Str::new(&prefix).starts_with()).into_stream();
        let mut out = Vec::new();
        while let Some((k, v)) = stream.next() {
            out.push((String::from_utf8_lossy(k).into_owned(), self.expand(v)));
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    /// 模糊查询（Levenshtein 距离 ≤ `max_dist`），按（距离, 键）排序，最多 `limit` 个。
    /// 输入短于 `MIN_FUZZY_CHARS` 个字符时返回空。
    pub fn fuzzy(&self, key: &str, max_dist: u32, limit: usize) -> Result<Vec<(String, Vec<u64>)>> {
        let key = normalize_key(key);
        if key.chars().count() < MIN_FUZZY_CHARS || limit == 0 {
            return Ok(Vec::new());
        }
        let aut = Levenshtein::new(&key, max_dist)
            .map_err(|e| Error::Other(format!("模糊查询自动机构建失败：{e}")))?;
        let mut stream = self.map.search(aut).into_stream();
        let mut hits = Vec::new();
        while let Some((k, v)) = stream.next() {
            let k = String::from_utf8_lossy(k).into_owned();
            hits.push((edit_distance(&key, &k), k, v));
            if hits.len() >= FUZZY_SCAN_CAP {
                break;
            }
        }
        hits.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        Ok(hits.into_iter().take(limit).map(|(_, k, v)| (k, self.expand(v))).collect())
    }

    fn expand(&self, v: u64) -> Vec<u64> {
        if v & DUP_FLAG == 0 {
            return vec![v];
        }
        // 下标越界说明 .dup 与 .fst 不匹配；返回空而不是 panic，由上层当作未命中
        usize::try_from(v & !DUP_FLAG)
            .ok()
            .and_then(|i| self.dups.get(i))
            .cloned()
            .unwrap_or_default()
    }
}

/// 按字符计的 OSA（受限 Damerau-Levenshtein）距离，仅用于给模糊结果排序：
/// 相邻换位（recieve → receive）算 1 次编辑，因为这是最常见的拼写错误。
/// FST 自动机按普通 Levenshtein 筛选，换位算 2 次，所以 `max_dist` ≥ 2 时仍能召回。
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = v;
        }
    }
    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(pairs: &[(&str, u64)]) -> Result<(tempfile::TempDir, DictIndex)> {
        let dir = tempfile::tempdir()?;
        let (fst, dup) = index_paths(dir.path(), "t");
        let mut b = IndexBuilder::new();
        for (k, v) in pairs {
            b.add(k, *v)?;
        }
        b.write(&fst, &dup)?;
        let idx = DictIndex::open(&fst, &dup)?;
        Ok((dir, idx))
    }

    #[test]
    fn normalizes_case_whitespace_and_unicode_form() {
        assert_eq!(normalize_key("  Ubiquitous "), "ubiquitous");
        // "café" 的 NFD 形式（e + U+0301）与 NFC 形式归一化后相同
        assert_eq!(normalize_key("cafe\u{301}"), normalize_key("caf\u{e9}"));
    }

    #[test]
    fn exact_lookup_merges_case_variants_into_dup_group() -> Result<()> {
        let (_d, idx) = build(&[("polish", 0), ("Polish", 1), ("run", 2), ("", 3)])?;
        assert_eq!(idx.len(), 2);
        assert_eq!(idx.get("POLISH"), vec![0, 1]);
        assert_eq!(idx.get("run"), vec![2]);
        assert!(idx.get("walk").is_empty());
        Ok(())
    }

    #[test]
    fn dup_group_keeps_insertion_order_and_drops_repeated_values() -> Result<()> {
        let (_d, idx) = build(&[("bank", 5), ("bank", 1), ("Bank", 5), ("ubiquity", 7), ("Ubiquity", 7)])?;
        assert_eq!(idx.get("bank"), vec![5, 1]);
        assert_eq!(idx.get("ubiquity"), vec![7]);
        Ok(())
    }

    #[test]
    fn prefix_returns_sorted_and_limited() -> Result<()> {
        let (_d, idx) = build(&[("ubiquity", 0), ("ubiquitous", 1), ("ubique", 2), ("umbrella", 3)])?;
        let keys: Vec<_> = idx.prefix("UBIQ", 2).into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["ubique", "ubiquitous"]);
        assert!(idx.prefix("", 10).is_empty());
        Ok(())
    }

    #[test]
    fn fuzzy_sorts_by_distance_and_skips_short_input() -> Result<()> {
        let (_d, idx) = build(&[("receive", 0), ("deceive", 1), ("recipe", 2), ("relieve", 3)])?;
        let hits: Vec<_> = idx.fuzzy("recieve", 2, 5)?.into_iter().map(|(k, _)| k).collect();
        // receive（换位）与 relieve（替换）距离 1，同距离按字典序；recipe 距离 2 排后；
        // deceive 的普通 Levenshtein 距离是 3，FST 不会召回
        assert_eq!(hits, ["receive", "relieve", "recipe"]);
        assert!(idx.fuzzy("rec", 2, 5)?.is_empty());
        Ok(())
    }

    #[test]
    fn rejects_values_using_the_flag_bit() {
        assert!(IndexBuilder::new().add("x", DUP_FLAG).is_err());
    }

    #[test]
    fn corrupt_dup_file_is_an_error_not_a_panic() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let (fst, dup) = index_paths(dir.path(), "t");
        IndexBuilder::new().write(&fst, &dup)?;
        fs::write(&dup, b"LENSDUP1\x05")?;
        assert!(DictIndex::open(&fst, &dup).is_err());
        Ok(())
    }

    #[test]
    fn edit_distance_counts_chars() {
        assert_eq!(edit_distance("recieve", "receive"), 1);
        assert_eq!(edit_distance("recieve", "relieve"), 1);
        assert_eq!(edit_distance("ca", "abc"), 3);
        assert_eq!(edit_distance("café", "cafe"), 1);
    }

    /// 规模 / 性能检查：`cargo test --release -- --ignored index_scale --nocapture`
    #[test]
    #[ignore]
    fn index_scale() -> Result<()> {
        use std::time::Instant;
        let n = 300_000u64;
        let dir = tempfile::tempdir()?;
        let (fst, dup) = index_paths(dir.path(), "scale");
        // 伪随机字母键（5–12 个字母），避免顺序编号被 FST 压成几个状态而失真
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut rand = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let keys: Vec<String> = (0..n)
            .map(|_| {
                let len = 5 + (rand() % 8) as usize;
                (0..len).map(|_| char::from(b'a' + (rand() % 26) as u8)).collect()
            })
            .collect();
        let t = Instant::now();
        let mut b = IndexBuilder::new();
        for (i, k) in keys.iter().enumerate() {
            b.add(k, i as u64)?;
        }
        b.write(&fst, &dup)?;
        let build = t.elapsed();

        let t = Instant::now();
        let idx = DictIndex::open(&fst, &dup)?;
        let open = t.elapsed();

        let mut times: Vec<_> = (0..10_000u64)
            .map(|i| {
                let t = Instant::now();
                let hit = idx.get(&keys[(i * 29 % n) as usize].to_uppercase());
                assert!(!hit.is_empty());
                t.elapsed()
            })
            .collect();
        times.sort();
        let t = Instant::now();
        let fuzzy = idx.fuzzy(&format!("{}q", keys[7]), 2, 5)?;
        let fuzzy_t = t.elapsed();
        println!(
            "keys={n} build={build:?} open={open:?} fst={}KB get_p50={:?} get_p95={:?} fuzzy={fuzzy_t:?} ({} hits)",
            fs::metadata(&fst)?.len() / 1024,
            times[times.len() / 2],
            times[times.len() * 95 / 100],
            fuzzy.len()
        );
        Ok(())
    }
}
