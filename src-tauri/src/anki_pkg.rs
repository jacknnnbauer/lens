//! Anki 牌组包（.apkg）：zip 里放 `collection.anki2`（Anki 2.1 旧版集合格式，schema 11，各版本 Anki 都能导入）、
//! `media`（序号 → 文件名的 JSON）和以序号命名的媒体文件。
//!
//! 笔记类型「词镜 Lens」自带两种卡片：识记（单词 → 释义）、语境（挖空的原句 → 单词）。
//! 笔记的 guid 由单词算出，同一个词再导出一次，Anki 导入时更新原来的笔记而不是新增一张。

use std::io::Write;
use std::path::Path;

use flate2::write::DeflateEncoder;
use flate2::Compression;
use rusqlite::{params, Connection};
use serde_json::json;

use crate::error::Result;

/// 笔记类型 id 固定：每次导出都是同一个笔记类型
const MODEL_ID: i64 = 1_729_640_000_123;

pub const FIELDS: [&str; 9] = ["Word", "Pronunciation", "POS", "Meaning", "Sentence", "Cloze", "Source", "MoreContexts", "Added"];

pub struct Note {
    pub word: String,
    /// `[sound:…]`，没有发音时为空
    pub pronunciation: String,
    pub pos: String,
    pub meaning: String,
    pub sentence: String,
    /// 单词挖空的原句；没有语境（或定位不到单词）时为空，不生成语境卡
    pub cloze: String,
    pub source: String,
    pub more: String,
    pub added: String,
    pub tags: Vec<String>,
}

pub struct Media {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cards {
    pub recall: bool,
    pub context: bool,
}

// ── SHA-1（笔记的 csum、guid 用；不为这一处引入依赖）──

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, word) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// 去掉 HTML 标签（Anki 的排序字段、校验和用纯文本）
fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

/// 牌组 id：由牌组名算出，同名牌组每次导出都落到同一个牌组
fn deck_id(name: &str) -> i64 {
    let d = sha1(name.as_bytes());
    1_500_000_000_000 + (u64::from_be_bytes([0, 0, 0, d[0], d[1], d[2], d[3], d[4]]) % 100_000_000_000) as i64
}

