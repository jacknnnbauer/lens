//! mdx / mdd 词典（SPEC 7.3）。
//!
//! 导入时读出全部词头，按记录区偏移算出每条记录的起止，写词条表 `<id>.ent` 和 FST（值 = 词条序号）。
//! 同名的 `x.mdd`、`x.1.mdd`、`x.2.mdd`… 各自建 `<id>.mdd<N>.{fst,dup,ent}`，键为资源路径。
//! 运行时 mmap 原文件，按需解压记录块（缓存最近几块）。
//! 资源先找 mdx 同目录的散装文件，再找 mdd。

pub mod block;
pub mod entries;
pub mod file;
pub mod header;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::index::{index_paths, DictIndex, IndexBuilder};
use super::{read_local_resource, DictMeta, DictSource, EntryBody, RawEntry};
use crate::error::{Error, Result};
use entries::{ent_path, EntryTable};
use file::MdxFile;

/// `@@@LINK=` 最多跟随几层（SPEC 7.3）
const MAX_LINK_DEPTH: usize = 5;

/// 同名 mdd：`x.mdd`、`x.1.mdd`、`x.2.mdd`…，遇到第一个缺号即停。
pub fn discover_mdds(mdx_path: &Path) -> Vec<PathBuf> {
    let base = mdx_path.with_extension("");
    let mut out = Vec::new();
    let first = PathBuf::from(format!("{}.mdd", base.display()));
    if first.is_file() {
        out.push(first);
        for n in 1.. {
            let p = PathBuf::from(format!("{}.{n}.mdd", base.display()));
            if !p.is_file() {
                break;
            }
            out.push(p);
        }
    }
    out
}

/// 资源路径归一化：`\img\A.png` → `/img/a.png`（小写由索引归一化完成）。
pub fn normalize_res_path(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    if p.starts_with('/') {
        p
    } else {
        format!("/{p}")
    }
}

fn mdd_stem(dict_id: &str, n: usize) -> String {
    format!("{dict_id}.mdd{n}")
}

/// 导入前的解析结果。
pub struct Prepared {
    pub name: String,
    main: MdxFile,
    keys: Vec<(u64, String)>,
    mdds: Vec<(MdxFile, Vec<(u64, String)>)>,
}

impl Prepared {
    /// 取头部声明的词条数，打开时用它判断文件是否被替换过。
    pub fn entry_count(&self) -> u64 {
        self.main.num_entries()
    }
}

pub fn prepare(path: &Path) -> Result<Prepared> {
    let main = MdxFile::open(path, false)?;
    let keys = main.read_keys()?;
    let mut mdds = Vec::new();
    for p in discover_mdds(path) {
        // mdd 损坏不影响释义本身，跳过并记日志
        match MdxFile::open(&p, true).and_then(|f| f.read_keys().map(|k| (f, k))) {
            Ok(m) => mdds.push(m),
            Err(e) => tracing::warn!(mdd = %p.display(), error = %e, "mdd 解析失败，已跳过"),
        }
    }
    let title = &main.header.title;
    let name = if title.is_empty() || title.contains("No HTML code allowed") {
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        title.clone()
    };
    Ok(Prepared { name, main, keys, mdds })
}

/// 按记录区偏移算出每条记录的结束位置：下一个更大的偏移，最后一条到记录区末尾。
fn with_ends(keys: &[(u64, String)], total: u64) -> Vec<(u64, u64, &str)> {
    let mut starts: Vec<u64> = keys.iter().map(|k| k.0).collect();
    starts.sort_unstable();
    starts.dedup();
    keys.iter()
        .map(|(start, hw)| {
            let next = starts.partition_point(|s| s <= start);
            let end = starts.get(next).copied().unwrap_or(total);
            (*start, end, hw.as_str())
        })
        .collect()
}

fn build_part(keys: &[(u64, String)], total: u64, dir: &Path, stem: &str, res_keys: bool) -> Result<usize> {
    let rows = with_ends(keys, total);
    entries::write(&ent_path(dir, stem), &rows)?;
    let mut builder = IndexBuilder::new();
    for (i, (_, hw)) in keys.iter().enumerate() {
        if res_keys {
            builder.add(&normalize_res_path(hw), i as u64)?;
        } else {
            builder.add(hw, i as u64)?;
        }
    }
    let (fst, dup) = index_paths(dir, stem);
    builder.write(&fst, &dup)
}

