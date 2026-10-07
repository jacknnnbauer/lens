//! mdx / mdd 文件结构（SPEC 7.3）：
//! `header_len(u32 BE)` + 头部（UTF-16LE）+ adler32(LE)
//! → 关键字区头 → key block info → key blocks → 记录区头 → record block info → record blocks。
//! 2.0 版整数为 8 字节，1.x 为 4 字节。

use std::collections::VecDeque;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use encoding_rs::{Encoding, UTF_16LE};
use memmap2::Mmap;

use super::block::{adler32, decompress_block, decrypt_key_block_info, zlib_decompress};
use super::header::{self, Header};
use crate::error::{Error, Result};

/// 解压后的记录块缓存个数
const BLOCK_CACHE: usize = 8;

struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self
            .b
            .get(self.pos..self.pos.saturating_add(n))
            .ok_or_else(|| Error::InvalidDict(format!("文件在位置 {} 处截断", self.pos)))?;
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u64> {
        Ok(u64::from(self.take(1)?[0]))
    }

    fn u16(&mut self) -> Result<u64> {
        let s = self.take(2)?;
        Ok(u64::from(u16::from_be_bytes([s[0], s[1]])))
    }

    fn u32(&mut self) -> Result<u64> {
        let s = self.take(4)?;
        Ok(u64::from(u32::from_be_bytes([s[0], s[1], s[2], s[3]])))
    }

    fn u64(&mut self) -> Result<u64> {
        let s = self.take(8)?;
        Ok(u64::from_be_bytes(s.try_into().map_err(|_| Error::InvalidDict("整数截断".into()))?))
    }

    fn num(&mut self, v2: bool) -> Result<u64> {
        if v2 {
            self.u64()
        } else {
            self.u32()
        }
    }

    fn usize(&mut self, v2: bool) -> Result<usize> {
        usize::try_from(self.num(v2)?).map_err(|_| Error::InvalidDict("数值超出范围".into()))
    }
}

#[derive(Debug, Clone, Copy)]
struct RecordBlock {
    file_off: usize,
    comp_size: usize,
    decomp_off: u64,
    decomp_size: usize,
}

pub struct MdxFile {
    pub path: PathBuf,
    pub header: Header,
    pub is_mdd: bool,
    data: Mmap,
    num_entries: u64,
    key_info_off: usize,
    key_info_size: usize,
    key_info_decomp_size: usize,
    key_blocks_off: usize,
    record_blocks: Vec<RecordBlock>,
    total_record_size: u64,
    cache: Mutex<VecDeque<(usize, Arc<Vec<u8>>)>>,
}

impl MdxFile {
    pub fn open(path: &Path, is_mdd: bool) -> Result<Self> {
        let file = File::open(path)?;
        // SAFETY: 词典文件只读打开；运行期间被外部改写时，所有读取都有边界检查，
        // 最坏得到 InvalidDict 错误，不会越界访问。
        let data = unsafe { Mmap::map(&file)? };
        let mut r = Reader { b: &data, pos: 0 };

        let header_len = r.usize(false)?;
        let header_bytes = r.take(header_len)?;
        let header_sum = r.take(4)?;
        if adler32(header_bytes).to_le_bytes() != header_sum {
            tracing::warn!(path = %path.display(), "mdx 头部校验和不匹配，继续解析");
        }
        let header = header::parse(header_bytes, is_mdd)?;
        if header.version >= 3.0 {
            return Err(Error::UnsupportedFormat(format!(
                "MDict {} 格式（3.0 及以上）暂不支持",
                header.version
            )));
        }
        if header.encrypted & 1 != 0 {
            return Err(Error::InvalidDict("此词典需要授权文件（注册码加密），不支持".into()));
        }
        let v2 = header.is_v2();

        // 关键字区头
        let kw_start = r.pos;
        let _num_key_blocks = r.num(v2)?;
        let num_entries = r.num(v2)?;
        let key_info_decomp_size = if v2 { r.usize(true)? } else { 0 };
        let key_info_size = r.usize(v2)?;
        let key_blocks_size = r.usize(v2)?;
        if v2 {
            let sum = r.u32()?;
            if u64::from(adler32(&data[kw_start..kw_start + 40])) != sum {
                return Err(Error::InvalidDict("关键字区头校验和不匹配".into()));
            }
        }
        let key_info_off = r.pos;
        r.take(key_info_size)?;
        let key_blocks_off = r.pos;
        r.take(key_blocks_size)?;

        // 记录区
        let num_record_blocks = r.usize(v2)?;
        let _record_entries = r.num(v2)?;
        let record_info_size = r.usize(v2)?;
        let _record_blocks_size = r.usize(v2)?;
        let width = if v2 { 16 } else { 8 };
        if record_info_size != num_record_blocks.saturating_mul(width) {
            return Err(Error::InvalidDict("record block info 长度与块数不符".into()));
        }
        let mut sizes = Vec::with_capacity(num_record_blocks);
        for _ in 0..num_record_blocks {
            sizes.push((r.usize(v2)?, r.usize(v2)?));
        }
        let mut record_blocks = Vec::with_capacity(num_record_blocks);
        let mut decomp_off = 0u64;
        for (comp_size, decomp_size) in sizes {
            record_blocks.push(RecordBlock { file_off: r.pos, comp_size, decomp_off, decomp_size });
            r.take(comp_size)?;
            decomp_off += decomp_size as u64;
        }

        Ok(Self {
            path: path.to_path_buf(),
            header,
            is_mdd,
            num_entries,
            key_info_off,
            key_info_size,
            key_info_decomp_size,
            key_blocks_off,
            record_blocks,
            total_record_size: decomp_off,
            cache: Mutex::new(VecDeque::new()),
            data,
        })
    }

