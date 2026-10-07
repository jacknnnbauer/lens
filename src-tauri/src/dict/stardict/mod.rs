//! StarDict（SPEC 7.4）：`.ifo` + `.idx`/`.idx.gz` + `.dict`/`.dict.dz` + 可选 `.syn`。
//!
//! FST 的值 = 词条在 `.idx` 里的字节位置；从那里读出原始词头和 offset/size。
//! 这样 `.syn` 的同义词命中时也能显示原词头。`.idx.gz` 导入时解压到 `index/<id>.idx`。
//! `.idx` 与 `.dict` 都 mmap，`.dict.dz` 按 dictzip 块随机解压。

pub mod dictzip;
pub mod ifo;

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use memmap2::Mmap;

use super::index::{DictIndex, IndexBuilder};
use super::{DictMeta, DictSource, EntryBody, RawEntry};
use crate::error::{Error, Result};
use dictzip::DictZip;
use ifo::Ifo;

struct Files {
    idx: PathBuf,
    idx_gz: bool,
    dict: PathBuf,
    dict_dz: bool,
    syn: Option<PathBuf>,
}

fn locate(ifo_path: &Path) -> Result<Files> {
    let base = ifo_path.with_extension("");
    let with = |ext: &str| PathBuf::from(format!("{}.{ext}", base.display()));
    let pick = |plain: &str, packed: &str, what: &str| -> Result<(PathBuf, bool)> {
        let (p, z) = (with(plain), with(packed));
        if p.is_file() {
            Ok((p, false))
        } else if z.is_file() {
            Ok((z, true))
        } else {
            Err(Error::InvalidDict(format!("找不到 {what}：{} 或 {}", p.display(), z.display())))
        }
    };
    let (idx, idx_gz) = pick("idx", "idx.gz", ".idx")?;
    let (dict, dict_dz) = pick("dict", "dict.dz", ".dict")?;
    let syn = Some(with("syn")).filter(|p| p.is_file());
    Ok(Files { idx, idx_gz, dict, dict_dz, syn })
}

/// 解压后的 `.idx` 缓存位置（仅 `.idx.gz` 词典使用）。
pub fn idx_cache_path(index_dir: &Path, dict_id: &str) -> PathBuf {
    index_dir.join(format!("{dict_id}.idx"))
}

fn read_maybe_gz(path: &Path, gz: bool) -> Result<Vec<u8>> {
    let raw = fs::read(path)?;
    if !gz {
        return Ok(raw);
    }
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(&raw[..])
        .read_to_end(&mut out)
        .map_err(|e| Error::InvalidDict(format!("{} 解压失败：{e}", path.display())))?;
    Ok(out)
}

/// `.idx` 里一条记录：`word\0` + offset（4/8 字节 BE）+ size（4 字节 BE）。
fn read_idx_record(idx: &[u8], pos: usize, offset_bits: u8) -> Result<(&[u8], u64, u32, usize)> {
    let bad = || Error::InvalidDict(format!(".idx 在位置 {pos} 处损坏"));
    let rest = idx.get(pos..).ok_or_else(bad)?;
    let nul = rest.iter().position(|&b| b == 0).ok_or_else(bad)?;
    let mut p = pos + nul + 1;
    let offset = if offset_bits == 64 {
        let b = idx.get(p..p + 8).ok_or_else(bad)?;
        p += 8;
        u64::from_be_bytes(b.try_into().map_err(|_| bad())?)
    } else {
        let b = idx.get(p..p + 4).ok_or_else(bad)?;
        p += 4;
        u64::from(u32::from_be_bytes(b.try_into().map_err(|_| bad())?))
    };
    let b = idx.get(p..p + 4).ok_or_else(bad)?;
    let size = u32::from_be_bytes(b.try_into().map_err(|_| bad())?);
    Ok((&rest[..nul], offset, size, p + 4))
}

fn parse_syn(bytes: &[u8]) -> Result<Vec<(String, u32)>> {
    let mut out = Vec::new();
    let mut p = 0;
    while p < bytes.len() {
        let bad = || Error::InvalidDict(format!(".syn 在位置 {p} 处损坏"));
        let nul = bytes[p..].iter().position(|&b| b == 0).ok_or_else(bad)?;
        let word = String::from_utf8_lossy(&bytes[p..p + nul]).into_owned();
        let n = bytes.get(p + nul + 1..p + nul + 5).ok_or_else(bad)?;
        out.push((word, u32::from_be_bytes(n.try_into().map_err(|_| bad())?)));
        p += nul + 5;
    }
    Ok(out)
}

/// 导入前的解析结果：已校验，可直接建索引。
pub struct Prepared {
    pub ifo: Ifo,
    files: Files,
    idx: Vec<u8>,
    /// (词头, 在 .idx 中的位置)
    entries: Vec<(String, u64)>,
    syn: Vec<(String, u32)>,
}

