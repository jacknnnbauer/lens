//! `.ifo` 元信息文件。

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::error::{Error, Result};

const MAGIC: &str = "StarDict's dict ifo file";

#[derive(Debug, Clone, PartialEq)]
pub struct Ifo {
    pub version: String,
    pub bookname: String,
    pub wordcount: u64,
    pub synwordcount: Option<u64>,
    pub idxfilesize: Option<u64>,
    /// `.idx` 里 offset 字段的宽度：32 或 64（仅 3.0.0 可为 64）
    pub idx_offset_bits: u8,
    pub sametypesequence: Option<String>,
    pub description: Option<String>,
}

pub fn parse(path: &Path) -> Result<Ifo> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let bad = |msg: &str| Error::InvalidDict(format!("{}：{msg}", path.display()));

    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some(MAGIC) {
        return Err(bad("不是 StarDict .ifo 文件"));
    }
    let fields: HashMap<&str, &str> = lines
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim_end_matches('\r')))
        .collect();
    let get = |k: &str| fields.get(k).copied().filter(|v| !v.is_empty());
    let num = |k: &str| -> Result<Option<u64>> {
        get(k)
            .map(|v| v.trim().parse::<u64>().map_err(|_| bad(&format!("{k} 不是数字：{v}"))))
            .transpose()
    };

    let version = get("version").ok_or_else(|| bad("缺少 version"))?.trim().to_string();
    if version != "2.4.2" && version != "3.0.0" {
        return Err(bad(&format!("不支持的版本 {version}")));
    }
    let idx_offset_bits = match num("idxoffsetbits")? {
        None | Some(32) => 32,
        Some(64) if version == "3.0.0" => 64,
        Some(other) => return Err(bad(&format!("idxoffsetbits={other} 无效"))),
    };

    Ok(Ifo {
        bookname: get("bookname").ok_or_else(|| bad("缺少 bookname"))?.to_string(),
        wordcount: num("wordcount")?.ok_or_else(|| bad("缺少 wordcount"))?,
        synwordcount: num("synwordcount")?,
        idxfilesize: num("idxfilesize")?,
        idx_offset_bits,
        sametypesequence: get("sametypesequence").map(|s| s.trim().to_string()),
        description: get("description").map(str::to_string),
        version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(s: &str) -> Result<Ifo> {
        let dir = tempfile::tempdir()?;
        let p = dir.path().join("d.ifo");
        fs::write(&p, s)?;
        parse(&p)
    }

    #[test]
    fn parses_fields_with_crlf_and_bom() -> Result<()> {
        let ifo = parse_str(
            "\u{feff}StarDict's dict ifo file\r\nversion=3.0.0\r\nbookname=Test = Book\r\nwordcount=12\r\nidxoffsetbits=64\r\nsametypesequence=h\r\n",
        )?;
        assert_eq!(ifo.bookname, "Test = Book");
        assert_eq!(ifo.wordcount, 12);
        assert_eq!(ifo.idx_offset_bits, 64);
        assert_eq!(ifo.sametypesequence.as_deref(), Some("h"));
        Ok(())
    }

    #[test]
    fn rejects_invalid_headers() {
        for s in [
            "not an ifo\nversion=2.4.2\nbookname=x\nwordcount=1",
            "StarDict's dict ifo file\nversion=1.0\nbookname=x\nwordcount=1",
            "StarDict's dict ifo file\nversion=2.4.2\nbookname=x",
            "StarDict's dict ifo file\nversion=2.4.2\nbookname=x\nwordcount=1\nidxoffsetbits=64",
        ] {
            assert!(matches!(parse_str(s), Err(Error::InvalidDict(_))), "{s}");
        }
    }
}
