//! dictzip（`.dict.dz`）随机访问：gzip 头的 FEXTRA 里有 `RA` 子字段，记录每块的压缩长度；
//! 块之间做过 Z_FULL_FLUSH，所以每块都能用新的 raw inflate 单独解压。

use std::sync::{Arc, Mutex};

use flate2::{Decompress, FlushDecompress, Status};
use memmap2::Mmap;

use crate::error::{Error, Result};

const FTEXT_MASK: u8 = 0x01;
const FHCRC: u8 = 0x02;
const FEXTRA: u8 = 0x04;
const FNAME: u8 = 0x08;
const FCOMMENT: u8 = 0x10;

pub struct DictZip {
    data: Mmap,
    chunk_len: usize,
    /// 每块在文件中的起止字节
    chunks: Vec<(usize, usize)>,
    /// 最近一次解压的块（查一个词通常只落在 1–2 块里）
    cache: Mutex<Option<(usize, Arc<Vec<u8>>)>>,
}

/// (块长度, 每块在文件中的起止字节)
type ChunkTable = (usize, Vec<(usize, usize)>);

/// gzip 头解析结果：`None` 表示是普通 gzip（没有 RA 子字段）。
fn parse_header(b: &[u8]) -> Result<Option<ChunkTable>> {
    let bad = |m: &str| Error::InvalidDict(format!(".dict.dz 头部损坏：{m}"));
    if b.len() < 10 || b[0] != 0x1f || b[1] != 0x8b || b[2] != 8 {
        return Err(bad("不是 gzip 文件"));
    }
    let flg = b[3] & !FTEXT_MASK;
    let mut pos = 10;
    let u16_at = |p: usize| -> Result<usize> {
        b.get(p..p + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize).ok_or_else(|| bad("长度不足"))
    };

    let mut ra = None;
    if flg & FEXTRA != 0 {
        let xlen = u16_at(pos)?;
        pos += 2;
        let extra = b.get(pos..pos + xlen).ok_or_else(|| bad("FEXTRA 越界"))?;
        let mut p = 0;
        while p + 4 <= extra.len() {
            let (si1, si2) = (extra[p], extra[p + 1]);
            let len = u16::from_le_bytes([extra[p + 2], extra[p + 3]]) as usize;
            let body = extra.get(p + 4..p + 4 + len).ok_or_else(|| bad("子字段越界"))?;
            if si1 == b'R' && si2 == b'A' {
                ra = Some(body);
            }
            p += 4 + len;
        }
        pos += xlen;
    }
    for flag in [FNAME, FCOMMENT] {
        if flg & flag != 0 {
            let rest = b.get(pos..).ok_or_else(|| bad("头部越界"))?;
            let end = rest.iter().position(|&c| c == 0).ok_or_else(|| bad("文件名未结束"))?;
            pos += end + 1;
        }
    }
    if flg & FHCRC != 0 {
        pos += 2;
    }

    let Some(ra) = ra else { return Ok(None) };
    let word = |i: usize| -> Result<usize> {
        ra.get(i * 2..i * 2 + 2).map(|s| u16::from_le_bytes([s[0], s[1]]) as usize).ok_or_else(|| bad("RA 越界"))
    };
    if word(0)? != 1 {
        return Err(bad("RA 版本不是 1"));
    }
    let chunk_len = word(1)?;
    let count = word(2)?;
    if chunk_len == 0 {
        return Err(bad("块长度为 0"));
    }
    let mut chunks = Vec::with_capacity(count);
    for i in 0..count {
        let size = word(3 + i)?;
        chunks.push((pos, pos + size));
        pos += size;
    }
    if pos > b.len() {
        return Err(bad("块表超出文件长度"));
    }
    Ok(Some((chunk_len, chunks)))
}

impl DictZip {
    /// 没有 RA 子字段时返回 `Ok(None)`，调用方改用整体解压。
    pub fn open(data: Mmap) -> Result<Option<Self>> {
        Ok(parse_header(&data)?.map(|(chunk_len, chunks)| Self {
            data,
            chunk_len,
            chunks,
            cache: Mutex::new(None),
        }))
    }

    pub fn read(&self, offset: u64, size: u32) -> Result<Vec<u8>> {
        let start = usize::try_from(offset).map_err(|_| out_of_range(offset, size))?;
        let end = start + size as usize;
        let mut out = Vec::with_capacity(size as usize);
        if size == 0 {
            return Ok(out);
        }
        for c in start / self.chunk_len..=(end - 1) / self.chunk_len {
            let chunk = self.chunk(c)?;
            let base = c * self.chunk_len;
            let from = start.max(base) - base;
            let to = (end.min(base + self.chunk_len) - base).min(chunk.len());
            if from >= to {
                return Err(out_of_range(offset, size));
            }
            out.extend_from_slice(&chunk[from..to]);
        }
        if out.len() != size as usize {
            return Err(out_of_range(offset, size));
        }
        Ok(out)
    }

    fn chunk(&self, i: usize) -> Result<Arc<Vec<u8>>> {
        if let Some((ci, data)) = &*self.cache.lock().map_err(|_| Error::LockPoisoned)? {
            if *ci == i {
                return Ok(data.clone());
            }
        }
        let &(from, to) = self
            .chunks
            .get(i)
            .ok_or_else(|| Error::InvalidDict(format!("dictzip 块号 {i} 超出范围")))?;
        let mut out = vec![0u8; self.chunk_len];
        let mut inflate = Decompress::new(false);
        let status = inflate
            .decompress(&self.data[from..to], &mut out, FlushDecompress::Sync)
            .map_err(|e| Error::InvalidDict(format!("dictzip 第 {i} 块解压失败：{e}")))?;
        if !matches!(status, Status::Ok | Status::StreamEnd | Status::BufError) {
            return Err(Error::InvalidDict(format!("dictzip 第 {i} 块解压状态异常")));
        }
        out.truncate(inflate.total_out() as usize);
        let out = Arc::new(out);
        *self.cache.lock().map_err(|_| Error::LockPoisoned)? = Some((i, out.clone()));
        Ok(out)
    }
}

fn out_of_range(offset: u64, size: u32) -> Error {
    Error::InvalidDict(format!("词条数据越界：offset={offset} size={size}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_gzip_has_no_ra() -> Result<()> {
        use std::io::Write;
        let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(b"hello")?;
        let gz = enc.finish()?;
        assert!(parse_header(&gz)?.is_none());
        Ok(())
    }

    #[test]
    fn rejects_non_gzip() {
        assert!(parse_header(b"PK\x03\x04 not gzip").is_err());
    }
}