const CSS: &str = r#".card { font-family: "Segoe UI", "Microsoft YaHei", "PingFang SC", sans-serif; font-size: 18px; line-height: 1.6; color: #1f2328; background: #fbfaf7; text-align: center; padding: 24px 16px; }
.nightMode .card, .card.nightMode { color: #e6e6e6; background: #1e1f22; }
.word { font-size: 34px; font-weight: 600; letter-spacing: .01em; }
.audio { margin-top: 6px; }
.meaning { font-size: 20px; margin: 12px auto; max-width: 36em; }
.pos { font-style: italic; opacity: .65; margin-right: .3em; }
.sentence { font-size: 19px; margin: 14px auto; max-width: 36em; text-align: left; }
.sentence b { color: #b4532a; }
.nightMode .sentence b { color: #f0a070; }
.blank { display: inline-block; min-width: 4em; border-bottom: 2px solid currentColor; }
.hint { font-size: 16px; opacity: .7; }
.source { font-size: 13px; opacity: .6; }
.source a { color: inherit; }
.more { font-size: 15px; text-align: left; max-width: 36em; margin: 12px auto; opacity: .85; }
hr#answer { border: 0; border-top: 1px solid rgba(127,127,127,.3); margin: 18px 0; }"#;

fn templates(more_label: &str) -> serde_json::Value {
    let meaning = r#"<div class="meaning">{{#POS}}<span class="pos">{{POS}}</span>{{/POS}}{{Meaning}}</div>"#;
    let source = r#"{{#Source}}<div class="source">{{Source}}</div>{{/Source}}"#;
    let recall_q = r#"<div class="word">{{Word}}</div><div class="audio">{{Pronunciation}}</div>"#;
    let recall_a = format!(
        r#"{{{{FrontSide}}}}<hr id="answer">{meaning}{{{{#Sentence}}}}<div class="sentence">{{{{Sentence}}}}</div>{{{{/Sentence}}}}{source}{{{{#MoreContexts}}}}<details class="more"><summary>{more_label}</summary>{{{{MoreContexts}}}}</details>{{{{/MoreContexts}}}}"#
    );
    let context_q = r#"{{#Cloze}}<div class="sentence">{{Cloze}}</div>{{#Meaning}}<div class="hint">{{Meaning}}</div>{{/Meaning}}{{/Cloze}}"#;
    let context_a =
        format!(r#"<div class="sentence">{{{{Sentence}}}}</div><hr id="answer"><div class="word">{{{{Word}}}}</div><div class="audio">{{{{Pronunciation}}}}</div>{meaning}{source}"#);
    json!([
        { "name": crate::i18n::t("识记", "Recall"), "ord": 0, "qfmt": recall_q, "afmt": recall_a, "bqfmt": "", "bafmt": "", "did": null, "bfont": "", "bsize": 0 },
        { "name": crate::i18n::t("语境", "Context"), "ord": 1, "qfmt": context_q, "afmt": context_a, "bqfmt": "", "bafmt": "", "did": null, "bfont": "", "bsize": 0 },
    ])
}

/// 写 `collection.anki2`
pub fn write_collection(path: &Path, deck: &str, notes: &[Note], cards: Cards, now_ms: i64) -> Result<usize> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE col (id integer primary key, crt integer not null, mod integer not null, scm integer not null, ver integer not null,
           dty integer not null, usn integer not null, ls integer not null, conf text not null, models text not null,
           decks text not null, dconf text not null, tags text not null);
         CREATE TABLE notes (id integer primary key, guid text not null, mid integer not null, mod integer not null, usn integer not null,
           tags text not null, flds text not null, sfld integer not null, csum integer not null, flags integer not null, data text not null);
         CREATE TABLE cards (id integer primary key, nid integer not null, did integer not null, ord integer not null, mod integer not null,
           usn integer not null, type integer not null, queue integer not null, due integer not null, ivl integer not null,
           factor integer not null, reps integer not null, lapses integer not null, left integer not null, odue integer not null,
           odid integer not null, flags integer not null, data text not null);
         CREATE TABLE revlog (id integer primary key, cid integer not null, usn integer not null, ease integer not null, ivl integer not null,
           lastIvl integer not null, factor integer not null, time integer not null, type integer not null);
         CREATE TABLE graves (usn integer not null, oid integer not null, type integer not null);
         CREATE INDEX ix_notes_usn on notes (usn);
         CREATE INDEX ix_cards_usn on cards (usn);
         CREATE INDEX ix_revlog_usn on revlog (usn);
         CREATE INDEX ix_cards_nid on cards (nid);
         CREATE INDEX ix_cards_sched on cards (did, queue, due);
         CREATE INDEX ix_revlog_cid on revlog (cid);
         CREATE INDEX ix_notes_csum on notes (csum);",
    )?;
    let secs = now_ms / 1000;
    let did = deck_id(deck);
    let flds: Vec<serde_json::Value> = FIELDS
        .iter()
        .enumerate()
        .map(|(i, n)| json!({ "name": n, "ord": i, "sticky": false, "rtl": false, "font": "Arial", "size": 20, "media": [] }))
        .collect();
    let models = json!({ MODEL_ID.to_string(): {
        "id": MODEL_ID, "name": crate::i18n::t("词镜 Lens", "Lens"), "type": 0, "mod": secs, "usn": -1, "sortf": 0, "did": did,
        "tmpls": templates(crate::i18n::t("更多语境", "More contexts")), "flds": flds, "css": CSS,
        "latexPre": "\\documentclass[12pt]{article}\n\\special{papersize=3in,5in}\n\\usepackage[utf8]{inputenc}\n\\usepackage{amssymb,amsmath}\n\\pagestyle{empty}\n\\setlength{\\parindent}{0in}\n\\begin{document}\n",
        "latexPost": "\\end{document}", "latexsvg": false,
        "req": [[0, "any", [0]], [1, "any", [5]]], "tags": [], "vers": []
    }});
    let deck_obj = |id: i64, name: &str| {
        json!({ "id": id, "name": name, "mod": secs, "usn": -1, "lrnToday": [0, 0], "revToday": [0, 0], "newToday": [0, 0],
                "timeToday": [0, 0], "collapsed": false, "browserCollapsed": false, "desc": "", "dyn": 0, "conf": 1,
                "extendNew": 0, "extendRev": 50 })
    };
    let decks = json!({ "1": deck_obj(1, "Default"), did.to_string(): deck_obj(did, deck) });
    let dconf = json!({ "1": {
        "id": 1, "name": "Default", "mod": 0, "usn": 0, "maxTaken": 60, "autoplay": true, "timer": 0, "replayq": true, "dyn": false,
        "new": { "bury": true, "delays": [1.0, 10.0], "initialFactor": 2500, "ints": [1, 4, 0], "order": 1, "perDay": 20, "separate": true },
        "lapse": { "delays": [10.0], "leechAction": 1, "leechFails": 8, "minInt": 1, "mult": 0.0 },
        "rev": { "bury": true, "ease4": 1.3, "fuzz": 0.05, "ivlFct": 1.0, "maxIvl": 36500, "minSpace": 1, "perDay": 200 }
    }});
    let conf = json!({ "activeDecks": [did], "curDeck": did, "newSpread": 0, "collapseTime": 1200, "timeLim": 0, "estTimes": true,
                       "dueCounts": true, "curModel": MODEL_ID.to_string(), "nextPos": notes.len() + 1, "sortType": "noteFld",
                       "sortBackwards": false, "addToCur": true });
    conn.execute(
        "INSERT INTO col VALUES (1, ?1, ?2, ?2, 11, 0, 0, 0, ?3, ?4, ?5, ?6, '{}')",
        params![secs - secs % 86_400, now_ms, conf.to_string(), models.to_string(), decks.to_string(), dconf.to_string()],
    )?;
    let mut n_cards = 0;
    let tx = conn.unchecked_transaction()?;
    for (i, n) in notes.iter().enumerate() {
        let nid = now_ms * 10 + i as i64;
        let fields = [&n.word, &n.pronunciation, &n.pos, &n.meaning, &n.sentence, &n.cloze, &n.source, &n.more, &n.added];
        let flds = fields.iter().map(|f| f.replace('\x1f', " ")).collect::<Vec<_>>().join("\x1f");
        let sfld = strip_html(&n.word);
        let digest = sha1(sfld.as_bytes());
        let csum = i64::from(u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]));
        let guid = format!("lens-{}", &hex(&sha1(format!("lens:{}", n.word.to_lowercase()).as_bytes()))[..16]);
        let tags = if n.tags.is_empty() { String::new() } else { format!(" {} ", n.tags.join(" ")) };
        tx.execute(
            "INSERT INTO notes VALUES (?1, ?2, ?3, ?4, -1, ?5, ?6, ?7, ?8, 0, '')",
            params![nid, guid, MODEL_ID, secs, tags, flds, sfld, csum],
        )?;
        for (ord, on) in [(0, cards.recall), (1, cards.context && !n.cloze.is_empty())] {
            if on {
                tx.execute(
                    "INSERT INTO cards VALUES (?1, ?2, ?3, ?4, ?5, -1, 0, 0, ?6, 0, 0, 0, 0, 0, 0, 0, 0, '')",
                    params![nid * 2 + ord, nid, did, ord, secs, i as i64 + 1],
                )?;
                n_cards += 1;
            }
        }
    }
    tx.commit()?;
    Ok(n_cards)
}

