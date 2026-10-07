//! 自定义 JSON 词典（SPEC 7.5），供词根词缀等小词典使用：
//!
//! ```json
//! { "name": "词根词缀", "category": "词源", "version": 1,
//!   "entries": [ { "headword": "ubiquitous", "html": "<p>…</p>", "aliases": ["ubiquity"] } ] }
//! ```
//!
//! 整个文件读进内存（这类词典很小）；FST 的值 = 条目下标，别名指向同一条目。

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use super::index::{DictIndex, IndexBuilder};
use super::{DictMeta, DictSource, EntryBody, RawEntry};
use crate::error::{Error, Result};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
pub struct JsonDictFile {
    pub name: String,
    #[serde(default)]
    pub category: Option<String>,
    pub version: u32,
    pub entries: Vec<JsonEntry>,
}

#[derive(Debug, Deserialize)]
pub struct JsonEntry {
    pub headword: String,
    pub html: String,
    #[serde(default)]
    pub aliases: Vec<String>,
}

/// 读取并校验 JSON 词典文件。
pub fn parse(path: &Path) -> Result<JsonDictFile> {
    let text = fs::read_to_string(path)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let file: JsonDictFile = serde_json::from_str(text)
        .map_err(|e| Error::InvalidDict(format!("{}：{e}", path.display())))?;
    if file.version != FORMAT_VERSION {
        return Err(Error::InvalidDict(format!(
            "不支持的 JSON 词典版本 {}（应为 {FORMAT_VERSION}）",
            file.version
        )));
    }
    if file.name.trim().is_empty() {
        return Err(Error::InvalidDict("词典缺少 name".into()));
    }
    if let Some(i) = file.entries.iter().position(|e| e.headword.trim().is_empty()) {
        return Err(Error::InvalidDict(format!("第 {} 条词条的 headword 为空", i + 1)));
    }
    Ok(file)
}

/// 为词头和别名建索引，写到 `fst_path` / `dup_path`。
pub fn build_index(file: &JsonDictFile, fst_path: &Path, dup_path: &Path) -> Result<usize> {
    let mut builder = IndexBuilder::new();
    for (i, entry) in file.entries.iter().enumerate() {
        let pos = i as u64;
        builder.add(&entry.headword, pos)?;
        for alias in &entry.aliases {
            builder.add(alias, pos)?;
        }
    }
    builder.write(fst_path, dup_path)
}

pub struct JsonDict {
    meta: DictMeta,
    entries: Vec<JsonEntry>,
    index: Arc<DictIndex>,
}

impl JsonDict {
    pub fn open(meta: DictMeta, path: &Path, index: Arc<DictIndex>) -> Result<Self> {
        let file = parse(path)?;
        if file.entries.len() as u64 != meta.entry_count {
            return Err(Error::InvalidDict(format!(
                "{} 自导入后已被修改（词条数 {} → {}），需要重建索引",
                path.display(),
                meta.entry_count,
                file.entries.len()
            )));
        }
        Ok(Self { meta, entries: file.entries, index })
    }
}

impl DictSource for JsonDict {
    fn meta(&self) -> &DictMeta {
        &self.meta
    }

    fn lookup(&self, key: &str) -> Result<Vec<RawEntry>> {
        self.index
            .get(key)
            .into_iter()
            .map(|pos| {
                let entry = usize::try_from(pos)
                    .ok()
                    .and_then(|i| self.entries.get(i))
                    .ok_or_else(|| Error::InvalidDict(format!("索引指向不存在的词条 {pos}")))?;
                Ok(RawEntry {
                    headword: entry.headword.clone(),
                    body: EntryBody::Html(entry.html.clone()),
                })
            })
            .collect()
    }

    fn resource(&self, _path: &str) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(json: &str) -> Result<JsonDictFile> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("d.json");
        fs::write(&path, json)?;
        parse(&path)
    }

    #[test]
    fn accepts_utf8_bom_and_missing_optional_fields() -> Result<()> {
        let f = parse_str("\u{feff}{\"name\":\"x\",\"version\":1,\"entries\":[{\"headword\":\"a\",\"html\":\"<b>a</b>\"}]}")?;
        assert_eq!(f.entries.len(), 1);
        assert!(f.category.is_none());
        assert!(f.entries[0].aliases.is_empty());
        Ok(())
    }

    #[test]
    fn rejects_bad_files_with_clear_errors() {
        let cases = [
            (r#"{"name":"x","version":2,"entries":[]}"#, "版本"),
            (r#"{"name":" ","version":1,"entries":[]}"#, "name"),
            (r#"{"name":"x","version":1,"entries":[{"headword":"","html":""}]}"#, "第 1 条"),
            (r#"{"name":"x","version":1}"#, "entries"),
        ];
        for (json, expect) in cases {
            match parse_str(json) {
                Err(Error::InvalidDict(msg)) => assert!(msg.contains(expect), "{msg}"),
                other => panic!("{json} 应报 InvalidDict，实际 {other:?}"),
            }
        }
    }
}
