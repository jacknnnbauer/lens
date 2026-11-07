//! 全书预习（第一期，只做本机计算）：把整本电子书扫一遍，按「词汇量门槛」挑出生词，
//! 统计每章的篇幅、预计阅读用时和生词密度。释义、排版交给前端现有的导出流水线。
//!
//! 分析结果不记已收藏 / 认识：这两样随时会变，挑词时再按当前的集合判断，分析结果就能一直缓存。

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::scan::{scan, Lexicon, ScanWord};

/// 预计用时按非母语读者读小说的速度估算（词 / 分钟）
pub const READING_WPM: u32 = 180;

/// 门槛预览里列出的几档词汇量
pub const LEVELS: [u32; 7] = [3000, 5000, 8000, 10000, 12000, 15000, 20000];

pub struct ChapterText {
    pub idx: i64,
    pub title: String,
    pub text: String,
}

/// 书前书后的附加内容（标题页、版权、前言、导读、注释、索引…）：默认不算进预习，用户可以加回来。
/// 只按标题开头认；Etymology、Extracts 这种小说自己的部分不在里面
pub fn is_extra(title: &str) -> bool {
    const HEADS: &[&str] = &[
        "cover", "title page", "half title", "copyright", "contents", "table of contents", "dedication", "epigraph page", "also by", "praise for", "about the author",
        "about this book", "about the publisher", "foreword", "introduction", "preface", "editor", "translator", "chronology", "a note on the text", "note on the text",
        "notes", "explanatory notes", "endnotes", "further reading", "suggested further", "suggestions for further", "select bibliography", "bibliography",
        "list of textual", "textual", "glossary", "index", "acknowledg", "afterword", "appendix", "map", "list of illustrations", "colophon", "newsletter",
        "illustrations", "filmography", "how to navigate", "reading group", "discussion questions", "credits",
    ];
    let t = title.trim().to_lowercase();
    HEADS.iter().any(|h| t.starts_with(h))
}

/// 带编号的章：Chapter 1、BOOK II、Part One、12. The Quarrel、IV. Night
fn is_numbered(title: &str) -> bool {
    let t = title.trim().to_lowercase();
    if ["chapter", "chap.", "book ", "part ", "letter "].iter().any(|h| t.starts_with(h)) || t.starts_with(|c: char| c.is_ascii_digit()) {
        return true;
    }
    let first = t.split([' ', '.']).next().unwrap_or_default();
    !first.is_empty() && first.len() <= 6 && first.chars().all(|c| "ivxlcdm".contains(c)) && t.len() > first.len()
}

/// 默认的预习范围：从第一个不是附加内容的部分开始；书里有带编号的章时，到最后一个编号章为止（后面紧跟的尾声也算），
/// 这样书末出版社附赠的书评、影视、导读不算进来。没有编号章时到最后一个不是附加内容的部分
pub fn default_range(parts: &[(i64, &str)]) -> Option<(i64, i64)> {
    let body: Vec<&(i64, &str)> = parts.iter().filter(|(_, t)| !is_extra(t)).collect();
    let start = body.first()?.0;
    let mut end = body.last()?.0;
    if let Some(pos) = parts.iter().rposition(|(_, t)| is_numbered(t)) {
        let mut last = pos;
        while let Some((_, t)) = parts.get(last + 1) {
            let t = t.trim().to_lowercase();
            if ["epilogue", "coda", "postscript", "finale", "conclusion", "the end"].iter().any(|h| t.starts_with(h)) {
                last += 1;
            } else {
                break;
            }
        }
        end = parts[last].0;
    }
    Some((start, end.max(start)))
}

/// 一章的扫描结果（门槛 0：每个计入的词都在），整本书按章缓存，换范围时直接合并
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterScan {
    pub idx: i64,
    pub title: String,
    pub tokens: u32,
    pub words: Vec<ScanWord>,
}