impl Prepared {
    pub fn entry_count(&self) -> u64 {
        self.entries.len() as u64
    }
}

pub fn prepare(ifo_path: &Path) -> Result<Prepared> {
    let ifo = ifo::parse(ifo_path)?;
    let files = locate(ifo_path)?;
    let idx = read_maybe_gz(&files.idx, files.idx_gz)?;

    let mut entries = Vec::new();
    let mut pos = 0;
    while pos < idx.len() {
        let (word, _, _, next) = read_idx_record(&idx, pos, ifo.idx_offset_bits)?;
        entries.push((String::from_utf8_lossy(word).into_owned(), pos as u64));
        pos = next;
    }
    if entries.len() as u64 != ifo.wordcount {
        tracing::warn!(declared = ifo.wordcount, actual = entries.len(), "StarDict wordcount 与 .idx 不一致，以 .idx 为准");
    }
    let syn = match &files.syn {
        Some(p) => parse_syn(&fs::read(p)?)?,
        None => Vec::new(),
    };
    Ok(Prepared { ifo, files, idx, entries, syn })
}

pub fn build_index(
    p: &Prepared,
    index_dir: &Path,
    dict_id: &str,
    fst_path: &Path,
    dup_path: &Path,
) -> Result<usize> {
    if p.files.idx_gz {
        let cache = idx_cache_path(index_dir, dict_id);
        let tmp = cache.with_extension("idx.tmp");
        fs::write(&tmp, &p.idx)?;
        fs::rename(&tmp, &cache)?;
    }
    let mut builder = IndexBuilder::new();
    for (word, pos) in &p.entries {
        builder.add(word, *pos)?;
    }
    for (word, ordinal) in &p.syn {
        let (_, pos) = p.entries.get(*ordinal as usize).ok_or_else(|| {
            Error::InvalidDict(format!(".syn 中的 {word} 指向不存在的词条 #{ordinal}"))
        })?;
        builder.add(word, *pos)?;
    }
    builder.write(fst_path, dup_path)
}

enum DictData {
    Plain(Mmap),
    Zip(DictZip),
    /// 没有 RA 块表的普通 gzip，只能整体解压
    Memory(Vec<u8>),
}

impl DictData {
    fn read(&self, offset: u64, size: u32) -> Result<Vec<u8>> {
        let slice = |b: &[u8]| {
            usize::try_from(offset)
                .ok()
                .and_then(|s| b.get(s..s + size as usize))
                .map(<[u8]>::to_vec)
                .ok_or_else(|| Error::InvalidDict(format!("词条数据越界：offset={offset} size={size}")))
        };
        match self {
            Self::Plain(m) => slice(m),
            Self::Memory(v) => slice(v),
            Self::Zip(z) => z.read(offset, size),
        }
    }
}

fn mmap(path: &Path) -> Result<Mmap> {
    let file = File::open(path)?;
    // SAFETY: 词典文件只读打开；用户在运行期间改写词典文件属于外部破坏，最坏读到错误数据，
    // 由各处的边界检查转为 InvalidDict，不会越界访问。
    Ok(unsafe { Mmap::map(&file)? })
}

pub struct StarDict {
    meta: DictMeta,
    idx: Mmap,
    offset_bits: u8,
    sametypesequence: Option<Vec<u8>>,
    data: DictData,
    index: Arc<DictIndex>,
}

impl StarDict {
    pub fn open(
        meta: DictMeta,
        ifo_path: &Path,
        index_dir: &Path,
        index: Arc<DictIndex>,
    ) -> Result<Self> {
        let ifo = ifo::parse(ifo_path)?;
        let files = locate(ifo_path)?;
        let idx_path = if files.idx_gz { idx_cache_path(index_dir, &meta.id) } else { files.idx };
        let idx = mmap(&idx_path)?;

        let data = if files.dict_dz {
            match DictZip::open(mmap(&files.dict)?)? {
                Some(z) => DictData::Zip(z),
                None => {
                    tracing::warn!(dict = %meta.name, "dict.dz 没有 dictzip 块表，整体解压到内存");
                    DictData::Memory(read_maybe_gz(&files.dict, true)?)
                }
            }
        } else {
            DictData::Plain(mmap(&files.dict)?)
        };

        Ok(Self {
            sametypesequence: ifo.sametypesequence.map(String::into_bytes),
            offset_bits: ifo.idx_offset_bits,
            meta,
            idx,
            data,
            index,
        })
    }
}

impl DictSource for StarDict {
    fn meta(&self) -> &DictMeta {
        &self.meta
    }

    fn lookup(&self, key: &str) -> Result<Vec<RawEntry>> {
        let mut out = Vec::new();
        for pos in self.index.get(key) {
            let pos = usize::try_from(pos).map_err(|_| Error::InvalidDict(format!("索引位置无效 {pos}")))?;
            let (word, offset, size, _) = read_idx_record(&self.idx, pos, self.offset_bits)?;
            let data = self.data.read(offset, size)?;
            if let Some(body) = decode(&data, self.sametypesequence.as_deref())? {
                out.push(RawEntry { headword: String::from_utf8_lossy(word).into_owned(), body });
            }
        }
        Ok(out)
    }

