//! 生词本复习（M8）：取今日队列、评分写回、统计。调度算法见 `crate::review`。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::review::{self, Card, Rating, State};

/// 每天最多引入多少个新词
pub const NEW_PER_DAY: i64 = 20;
/// 学习中的词提前多久可以出现（一轮复习里「忘了」的词 10 分钟后会再来）
pub const LEARN_AHEAD_MS: i64 = 20 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCounts {
    /// 到期的复习（含学习中、重学）
    pub due: i64,
    /// 今天还可以引入的新词
    pub new: i64,
    /// 今天已复习的次数
    pub done_today: i64,
}

pub fn card(conn: &Connection, id: i64) -> Result<Card> {
    conn.query_row(
        "SELECT review_state, stability, difficulty, reps, lapses, last_review, due_at FROM word WHERE id = ?1",
        params![id],
        |r| {
            Ok(Card {
                state: State::from_i64(r.get(0)?),
                stability: r.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                difficulty: r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                reps: r.get(3)?,
                lapses: r.get(4)?,
                last_review: r.get(5)?,
                due: r.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| Error::Other(crate::i18n::t("生词本里没有这个词", "This word isn't in the wordbook").into()))
}

fn new_left(conn: &Connection, day_start: i64) -> Result<i64> {
    let introduced: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT word_id) FROM review_log WHERE state_before = 0 AND reviewed_at >= ?1",
        params![day_start],
        |r| r.get(0),
    )?;
    Ok((NEW_PER_DAY - introduced).max(0))
}

/// 新词：没复习过、用户没手动标「已掌握」
const NEW_WHERE: &str = "review_state = 0 AND familiarity < 5";

pub fn counts(conn: &Connection, now: i64, day_start: i64) -> Result<ReviewCounts> {
    let due = conn.query_row(
        "SELECT COUNT(*) FROM word WHERE review_state > 0 AND due_at <= ?1",
        params![now + LEARN_AHEAD_MS],
        |r| r.get(0),
    )?;
    let new_total: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM word WHERE {NEW_WHERE}"), [], |r| r.get(0))?;
    let done_today = conn.query_row("SELECT COUNT(*) FROM review_log WHERE reviewed_at >= ?1", params![day_start], |r| r.get(0))?;
    Ok(ReviewCounts { due, new: new_total.min(new_left(conn, day_start)?), done_today })
}

/// 今日队列：先到期的复习（最早到期的在前），再按收藏顺序补新词。
pub fn queue(conn: &Connection, now: i64, day_start: i64, limit: i64) -> Result<Vec<i64>> {
    let limit = limit.clamp(1, 500);
    let mut stmt = conn.prepare("SELECT id FROM word WHERE review_state > 0 AND due_at <= ?1 ORDER BY due_at, id LIMIT ?2")?;
    let mut ids = stmt
        .query_map(params![now + LEARN_AHEAD_MS, limit], |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let room = (limit - ids.len() as i64).min(new_left(conn, day_start)?);
    if room > 0 {
        let mut stmt = conn.prepare(&format!("SELECT id FROM word WHERE {NEW_WHERE} ORDER BY created_at, id LIMIT ?1"))?;
        ids.extend(stmt.query_map(params![room], |r| r.get::<_, i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    Ok(ids)
}

/// 四个评分各自的下次到期时间（毫秒时间戳）
pub fn preview(conn: &Connection, id: i64, now: i64) -> Result<[i64; 4]> {
    let p = review::preview(&card(conn, id)?, now);
    Ok(p.map(|c| c.due.unwrap_or(now)))
}

/// 评分并写回；熟悉度随复习状态更新。返回新的到期时间。
pub fn rate(conn: &Connection, id: i64, rating: Rating, now: i64) -> Result<i64> {
    if !(1..=4).contains(&rating) {
        return Err(Error::Other(crate::tf!("评分应为 1–4，收到 {}", "Rating must be 1–4, got {}", rating)));
    }
    let before = card(conn, id)?;
    let after = review::schedule(&before, rating, now);
    let due = after.due.unwrap_or(now);
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE word SET review_state = ?2, stability = ?3, difficulty = ?4, reps = ?5, lapses = ?6,
                         last_review = ?7, due_at = ?8, familiarity = ?9 WHERE id = ?1",
        params![
            id,
            after.state as i64,
            after.stability,
            after.difficulty,
            after.reps,
            after.lapses,
            after.last_review,
            due,
            review::familiarity(&after)
        ],
    )?;
    tx.execute(
        "INSERT INTO review_log (word_id, rating, state_before, reviewed_at, due_after) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, rating, before.state as i64, now, due],
    )?;
    tx.commit()?;
    Ok(due)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::words::save;
    use crate::db::Db;
    use crate::review::{DAY_MS, RELEARN_MS};

    const T0: i64 = 1_700_000_000_000;

    #[test]
    fn queue_rate_and_counts() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let a = save(c, "alpha", None, None, &[])?;
            let b = save(c, "beta", None, None, &[])?;
            let m = save(c, "mastered", None, None, &[])?;
            c.execute("UPDATE word SET familiarity = 5 WHERE id = ?1", params![m])?;
            assert_eq!(queue(c, T0, T0, 50)?, vec![a, b], "手动标已掌握的不进新词队列");
            assert_eq!(counts(c, T0, T0)?, ReviewCounts { due: 0, new: 2, done_today: 0 });

            let due_a = rate(c, a, 1, T0)?;
            assert_eq!(due_a, T0 + RELEARN_MS);
            let due_b = rate(c, b, 3, T0)?;
            assert_eq!(due_b, T0 + 4 * DAY_MS);
            // a 10 分钟后再来（提前量内），b 不在今天
            assert_eq!(queue(c, T0 + 1000, T0, 50)?, vec![a]);
            assert_eq!(counts(c, T0 + 1000, T0)?, ReviewCounts { due: 1, new: 0, done_today: 2 });
            let fam: i64 = c.query_row("SELECT familiarity FROM word WHERE id = ?1", params![b], |r| r.get(0))?;
            assert_eq!(fam, 2);
            assert!(rate(c, a, 5, T0).is_err());
            assert_eq!(preview(c, b, T0 + 4 * DAY_MS)?[0], T0 + 4 * DAY_MS + RELEARN_MS);
            Ok(())
        })
    }

    #[test]
    fn new_words_capped_per_day() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            for i in 0..(NEW_PER_DAY + 5) {
                save(c, &format!("w{i}"), None, None, &[])?;
            }
            let q = queue(c, T0, T0, 500)?;
            assert_eq!(q.len() as i64, NEW_PER_DAY);
            for id in q.iter().take(3) {
                rate(c, *id, 3, T0)?;
            }
            assert_eq!(counts(c, T0, T0)?.new, NEW_PER_DAY - 3);
            assert_eq!(counts(c, T0 + DAY_MS, T0 + DAY_MS)?.new, NEW_PER_DAY, "第二天重新计数");
            Ok(())
        })
    }
}
