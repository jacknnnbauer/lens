//! PalmDB 容器：mobi 文件就是一串记录（record）。

use crate::error::Result;

pub struct PalmDb<'a> {
    /// 类型 + 创建者，例如 `BOOKMOBI`、`TEXtREAd`
    pub kind: [u8; 8],
    /// 数据库名（没有书名时兜底）
    pub name: &'a [u8],
    pub records: Vec<&'a [u8]>,
}

pub fn parse(bytes: &[u8]) -> Result<PalmDb<'_>> {
    let bad = super::corrupt_error;
    if bytes.len() < 78 {
        return Err(bad());
    }
    let mut kind = [0u8; 8];
    kind.copy_from_slice(&bytes[60..68]);
    let name_end = bytes[..32].iter().position(|&b| b == 0).unwrap_or(32);
    let n = u16::from_be_bytes([bytes[76], bytes[77]]) as usize;
    if n == 0 || bytes.len() < 78 + n * 8 {
        return Err(bad());
    }
    let offsets: Vec<usize> = (0..n).map(|i| u32::from_be_bytes(bytes[78 + i * 8..82 + i * 8].try_into().unwrap()) as usize).collect();
    let mut records = Vec::with_capacity(n);
    for i in 0..n {
        let start = offsets[i];
        let end = offsets.get(i + 1).copied().unwrap_or(bytes.len());
        if start > end || end > bytes.len() {
            return Err(bad());
        }
        records.push(&bytes[start..end]);
    }
    Ok(PalmDb { kind, name: &bytes[..name_end], records })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 按 PalmDB 格式拼出文件（测试用）
    pub fn build(kind: &[u8; 8], records: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![0u8; 78];
        out[0..4].copy_from_slice(b"test");
        out[60..68].copy_from_slice(kind);
        out[76..78].copy_from_slice(&(records.len() as u16).to_be_bytes());
        let mut off = 78 + records.len() * 8 + 2;
        for (i, r) in records.iter().enumerate() {
            out.extend_from_slice(&(off as u32).to_be_bytes());
            out.extend_from_slice(&((i as u32) << 1).to_be_bytes());
            off += r.len();
        }
        out.extend_from_slice(&[0, 0]);
        for r in records {
            out.extend_from_slice(r);
        }
        out
    }

    #[test]
    fn splits_records() -> Result<()> {
        let bytes = build(b"BOOKMOBI", &[b"abc".to_vec(), vec![], b"de".to_vec()]);
        let db = parse(&bytes)?;
        assert_eq!(&db.kind, b"BOOKMOBI");
        assert_eq!(db.name, b"test");
        assert_eq!(db.records, vec![&b"abc"[..], &b""[..], &b"de"[..]]);
        assert!(parse(&bytes[..70]).is_err());
        Ok(())
    }
}