pub fn build_index(p: &Prepared, index_dir: &Path, dict_id: &str) -> Result<usize> {
    let keys = build_part(&p.keys, p.main.total_record_size(), index_dir, dict_id, false)?;
    for (n, (file, mkeys)) in p.mdds.iter().enumerate() {
        let res = build_part(mkeys, file.total_record_size(), index_dir, &mdd_stem(dict_id, n), true)?;
        tracing::info!(mdd = %file.path.display(), resources = res, "mdd 索引完成");
    }
    Ok(keys)
}

struct Part {
    file: MdxFile,
    entries: EntryTable,
    index: DictIndex,
}

pub struct Mdx {
    meta: DictMeta,
    /// mdx 所在目录，散装资源（常见为 CSS）从这里找
    dir: PathBuf,
    main: MdxFile,
    entries: EntryTable,
    index: Arc<DictIndex>,
    mdds: Vec<Part>,
}

impl Mdx {
    pub fn open(meta: DictMeta, path: &Path, index_dir: &Path, index: Arc<DictIndex>) -> Result<Self> {
        let main = MdxFile::open(path, false)?;
        if main.num_entries() != meta.entry_count {
            return Err(Error::InvalidDict(format!(
                "{} 自导入后已被修改（词条数 {} → {}），需要重建索引",
                path.display(),
                meta.entry_count,
                main.num_entries()
            )));
        }
        let entries = EntryTable::open(&ent_path(index_dir, &meta.id))?;

        let mut mdds = Vec::new();
        for (n, p) in discover_mdds(path).into_iter().enumerate() {
            let stem = mdd_stem(&meta.id, n);
            let (fst, dup) = index_paths(index_dir, &stem);
            if !fst.is_file() {
                tracing::warn!(mdd = %p.display(), "mdd 没有索引（导入后新增？），已跳过");
                continue;
            }
            let part = MdxFile::open(&p, true).and_then(|file| {
                Ok(Part {
                    file,
                    entries: EntryTable::open(&ent_path(index_dir, &stem))?,
                    index: DictIndex::open(&fst, &dup)?,
                })
            });
            match part {
                Ok(part) => mdds.push(part),
                Err(e) => tracing::warn!(mdd = %p.display(), error = %e, "mdd 打开失败，已跳过"),
            }
        }
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok(Self { meta, dir, main, entries, index, mdds })
    }

    fn record_text(&self, start: u64, end: u64) -> Result<String> {
        let bytes = self.main.read_record(start, end)?;
        let (text, _) = self.main.header.encoding.decode_without_bom_handling(&bytes);
        Ok(text.trim_end_matches(['\0', '\r', '\n', ' ', '\t']).to_string())
    }

    fn body_of(&self, text: String) -> EntryBody {
        if self.main.header.format_text {
            EntryBody::Text(text)
        } else {
            let styled = header::apply_stylesheet(&text, &self.main.header.stylesheet);
            EntryBody::Html(styled.trim_end().to_string())
        }
    }

    fn resolve(&self, i: u64, depth: usize, seen: &mut HashSet<u64>, out: &mut Vec<RawEntry>) -> Result<()> {
        let e = self.entries.get(i)?;
        let text = self.record_text(e.start, e.end)?;
        if let Some(target) = text.trim_start().strip_prefix("@@@LINK=") {
            let target = target.lines().next().unwrap_or_default().trim();
            if depth >= MAX_LINK_DEPTH {
                tracing::debug!(dict = %self.meta.name, from = e.headword, "@@@LINK 超过 {MAX_LINK_DEPTH} 层，停止跟随");
                return Ok(());
            }
            for j in self.index.get(target) {
                self.resolve(j, depth + 1, seen, out)?;
            }
            return Ok(());
        }
        if !seen.insert(e.start) {
            return Ok(());
        }
        out.push(RawEntry { headword: e.headword.to_string(), body: self.body_of(text) });
        Ok(())
    }
}

impl DictSource for Mdx {
    fn meta(&self) -> &DictMeta {
        &self.meta
    }

