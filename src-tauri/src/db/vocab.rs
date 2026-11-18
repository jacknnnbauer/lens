//! 词汇量成长：测试记录、每日快照，以及「测试之后学会了哪些词」的计入规则。
//!
//! 当前词汇量 = 最近一次测试的结果 + 测试之后学会的词数。学会 = 生词本里记忆稳定度达到 21 天，
//! 或手动标成「认识」且排名在测试结果之后。每个词只算一次，忘了（稳定度跌回去）或删了就减回来。

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::Result;

/// 算「记牢」的记忆稳定度（天）
pub const STABLE_DAYS: f64 = 21.0;

pub struct NewTest<'a> {
    pub taken_at: i64,
    pub day: &'a str,
    pub value: u32,
    pub low: u32,
    pub high: u32,
    pub bank_version: u32,
    /// 这次答「认识」的词
    pub said_known: &'a [String],
    /// 这次考过的全部词
    pub asked: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestPoint {
    pub day: String,
    pub taken_at: i64,
    pub value: u32,
    pub low: u32,
    pub high: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayPoint {
    pub day: String,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub current: u32,
    pub gained: u32,
    /// 本月新学会的词数（忘了会减少；重测的跳变不算）
    pub month_gain: i64,
    pub last_test: TestPoint,
    pub tests: Vec<TestPoint>,
    pub days: Vec<DayPoint>,
}

fn stable_lemmas(conn: &Connection) -> Result<Vec<String>> {
    let mut st = conn.prepare("SELECT lemma FROM word WHERE stability >= ?1")?;
    let rows = st.query_map(params![STABLE_DAYS], |r| r.get(0))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// 记下一次测试。此刻已经记牢、已经标「认识」的词都算进了结果，之后不再计入「学会」
pub fn insert_test(conn: &Connection, t: &NewTest) -> Result<i64> {
    let mut excluded: HashSet<String> = t.said_known.iter().cloned().collect();
    excluded.extend(stable_lemmas(conn)?);
    excluded.extend(crate::db::words::known_lemmas(conn)?);
    let mut excluded: Vec<String> = excluded.into_iter().collect();
    excluded.sort();
    conn.execute(
        "INSERT INTO vocab_test (taken_at, day, value, low, high, bank_version, excluded, asked)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![t.taken_at, t.day, t.value, t.low, t.high, t.bank_version, serde_json::to_string(&excluded)?, serde_json::to_string(t.asked)?],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 最近几次测试考过的词：重测时避开
pub fn recent_asked(conn: &Connection, tests: usize) -> Result<HashSet<String>> {
    let mut st = conn.prepare("SELECT asked FROM vocab_test ORDER BY taken_at DESC, id DESC LIMIT ?1")?;
    let rows = st.query_map(params![tests as i64], |r| r.get::<_, String>(0))?;
    let mut out = HashSet::new();
    for json in rows {
        out.extend(serde_json::from_str::<Vec<String>>(&json?)?);
    }
    Ok(out)
}

/// 最近一次测试：(时间, 结果, 不计入的词)
fn latest(conn: &Connection) -> Result<Option<(i64, u32, HashSet<String>)>> {
    let row: Option<(i64, u32, String)> = conn
        .query_row("SELECT taken_at, value, excluded FROM vocab_test ORDER BY taken_at DESC, id DESC LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?;
    row.map(|(at, value, json)| Ok((at, value, serde_json::from_str(&json)?))).transpose()
}

/// 最近一次测试之后学会的词数
pub fn gained(conn: &Connection, rank: &dyn Fn(&str) -> Option<u32>) -> Result<u32> {
    let Some((taken_at, value, excluded)) = latest(conn)? else { return Ok(0) };
    let mut learned: HashSet<String> = stable_lemmas(conn)?.into_iter().filter(|l| !excluded.contains(l)).collect();
    let mut st = conn.prepare("SELECT lemma FROM known_word WHERE source = 'manual' AND created_at > ?1")?;
    for lemma in st.query_map(params![taken_at], |r| r.get::<_, String>(0))? {
        let lemma = lemma?;
        // 词频表里没有的词比所有排名都靠后
        if !excluded.contains(&lemma) && rank(&lemma).is_none_or(|r| r > value) {
            learned.insert(lemma);
        }
    }
    Ok(learned.len() as u32)
}

/// 更新当天的快照，返回当前词汇量；还没测过时返回 None
pub fn refresh(conn: &Connection, day: &str, rank: &dyn Fn(&str) -> Option<u32>) -> Result<Option<u32>> {
    let Some((_, value, _)) = latest(conn)? else { return Ok(None) };
    let gain = gained(conn, rank)?;
    conn.execute(
        "INSERT INTO vocab_snapshot (day, value, gained) VALUES (?1, ?2, ?3)
         ON CONFLICT (day) DO UPDATE SET value = excluded.value, gained = excluded.gained",
        params![day, value + gain, gain],
    )?;
    Ok(Some(value + gain))
}

/// 成长曲线的全部数据。`month_start_day`：本月 1 日（本地日期）
pub fn summary(conn: &Connection, month_start_day: &str) -> Result<Option<Summary>> {
    let mut st = conn.prepare("SELECT day, taken_at, value, low, high FROM vocab_test ORDER BY taken_at, id")?;
    let tests: Vec<TestPoint> = st
        .query_map([], |r| Ok(TestPoint { day: r.get(0)?, taken_at: r.get(1)?, value: r.get(2)?, low: r.get(3)?, high: r.get(4)? }))?
        .collect::<std::result::Result<_, _>>()?;
    let Some(last_test) = tests.last().cloned() else { return Ok(None) };
    let mut st = conn.prepare("SELECT day, value FROM vocab_snapshot ORDER BY day")?;
    let days: Vec<DayPoint> = st.query_map([], |r| Ok(DayPoint { day: r.get(0)?, value: r.get(1)? }))?.collect::<std::result::Result<_, _>>()?;
    let (current, gained): (u32, u32) = conn
        .query_row("SELECT value, gained FROM vocab_snapshot ORDER BY day DESC LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .unwrap_or((last_test.value, 0));
    // 本月重测过：新起点之后学会的都算本月；否则减去上月底已经学会的
    let before: i64 = if last_test.day.as_str() >= month_start_day {
        0
    } else {
        conn.query_row("SELECT gained FROM vocab_snapshot WHERE day < ?1 ORDER BY day DESC LIMIT 1", params![month_start_day], |r| r.get(0))
            .optional()?
            .unwrap_or(0)
    };
    Ok(Some(Summary { current, gained, month_gain: gained as i64 - before, last_test, tests, days }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::words::{delete, save, set_known};
    use crate::db::Db;
    use rusqlite::params;

    fn rank(w: &str) -> Option<u32> {
        match w {
            "easy" => Some(100),
            "hard" => Some(20_000),
            _ => Some(30_000),
        }
    }

    fn test_at(c: &Connection, taken_at: i64, day: &str, value: u32, said_known: &[&str]) -> Result<i64> {
        let said: Vec<String> = said_known.iter().map(|s| s.to_string()).collect();
        insert_test(c, &NewTest { taken_at, day, value, low: value - 100, high: value + 100, bank_version: 1, said_known: &said, asked: &said })
    }

    fn word(c: &Connection, lemma: &str, stability: f64) -> Result<i64> {
        let id = save(c, lemma, None, None, &[])?;
        c.execute("UPDATE word SET stability = ?2 WHERE id = ?1", params![id, stability])?;
        Ok(id)
    }

    fn known_at(c: &Connection, lemma: &str, source: &str, at: i64) -> Result<()> {
        set_known(c, &[lemma.to_string()], true, source)?;
        c.execute("UPDATE known_word SET created_at = ?2 WHERE lemma = ?1", params![lemma, at])?;
        Ok(())
    }

    fn with_db(f: impl FnOnce(&Connection) -> Result<()>) -> Result<()> {
        Db::open_in_memory()?.with_conn(f)
    }

    #[test]
    fn stable_word_after_test_counts_once() -> Result<()> {
        with_db(|c| {
            test_at(c, 10, "2026-01-01", 5000, &[])?;
            word(c, "alpha", 30.0)?;
            word(c, "beta", 20.9)?;
            assert_eq!(gained(c, &rank)?, 1);
            assert_eq!(refresh(c, "2026-01-02", &rank)?, Some(5001));
            Ok(())
        })
    }

    #[test]
    fn word_falling_below_21_days_is_removed() -> Result<()> {
        with_db(|c| {
            test_at(c, 10, "2026-01-01", 5000, &[])?;
            let id = word(c, "alpha", 21.0)?;
            assert_eq!(gained(c, &rank)?, 1);
            c.execute("UPDATE word SET stability = 3 WHERE id = ?1", params![id])?;
            assert_eq!(gained(c, &rank)?, 0);
            Ok(())
        })
    }

    #[test]
    fn deleted_word_is_removed() -> Result<()> {
        with_db(|c| {
            test_at(c, 10, "2026-01-01", 5000, &[])?;
            let id = word(c, "alpha", 40.0)?;
            delete(c, id)?;
            assert_eq!(gained(c, &rank)?, 0);
            Ok(())
        })
    }

    #[test]
    fn said_known_and_already_stable_words_do_not_count() -> Result<()> {
        with_db(|c| {
            word(c, "before", 40.0)?;
            test_at(c, 10, "2026-01-01", 5000, &["said"])?;
            word(c, "said", 40.0)?;
            assert_eq!(gained(c, &rank)?, 0);
            Ok(())
        })
    }

    #[test]
    fn manual_known_counts_only_beyond_level_and_after_test() -> Result<()> {
        with_db(|c| {
            known_at(c, "earlier", "manual", 5)?;
            test_at(c, 10, "2026-01-01", 5000, &[])?;
            known_at(c, "easy", "manual", 20)?;
            known_at(c, "hard", "manual", 20)?;
            assert_eq!(gained(c, &rank)?, 1, "只有 hard：easy 排名在词汇量之内，earlier 在测试之前");
            Ok(())
        })
    }

    #[test]
    fn imported_known_words_never_count() -> Result<()> {
        with_db(|c| {
            test_at(c, 10, "2026-01-01", 5000, &[])?;
            known_at(c, "hard", "import", 20)?;
            assert_eq!(gained(c, &rank)?, 0);
            Ok(())
        })
    }

    #[test]
    fn word_in_both_wordbook_and_known_counts_once() -> Result<()> {
        with_db(|c| {
            test_at(c, 10, "2026-01-01", 5000, &[])?;
            word(c, "hard", 40.0)?;
            known_at(c, "hard", "manual", 20)?;
            assert_eq!(gained(c, &rank)?, 1);
            Ok(())
        })
    }

    #[test]
    fn retest_resets_gain_and_keeps_history() -> Result<()> {
        with_db(|c| {
            test_at(c, 10, "2026-01-01", 5000, &[])?;
            word(c, "alpha", 40.0)?;
            refresh(c, "2026-01-02", &rank)?;
            test_at(c, 100, "2026-02-01", 5600, &[])?;
            assert_eq!(gained(c, &rank)?, 0, "重测前学会的词已经算进新结果");
            assert_eq!(refresh(c, "2026-02-01", &rank)?, Some(5600));
            let s = summary(c, "2026-02-01")?.unwrap();
            assert_eq!((s.tests.len(), s.days.len(), s.current), (2, 2, 5600));
            assert_eq!(s.last_test.value, 5600);
            assert_eq!(s.month_gain, 0, "重测的跳变不算本月学会的词");
            Ok(())
        })
    }

    #[test]
    fn month_gain_counts_only_learning() -> Result<()> {
        with_db(|c| {
            test_at(c, 10, "2026-01-01", 5000, &[])?;
            word(c, "alpha", 40.0)?;
            refresh(c, "2026-01-31", &rank)?;
            word(c, "beta", 40.0)?;
            word(c, "gamma", 40.0)?;
            refresh(c, "2026-02-02", &rank)?;
            assert_eq!(summary(c, "2026-02-01")?.unwrap().month_gain, 2);
            Ok(())
        })
    }

    #[test]
    fn refresh_upserts_one_row_per_day() -> Result<()> {
        with_db(|c| {
            test_at(c, 10, "2026-01-01", 5000, &[])?;
            refresh(c, "2026-01-02", &rank)?;
            word(c, "alpha", 40.0)?;
            refresh(c, "2026-01-02", &rank)?;
            let s = summary(c, "2026-01-01")?.unwrap();
            assert_eq!((s.days.len(), s.days[0].value), (1, 5001));
            Ok(())
        })
    }

    #[test]
    fn no_test_means_no_summary() -> Result<()> {
        with_db(|c| {
            assert_eq!(refresh(c, "2026-01-02", &rank)?, None);
            assert!(summary(c, "2026-01-01")?.is_none());
            Ok(())
        })
    }

    #[test]
    fn recent_asked_covers_the_last_tests_only() -> Result<()> {
        with_db(|c| {
            test_at(c, 1, "2026-01-01", 5000, &["old"])?;
            test_at(c, 2, "2026-01-02", 5000, &["mid"])?;
            test_at(c, 3, "2026-01-03", 5000, &["new"])?;
            let got = recent_asked(c, 2)?;
            assert!(got.contains("new") && got.contains("mid") && !got.contains("old"));
            Ok(())
        })
    }
}
