//! vocabulary.com 词典包的两块联网内容（词典 mdx 本身只带释义）：
//! - 真实例句：`corpus.vocabulary.com/api/1.0/examples.json`（原词条里由 Vue 组件在线拉取；Lens 不运行词典脚本，改由这里获取）
//! - 发音：词条写的是 `data-audio="T/IL1O8QHBYQNZ"`，mdd 里没有音频文件，从 `audio.vocab.com` 下载并存到本地
//!
//! 这不是公开 API，随时可能变化；失败时只影响这两块，释义照常显示。结果都缓存，离线时用缓存。

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const EXAMPLES_URL: &str = "https://corpus.vocabulary.com/api/1.0/examples.json";
const AUDIO_URL: &str = "https://audio.vocab.com/1.0/us/";
pub const PAGE_SIZE: u32 = 24;
const TIMEOUT: Duration = Duration::from_secs(10);
/// 来源筛选（与原词条里的按钮一致）：F 小说 · A 艺术文化 · N 新闻 · B 商业 · S 体育 · M 科学医学 · T 科技
pub const DOMAINS: &[&str] = &["F", "A", "N", "B", "S", "M", "T"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sentence {
    pub text: String,
    /// 词在句中的位置（字符下标，[起, 止)），用于高亮
    pub hl: Option<(usize, usize)>,
    /// 语料来源，如 Literature、The New York Times
    pub source: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub published: Option<i64>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub word: String,
    pub domain: String,
    pub offset: u32,
    pub total: u64,
    pub sentences: Vec<Sentence>,
    /// 来自缓存时的获取时间
    pub cached_at: Option<i64>,
    /// 联网失败、用的是过期缓存
    pub stale: bool,
}

/// 只认原词条筛选按钮里的那几个代码；空串 = 全部来源
pub fn normalize_domain(d: &str) -> String {
    let d = d.trim().to_ascii_uppercase();
    if DOMAINS.contains(&d.as_str()) { d } else { String::new() }
}

pub fn examples_url(word: &str, domain: &str, offset: u32) -> Result<url::Url> {
    let mut u = url::Url::parse(EXAMPLES_URL).map_err(|e| Error::Other(e.to_string()))?;
    {
        let mut q = u.query_pairs_mut();
        q.append_pair("query", word);
        q.append_pair("maxResults", &PAGE_SIZE.to_string());
        q.append_pair("startOffset", &offset.to_string());
        if !domain.is_empty() {
            q.append_pair("domain", domain);
        }
    }
    Ok(u)
}

#[derive(Deserialize)]
struct Raw {
    result: RawResult,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawResult {
    #[serde(default)]
    sentences: Vec<RawSentence>,
    #[serde(default)]
    total_hits: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSentence {
    sentence: String,
    #[serde(default)]
    offsets: Vec<usize>,
    #[serde(default)]
    volume: RawVolume,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawVolume {
    #[serde(default)]
    corpus: RawCorpus,
    title: Option<String>,
    author: Option<String>,
    date_published: Option<i64>,
    locator: Option<String>,
}

#[derive(Deserialize, Default)]
struct RawCorpus {
    #[serde(default)]
    name: String,
}

pub fn parse(body: &str) -> Result<(u64, Vec<Sentence>)> {
    let raw: Raw = serde_json::from_str(body).map_err(|e| Error::Online(crate::tf!("例句数据格式不对：{}", "Unexpected example data: {}", e)))?;
    let sentences = raw
        .result
        .sentences
        .into_iter()
        .filter(|s| !s.sentence.trim().is_empty())
        .map(|s| {
            let chars: Vec<char> = s.sentence.chars().collect();
            let hl = match s.offsets.as_slice() {
                [a, b, ..] if a < b && *b <= chars.len() => {
                    // 接口给的范围常带上紧跟的标点（serendipity, / serendipity;），高亮只到词本身
                    let mut end = *b;
                    while end > *a + 1 && !chars[end - 1].is_alphanumeric() {
                        end -= 1;
                    }
                    Some((*a, end))
                }
                _ => None,
            };
            let url = s.volume.locator.filter(|u| u.starts_with("https://") || u.starts_with("http://"));
            Sentence {
                text: s.sentence,
                hl,
                source: s.volume.corpus.name,
                title: s.volume.title.filter(|t| !t.trim().is_empty()),
                author: s.volume.author.filter(|t| !t.trim().is_empty()),
                published: s.volume.date_published,
                url,
            }
        })
        .collect();
    Ok((raw.result.total_hits, sentences))
}

pub async fn fetch(client: &reqwest::Client, word: &str, domain: &str, offset: u32) -> Result<(u64, Vec<Sentence>)> {
    let url = examples_url(word, domain, offset)?;
    let resp = client
        .get(url)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| Error::Online(crate::tf!("连不上 vocabulary.com：{}", "Could not reach vocabulary.com: {}", e)))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Error::Online(crate::tf!("vocabulary.com 返回 {}", "vocabulary.com returned {}", status.as_u16())));
    }
    let body = resp.text().await.map_err(|e| Error::Online(e.to_string()))?;
    parse(&body)
}

// —— 缓存（迁移 0005 的 vocab_examples 表）——

fn key(word: &str, domain: &str, offset: u32) -> String {
    format!("{}\u{1}{domain}\u{1}{offset}", word.to_lowercase())
}

pub fn cached(conn: &Connection, word: &str, domain: &str, offset: u32) -> Result<Option<(u64, Vec<Sentence>, i64)>> {
    let row: Option<(i64, String, i64)> = conn
        .query_row("SELECT total, payload, fetched_at FROM vocab_examples WHERE key = ?1", params![key(word, domain, offset)], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?;
    Ok(row.and_then(|(t, p, at)| serde_json::from_str(&p).ok().map(|s| (t as u64, s, at))))
}

pub fn store(conn: &Connection, word: &str, domain: &str, offset: u32, total: u64, sentences: &[Sentence], now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO vocab_examples (key, total, payload, fetched_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (key) DO UPDATE SET total = excluded.total, payload = excluded.payload, fetched_at = excluded.fetched_at",
        params![key(word, domain, offset), total as i64, serde_json::to_string(sentences)?, now],
    )?;
    Ok(())
}

// —— 发音 ——

/// `T/IL1O8QHBYQNZ.mp3` 这种 vocabulary.com 的音频路径（一个字母目录 + 10–16 位大写字母数字）
pub fn is_audio_path(path: &str) -> bool {
    let p = path.trim_start_matches(['/', '\\']).replace('\\', "/");
    let Some((dir, file)) = p.split_once('/') else { return false };
    let Some(id) = file.strip_suffix(".mp3") else { return false };
    dir.len() == 1
        && dir.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && (10..=16).contains(&id.len())
        && id.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// 本地缓存文件：`<数据目录>/media-cache/vocab/T_IL1O8QHBYQNZ.mp3`
pub fn audio_cache_path(root: &Path, path: &str) -> PathBuf {
    let p = path.trim_start_matches(['/', '\\']).replace(['\\', '/'], "_");
    root.join("media-cache").join("vocab").join(p)
}

/// 先读本地缓存，没有再下载并保存（在阻塞线程里调用）。
pub fn audio(client: &reqwest::Client, root: &Path, path: &str) -> Result<Option<Vec<u8>>> {
    if !is_audio_path(path) {
        return Ok(None);
    }
    let cache = audio_cache_path(root, path);
    if let Ok(bytes) = std::fs::read(&cache) {
        return Ok(Some(bytes));
    }
    let url = format!("{AUDIO_URL}{}", path.trim_start_matches(['/', '\\']).replace('\\', "/"));
    let bytes = tauri::async_runtime::block_on(async {
        let res = client.get(url).timeout(TIMEOUT).send().await.map_err(|e| Error::Online(crate::tf!("下载发音失败：{}", "Failed to download audio: {}", e)))?;
        if !res.status().is_success() {
            return Ok::<_, Error>(None);
        }
        Ok(Some(res.bytes().await.map_err(|e| Error::Online(e.to_string()))?.to_vec()))
    })?;
    if let Some(b) = &bytes {
        if let Some(dir) = cache.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = cache.with_extension("mp3.part");
        std::fs::write(&tmp, b)?;
        std::fs::rename(&tmp, &cache)?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    const SAMPLE: &str = r#"{"result":{"totalHits":9929,"sentences":[
        {"offsets":[17,22],"sentence":"It was the first time, we met.","volume":{"corpus":{"id":"LIT","name":"Literature"},"title":"A Book","author":"A. Writer","datePublished":1746489600000}},
        {"offsets":[99,120],"sentence":"Bad offsets are ignored.","volume":{"corpus":{"name":"News"},"locator":"https://example.com/a"}},
        {"sentence":"   ","volume":{}}
    ]}}"#;

    #[test]
    fn parses_sentences_and_highlights() -> Result<()> {
        let (total, s) = parse(SAMPLE)?;
        assert_eq!((total, s.len()), (9929, 2));
        assert_eq!(s[0].hl, Some((17, 21)));
        assert_eq!(s[0].text.chars().skip(17).take(4).collect::<String>(), "time");
        assert_eq!((s[0].source.as_str(), s[0].title.as_deref()), ("Literature", Some("A Book")));
        assert_eq!(s[1].hl, None, "越界的位置不高亮");
        assert_eq!(s[1].url.as_deref(), Some("https://example.com/a"));
        assert!(parse("<html>").is_err());
        Ok(())
    }

    #[test]
    fn builds_urls_and_domains() -> Result<()> {
        let u = examples_url("run up", "N", 24)?;
        assert_eq!(u.as_str(), "https://corpus.vocabulary.com/api/1.0/examples.json?query=run+up&maxResults=24&startOffset=24&domain=N");
        assert!(!examples_url("x", "", 0)?.as_str().contains("domain"));
        assert_eq!(normalize_domain("n"), "N");
        assert_eq!(normalize_domain("zz"), "");
        Ok(())
    }

    #[test]
    fn recognizes_audio_paths() {
        assert!(is_audio_path("T/IL1O8QHBYQNZ.mp3"));
        assert!(is_audio_path("/S/ZD9OJJH3P1XR.mp3"));
        assert!(!is_audio_path("uk/time.mp3"));
        assert!(!is_audio_path("T/il1o8qhbyqnz.mp3"));
        assert!(!is_audio_path("T/IL1O8QHBYQNZ.spx"));
        assert!(!is_audio_path("../T/IL1O8QHBYQNZ.mp3"));
    }

    #[test]
    fn caches_pages() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let (total, s) = parse(SAMPLE)?;
            store(c, "Time", "", 0, total, &s, 5)?;
            let (t, back, at) = cached(c, "time", "", 0)?.ok_or(Error::Other("miss".into()))?;
            assert_eq!((t, back.len(), at), (9929, 2, 5));
            assert!(cached(c, "time", "N", 0)?.is_none());
            Ok(())
        })
    }
}