// ── zip（只写，deflate；不为这一处引入依赖）──

struct ZipEntry {
    name: String,
    crc: u32,
    size: u32,
    comp: u32,
    method: u16,
    offset: u32,
}

fn zip(out: &Path, files: &[(String, Vec<u8>)]) -> Result<()> {
    let too_big = || crate::error::Error::Other(crate::i18n::t("牌组包超过 4 GB", "The deck package is over 4 GB").into());
    let mut buf: Vec<u8> = Vec::new();
    let mut entries = Vec::new();
    for (name, data) in files {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data)?;
        let deflated = enc.finish()?;
        // 已经压缩过的（mp3 等）存原样
        let (method, body) = if deflated.len() < data.len() { (8u16, deflated) } else { (0u16, data.clone()) };
        let e = ZipEntry {
            name: name.clone(),
            crc: crc.sum(),
            size: u32::try_from(data.len()).map_err(|_| too_big())?,
            comp: u32::try_from(body.len()).map_err(|_| too_big())?,
            method,
            offset: u32::try_from(buf.len()).map_err(|_| too_big())?,
        };
        buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        buf.extend_from_slice(&0x0800u16.to_le_bytes()); // 文件名 UTF-8
        buf.extend_from_slice(&e.method.to_le_bytes());
        buf.extend_from_slice(&[0, 0, 0x21, 0]); // 时间、日期（1980-01-01）
        buf.extend_from_slice(&e.crc.to_le_bytes());
        buf.extend_from_slice(&e.comp.to_le_bytes());
        buf.extend_from_slice(&e.size.to_le_bytes());
        buf.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(e.name.as_bytes());
        buf.extend_from_slice(&body);
        entries.push(e);
    }
    let cd_start = u32::try_from(buf.len()).map_err(|_| too_big())?;
    for e in &entries {
        buf.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        buf.extend_from_slice(&20u16.to_le_bytes()); // version made by
        buf.extend_from_slice(&20u16.to_le_bytes());
        buf.extend_from_slice(&0x0800u16.to_le_bytes());
        buf.extend_from_slice(&e.method.to_le_bytes());
        buf.extend_from_slice(&[0, 0, 0x21, 0]);
        buf.extend_from_slice(&e.crc.to_le_bytes());
        buf.extend_from_slice(&e.comp.to_le_bytes());
        buf.extend_from_slice(&e.size.to_le_bytes());
        buf.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
        buf.extend_from_slice(&[0; 12]); // extra、comment 长度，磁盘号，内部 / 外部属性
        buf.extend_from_slice(&e.offset.to_le_bytes());
        buf.extend_from_slice(e.name.as_bytes());
    }
    let cd_len = u32::try_from(buf.len()).map_err(|_| too_big())? - cd_start;
    buf.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    buf.extend_from_slice(&[0; 4]);
    buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    buf.extend_from_slice(&cd_len.to_le_bytes());
    buf.extend_from_slice(&cd_start.to_le_bytes());
    buf.extend_from_slice(&0u16.to_le_bytes());
    let tmp = out.with_extension("apkg.tmp");
    std::fs::write(&tmp, buf)?;
    std::fs::rename(&tmp, out)?;
    Ok(())
}

