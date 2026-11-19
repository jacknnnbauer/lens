//! INDX 索引（目录 NCX、KF8 的骨架和片段表）

use std::collections::BTreeMap;

use crate::error::Result;

/// 一条索引：名字和各个标签的值
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub ident: String,
    pub tags: BTreeMap<u8, Vec<u32>>,
}

impl Entry {
    pub fn tag(&self, t: u8, i: usize) -> Option<u32> {
        self.tags.get(&t).and_then(|v| v.get(i)).copied()
    }
}

/// 读从 `records[start]` 开始的索引；返回各条目和标签里引用的字符串表（按偏移查）
pub fn read(records: &[&[u8]], start: usize, decode: &dyn Fn(&[u8]) -> String) -> Result<(Vec<Entry>, BTreeMap<u32, String>)> {
    let bad = super::corrupt_error;
    let hdr = *records.get(start).ok_or_else(bad)?;
    if !hdr.starts_with(b"INDX") {
        return Err(bad());
    }
    let u = |b: &[u8], at: usize| b.get(at..at + 4).map(|s| u32::from_be_bytes(s.try_into().unwrap())).ok_or_else(bad);
    let header_len = u(hdr, 4)? as usize;
    let count = u(hdr, 24)? as usize;
    let ncncx = u(hdr, 52)? as usize;
    let tagx = hdr.get(header_len..).ok_or_else(bad)?;
    if !tagx.starts_with(b"TAGX") {
        return Err(bad());
    }
    let tagx_len = u(tagx, 4)? as usize;
    let control_bytes = u(tagx, 8)? as usize;
    let mut tags: Vec<[u8; 4]> = Vec::new();
    let mut at = 12;
    while at + 4 <= tagx_len.min(tagx.len()) {
        tags.push(tagx[at..at + 4].try_into().unwrap());
        at += 4;
    }

    let mut cncx = BTreeMap::new();
    for k in 0..ncncx {
        let Some(raw) = records.get(start + count + 1 + k) else { break };
        let mut pos = 0;
        while pos < raw.len() {
            let (len, used) = decint(&raw[pos..]);
            if used == 0 {
                break;
            }
            let from = pos + used;
            let to = (from + len as usize).min(raw.len());
            if len > 0 {
                cncx.insert((k as u32) * 0x10000 + pos as u32, decode(&raw[from..to]));
            }
            pos = to.max(pos + 1);
        }
    }

    let mut entries = Vec::new();
    for i in start + 1..=start + count {
        let d = *records.get(i).ok_or_else(bad)?;
        if !d.starts_with(b"INDX") {
            return Err(bad());
        }
        let idxt = u(d, 20)? as usize;
        let n = u(d, 24)? as usize;
        let mut positions = Vec::with_capacity(n + 1);
        for j in 0..n {
            let p = d.get(idxt + 4 + j * 2..idxt + 6 + j * 2).ok_or_else(bad)?;
            positions.push(u16::from_be_bytes([p[0], p[1]]) as usize);
        }
        positions.push(idxt);
        for j in 0..n {
            let rec = d.get(positions[j]..positions[j + 1]).ok_or_else(bad)?;
            let len = *rec.first().ok_or_else(bad)? as usize;
            let ident = decode(rec.get(1..1 + len).ok_or_else(bad)?);
            let tags = tag_map(control_bytes, &tags, rec.get(1 + len..).ok_or_else(bad)?);
            entries.push(Entry { ident, tags });
        }
    }
    Ok((entries, cncx))
}

/// 按 TAGX 解出一条索引里的各个标签值
fn tag_map(control_count: usize, tagx: &[[u8; 4]], data: &[u8]) -> BTreeMap<u8, Vec<u32>> {
    let mut controls: Vec<u8> = data.iter().take(control_count).copied().collect();
    let mut data = data.get(control_count..).unwrap_or_default();
    // （标签, 值的个数, 值占的字节数, 每项几个值）
    let mut present: Vec<(u8, Option<u32>, Option<u32>, u8)> = Vec::new();
    for &[tag, per_entry, mask, eof] in tagx {
        if eof == 1 {
            if !controls.is_empty() {
                controls.remove(0);
            }
            continue;
        }
        let Some(&cb) = controls.first() else { break };
        let mut value = cb & mask;
        if value == 0 {
            continue;
        }
        if value == mask {
            if mask.count_ones() > 1 {
                let (bytes, used) = decint(data);
                data = &data[used.min(data.len())..];
                present.push((tag, None, Some(bytes), per_entry));
            } else {
                present.push((tag, Some(1), None, per_entry));
            }
        } else {
            let mut m = mask;
            while m & 1 == 0 {
                m >>= 1;
                value >>= 1;
            }
            present.push((tag, Some(u32::from(value)), None, per_entry));
        }
    }
    let mut out = BTreeMap::new();
    for (tag, count, bytes, per_entry) in present {
        let mut values = Vec::new();
        if let Some(c) = count {
            for _ in 0..c * u32::from(per_entry) {
                let (v, used) = decint(data);
                if used == 0 {
                    break;
                }
                data = &data[used..];
                values.push(v);
            }
        } else if let Some(total) = bytes {
            let mut consumed = 0u32;
            while consumed < total {
                let (v, used) = decint(data);
                if used == 0 {
                    break;
                }
                data = &data[used..];
                consumed += used as u32;
                values.push(v);
            }
        }
        out.insert(tag, values);
    }
    out
}

/// 向前读变长整数（高位为 1 的字节结束）
pub fn decint(data: &[u8]) -> (u32, usize) {
    let mut v: u32 = 0;
    for (i, &b) in data.iter().enumerate() {
        v = (v << 7) | u32::from(b & 0x7F);
        if b & 0x80 != 0 || i == 4 {
            return (v, i + 1);
        }
    }
    (v, data.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decint_reads_forward() {
        assert_eq!(decint(&[0x81]), (1, 1));
        assert_eq!(decint(&[0x01, 0x80]), (128, 2));
        assert_eq!(decint(&[0x00, 0x00, 0x85, 0x99]), (5, 3));
    }
}
