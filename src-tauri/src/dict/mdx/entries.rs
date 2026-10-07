//! 词条表 `<id>.ent`：FST 的值是这里的序号。
//!
//! 格式：`LENSENT1` + count(u64 LE) + count × [rec_start u64, rec_end u64, hw_off u32, hw_len u32]
//! + 词头 UTF-8 拼接区。mmap 读取。

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::error::{Error, Result};

const MAGIC: &[u8; 8] = b"LENSENT1";
const ROW: usize = 24;
const HEAD: usize = 16;

pub fn ent_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.ent"))
}

pub struct EntryRow<'a> {
    pub start: u64,
    pub end: u64,
    pub headword: &'a str,
}

/// rows: (rec_start, rec_end, headword)
pub fn write(path: &Path, rows: &[(u64, u64, &str)]) -> Result<()> {
    let tmp = path.with_extension("ent.tmp");
    let mut w = BufWriter::new(File::create(&tmp)?);
    w.write_all(MAGIC)?;
    w.write_all(&(rows.len() as u64).to_le_bytes())?;
    let mut off = 0u64;
    for (start, end, hw) in rows {
        let (o, l) = (u32::try_from(off), u32::try_from(hw.len()));
        let (Ok(o), Ok(l)) = (o, l) else {
            return Err(Error::InvalidDict("词头总长度超过 4GB".into()));
        };
        w.write_all(&start.to_le_bytes())?;
        w.write_all(&end.to_le_bytes())?;
        w.write_all(&o.to_le_bytes())?;
        w.write_all(&l.to_le_bytes())?;
        off += hw.len() as u64;
    }
    for (_, _, hw) in rows {
        w.write_all(hw.as_bytes())?;
    }
    w.flush()?;
    drop(w);
    fs::rename(&tmp, path)?;
    Ok(())
}

pub struct EntryTable {
    data: Mmap,
    count: usize,
}

impl EntryTable {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        // SAFETY: .ent 由本程序生成，只写新文件后改名，不原地修改已映射的文件。
        let data = unsafe { Mmap::map(&file)? };
        let bad = || Error::InvalidDict(format!("词条表损坏：{}", path.display()));
        if data.get(..8) != Some(&MAGIC[..]) {
            return Err(bad());
        }
        let count = u64::from_le_bytes(data[8..16].try_into().map_err(|_| bad())?);
        let count = usize::try_from(count).map_err(|_| bad())?;
        if data.len() < HEAD + count.saturating_mul(ROW) {
            return Err(bad());
        }
        Ok(Self { data, count })
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn get(&self, i: u64) -> Result<EntryRow<'_>> {
        let bad = || Error::InvalidDict(format!("词条序号 {i} 越界"));
        let i = usize::try_from(i).map_err(|_| bad())?;
        if i >= self.count {
            return Err(bad());
        }
        let row = &self.data[HEAD + i * ROW..HEAD + (i + 1) * ROW];
        let u64_at = |o: usize| u64::from_le_bytes(row[o..o + 8].try_into().unwrap_or_default());
        let u32_at = |o: usize| u32::from_le_bytes(row[o..o + 4].try_into().unwrap_or_default()) as usize;
        let blob = HEAD + self.count * ROW;
        let (o, l) = (blob + u32_at(16), u32_at(20));
        let hw = self.data.get(o..o + l).ok_or_else(bad)?;
        Ok(EntryRow {
            start: u64_at(0),
            end: u64_at(8),
            headword: std::str::from_utf8(hw).map_err(|_| bad())?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = ent_path(dir.path(), "t");
        write(&p, &[(0, 10, "Polish"), (10, 25, "café"), (25, 25, "")])?;
        let t = EntryTable::open(&p)?;
        assert_eq!(t.len(), 3);
        let e = t.get(1)?;
        assert_eq!((e.start, e.end, e.headword), (10, 25, "café"));
        assert_eq!(t.get(2)?.headword, "");
        assert!(t.get(3).is_err());
        Ok(())
    }
}
