//! 生词本（SPEC 6 `word` / `word_context` / `tag` / `word_tag`、M4）。
//! 熟悉度 0..5：0 = 新词，1–4 = 学习中，5 = 已掌握。复习后由 FSRS 状态推出（db::review），也可手动改。

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::history::ContextRow;
use super::now_ms;
use crate::error::{Error, Result};

pub const MAX_FAMILIARITY: i64 = 5;
/// 「查过这么多次还没收藏」
pub const FREQUENT_LOOKUPS: i64 = 3;
const WEEK_MS: i64 = 7 * 24 * 3600 * 1000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordContext {
    #[serde(flatten)]
    pub context: ContextRow,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub id: i64,
    pub lemma: String,
    pub brief: Option<String>,
    pub familiarity: i64,
    pub due_at: Option<i64>,
    /// 复习次数（0 = 还没开始复习）
    pub reps: i64,
    pub created_at: i64,
    pub tags: Vec<String>,
    /// 全部语境句，新的在前
    pub contexts: Vec<WordContext>,
    pub lookups: i64,
}

#[derive(Debug, Clone, Default)]
pub struct WordFilter {
    /// new | learning | mastered；None 为全部
    pub status: Option<String>,
    pub tag: Option<String>,
    pub search: Option<String>,
    /// added（默认，新的在前）| alpha | familiarity | due（下次复习，最早的在前；没开始复习的在后）
    pub sort: Option<String>,
    /// 只取这一个词
    pub id: Option<i64>,
    pub limit: i64,
    pub offset: i64,
}

fn ensure_tag(conn: &Connection, name: &str) -> Result<i64> {
    conn.execute("INSERT OR IGNORE INTO tag (name) VALUES (?1)", params![name])?;
    Ok(conn.query_row("SELECT id FROM tag WHERE name = ?1", params![name], |r| r.get(0))?)
}

fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t = t.trim().trim_start_matches('#').trim().to_string();
        if !t.is_empty() && t.chars().count() <= 32 && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

pub fn id_by_lemma(conn: &Connection, lemma: &str) -> Result<Option<i64>> {
    Ok(conn.query_row("SELECT id FROM word WHERE lemma = ?1", params![lemma], |r| r.get(0)).optional()?)
}

pub fn link_context(conn: &Connection, word_id: i64, context_id: i64) -> Result<()> {
    conn.execute("INSERT OR IGNORE INTO word_context (word_id, context_id) VALUES (?1, ?2)", params![word_id, context_id])?;
    Ok(())
}

/// 把按屈折形式存的词并到原形下（recaptured → recapture）：语境、标签、复习记录都转过去。
/// 原形还没收藏时只改名；已收藏时保留原形那条的复习状态（复习过的优先），简释为空时补上。返回最终的词 id。
pub fn merge_into(conn: &Connection, from_id: i64, lemma: &str) -> Result<i64> {
    let tx = conn.unchecked_transaction()?;
    let target: Option<i64> = tx.query_row("SELECT id FROM word WHERE lemma = ?1", params![lemma], |r| r.get(0)).optional()?;
    let id = match target {
        None => {
            tx.execute("UPDATE word SET lemma = ?2 WHERE id = ?1", params![from_id, lemma])?;
            from_id
        }
        Some(to) if to == from_id => to,
        Some(to) => {
            tx.execute("INSERT OR IGNORE INTO word_context (word_id, context_id) SELECT ?2, context_id FROM word_context WHERE word_id = ?1", params![from_id, to])?;
            tx.execute("INSERT OR IGNORE INTO word_tag (word_id, tag_id) SELECT ?2, tag_id FROM word_tag WHERE word_id = ?1", params![from_id, to])?;
            tx.execute("UPDATE review_log SET word_id = ?2 WHERE word_id = ?1", params![from_id, to])?;
            // 原形没复习过、屈折形式复习过：用屈折形式的复习状态
            tx.execute(
                "UPDATE word SET
                   stability = f.stability, difficulty = f.difficulty, reps = f.reps, lapses = f.lapses,
                   last_review = f.last_review, review_state = f.review_state, due_at = f.due_at, familiarity = f.familiarity
                 FROM (SELECT * FROM word WHERE id = ?1) AS f
                 WHERE word.id = ?2 AND word.reps = 0 AND f.reps > 0",
                params![from_id, to],
            )?;
            tx.execute(
                "UPDATE word SET brief = (SELECT brief FROM word WHERE id = ?1),
                   created_at = MIN(created_at, (SELECT created_at FROM word WHERE id = ?1))
                 WHERE id = ?2 AND (brief IS NULL OR brief = '')",
                params![from_id, to],
            )?;
            tx.execute("DELETE FROM word WHERE id = ?1", params![from_id])?;
            to
        }
    };
    tx.commit()?;
    Ok(id)
}