    pub fn num_entries(&self) -> u64 {
        self.num_entries
    }

    pub fn total_record_size(&self) -> u64 {
        self.total_record_size
    }

    fn key_encoding(&self) -> &'static Encoding {
        if self.is_mdd {
            UTF_16LE
        } else {
            self.header.encoding
        }
    }

    /// 读出全部词头：`(记录区偏移, 词头)`，按文件顺序。只在导入时调用。
    pub fn read_keys(&self) -> Result<Vec<(u64, String)>> {
        let v2 = self.header.is_v2();
        let raw_info = &self.data[self.key_info_off..self.key_info_off + self.key_info_size];
        let info = if v2 {
            if raw_info.get(..4) != Some(&[2, 0, 0, 0][..]) {
                return Err(Error::InvalidDict("key block info 不是 zlib 压缩".into()));
            }
            let decrypted;
            let block = if self.header.encrypted & 2 != 0 {
                decrypted = decrypt_key_block_info(raw_info)?;
                &decrypted[..]
            } else {
                raw_info
            };
            let out = zlib_decompress(&block[8..], self.key_info_decomp_size)?;
            if adler32(&out).to_be_bytes() != block[4..8] {
                return Err(Error::InvalidDict("key block info 校验和不匹配（加密方式不对或文件损坏）".into()));
            }
            out
        } else {
            raw_info.to_vec()
        };

        let enc = self.key_encoding();
        let unit = if enc == UTF_16LE { 2 } else { 1 };
        let term = if v2 { 1 } else { 0 };

        // 每个 key block 的 (压缩长度, 解压长度)
        let mut blocks = Vec::new();
        let mut r = Reader { b: &info, pos: 0 };
        while r.pos < info.len() {
            r.num(v2)?;
            for _ in 0..2 {
                let len = if v2 { r.u16()? } else { r.u8()? } as usize;
                r.take((len + term) * unit)?;
            }
            blocks.push((r.usize(v2)?, r.usize(v2)?));
        }

        let mut keys = Vec::with_capacity(self.num_entries as usize);
        let mut off = self.key_blocks_off;
        for (comp, decomp) in blocks {
            let raw = self
                .data
                .get(off..off + comp)
                .ok_or_else(|| Error::InvalidDict("key block 越界".into()))?;
            let block = decompress_block(raw, decomp)?;
            off += comp;

            let mut r = Reader { b: &block, pos: 0 };
            while r.pos < block.len() {
                let id = r.num(v2)?;
                let rest = &block[r.pos..];
                let end = rest
                    .chunks(unit)
                    .position(|c| c.iter().all(|&b| b == 0))
                    .map(|i| i * unit)
                    .unwrap_or(rest.len());
                let (text, _) = enc.decode_without_bom_handling(&rest[..end]);
                keys.push((id, text.into_owned()));
                r.pos += (end + unit).min(rest.len());
            }
        }
        if keys.len() as u64 != self.num_entries {
            tracing::warn!(path = %self.path.display(), declared = self.num_entries, actual = keys.len(), "词头数与声明不一致");
        }
        Ok(keys)
    }

    /// 读记录区 `[start, end)`（解压后的全局偏移），可跨块。
    pub fn read_record(&self, start: u64, end: u64) -> Result<Vec<u8>> {
        if end < start || end > self.total_record_size {
            return Err(Error::InvalidDict(format!("记录范围越界：{start}..{end}")));
        }
        let mut out = Vec::with_capacity((end - start) as usize);
        let mut i = self.record_blocks.partition_point(|b| b.decomp_off <= start).saturating_sub(1);
        let mut pos = start;
        while pos < end {
            let b = *self
                .record_blocks
                .get(i)
                .ok_or_else(|| Error::InvalidDict("记录块越界".into()))?;
            let data = self.block(i, b)?;
            let from = (pos - b.decomp_off) as usize;
            let to = ((end - b.decomp_off) as usize).min(data.len());
            if from >= to {
                return Err(Error::InvalidDict("记录块内偏移无效".into()));
            }
            out.extend_from_slice(&data[from..to]);
            pos = b.decomp_off + to as u64;
            i += 1;
        }
        Ok(out)
    }

    fn block(&self, i: usize, b: RecordBlock) -> Result<Arc<Vec<u8>>> {
        {
            let cache = self.cache.lock().map_err(|_| Error::LockPoisoned)?;
            if let Some((_, d)) = cache.iter().find(|(ci, _)| *ci == i) {
                return Ok(d.clone());
            }
        }
        let raw = self
            .data
            .get(b.file_off..b.file_off + b.comp_size)
            .ok_or_else(|| Error::InvalidDict("记录块越界".into()))?;
        let data = Arc::new(decompress_block(raw, b.decomp_size)?);
        let mut cache = self.cache.lock().map_err(|_| Error::LockPoisoned)?;
        if cache.len() >= BLOCK_CACHE {
            cache.pop_back();
        }
        cache.push_front((i, data.clone()));
        Ok(data)
    }
}