pub fn scan_chapter(ch: &ChapterText, lx: &Lexicon) -> ChapterScan {
    let none = HashSet::new();
    let r = scan(&ch.text, 0, lx, &none, &none);
    ChapterScan { idx: ch.idx, title: ch.title.clone(), tokens: r.total, words: r.words }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BookLemma {
    pub lemma: String,
    pub rank: Option<u32>,
    /// 全书出现次数
    pub count: u32,
    /// 文中出现过的写法（小写，按出现顺序）
    pub forms: Vec<String>,
    /// 第一次出现在哪一章，以及那句话
    pub chapter: i64,
    pub sentence: Option<String>,
    pub hl_start: usize,
    pub hl_end: usize,
    /// 各章出现次数（章号, 次数）
    pub per_chapter: Vec<(i64, u32)>,
    /// 每次出现都是大写开头
    pub capitalized: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChapterStat {
    pub idx: i64,
    pub title: String,
    /// 计入的词次（不含专有名词、缩写）
    pub tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BookStudy {
    /// 只含有正文的章（封面、版权页这类没有词的不算）
    pub chapters: Vec<ChapterStat>,
    /// 按第一次出现的先后
    pub lemmas: Vec<BookLemma>,
    pub total: u32,
}

/// 逐章扫描，合并成全书的词表（测试用；命令里按章缓存扫描结果，再按范围合并）
#[cfg(test)]
pub fn analyze(chapters: &[ChapterText], lx: &Lexicon) -> BookStudy {
    let scans: Vec<ChapterScan> = chapters.iter().map(|c| scan_chapter(c, lx)).collect();
    merge(scans.iter())
}

/// 把各章的扫描结果按章的先后合并成词表（没有正文的章跳过）
pub fn merge<'a>(chapters: impl Iterator<Item = &'a ChapterScan>) -> BookStudy {
    let mut out = BookStudy::default();
    let mut index: HashMap<String, usize> = HashMap::new();
    for ch in chapters {
        if ch.tokens == 0 {
            continue;
        }
        out.total += ch.tokens;
        out.chapters.push(ChapterStat { idx: ch.idx, title: ch.title.clone(), tokens: ch.tokens });
        for w in ch.words.iter().cloned() {
            match index.get(&w.lemma) {
                Some(&i) => {
                    let l = &mut out.lemmas[i];
                    l.count += w.count;
                    l.capitalized &= w.capitalized;
                    l.per_chapter.push((ch.idx, w.count));
                    for f in w.forms {
                        if !l.forms.contains(&f) {
                            l.forms.push(f);
                        }
                    }
                }
                None => {
                    index.insert(w.lemma.clone(), out.lemmas.len());
                    out.lemmas.push(BookLemma {
                        lemma: w.lemma,
                        rank: w.rank,
                        count: w.count,
                        forms: w.forms,
                        chapter: ch.idx,
                        sentence: w.sentence,
                        hl_start: w.hl_start,
                        hl_end: w.hl_end,
                        per_chapter: vec![(ch.idx, w.count)],
                        capitalized: w.capitalized,
                    });
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    New,
    Saved,
}

#[derive(Debug, Clone, Copy)]
pub struct Filter {
    /// 词汇量：排名在这之后的算生词
    pub level: u32,
    /// 全书至少出现几次
    pub min_count: u32,
    /// 已收藏的词也列进来
    pub include_saved: bool,
}

/// 按门槛挑词：标过「认识」的不要；没有排名的（词频表外的真词）算超出任何门槛；
/// 出现多次、每次都大写开头的（Moby、Melville）当人名地名不要
pub fn pick<'a>(study: &'a BookStudy, f: Filter, saved: &HashSet<String>, known: &HashSet<String>) -> Vec<(&'a BookLemma, Mark)> {
    study
        .lemmas
        .iter()
        .filter(|l| !(l.capitalized && l.count >= 2))
        .filter(|l| !known.contains(&l.lemma) && l.rank.is_none_or(|r| r > f.level) && l.count >= f.min_count.max(1))
        .filter_map(|l| match (saved.contains(&l.lemma), f.include_saved) {
            (true, true) => Some((l, Mark::Saved)),
            (true, false) => None,
            (false, _) => Some((l, Mark::New)),
        })
        .collect()
}

/// 只留 `n` 个：书里出现次数多的优先（这本书里反复出现，背了最划算），一样多时留更常用的词；
/// 留下的词保持原来的先后
pub fn limit(picked: Vec<(&BookLemma, Mark)>, n: usize) -> Vec<(&BookLemma, Mark)> {
    if picked.len() <= n {
        return picked;
    }
    let mut order: Vec<usize> = (0..picked.len()).collect();
    order.sort_by_key(|&i| (std::cmp::Reverse(picked[i].0.count), picked[i].0.rank.unwrap_or(u32::MAX), i));
    let mut keep: Vec<usize> = order.into_iter().take(n).collect();
    keep.sort_unstable();
    keep.into_iter().map(|i| picked[i]).collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelCount {
    pub level: u32,
    /// 生词数（不含已收藏）
    pub words: u32,
    /// 其中已收藏过的
    pub saved: u32,
    /// 认识的词次占比
    pub coverage: f32,
}

/// 每档门槛有多少生词，选门槛前先看数量
pub fn level_counts(study: &BookStudy, min_count: u32, saved: &HashSet<String>, known: &HashSet<String>) -> Vec<LevelCount> {
    LEVELS
        .iter()
        .map(|&level| {
            let picked = pick(study, Filter { level, min_count, include_saved: true }, saved, known);
            let words = picked.iter().filter(|(_, m)| *m == Mark::New).count() as u32;
            let new_tokens: u32 = picked.iter().filter(|(_, m)| *m == Mark::New).map(|(l, _)| l.count).sum();
            let coverage = if study.total == 0 { 1.0 } else { 1.0 - new_tokens as f32 / study.total as f32 };
            LevelCount { level, words, saved: picked.len() as u32 - words, coverage }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterRow {
    pub idx: i64,
    pub title: String,
    pub tokens: u32,
    pub minutes: u32,
    /// 这一章第一次出现的生词数（分章预习背这些）
    pub first_words: u32,
    /// 这一章里生词出现的次数，和每千词的密度
    pub new_tokens: u32,
    pub density: f32,
}

/// 阅读路线表里本机能算的几列
pub fn chapter_rows(study: &BookStudy, picked: &[(&BookLemma, Mark)]) -> Vec<ChapterRow> {
    let mut first: HashMap<i64, u32> = HashMap::new();
    let mut tokens: HashMap<i64, u32> = HashMap::new();
    for (l, _) in picked {
        *first.entry(l.chapter).or_default() += 1;
        for &(c, n) in &l.per_chapter {
            *tokens.entry(c).or_default() += n;
        }
    }
    study
        .chapters
        .iter()
        .map(|c| {
            let new_tokens = tokens.get(&c.idx).copied().unwrap_or(0);
            ChapterRow {
                idx: c.idx,
                title: c.title.clone(),
                tokens: c.tokens,
                minutes: minutes(c.tokens),
                first_words: first.get(&c.idx).copied().unwrap_or(0),
                new_tokens,
                density: if c.tokens == 0 { 0.0 } else { new_tokens as f32 * 1000.0 / c.tokens as f32 },
            }
        })
        .collect()
}

/// 预计阅读分钟数（至少 1 分钟）
pub fn minutes(tokens: u32) -> u32 {
    tokens.div_ceil(READING_WPM).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn study() -> BookStudy {
        let ranks: HashMap<&str, u32> = [("the", 1), ("whale", 9000), ("sea", 500), ("harpoon", 15000), ("ship", 800), ("sail", 2000), ("a", 4), ("is", 2), ("be", 2)]
            .into_iter()
            .collect();
        let rank = |w: &str| ranks.get(w).copied();
        let is_word = |w: &str| ranks.contains_key(w) || w == "leviathan";
        let irregular = |_: &str| None;
        let lx = Lexicon { rank: &rank, is_word: &is_word, irregular: &irregular };
        let chapters = vec![
            ChapterText { idx: 0, title: "Cover".into(), text: "".into() },
            ChapterText { idx: 1, title: "Loomings".into(), text: "The whale is a leviathan.\nThe sea is the sea.".into() },
            ChapterText { idx: 2, title: "The Chase".into(), text: "A harpoon! The whale.\nThe ship and the whales sail.".into() },
        ];
        analyze(&chapters, &lx)
    }

    #[test]
    fn merges_chapters_in_order_of_first_appearance() {
        let s = study();
        assert_eq!(s.chapters.iter().map(|c| c.idx).collect::<Vec<_>>(), [1, 2], "没有正文的章不算");
        let whale = s.lemmas.iter().find(|l| l.lemma == "whale").expect("whale");
        assert_eq!((whale.count, whale.chapter), (3, 1));
        assert_eq!(whale.per_chapter, [(1, 1), (2, 2)]);
        assert_eq!(whale.forms, ["whale", "whales"]);
        assert_eq!(whale.sentence.as_deref(), Some("The whale is a leviathan."));
        let order: Vec<&str> = s.lemmas.iter().map(|l| l.lemma.as_str()).collect();
        assert!(order.iter().position(|w| *w == "leviathan") < order.iter().position(|w| *w == "harpoon"));
    }

    #[test]
    fn default_range_skips_extras_and_publisher_bonus() {
        let moby = [
            (0, "Cover"), (1, "Title Page"), (2, "Foreword by Nathaniel Philbrick"), (3, "ETYMOLOGY"), (4, "EXTRACTS"), (5, "CHAPTER 1 Loomings"),
            (6, "CHAPTER 135 The Chase\u{2014}Third Day"), (7, "EPILOGUE"), (8, "Nineteenth-Century Reviews"), (9, "Cannibal Talk in Moby-Dick"), (10, "Explanatory Notes"),
        ];
        assert_eq!(default_range(&moby), Some((3, 7)));
        // 没有编号章：到最后一个不是附加内容的部分
        let essays = [(0, "Contents"), (1, "On Running"), (2, "On Walking"), (3, "Notes")];
        assert_eq!(default_range(&essays), Some((1, 2)));
        // 罗马数字、纯数字编号
        assert_eq!(default_range(&[(0, "I. Night"), (1, "II. Day"), (2, "Reviews")]), Some((0, 1)));
        assert_eq!(default_range(&[(0, "Prologue"), (1, "12"), (2, "Interview")]), Some((0, 1)));
        assert_eq!(default_range(&[(0, "Cover")]), None);
        assert!(!is_numbered("Ivanhoe"), "单独一个像罗马数字的词不算");
    }

    #[test]
    fn spots_front_and_back_matter() {
        for t in ["Illustrations for Moby-Dick", "Filmography of Herman Melville", "How to Navigate Guide", "Title Page", "Copyright", "Foreword by Nathaniel Philbrick", "Introduction by Andrew Delbanco", "Suggestions for Further Reading", "List of Textual Emendations", "Notes", "Glossary"] {
            assert!(is_extra(t), "{t}");
        }
        for t in ["ETYMOLOGY", "EXTRACTS", "CHAPTER 1 Loomings", "The Chase—First Day", "Epilogue"] {
            assert!(!is_extra(t), "{t}");
        }
    }

    #[test]
    fn names_that_are_always_capitalised_are_skipped() {
        let ranks: HashMap<&str, u32> = [("the", 1), ("moby", 30000)].into_iter().collect();
        let rank = |w: &str| ranks.get(w).copied();
        let is_word = |w: &str| ranks.contains_key(w) || w == "explanatory";
        let irregular = |_: &str| None;
        let lx = Lexicon { rank: &rank, is_word: &is_word, irregular: &irregular };
        let s = analyze(&[ChapterText { idx: 0, title: "x".into(), text: "Moby swam.\nMoby dove.\nExplanatory note.".into() }], &lx);
        let none = HashSet::new();
        let picked: Vec<&str> = pick(&s, Filter { level: 5000, min_count: 1, include_saved: false }, &none, &none).iter().map(|(l, _)| l.lemma.as_str()).collect();
        assert_eq!(picked, ["explanatory"], "只在句首出现一次的照常算");
    }

    #[test]
    fn picks_by_level_count_and_status() {
        let s = study();
        let none = HashSet::new();
        let lemmas = |p: Vec<(&BookLemma, Mark)>| p.into_iter().map(|(l, m)| (l.lemma.clone(), m)).collect::<Vec<_>>();
        // 8000 以外：whale 9000、harpoon 15000、leviathan 无排名
        let p = pick(&s, Filter { level: 8000, min_count: 1, include_saved: false }, &none, &none);
        assert_eq!(lemmas(p), [("whale".into(), Mark::New), ("leviathan".into(), Mark::New), ("harpoon".into(), Mark::New)]);
        // 至少出现 2 次
        let p = pick(&s, Filter { level: 8000, min_count: 2, include_saved: false }, &none, &none);
        assert_eq!(lemmas(p), [("whale".into(), Mark::New)]);
        // 认识的不要；收藏过的按选项
        let known: HashSet<String> = ["leviathan".to_string()].into();
        let saved: HashSet<String> = ["whale".to_string()].into();
        let p = pick(&s, Filter { level: 8000, min_count: 1, include_saved: false }, &saved, &known);
        assert_eq!(lemmas(p), [("harpoon".into(), Mark::New)]);
        let p = pick(&s, Filter { level: 8000, min_count: 1, include_saved: true }, &saved, &known);
        assert_eq!(lemmas(p), [("whale".into(), Mark::Saved), ("harpoon".into(), Mark::New)]);
    }

    #[test]
    fn limit_keeps_the_most_frequent_in_book_order() {
        let s = study();
        let none = HashSet::new();
        let picked = pick(&s, Filter { level: 8000, min_count: 1, include_saved: false }, &none, &none);
        // whale 3 次、leviathan 1 次（无排名）、harpoon 1 次（15000）
        let two: Vec<&str> = limit(picked.clone(), 2).iter().map(|(l, _)| l.lemma.as_str()).collect();
        assert_eq!(two, ["whale", "harpoon"], "次数一样时留更常用的 harpoon，顺序不变");
        assert_eq!(limit(picked.clone(), 10).len(), 3);
        assert!(limit(picked, 0).is_empty());
    }

    #[test]
    fn level_preview_and_chapter_rows() {
        let s = study();
        let none = HashSet::new();
        let counts = level_counts(&s, 1, &none, &none);
        let at = |lv: u32| counts.iter().find(|c| c.level == lv).map(|c| c.words);
        assert_eq!(at(8000), Some(3));
        assert_eq!(at(10000), Some(2));
        assert_eq!(at(20000), Some(1), "只剩没有排名的 leviathan");
        let picked = pick(&s, Filter { level: 8000, min_count: 1, include_saved: false }, &none, &none);
        let rows = chapter_rows(&s, &picked);
        assert_eq!(rows.iter().map(|r| (r.idx, r.first_words, r.new_tokens)).collect::<Vec<_>>(), [(1, 2, 2), (2, 1, 3)]);
        assert!(rows[1].density > rows[0].density);
        assert_eq!(rows[0].minutes, 1);
        assert_eq!(minutes(361), 3);
    }
}