/// 有简释的词（id, brief），启动时清理用
pub fn all_briefs(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut st = conn.prepare("SELECT id, brief FROM word WHERE brief IS NOT NULL AND brief <> ''")?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// 文章生词扫描里标了「认识」的词
pub fn known_lemmas(conn: &Connection) -> Result<Vec<String>> {
    let mut st = conn.prepare("SELECT lemma FROM known_word")?;
    let rows = st.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// 标记 / 取消「认识」
pub fn set_known(conn: &Connection, lemmas: &[String], known: bool) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for l in lemmas.iter().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty()) {
        if known {
            tx.execute("INSERT OR IGNORE INTO known_word (lemma, created_at) VALUES (?1, ?2)", params![l, now_ms()])?;
        } else {
            tx.execute("DELETE FROM known_word WHERE lemma = ?1", params![l])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// 全部收藏词（id, lemma），启动时整理原形用
pub fn all_lemmas(conn: &Connection) -> Result<Vec<(i64, String)>> {
    let mut st = conn.prepare("SELECT id, lemma FROM word")?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// 收藏（已收藏则合并）：补上简释、追加语境、追加标签。
/// 没给 context_id 时，自动带上这个词最近一次划词捕获的语境句。返回词 id。
pub fn save(
    conn: &Connection,
    lemma: &str,
    brief: Option<&str>,
    context_id: Option<i64>,
    tags: &[String],
) -> Result<i64> {
    if lemma.trim().is_empty() {
        return Err(Error::Other("词为空".into()));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute("INSERT OR IGNORE INTO word (lemma, brief, created_at) VALUES (?1, ?2, ?3)", params![lemma, brief, now_ms()])?;
    let id: i64 = tx.query_row("SELECT id FROM word WHERE lemma = ?1", params![lemma], |r| r.get(0))?;
    if let Some(b) = brief.filter(|b| !b.trim().is_empty()) {
        tx.execute("UPDATE word SET brief = ?2 WHERE id = ?1 AND (brief IS NULL OR brief = '')", params![id, b])?;
    }
    let context_id = match context_id {
        Some(c) => Some(c),
        None => tx
            .query_row("SELECT id FROM context WHERE lemma = ?1 ORDER BY created_at DESC LIMIT 1", params![lemma], |r| r.get(0))
            .optional()?,
    };
    if let Some(c) = context_id {
        link_context(&tx, id, c)?;
    }
    for t in clean_tags(tags) {
        let tid = ensure_tag(&tx, &t)?;
        tx.execute("INSERT OR IGNORE INTO word_tag (word_id, tag_id) VALUES (?1, ?2)", params![id, tid])?;
    }
    tx.commit()?;
    Ok(id)
}

#[derive(Debug, Clone, Default)]
pub struct WordPatch {
    pub brief: Option<String>,
    pub familiarity: Option<i64>,
    /// 给出时整体替换标签
    pub tags: Option<Vec<String>>,
}

pub fn update(conn: &Connection, id: i64, p: &WordPatch) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    if let Some(b) = &p.brief {
        let b = b.trim();
        tx.execute("UPDATE word SET brief = ?2 WHERE id = ?1", params![id, (!b.is_empty()).then_some(b)])?;
    }
    if let Some(f) = p.familiarity {
        tx.execute("UPDATE word SET familiarity = ?2 WHERE id = ?1", params![id, f.clamp(0, MAX_FAMILIARITY)])?;
    }
    if let Some(tags) = &p.tags {
        tx.execute("DELETE FROM word_tag WHERE word_id = ?1", params![id])?;
        for t in clean_tags(tags) {
            let tid = ensure_tag(&tx, &t)?;
            tx.execute("INSERT OR IGNORE INTO word_tag (word_id, tag_id) VALUES (?1, ?2)", params![id, tid])?;
        }
        tx.execute("DELETE FROM tag WHERE id NOT IN (SELECT tag_id FROM word_tag)", [])?;
    }
    tx.commit()?;
    Ok(())
}

/// 删除生词（语境句若仍被历史引用则保留），并清理无人使用的标签。
pub fn delete(conn: &Connection, id: i64) -> Result<bool> {
    let tx = conn.unchecked_transaction()?;
    let n = tx.execute("DELETE FROM word WHERE id = ?1", params![id])?;
    tx.execute("DELETE FROM tag WHERE id NOT IN (SELECT tag_id FROM word_tag)", [])?;
    tx.execute(
        "DELETE FROM context WHERE id NOT IN (SELECT context_id FROM history WHERE context_id IS NOT NULL)
                                AND id NOT IN (SELECT context_id FROM word_context)",
        [],
    )?;
    tx.commit()?;
    Ok(n > 0)
}

pub fn unlink_context(conn: &Connection, word_id: i64, context_id: i64) -> Result<()> {
    conn.execute("DELETE FROM word_context WHERE word_id = ?1 AND context_id = ?2", params![word_id, context_id])?;
    Ok(())
}

fn contexts_of(conn: &Connection, word_id: i64) -> Result<Vec<WordContext>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.sentence, c.hl_start, c.hl_end, c.app, c.window_title, c.url, c.created_at
         FROM word_context wc JOIN context c ON c.id = wc.context_id
         WHERE wc.word_id = ?1 ORDER BY c.created_at DESC, c.id DESC",
    )?;
    let rows = stmt.query_map(params![word_id], |r| {
        Ok(WordContext {
            context: ContextRow {
                id: r.get(0)?,
                sentence: r.get(1)?,
                hl_start: r.get(2)?,
                hl_end: r.get(3)?,
                app: r.get(4)?,
                window_title: r.get(5)?,
                url: r.get(6)?,
                captured_at: r.get(7)?,
            },
            created_at: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn tags_of(conn: &Connection, word_id: i64) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT t.name FROM word_tag wt JOIN tag t ON t.id = wt.tag_id WHERE wt.word_id = ?1 ORDER BY t.name")?;
    let rows = stmt.query_map(params![word_id], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<String>>>()?)
}

pub fn list(conn: &Connection, f: &WordFilter) -> Result<Vec<Word>> {
    let (fmin, fmax) = match f.status.as_deref() {
        Some("new") => (0, 0),
        Some("learning") => (1, MAX_FAMILIARITY - 1),
        Some("mastered") => (MAX_FAMILIARITY, MAX_FAMILIARITY),
        _ => (0, MAX_FAMILIARITY),
    };
    let like = f.search.as_deref().map(|s| format!("%{}%", s.trim().to_lowercase().replace('%', "\\%").replace('_', "\\_")));
    let order = match f.sort.as_deref() {
        Some("alpha") => "w.lemma ASC",
        Some("familiarity") => "w.familiarity ASC, w.created_at DESC",
        Some("due") => "w.due_at IS NULL, w.due_at ASC, w.created_at DESC",
        _ => "w.created_at DESC, w.id DESC",
    };
    let sql = format!(
        "SELECT w.id, w.lemma, w.brief, w.familiarity, w.due_at, w.created_at,
                (SELECT COUNT(*) FROM history h WHERE h.lemma = w.lemma), w.reps
         FROM word w
         WHERE w.familiarity BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR EXISTS (SELECT 1 FROM word_tag wt JOIN tag t ON t.id = wt.tag_id
                                       WHERE wt.word_id = w.id AND t.name = ?3))
           AND (?4 IS NULL OR w.lemma LIKE ?4 ESCAPE '\\' OR lower(coalesce(w.brief, '')) LIKE ?4 ESCAPE '\\')
           AND (?7 IS NULL OR w.id = ?7)
         ORDER BY {order} LIMIT ?5 OFFSET ?6"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![fmin, fmax, f.tag, like, f.limit.clamp(1, 1000), f.offset.max(0), f.id],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
                r.get::<_, i64>(7)?,
            ))
        },
    )?;
    let base = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    base.into_iter()
        .map(|(id, lemma, brief, familiarity, due_at, created_at, lookups, reps)| {
            Ok(Word {
                tags: tags_of(conn, id)?,
                contexts: contexts_of(conn, id)?,
                id,
                lemma,
                brief,
                familiarity,
                due_at,
                reps,
                created_at,
                lookups,
            })
        })
        .collect()
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<Word>> {
    Ok(list(conn, &WordFilter { id: Some(id), limit: 1, ..Default::default() })?.into_iter().next())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagCount {
    pub name: String,
    pub count: i64,
}

pub fn tags(conn: &Connection) -> Result<Vec<TagCount>> {
    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(wt.word_id) FROM tag t JOIN word_tag wt ON wt.tag_id = t.id
         GROUP BY t.id ORDER BY COUNT(wt.word_id) DESC, t.name",
    )?;
    let rows = stmt.query_map([], |r| Ok(TagCount { name: r.get(0)?, count: r.get(1)? }))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookStats {
    pub total: i64,
    /// 本周在阅读中再次遇到（新增语境句）的次数，只算已收藏的词
    pub met_this_week: i64,
    /// 查过 FREQUENT_LOOKUPS 次以上但没收藏的词
    pub frequent_unsaved: Vec<String>,
}

pub fn stats(conn: &Connection, now: i64) -> Result<WordbookStats> {
    let total = conn.query_row("SELECT COUNT(*) FROM word", [], |r| r.get(0))?;
    let met_this_week = conn.query_row(
        "SELECT COUNT(*) FROM word_context wc JOIN context c ON c.id = wc.context_id
         JOIN word w ON w.id = wc.word_id WHERE c.created_at >= ?1 AND c.created_at > w.created_at",
        params![now - WEEK_MS],
        |r| r.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT h.lemma FROM history h WHERE h.lemma NOT IN (SELECT lemma FROM word)
         GROUP BY h.lemma HAVING COUNT(*) >= ?1 ORDER BY MAX(h.created_at) DESC LIMIT 20",
    )?;
    let frequent_unsaved = stmt
        .query_map(params![FREQUENT_LOOKUPS], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(WordbookStats { total, met_this_week, frequent_unsaved })
}

#[cfg(test)]
mod tests {
    #[test]
    fn merges_inflected_word_into_lemma() -> crate::error::Result<()> {
        let db = crate::db::Db::open_in_memory()?;
        db.with_conn(|c| {
            c.execute_batch(
                "INSERT INTO context (id, lemma, surface, sentence, hl_start, hl_end, method, created_at) VALUES
                   (1, 'recaptured', 'recaptured', 'They recaptured it.', 5, 15, 'uia', 0),
                   (2, 'recapture', 'recapture', 'A recapture.', 2, 11, 'uia', 0);",
            )?;
            let a = save(c, "recaptured", Some("to capture again"), Some(1), &["news".to_string()])?;
            let b = save(c, "recapture", None, Some(2), &[])?;
            c.execute("UPDATE word SET reps = 3, review_state = 2 WHERE id = ?1", params![a])?;
            let id = merge_into(c, a, "recapture")?;
            assert_eq!(id, b);
            let n: i64 = c.query_row("SELECT COUNT(*) FROM word_context WHERE word_id = ?1", params![b], |r| r.get(0))?;
            assert_eq!(n, 2, "两条语境都在原形下");
            let (reps, brief): (i64, Option<String>) = c.query_row("SELECT reps, brief FROM word WHERE id = ?1", params![b], |r| Ok((r.get(0)?, r.get(1)?)))?;
            assert_eq!((reps, brief.as_deref()), (3, Some("to capture again")));
            assert!(id_by_lemma(c, "recaptured")?.is_none());
            // 原形还没收藏：直接改名
            let x = save(c, "convulsed", None, None, &[])?;
            assert_eq!(merge_into(c, x, "convulse")?, x);
            assert_eq!(id_by_lemma(c, "convulse")?, Some(x));
            Ok(())
        })
    }

    use super::*;
    use crate::db::history::{insert_context, record, NewContext};
    use crate::db::Db;

    fn ctx(c: &Connection, lemma: &str, sentence: &str) -> Result<i64> {
        insert_context(
            c,
            &NewContext {
                lemma,
                surface: lemma,
                sentence,
                hl_start: 0,
                hl_end: 3,
                app: Some("msedge.exe"),
                window_title: Some("T"),
                url: Some("https://e.com/a"),
                method: "uia",
            },
        )
    }

    #[test]
    fn save_merges_and_auto_attaches_latest_context() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let c1 = ctx(c, "ubiquitous", "first sentence")?;
            std::thread::sleep(std::time::Duration::from_millis(2));
            let c2 = ctx(c, "ubiquitous", "second sentence")?;
            let id = save(c, "ubiquitous", Some("无处不在的"), None, &["#技术文档".into(), " 论文 ".into(), "论文".into()])?;
            let again = save(c, "ubiquitous", Some("另一个释义"), Some(c1), &["写作".into()])?;
            assert_eq!(id, again);

            let w = &list(c, &WordFilter { limit: 10, ..Default::default() })?[0];
            assert_eq!(w.brief.as_deref(), Some("无处不在的"), "已有简释不被覆盖");
            assert_eq!(w.tags, ["写作", "技术文档", "论文"]);
            let ids: Vec<i64> = w.contexts.iter().map(|x| x.context.id).collect();
            assert_eq!(ids, [c2, c1], "自动带上最近的语境，新的在前");
            Ok(())
        })
    }

    #[test]
    fn filters_by_status_tag_search_and_sorts() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let a = save(c, "apple", Some("苹果"), None, &["水果".into()])?;
            let b = save(c, "banana", None, None, &[])?;
            save(c, "cherry", None, None, &["水果".into()])?;
            update(c, a, &WordPatch { familiarity: Some(9), ..Default::default() })?;
            update(c, b, &WordPatch { familiarity: Some(2), ..Default::default() })?;

            let names = |f: WordFilter| -> Result<Vec<String>> {
                Ok(list(c, &WordFilter { limit: 10, ..f })?.into_iter().map(|w| w.lemma).collect())
            };
            assert_eq!(names(WordFilter { status: Some("mastered".into()), ..Default::default() })?, ["apple"]);
            assert_eq!(names(WordFilter { status: Some("learning".into()), ..Default::default() })?, ["banana"]);
            assert_eq!(names(WordFilter { status: Some("new".into()), ..Default::default() })?, ["cherry"]);
            let mut fruit = names(WordFilter { tag: Some("水果".into()), ..Default::default() })?;
            fruit.sort();
            assert_eq!(fruit, ["apple", "cherry"]);
            assert_eq!(names(WordFilter { search: Some("苹".into()), ..Default::default() })?, ["apple"]);
            assert_eq!(names(WordFilter { sort: Some("alpha".into()), ..Default::default() })?, ["apple", "banana", "cherry"]);
            assert_eq!(tags(c)?, [TagCount { name: "水果".into(), count: 2 }]);
            Ok(())
        })
    }

    #[test]
    fn update_tags_and_delete_clean_up() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let cid = ctx(c, "run", "She ran home.")?;
            let id = save(c, "run", None, Some(cid), &["旧".into()])?;
            update(c, id, &WordPatch { tags: Some(vec!["新".into()]), brief: Some("  跑  ".into()), ..Default::default() })?;
            let w = &list(c, &WordFilter { limit: 10, ..Default::default() })?[0];
            assert_eq!((w.tags.clone(), w.brief.clone()), (vec!["新".to_string()], Some("跑".to_string())));
            assert_eq!(tags(c)?.len(), 1, "无人使用的旧标签被删除");

            assert!(delete(c, id)?);
            assert!(tags(c)?.is_empty());
            let n: i64 = c.query_row("SELECT COUNT(*) FROM context", [], |r| r.get(0))?;
            assert_eq!(n, 0, "语境句没有被历史引用，随生词一起删除");
            Ok(())
        })
    }

    #[test]
    fn stats_count_new_encounters_and_frequent_unsaved() -> Result<()> {
        let db = Db::open_in_memory()?;
        db.with_conn(|c| {
            let id = save(c, "ubiquitous", None, None, &[])?;
            std::thread::sleep(std::time::Duration::from_millis(2));
            let cid = ctx(c, "ubiquitous", "met again")?;
            link_context(c, id, cid)?;
            for t in 0..3 {
                record(c, "ephemeral", "ephemeral", "main", None, 1_000 + t * 20 * 60 * 1000)?;
            }
            record(c, "rare", "rare", "main", None, 1)?;
            let s = stats(c, now_ms())?;
            assert_eq!((s.total, s.met_this_week), (1, 1));
            assert_eq!(s.frequent_unsaved, ["ephemeral"]);
            Ok(())
        })
    }
}
