//! HUFF/CDIC 压缩（亚马逊商店的书常用）

use crate::error::Result;

pub struct HuffCdic {
    /// 按码字最高 8 位查：码长、是否到此为止、该码长的最大码
    dict1: Vec<(u32, bool, u64)>,
    mincode: [u64; 33],
    maxcode: [u64; 33],
    /// 词条：内容，是否已经是展开好的
    dict: Vec<(Vec<u8>, bool)>,
    depth: u32,
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes(s.try_into().unwrap()))
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes(s.try_into().unwrap()))
}

impl HuffCdic {
    /// `huff` 是 HUFF 记录，`cdics` 是紧跟着的 CDIC 记录
    pub fn new(huff: &[u8], cdics: &[&[u8]]) -> Result<Self> {
        let bad = super::corrupt_error;
        if !huff.starts_with(b"HUFF\x00\x00\x00\x18") {
            return Err(bad());
        }
        let off1 = be32(huff, 8).ok_or_else(bad)? as usize;
        let off2 = be32(huff, 12).ok_or_else(bad)? as usize;
        let mut dict1 = Vec::with_capacity(256);
        for i in 0..256 {
            let v = be32(huff, off1 + i * 4).ok_or_else(bad)?;
            let codelen = v & 0x1F;
            if codelen == 0 {
                return Err(bad());
            }
            let maxcode = ((u64::from(v >> 8) + 1) << (32 - codelen)) - 1;
            dict1.push((codelen, v & 0x80 != 0, maxcode));
        }
        let mut mincode = [0u64; 33];
        let mut maxcode = [0u64; 33];
        maxcode[0] = (1u64 << 32) - 1;
        for len in 1..=32usize {
            let lo = be32(huff, off2 + (len - 1) * 8).ok_or_else(bad)?;
            let hi = be32(huff, off2 + (len - 1) * 8 + 4).ok_or_else(bad)?;
            mincode[len] = u64::from(lo) << (32 - len);
            maxcode[len] = ((u64::from(hi) + 1) << (32 - len)).wrapping_sub(1);
        }
        let mut dict = Vec::new();
        for cdic in cdics {
            if !cdic.starts_with(b"CDIC\x00\x00\x00\x10") {
                return Err(bad());
            }
            let phrases = be32(cdic, 8).ok_or_else(bad)? as usize;
            let bits = be32(cdic, 12).ok_or_else(bad)?.min(31);
            let n = (1usize << bits).min(phrases.saturating_sub(dict.len()));
            for i in 0..n {
                let off = be16(cdic, 16 + i * 2).ok_or_else(bad)? as usize;
                let blen = be16(cdic, 16 + off).ok_or_else(bad)?;
                let len = usize::from(blen & 0x7FFF);
                let slice = cdic.get(18 + off..18 + off + len).ok_or_else(bad)?;
                dict.push((slice.to_vec(), blen & 0x8000 != 0));
            }
        }
        Ok(HuffCdic { dict1, mincode, maxcode, dict, depth: 0 })
    }

    pub fn unpack(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let bad = super::corrupt_error;
        if self.depth > 32 {
            return Err(bad());
        }
        let mut padded = data.to_vec();
        padded.extend_from_slice(&[0; 8]);
        let word = |pos: usize| u64::from_be_bytes(padded[pos..pos + 8].try_into().unwrap());
        let mut bitsleft = data.len() as i64 * 8;
        let mut pos = 0usize;
        let mut x = word(0);
        let mut n: i64 = 32;
        let mut out = Vec::new();
        loop {
            if n <= 0 {
                pos += 4;
                if pos + 8 > padded.len() {
                    break;
                }
                x = word(pos);
                n += 32;
            }
            let code = (x >> n) & 0xFFFF_FFFF;
            let (mut codelen, term, mut maxcode) = self.dict1[(code >> 24) as usize];
            if !term {
                while codelen < 32 && code < self.mincode[codelen as usize] {
                    codelen += 1;
                }
                maxcode = self.maxcode[codelen as usize];
            }
            n -= i64::from(codelen);
            bitsleft -= i64::from(codelen);
            if bitsleft < 0 {
                break;
            }
            let r = (maxcode.checked_sub(code).ok_or_else(bad)? >> (32 - codelen)) as usize;
            let (slice, done) = self.dict.get(r).cloned().ok_or_else(bad)?;
            if done {
                out.extend_from_slice(&slice);
            } else {
                // 词条本身也是压缩的：展开一次存回去
                self.depth += 1;
                let expanded = self.unpack(&slice);
                self.depth -= 1;
                let expanded = expanded?;
                out.extend_from_slice(&expanded);
                self.dict[r] = (expanded, true);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 所有码长都是 1：高位 1 → 词条 0，高位 0 → 词条 1
    fn tables(entries: &[(&[u8], bool)]) -> (Vec<u8>, Vec<u8>) {
        let mut huff = b"HUFF\x00\x00\x00\x18".to_vec();
        huff.extend_from_slice(&24u32.to_be_bytes());
        huff.extend_from_slice(&(24u32 + 1024).to_be_bytes());
        huff.extend_from_slice(&[0; 8]); // 头 24 字节
        for _ in 0..256 {
            // codelen 1、term、maxcode 1
            huff.extend_from_slice(&((1u32 << 8) | 0x80 | 1).to_be_bytes());
        }
        for i in 0..32 {
            let (min, max) = if i == 0 { (0u32, 1u32) } else { (u32::MAX, 0) };
            huff.extend_from_slice(&min.to_be_bytes());
            huff.extend_from_slice(&max.to_be_bytes());
        }
        let mut cdic = b"CDIC\x00\x00\x00\x10".to_vec();
        cdic.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        cdic.extend_from_slice(&1u32.to_be_bytes());
        let mut offsets = Vec::new();
        let mut blob = Vec::new();
        let base = entries.len() * 2;
        for (bytes, literal) in entries {
            offsets.extend_from_slice(&((base + blob.len()) as u16).to_be_bytes());
            let flag = if *literal { 0x8000u16 } else { 0 };
            blob.extend_from_slice(&(bytes.len() as u16 | flag).to_be_bytes());
            blob.extend_from_slice(bytes);
        }
        cdic.extend_from_slice(&offsets);
        cdic.extend_from_slice(&blob);
        (huff, cdic)
    }

    #[test]
    fn unpacks_one_bit_codes() -> Result<()> {
        let (huff, cdic) = tables(&[(b"ab", true), (b"c", true)]);
        let mut h = HuffCdic::new(&huff, &[&cdic])?;
        // 1010 0000 → ab c ab c c c c c
        assert_eq!(h.unpack(&[0xA0])?, b"abcabccccc");
        Ok(())
    }

    #[test]
    fn expands_nested_entries_once() -> Result<()> {
        // 词条 1 本身是压缩过的：0xFF → 8 个 "ab"
        let (huff, cdic) = tables(&[(b"ab", true), (&[0xFF], false)]);
        let mut h = HuffCdic::new(&huff, &[&cdic])?;
        let want = b"ab".repeat(8);
        assert_eq!(h.unpack(&[0x7F])?, [want.clone(), b"ab".repeat(7)].concat());
        Ok(())
    }
}
