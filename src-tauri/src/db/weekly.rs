//! 阅读周报：每天每篇的阅读时长和读过的词数（reading_log），按周汇总时长、词汇量、新收生词、标注金句。
//! 日期都是本地日期字符串 YYYY-MM-DD；毫秒范围由前端按本地零点算好传进来（区间都是左闭右开）。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::Result;

/// 一周里最多给几条金句（卡片上可以「换一句」）
const MAX_QUOTES: usize = 20;
/// 新收生词列出几个
const SAMPLE_WORDS: usize = 6;

/// 累加某天某篇的阅读秒数和词数（负数按 0）
pub fn add_reading(conn: &Connection, day: &str, article_id: i64, seconds: i64, words: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO reading_log (day, article_id, seconds, words) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(day, article_id) DO UPDATE SET seconds = seconds + excluded.seconds, words = words + excluded.words",
        params![day, article_id, seconds.max(0), words.max(0)],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySeconds {
    pub day: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabWeek {
    /// 本周最后一个快照的词汇量
    pub value: i64,
    /// 本周之前最后一个快照（没有为 None）
    pub previous: Option<i64>,
    /// 本周每天的值（只有有快照的那几天）
    pub points: Vec<(String, i64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub quote: String,
    pub note: Option<String>,
    /// 书名 / 文章名；文章删了为「（已删除）」
    pub title: String,
    /// 电子书的章节名；普通文章为 None
    pub chapter: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyReport {
    /// 有阅读记录的那几天的秒数
    pub days: Vec<DaySeconds>,
    pub read_seconds: i64,
    pub read_words: i64,
    pub vocab: Option<VocabWeek>,
    pub new_words: i64,
    pub new_word_samples: Vec<String>,
    pub quotes: Vec<Quote>,
}

/// 汇总 [from_day, to_day) 这一周（毫秒范围 [from_ms, to_ms) 用于生词和标注）
pub fn weekly_report(conn: &Connection, from_day: &str, to_day: &str, from_ms: i64, to_ms: i64) -> Result<WeeklyReport> {
    let mut st = conn.prepare("SELECT day, SUM(seconds) FROM reading_log WHERE day >= ?1 AND day < ?2 GROUP BY day ORDER BY day")?;
    let days = st
        .query_map(params![from_day, to_day], |r| Ok(DaySeconds { day: r.get(0)?, seconds: r.get(1)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (read_seconds, read_words): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(seconds), 0), COALESCE(SUM(words), 0) FROM reading_log WHERE day >= ?1 AND day < ?2",
        params![from_day, to_day],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let mut st = conn.prepare("SELECT day, value FROM vocab_snapshot WHERE day >= ?1 AND day < ?2 ORDER BY day")?;
    let points = st.query_map(params![from_day, to_day], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<(String, i64)>>>()?;
    let vocab = match points.last() {
        None => None,
        Some(&(_, value)) => {
            let previous = conn
                .query_row("SELECT value FROM vocab_snapshot WHERE day < ?1 ORDER BY day DESC LIMIT 1", params![from_day], |r| r.get(0))
                .optional()?;
            Some(VocabWeek { value, previous, points })
        }
    };

    let new_words: i64 = conn.query_row("SELECT COUNT(*) FROM word WHERE created_at >= ?1 AND created_at < ?2", params![from_ms, to_ms], |r| r.get(0))?;
    let mut st = conn.prepare("SELECT lemma FROM word WHERE created_at >= ?1 AND created_at < ?2 ORDER BY created_at DESC LIMIT ?3")?;
    let new_word_samples = st.query_map(params![from_ms, to_ms, SAMPLE_WORDS as i64], |r| r.get(0))?.collect::<rusqlite::Result<Vec<String>>>()?;

    // 有笔记的在前，同类按时间新的在前
    let mut st = conn.prepare(
        "SELECT n.quote, n.note, a.title, c.title, n.created_at
         FROM annotation n
         LEFT JOIN article a ON a.id = n.article_id
         LEFT JOIN article_chapter c ON c.article_id = n.article_id AND c.idx = n.chapter AND a.book_hash IS NOT NULL
         WHERE n.created_at >= ?1 AND n.created_at < ?2
         ORDER BY (n.note IS NOT NULL AND TRIM(n.note) <> '') DESC, n.created_at DESC
         LIMIT ?3",
    )?;
    let quotes = st
        .query_map(params![from_ms, to_ms, MAX_QUOTES as i64], |r| {
            let note: Option<String> = r.get(1)?;
            let title: Option<String> = r.get(2)?;
            Ok(Quote {
                quote: r.get(0)?,
                note: note.filter(|n| !n.trim().is_empty()),
                title: title.unwrap_or_else(|| crate::i18n::t("（已删除）", "(deleted)").into()),
                chapter: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(WeeklyReport { days, read_seconds, read_words, vocab, new_words, new_word_samples, quotes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    // 2026-11-16（周一）零点到 2026-11-23 零点，按 UTC 算的毫秒（测试里只要区间一致）
    const FROM_MS: i64 = 1_763_251_200_000;
    const TO_MS: i64 = FROM_MS + 7 * 86_400_000;
    const DAY_MS: i64 = 86_400_000;

    fn report(c: &Connection) -> Result<WeeklyReport> {
        weekly_report(c, "2026-11-16", "2026-11-23", FROM_MS, TO_MS)
    }

    fn article(c: &Connection, title: &str) -> Result<i64> {
        c.execute("INSERT INTO article (title, body, created_at, updated_at) VALUES (?1, '', 0, 0)", params![title])?;
        Ok(c.last_insert_rowid())
    }

    fn annotate(c: &Connection, article_id: i64, quote: &str, note: Option<&str>, at: i64) -> Result<()> {
        c.execute(
            "INSERT INTO annotation (article_id, chapter, quote, color, note, created_at, updated_at) VALUES (?1, 0, ?2, 'yellow', ?3, ?4, ?4)",
            params![article_id, quote, note, at],
        )?;
        Ok(())
    }

    #[test]
    fn reading_is_accumulated_per_day_and_article() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            add_reading(c, "2026-11-16", 1, 30, 100)?;
            add_reading(c, "2026-11-16", 1, 30, 50)?;
            add_reading(c, "2026-11-17", 1, 30, 0)?;
            add_reading(c, "2026-11-17", 1, -5, -9)?;
            let rows: Vec<(String, i64, i64)> = c
                .prepare("SELECT day, seconds, words FROM reading_log ORDER BY day")?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?;
            assert_eq!(rows, vec![("2026-11-16".into(), 60, 150), ("2026-11-17".into(), 30, 0)]);
            Ok(())
        })
    }

    #[test]
    fn empty_week_is_all_empty() -> Result<()> {
        let db = Db::open_in_memory()?;
        let r = db.with_conn(report)?;
        assert!(r.days.is_empty() && r.vocab.is_none() && r.quotes.is_empty() && r.new_word_samples.is_empty());
        assert_eq!((r.read_seconds, r.read_words, r.new_words), (0, 0, 0));
        Ok(())
    }

    #[test]
    fn report_counts_only_this_week() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            add_reading(c, "2026-11-15", 1, 999, 999)?; // 上周日
            add_reading(c, "2026-11-16", 1, 600, 1000)?;
            add_reading(c, "2026-11-22", 2, 300, 500)?;
            add_reading(c, "2026-11-23", 1, 999, 999)?; // 下周一
            for (lemma, at) in [("old", FROM_MS - 1), ("ubiquitous", FROM_MS), ("reverie", TO_MS - 1), ("future", TO_MS)] {
                c.execute("INSERT INTO word (lemma, created_at) VALUES (?1, ?2)", params![lemma, at])?;
            }
            let r = report(c)?;
            assert_eq!(r.days.iter().map(|d| (d.day.as_str(), d.seconds)).collect::<Vec<_>>(), [("2026-11-16", 600), ("2026-11-22", 300)]);
            assert_eq!((r.read_seconds, r.read_words, r.new_words), (900, 1500, 2));
            assert_eq!(r.new_word_samples, ["reverie", "ubiquitous"]);
            Ok(())
        })
    }

    #[test]
    fn quotes_prefer_notes_then_recent() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let a = article(c, "Jane Eyre")?;
            annotate(c, a, "plain old", None, FROM_MS + DAY_MS)?;
            annotate(c, a, "plain new", Some("  "), FROM_MS + 3 * DAY_MS)?;
            annotate(c, a, "noted old", Some("好句"), FROM_MS + 2 * DAY_MS)?;
            annotate(c, a, "noted new", Some("再读"), FROM_MS + 4 * DAY_MS)?;
            annotate(c, a, "last week", Some("x"), FROM_MS - 1)?;
            let r = report(c)?;
            assert_eq!(r.quotes.iter().map(|q| q.quote.as_str()).collect::<Vec<_>>(), ["noted new", "noted old", "plain new", "plain old"]);
            assert_eq!((r.quotes[2].note.as_deref(), r.quotes[0].title.as_str()), (None, "Jane Eyre"));
            Ok(())
        })
    }

    #[test]
    fn deleted_article_quote_has_placeholder_title() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            c.execute_batch("PRAGMA foreign_keys = OFF")?;
            annotate(c, 999, "orphan", None, FROM_MS)?;
            let r = report(c)?;
            assert_eq!(r.quotes[0].title, crate::i18n::t("（已删除）", "(deleted)"));
            Ok(())
        })
    }

    #[test]
    fn vocab_previous_is_last_snapshot_before_week() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            for (day, v) in [("2026-11-10", 7800), ("2026-11-14", 7820), ("2026-11-17", 7900), ("2026-11-21", 7960), ("2026-11-24", 8000)] {
                c.execute("INSERT INTO vocab_snapshot (day, value, gained) VALUES (?1, ?2, 0)", params![day, v])?;
            }
            let v = report(c)?.vocab.expect("本周有快照");
            assert_eq!((v.value, v.previous, v.points.len()), (7960, Some(7820), 2));
            Ok(())
        })
    }
}