    fn lookup(&self, key: &str) -> Result<Vec<RawEntry>> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        for i in self.index.get(key) {
            self.resolve(i, 0, &mut seen, &mut out)?;
        }
        Ok(out)
    }

    fn entry_total(&self) -> u64 {
        self.entries.len() as u64
    }

    fn for_each_entry(&self, f: &mut dyn FnMut(u64, &str, &EntryBody) -> bool) -> Result<()> {
        let mut last_start = None;
        for i in 0..self.entries.len() as u64 {
            let e = self.entries.get(i)?;
            // 同一条记录的多个词头（键按记录顺序排列时相邻）只算一次
            if last_start == Some(e.start) {
                continue;
            }
            last_start = Some(e.start);
            let text = self.record_text(e.start, e.end)?;
            if text.trim_start().starts_with("@@@LINK=") {
                continue;
            }
            if !f(i, e.headword, &self.body_of(text)) {
                break;
            }
        }
        Ok(())
    }

    /// 先找 mdx 同目录的散装文件（用户改过的 CSS 优先生效），再按顺序找各个 mdd。
    fn resource(&self, path: &str) -> Result<Option<Vec<u8>>> {
        if let Some(bytes) = read_local_resource(&self.dir, path)? {
            return Ok(Some(bytes));
        }
        let key = normalize_res_path(path);
        for part in &self.mdds {
            if let Some(&i) = part.index.get(&key).first() {
                let e = part.entries.get(i)?;
                return Ok(Some(part.file.read_record(e.start, e.end)?));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_ends_follow_next_distinct_offset() {
        let keys = vec![(0, "a".into()), (10, "b".into()), (10, "b2".into()), (25, "c".into())];
        let rows = with_ends(&keys, 40);
        let spans: Vec<_> = rows.iter().map(|r| (r.0, r.1)).collect();
        assert_eq!(spans, [(0, 10), (10, 25), (10, 25), (25, 40)]);
    }

    /// 导入各阶段耗时：`LENS_PROFILE_MDX=x.mdx cargo test --release profile_import -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn profile_import() -> Result<()> {
        use std::time::Instant;
        let Ok(path) = std::env::var("LENS_PROFILE_MDX") else { return Ok(()) };
        let dir = tempfile::tempdir()?;
        let t = Instant::now();
        let main = MdxFile::open(Path::new(&path), false)?;
        let open = t.elapsed();
        let t = Instant::now();
        let keys = main.read_keys()?;
        let read_keys = t.elapsed();
        let mut lens: Vec<usize> = keys.iter().map(|k| k.1.len()).collect();
        lens.sort_unstable();
        let q = |p: usize| lens[(lens.len() * p / 100).min(lens.len() - 1)];
        let total: usize = lens.iter().sum();
        let over = |n: usize| lens.iter().filter(|&&l| l > n).count();
        println!(
            "key bytes: p50={} p90={} p99={} max={} avg={:.1} | >64B: {} >128B: {} | bytes in >64B keys: {:.0}%",
            q(50), q(90), q(99), lens[lens.len() - 1], total as f64 / lens.len() as f64, over(64), over(128),
            100.0 * lens.iter().filter(|&&l| l > 64).sum::<usize>() as f64 / total as f64
        );
        let t = Instant::now();
        let rows = with_ends(&keys, main.total_record_size());
        let ends = t.elapsed();
        let t = Instant::now();
        entries::write(&ent_path(dir.path(), "p"), &rows)?;
        let ent = t.elapsed();
        let t = Instant::now();
        let mut builder = IndexBuilder::new();
        for (i, (_, hw)) in keys.iter().enumerate() {
            builder.add(hw, i as u64)?;
        }
        let add = t.elapsed();
        let t = Instant::now();
        let (fst, dup) = index_paths(dir.path(), "p");
        let n = builder.write(&fst, &dup)?;
        let write = t.elapsed();
        println!(
            "keys={} distinct={n} open={open:?} read_keys={read_keys:?} ends={ends:?} ent={ent:?} add(normalize)={add:?} fst_write(sort+build)={write:?}",
            keys.len()
        );
        Ok(())
    }

    #[test]
    fn res_paths_normalize_separators() {
        assert_eq!(normalize_res_path("\\img\\Dot.png"), "/img/Dot.png");
        assert_eq!(normalize_res_path("img/dot.png"), "/img/dot.png");
    }
}