    fn resource(&self, _path: &str) -> Result<Option<Vec<u8>>> {
        // StarDict 的 res/ 目录资源在 M2 资源协议里再接
        Ok(None)
    }
}

/// 按类型字符拆字段。小写类型以 `\0` 结尾，大写类型前有 4 字节 BE 长度；
/// 有 sametypesequence 时省略类型字符，且最后一个字段占到词条末尾（没有结束符/长度）。
fn split_fields<'a>(data: &'a [u8], sts: Option<&[u8]>) -> Result<Vec<(u8, &'a [u8])>> {
    let bad = || Error::InvalidDict("词条字段结构损坏".into());
    let take = |rest: &'a [u8], t: u8, last: bool| -> Result<(&'a [u8], &'a [u8])> {
        if last {
            return Ok((rest, &[]));
        }
        if t.is_ascii_lowercase() {
            match rest.iter().position(|&b| b == 0) {
                Some(n) => Ok((&rest[..n], &rest[n + 1..])),
                None => Ok((rest, &[])),
            }
        } else {
            let len = rest.get(..4).ok_or_else(bad)?;
            let len = u32::from_be_bytes(len.try_into().map_err(|_| bad())?) as usize;
            let body = rest.get(4..4 + len).ok_or_else(bad)?;
            Ok((body, &rest[4 + len..]))
        }
    };

    let mut fields = Vec::new();
    let mut rest = data;
    match sts {
        Some(types) => {
            for (i, &t) in types.iter().enumerate() {
                let (field, r) = take(rest, t, i + 1 == types.len())?;
                fields.push((t, field));
                rest = r;
            }
        }
        None => {
            while let Some((&t, r)) = rest.split_first() {
                let (field, r) = take(r, t, false)?;
                fields.push((t, field));
                rest = r;
            }
        }
    }
    Ok(fields)
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\n', "<br>")
}

/// 把字段合成一个词条正文。h/g 按 HTML；其余小写类型按纯文本（t 音标加方括号）；
/// 大写二进制字段（音频/图片）跳过。全部跳过时返回 None。
fn decode(data: &[u8], sts: Option<&[u8]>) -> Result<Option<EntryBody>> {
    enum Part {
        Html(String),
        Text(String),
    }
    let parts: Vec<Part> = split_fields(data, sts)?
        .into_iter()
        .filter_map(|(t, f)| {
            let s = || String::from_utf8_lossy(f).trim_end_matches('\0').to_string();
            match t {
                b'h' | b'g' => Some(Part::Html(s())),
                b't' => Some(Part::Text(format!("[{}]", s()))),
                t if t.is_ascii_lowercase() => Some(Part::Text(s())),
                _ => None,
            }
        })
        .collect();

    if parts.is_empty() {
        return Ok(None);
    }
    if parts.iter().any(|p| matches!(p, Part::Html(_))) {
        let html = parts
            .into_iter()
            .map(|p| match p {
                Part::Html(h) => h,
                Part::Text(t) => format!("<div>{}</div>", escape_html(&t)),
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Some(EntryBody::Html(html)))
    } else {
        let text = parts
            .into_iter()
            .filter_map(|p| match p {
                Part::Text(t) => Some(t),
                Part::Html(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Some(EntryBody::Text(text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sametypesequence_last_field_runs_to_end() -> Result<()> {
        let body = decode(b"ran\0v. \xe8\xb7\x91", Some(b"tm"))?;
        assert_eq!(body, Some(EntryBody::Text("[ran]\nv. 跑".into())));
        Ok(())
    }

    #[test]
    fn typed_fields_mix_html_text_and_skip_binary() -> Result<()> {
        let mut data = b"h<b>x</b>\0ma & b\0W".to_vec();
        data.extend_from_slice(&4u32.to_be_bytes());
        data.extend_from_slice(b"RIFF");
        let body = decode(&data, None)?;
        assert_eq!(body, Some(EntryBody::Html("<b>x</b>\n<div>a &amp; b</div>".into())));

        let mut only_audio = b"W".to_vec();
        only_audio.extend_from_slice(&4u32.to_be_bytes());
        only_audio.extend_from_slice(b"RIFF");
        assert_eq!(decode(&only_audio, None)?, None);
        Ok(())
    }

    #[test]
    fn truncated_binary_field_is_an_error() {
        assert!(decode(b"W\0\0\0\x09RIFF", None).is_err());
    }

    #[test]
    fn idx_record_bounds_are_checked() {
        assert!(read_idx_record(b"word\0\0\0", 0, 32).is_err());
        assert!(read_idx_record(b"word", 0, 32).is_err());
    }
}
