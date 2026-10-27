//! 全文索引（反查 + 正文搜索）：每部本地词典一个 SQLite 文件 `index/<id>.fts.db`。
//!
//! - `entry(id, headword, gloss)`：gloss 是词条里像释义的中文短语（例句译文、语体标签去掉），用「；」连起来；
//!   只有英文词头的词条才收，汉英词典的中文词头直接查词就行。
//! - `fts(gloss, body)`：FTS5 无内容表（不再存一遍原文），rowid = entry.id。
//!   分词用 unicode61；汉字两边加空格逐字成词，查中文时按短语匹配连续的字。
//!
//! 反查只查 gloss 列（中文 → 英文词）；正文搜索查两列，片段从词典里重新读出词条再截取。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;

use crate::dict::{DictSource, EntryBody};
use crate::error::Result;
use crate::gloss::{chinese_segments, is_cjk, plain_text};

const VERSION: &str = "1";
/// 每部词典反查最多取这么多条命中再排序
const REVERSE_SCAN: i64 = 3000;
/// 词条里前这么多段中文释义算主要意思
const PRIMARY_SEGMENTS: usize = 4;

pub fn index_path(dir: &Path, dict_id: &str) -> PathBuf {
    dir.join(format!("{dict_id}.fts.db"))
}

/// 汉字两边加空格，让 unicode61 把每个字当成一个词
pub fn spaced(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 2);
    for c in text.chars() {
        if is_cjk(c) {
            out.push(' ');
            out.push(c);
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

/// 用户输入 → FTS5 短语查询（标点当分隔；没有可查的字时为 None）
pub fn phrase(query: &str) -> Option<String> {
    let s = spaced(query);
    let tokens: Vec<&str> = s.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).collect();
    (!tokens.is_empty()).then(|| format!("\"{}\"", tokens.join(" ")))
}

/// 英文词头（含字母、不含汉字）：反查只收这种
fn latin_headword(hw: &str) -> bool {
    hw.chars().any(|c| c.is_ascii_alphabetic()) && !hw.chars().any(is_cjk)
}

pub fn entry_text(body: &EntryBody) -> String {
    match body {
        EntryBody::Html(h) => plain_text(h),
        EntryBody::Text(t) => t.clone(),
    }
}

/// 建索引：先写 `<id>.fts.db.tmp`，完成后改名。`cancel` 置位时停下并删掉临时文件，返回 false。
pub fn build(source: &dyn DictSource, dir: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<bool> {
    let path = index_path(dir, &source.meta().id);
    let tmp = path.with_extension("db.tmp");
    let _ = std::fs::remove_file(&tmp);
    let result = write_index(source, &tmp, cancel, progress);
    match result {
        Ok(true) => {
            std::fs::rename(&tmp, &path)?;
            Ok(true)
        }
        other => {
            let _ = std::fs::remove_file(&tmp);
            other
        }
    }
}

fn write_index(source: &dyn DictSource, tmp: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<bool> {
    let mut conn = Connection::open(tmp)?;
    conn.execute_batch(
        "PRAGMA journal_mode = OFF;
         PRAGMA synchronous = OFF;
         CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE entry (id INTEGER PRIMARY KEY, headword TEXT NOT NULL, gloss TEXT NOT NULL);
         CREATE VIRTUAL TABLE fts USING fts5(gloss, body, content = '', tokenize = 'unicode61 remove_diacritics 2');",
    )?;
    let total = source.entry_total();
    let tx = conn.transaction()?;
    let mut failed: Option<rusqlite::Error> = None;
    let mut cancelled = false;
    let mut rowid = 0i64;
    {
        let mut ins_entry = tx.prepare("INSERT INTO entry (id, headword, gloss) VALUES (?1, ?2, ?3)")?;
        let mut ins_fts = tx.prepare("INSERT INTO fts (rowid, gloss, body) VALUES (?1, ?2, ?3)")?;
        let mut last = Instant::now();
        source.for_each_entry(&mut |i, headword, body| {
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                return false;
            }
            let text = entry_text(body);
            // 个别词典的词头里带着标签（<dt>count on</dt>）
            let cleaned = if headword.contains('<') { plain_text(headword).split_whitespace().collect::<Vec<_>>().join(" ") } else { String::new() };
            let headword = if cleaned.is_empty() { headword } else { cleaned.as_str() };
            let gloss = if latin_headword(headword) { chinese_segments(&text).join("；") } else { String::new() };
            rowid += 1;
            let r = ins_entry
                .execute(params![rowid, headword, gloss])
                .and_then(|_| ins_fts.execute(params![rowid, spaced(&gloss), spaced(&text)]));
            if let Err(e) = r {
                failed = Some(e);
                return false;
            }
            if last.elapsed() > Duration::from_millis(250) {
                progress(i, total);
                last = Instant::now();
            }
            true
        })?;
    }
    if let Some(e) = failed {
        return Err(e.into());
    }
    if cancelled {
        return Ok(false);
    }
    tx.execute(
        "INSERT INTO meta (key, value) VALUES ('version', ?1), ('entries', ?2), ('built_at', ?3)",
        params![VERSION, rowid.to_string(), crate::db::now_ms().to_string()],
    )?;
    tx.commit()?;
    conn.execute_batch("INSERT INTO fts (fts) VALUES ('optimize');")?;
    progress(total, total);
    Ok(true)
}

/// 打开已建好的索引（只读）；版本不对或文件坏了返回 None
pub fn open(dir: &Path, dict_id: &str) -> Option<Connection> {
    let path = index_path(dir, dict_id);
    if !path.is_file() {
        return None;
    }
    let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    let v: Option<String> = conn.query_row("SELECT value FROM meta WHERE key = 'version'", [], |r| r.get(0)).optional().ok()?;
    (v.as_deref() == Some(VERSION)).then_some(conn)
}

/// 反查：gloss 列里含这段中文的（词头, 释义）
pub fn reverse_hits(conn: &Connection, query: &str) -> Result<Vec<(String, String)>> {
    let Some(p) = phrase(query) else { return Ok(Vec::new()) };
    let mut st = conn.prepare(
        "SELECT e.headword, e.gloss FROM fts JOIN entry e ON e.id = fts.rowid WHERE fts MATCH ?1 LIMIT ?2",
    )?;
    let rows = st.query_map(params![format!("gloss : {p}"), REVERSE_SCAN], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// 正文搜索：命中总数和这一页的词头（按相关度）
pub fn search(conn: &Connection, query: &str, limit: u32, offset: u32) -> Result<(u64, Vec<String>)> {
    let Some(p) = phrase(query) else { return Ok((0, Vec::new())) };
    let total: i64 = conn.query_row("SELECT count(*) FROM fts WHERE fts MATCH ?1", [&p], |r| r.get(0))?;
    let mut st = conn.prepare(
        "SELECT e.headword FROM fts JOIN entry e ON e.id = fts.rowid WHERE fts MATCH ?1 ORDER BY rank LIMIT ?2 OFFSET ?3",
    )?;
    let rows = st.query_map(params![p, limit, offset], |r| r.get(0))?;
    Ok((total.max(0) as u64, rows.collect::<std::result::Result<_, _>>()?))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseWord {
    pub word: String,
    /// 和查询最贴近的那段释义
    pub gloss: String,
    /// 3 = 释义就是这个词（去掉括注和「的」）；2 = 以它开头或结尾；1 = 包含
    pub fit: u8,
    /// 这段释义是不是词条里靠前的几个意思（不是 man 的「勇敢的」这种次要义项）
    #[serde(skip)]
    primary: bool,
    /// 收了这条释义的词典名
    pub dicts: Vec<String>,
    /// 词频排名（COCA；没导入时用内置词频表）
    pub rank: Option<u32>,
    pub pos: Option<String>,
}

/// 去掉括注和结尾的「的 / 地」：「（使）生锈的」→「生锈」
fn core(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '（' | '(' => depth += 1,
            '）' | ')' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().trim_end_matches(['的', '地']).to_string()
}

fn fit(segment: &str, query: &str) -> u8 {
    let (s, q) = (core(segment), core(query));
    if q.is_empty() || !segment.contains(query.trim()) && !s.contains(&q) {
        0
    } else if s == q {
        3
    } else if s.starts_with(&q) || s.ends_with(&q) {
        2
    } else {
        1
    }
}

/// 词头归一：小写，去掉同形异义编号（bank1、rust²）
pub fn word_key(headword: &str) -> String {
    headword
        .trim()
        .trim_end_matches(|c: char| c.is_ascii_digit() || matches!(c, '¹' | '²' | '³' | '⁴' | '⁵' | '*' | ' '))
        .to_lowercase()
}

/// 同样贴合时的分数：几部词典收了这条释义、是不是主要义项、常用度，三者相加。
/// 常用词的次要义项（abandon 的「放弃」在第 7 段）还能排上来，但 man 的「勇敢的」排不过 brave
fn score(w: &ReverseWord) -> usize {
    let common = match w.rank {
        Some(r) if r <= 3000 => 2,
        Some(r) if r <= 8000 => 1,
        _ => 0,
    };
    w.dicts.len().min(3) * 2 + usize::from(w.primary) * 2 + common
}

/// 汇总各词典的反查命中并排序：贴合度 → 分数（词典数、主要义项、常用度）→ 排名 → 单词先于短语 → 短的在前
pub fn rank_reverse(query: &str, hits: &[(String, Vec<(String, String)>)], coca_rank: &dyn Fn(&str) -> Option<u32>, limit: usize) -> Vec<ReverseWord> {
    let mut by_word: HashMap<String, ReverseWord> = HashMap::new();
    for (dict, rows) in hits {
        for (headword, gloss) in rows {
            let best = gloss
                .split('；')
                .enumerate()
                .map(|(i, seg)| (fit(seg, query), i, seg))
                .filter(|(f, _, _)| *f > 0)
                .max_by_key(|(f, i, seg)| (*f, std::cmp::Reverse(*i), std::cmp::Reverse(seg.chars().count())));
            let Some((f, i, seg)) = best else { continue };
            let primary = i < PRIMARY_SEGMENTS;
            let key = word_key(headword);
            if key.is_empty() {
                continue;
            }
            let w = by_word.entry(key.clone()).or_insert_with(|| ReverseWord {
                word: key.clone(),
                gloss: seg.to_string(),
                fit: f,
                primary,
                dicts: Vec::new(),
                rank: coca_rank(&key),
                pos: None,
            });
            if (f, primary) > (w.fit, w.primary) {
                w.fit = f;
                w.primary = primary;
                w.gloss = seg.to_string();
            }
            if !w.dicts.contains(dict) {
                w.dicts.push(dict.clone());
            }
        }
    }
    let mut words: Vec<ReverseWord> = by_word.into_values().collect();
    words.sort_by(|a, b| {
        b.fit
            .cmp(&a.fit)
            .then(score(b).cmp(&score(a)))
            .then(a.rank.unwrap_or(u32::MAX).cmp(&b.rank.unwrap_or(u32::MAX)))
            .then(a.word.contains(' ').cmp(&b.word.contains(' ')))
            .then(a.word.len().cmp(&b.word.len()))
            .then(a.word.cmp(&b.word))
    });
    words.truncate(limit);
    words
}

/// 正文里命中的一段：前文 / 命中 / 后文
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snippet {
    pub before: String,
    pub hit: String,
    pub after: String,
}

/// 在纯文本里找查询（忽略大小写、标点和空白的差别），截出前后各约 `ctx` 个字符
pub fn snippet(text: &str, query: &str, ctx: usize) -> Option<Snippet> {
    // 折叠：字母数字小写，其余连续的字符折成一个空格；记下每个折叠字符在原文里的字符下标
    let fold = |s: &str| {
        let mut out: Vec<(char, usize)> = Vec::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_alphanumeric() {
                out.extend(c.to_lowercase().map(|l| (l, i)));
            } else if out.last().is_some_and(|(p, _)| *p != ' ') {
                out.push((' ', i));
            }
        }
        out
    };
    let t = fold(text);
    let q: Vec<char> = fold(query).into_iter().map(|(c, _)| c).collect();
    let q: Vec<char> = q.iter().copied().skip_while(|c| *c == ' ').collect::<Vec<_>>();
    let q = &q[..q.iter().rposition(|c| *c != ' ')? + 1];
    let ascii_start = q.first().is_some_and(char::is_ascii_alphanumeric);
    let pos = (0..t.len().saturating_sub(q.len() - 1)).find(|&i| {
        // 英文要从词首开始匹配（查 rust 不命中 trust）
        (!ascii_start || i == 0 || !t[i - 1].0.is_alphanumeric()) && t[i..i + q.len()].iter().map(|(c, _)| *c).eq(q.iter().copied())
    })?;
    let chars: Vec<char> = text.chars().collect();
    let (s, e) = (t[pos].1, t[pos + q.len() - 1].1 + 1);
    let from = s.saturating_sub(ctx);
    let to = (e + ctx).min(chars.len());
    let clean = |v: &[char]| v.iter().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let mut before = clean(&chars[from..s]);
    if from > 0 {
        before = format!("…{before}");
    }
    if s > from && chars[s - 1].is_whitespace() {
        before.push(' ');
    }
    let mut after = clean(&chars[e..to]);
    if e < to && chars[e].is_whitespace() {
        after.insert(0, ' ');
    }
    if to < chars.len() {
        after.push('…');
    }
    Some(Snippet { before, hit: chars[s..e].iter().collect(), after })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict::{DictKind, DictMeta, RawEntry};

    struct Fake {
        meta: DictMeta,
        entries: Vec<(&'static str, &'static str)>,
    }

    impl DictSource for Fake {
        fn meta(&self) -> &DictMeta {
            &self.meta
        }
        fn lookup(&self, key: &str) -> Result<Vec<RawEntry>> {
            Ok(self
                .entries
                .iter()
                .filter(|(h, _)| *h == key)
                .map(|(h, b)| RawEntry { headword: h.to_string(), body: EntryBody::Html(b.to_string()) })
                .collect())
        }
        fn resource(&self, _: &str) -> Result<Option<Vec<u8>>> {
            Ok(None)
        }
        fn for_each_entry(&self, f: &mut dyn FnMut(u64, &str, &EntryBody) -> bool) -> Result<()> {
            for (i, (h, b)) in self.entries.iter().enumerate() {
                if !f(i as u64, h, &EntryBody::Html(b.to_string())) {
                    break;
                }
            }
            Ok(())
        }
    }

    fn fake() -> Fake {
        Fake {
            meta: DictMeta { id: "d1".into(), name: "双解".into(), kind: DictKind::Json, category: None, entry_count: 4 },
            entries: vec![
                ("ubiquitous", "<b>adj.</b> seeming to be everywhere <i>无所不在的；普遍存在的</i> Mobile phones are ubiquitous. 手机随处可见。"),
                ("prevalent", "<b>adj.</b> that exists or is very common 流行的；普遍的"),
                ("rust", "<b>n.</b> 锈；铁锈 <i>take for granted</i> 认为…理所当然"),
                ("普遍", "universal; general"),
            ],
        }
    }

    #[test]
    fn phrase_spaces_chinese_and_drops_punctuation() {
        assert_eq!(phrase("普遍存在").as_deref(), Some("\"普 遍 存 在\""));
        assert_eq!(phrase("take for \"granted\"!").as_deref(), Some("\"take for granted\""));
        assert_eq!(phrase(" ，。"), None);
    }

    #[test]
    fn build_then_reverse_and_search() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut seen = 0;
        assert!(build(&fake(), dir.path(), &AtomicBool::new(false), &mut |d, _| seen = d)?);
        assert_eq!(seen, 4);
        let conn = open(dir.path(), "d1").ok_or_else(|| crate::error::Error::Other("no index".into()))?;

        let hits = reverse_hits(&conn, "普遍")?;
        let words: Vec<&str> = hits.iter().map(|(h, _)| h.as_str()).collect();
        assert!(words.contains(&"ubiquitous") && words.contains(&"prevalent"));
        assert!(!words.contains(&"普遍"), "中文词头不进反查");
        // 例句译文不进反查
        assert!(reverse_hits(&conn, "随处可见")?.is_empty());

        let (total, heads) = search(&conn, "Take for granted", 10, 0)?;
        assert_eq!((total, heads), (1, vec!["rust".to_string()]));
        // 正文搜索也能搜中文（包括例句译文）
        assert_eq!(search(&conn, "随处可见", 10, 0)?.0, 1);
        Ok(())
    }

    #[test]
    fn cancelled_build_leaves_nothing() -> Result<()> {
        let dir = tempfile::tempdir()?;
        assert!(!build(&fake(), dir.path(), &AtomicBool::new(true), &mut |_, _| {})?);
        assert!(open(dir.path(), "d1").is_none());
        assert_eq!(std::fs::read_dir(dir.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn reverse_ranks_exact_glosses_first() {
        let hits = vec![
            ("A".to_string(), vec![
                ("pervasive".to_string(), "遍布的；普遍存在的（尤指不好的东西）".to_string()),
                ("ubiquitous".to_string(), "无所不在的；普遍存在的".to_string()),
                ("common".to_string(), "共同的；共有的；公共的；一般的；常见的；普遍存在的".to_string()),
                ("universe".to_string(), "宇宙；普遍存在的事物的总体".to_string()),
            ]),
            ("B".to_string(), vec![("Ubiquitous1".to_string(), "普遍存在的".to_string())]),
        ];
        let rank = |w: &str| match w {
            "pervasive" => Some(9000),
            _ => None,
        };
        let r = rank_reverse("普遍存在", &hits, &rank, 10);
        let words: Vec<&str> = r.iter().map(|w| w.word.as_str()).collect();
        assert_eq!(words, vec!["ubiquitous", "pervasive", "common", "universe"]);
        assert_eq!(r[0].dicts, vec!["A", "B"]);
        assert_eq!((r[0].fit, r[0].gloss.as_str()), (3, "普遍存在的"));
        assert_eq!(r[3].fit, 2);
    }

    #[test]
    fn snippet_finds_phrase_across_punctuation_and_case() {
        let s = snippet("He takes it for granted, as always. Never Take-for granted things.", "take for granted", 15);
        assert_eq!(s, Some(Snippet { before: "…always. Never ".into(), hit: "Take-for granted".into(), after: " things.".into() }));
        assert!(snippet("entrust me", "rust", 5).is_none(), "英文从词首匹配");
        let z = snippet("锈；铁锈 rust", "铁锈", 2);
        assert_eq!(z.map(|s| (s.before, s.hit)), Some(("锈；".into(), "铁锈".into())));
    }
}

/// 真实词典：建索引的耗时、体积和反查效果。
/// `LENS_DATA_DIR=<沙盒数据目录> LENS_FT_OUT=<输出目录> cargo test --release real_index -- --ignored --nocapture`
#[cfg(test)]
mod real {
    use super::*;

    #[test]
    #[ignore]
    fn real_index() -> Result<()> {
        let root = PathBuf::from(std::env::var("LENS_DATA_DIR").unwrap_or_default());
        let out = PathBuf::from(std::env::var("LENS_FT_OUT").unwrap_or_default());
        let only = std::env::var("LENS_FT_ONLY").unwrap_or_default();
        let db = crate::db::Db::open(&root.join("lens.db"))?;
        let rows = db.with_conn(crate::db::dicts::list)?;
        let mut conns = Vec::new();
        for row in rows.iter().filter(|r| r.kind != "online" && (only.is_empty() || r.name.contains(&only))) {
            let d = crate::dict::open_dict(row, &root.join("index"))?;
            let t = Instant::now();
            if !open(&out, &row.id).is_some() {
                build(d.source.as_ref(), &out, &AtomicBool::new(false), &mut |_, _| {})?;
            }
            let size = std::fs::metadata(index_path(&out, &row.id))?.len();
            println!("{}: {} 条, {:.1}s, {:.1} MB", row.name, row.entry_count.unwrap_or(0), t.elapsed().as_secs_f32(), size as f64 / 1e6);
            if let Some(c) = open(&out, &row.id) {
                conns.push((row.name.clone(), c));
            }
        }
        for q in ["普遍存在", "生锈", "放弃", "固执", "尴尬", "认为理所当然", "勇敢"] {
            let t = Instant::now();
            let hits: Vec<(String, Vec<(String, String)>)> =
                conns.iter().map(|(n, c)| Ok((n.clone(), reverse_hits(c, q)?))).collect::<Result<_>>()?;
            let r = rank_reverse(q, &hits, &|_| None, 12);
            println!("{q} ({} ms): {}", t.elapsed().as_millis(), r.iter().map(|w| format!("{}[{}·{}·{}]", w.word, w.gloss, w.fit, w.dicts.len())).collect::<Vec<_>>().join("  "));
        }
        for q in ["take for granted", "out of the blue", "随处可见"] {
            let t = Instant::now();
            for (n, c) in &conns {
                let (total, heads) = search(c, q, 5, 0)?;
                if total > 0 {
                    println!("  [{q}] {n}: {total} → {}", heads.join(", "));
                }
            }
            println!("[{q}] {} ms", t.elapsed().as_millis());
        }
        Ok(())
    }
}