/// 写 .apkg，返回卡片数
pub fn write(out: &Path, deck: &str, notes: &[Note], media: &[Media], cards: Cards, now_ms: i64) -> Result<usize> {
    let dir = tempfile_dir(out)?;
    let col = dir.join("collection.anki2");
    let n = write_collection(&col, deck, notes, cards, now_ms)?;
    let mut files = vec![("collection.anki2".to_string(), std::fs::read(&col)?)];
    let _ = std::fs::remove_dir_all(&dir);
    let map: serde_json::Map<String, serde_json::Value> = media.iter().enumerate().map(|(i, m)| (i.to_string(), json!(m.name))).collect();
    files.push(("media".into(), serde_json::Value::Object(map).to_string().into_bytes()));
    for (i, m) in media.iter().enumerate() {
        files.push((i.to_string(), m.data.clone()));
    }
    zip(out, &files)?;
    Ok(n)
}

/// 临时目录放在系统临时目录下（不在用户选的保存位置留下中间文件）
fn tempfile_dir(out: &Path) -> Result<std::path::PathBuf> {
    let stem = out.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("lens-apkg-{}-{}", std::process::id(), hex(&sha1(stem.as_bytes()))[..8].to_string()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_known_vectors() {
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(hex(&sha1(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(hex(&sha1(b"The quick brown fox jumps over the lazy dog")), "2fd4e1c67a2d28fced849ee1bb76e7391b93eb12");
        assert_eq!(hex(&sha1(&vec![b'a'; 1_000_000])), "34aa973cd4c4daa4f61eeb2bdbad27316534016f");
    }

    fn note(word: &str, cloze: &str) -> Note {
        Note {
            word: word.into(),
            pronunciation: format!("[sound:lens-{word}.mp3]"),
            pos: "adj.".into(),
            meaning: "生锈的".into(),
            sentence: "a <b>rusty</b> nail".into(),
            cloze: cloze.into(),
            source: String::new(),
            more: String::new(),
            added: "2026-10-27".into(),
            tags: vec!["Lens".into()],
        }
    }

    #[test]
    fn package_has_collection_media_and_cards() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let out = dir.path().join("a.apkg");
        let notes = [note("rusty", "a ____ nail"), note("ubiquitous", "")];
        let media = [Media { name: "lens-rusty.mp3".into(), data: vec![1, 2, 3] }];
        let n = write(&out, "词镜 Lens", &notes, &media, Cards { recall: true, context: true }, 1_760_000_000_000)?;
        assert_eq!(n, 3, "没有挖空句的词只生成识记卡");

        // 用系统自带的 zip 读法验证不了，这里自己解析中央目录
        let bytes = std::fs::read(&out)?;
        let eocd = bytes.windows(4).rposition(|w| w == [0x50, 0x4b, 0x05, 0x06]).ok_or_else(|| crate::error::Error::Other("no eocd".into()))?;
        let count = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]);
        assert_eq!(count, 3, "collection.anki2 + media + 1 个媒体文件");

        // 集合本身：再写一份直接读
        let col = dir.path().join("c.anki2");
        write_collection(&col, "词镜 Lens", &notes, Cards { recall: true, context: false }, 1_760_000_000_000)?;
        let c = Connection::open(&col)?;
        let cards: i64 = c.query_row("SELECT count(*) FROM cards", [], |r| r.get(0))?;
        assert_eq!(cards, 2);
        let (flds, tags, guid): (String, String, String) = c.query_row("SELECT flds, tags, guid FROM notes ORDER BY id LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        assert_eq!(flds.split('\x1f').count(), FIELDS.len());
        assert_eq!(tags, " Lens ");
        assert!(guid.starts_with("lens-"));
        let decks: String = c.query_row("SELECT decks FROM col", [], |r| r.get(0))?;
        assert!(decks.contains("词镜 Lens"));
        Ok(())
    }

    #[test]
    fn same_word_same_guid() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let guid = |name: &str| -> Result<String> {
            let p = dir.path().join(name);
            write_collection(&p, "d", &[note("Rusty", "")], Cards { recall: true, context: true }, 1)?;
            Ok(Connection::open(&p)?.query_row("SELECT guid FROM notes", [], |r| r.get(0))?)
        };
        assert_eq!(guid("a")?, guid("b")?);
        Ok(())
    }
}
