//! 生词本、语境句用的原形（ran → run，convulsed → convulse）。
//! 很多词典把屈折形式也收成词条（多半是 `@@@LINK` 跳到原形），所以「词典里查得到」不能说明它就是原形。
//! 判断顺序：不规则表 → COCA 原形表（只收原形）→ 词典里这个词跳转到的词头 → 原样。

use crate::app_state::AppState;
use crate::batch::parse::{irregular, suffix_candidates};
use crate::dict::index::normalize_key;
use crate::dict::LoadedDict;
use crate::query::normalize_query;

/// 由查询词得到原形（小写、归一化）。短语原样返回。
pub fn base_form(state: &AppState, query: &str) -> String {
    let word = normalize_key(&normalize_query(query));
    let coca = state.coca.read().ok().and_then(|c| c.clone());
    let dicts = state.dicts.all().unwrap_or_default();
    resolve(
        &word,
        &|w| coca.as_ref().is_some_and(|c| c.has_lemma(w)),
        &|w| redirect_targets(&dicts, w),
    )
}

/// 各本词典里这个词的词条所显示的词头（跟随 `@@@LINK` 之后），小写
fn redirect_targets(dicts: &[LoadedDict], word: &str) -> Vec<String> {
    let mut out = Vec::new();
    for d in dicts {
        let Ok(hits) = d.source.lookup(word) else { continue };
        for h in hits {
            let hw = normalize_key(&h.headword);
            if !hw.is_empty() && !out.contains(&hw) {
                out.push(hw);
            }
        }
    }
    out
}

/// is_lemma：COCA 原形表里有没有；headwords：词典里查这个词得到的词头
pub fn resolve(word: &str, is_lemma: &dyn Fn(&str) -> bool, headwords: &dyn Fn(&str) -> Vec<String>) -> String {
    if word.is_empty() || word.contains(' ') {
        return word.to_string();
    }
    if let Some(l) = irregular(word) {
        return l.to_string();
    }
    if is_lemma(word) {
        return word.to_string();
    }
    let candidates = suffix_candidates(word);
    if let Some(c) = candidates.iter().find(|c| is_lemma(c)) {
        return c.clone();
    }
    // 词典里这个词跳到了某个候选原形（convulsed → convulse）
    let targets = headwords(word);
    if let Some(c) = candidates.iter().find(|c| targets.contains(c)) {
        return c.clone();
    }
    word.to_string()
}

/// 启动时整理：以前按屈折形式收藏的词（convulsed、recaptured）并到原形下，语境句的 lemma 一并改正。返回合并 / 改名的词数。
pub fn normalize_saved(state: &AppState) -> crate::error::Result<usize> {
    let words = state.db.with_conn(crate::db::words::all_lemmas)?;
    let mut changed = 0;
    for (id, lemma) in words {
        let base = base_form(state, &lemma);
        if base.is_empty() || base == lemma {
            continue;
        }
        state.db.with_conn(|c| {
            crate::db::words::merge_into(c, id, &base)?;
            c.execute("UPDATE context SET lemma = ?2 WHERE lemma = ?1", rusqlite::params![lemma, base])?;
            Ok(())
        })?;
        tracing::info!(from = %lemma, to = %base, "生词本：并到原形");
        changed += 1;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_irregulars_coca_then_redirects() {
        let coca = |w: &str| ["run", "capture", "thing", "news", "feed"].contains(&w);
        let dict = |w: &str| match w {
            "convulsed" => vec!["convulse".to_string()],
            "recaptured" => vec!["recapture".to_string()],
            "blorbed" => vec!["blorbed".to_string()],
            _ => vec![w.to_string()],
        };
        assert_eq!(resolve("ran", &coca, &dict), "run");
        assert_eq!(resolve("running", &coca, &dict), "run");
        assert_eq!(resolve("captured", &coca, &dict), "capture");
        assert_eq!(resolve("convulsed", &coca, &dict), "convulse");
        assert_eq!(resolve("recaptured", &coca, &dict), "recapture");
        // 本身就是原形的词不被词尾规则误伤（thing ≠ th + ing，news ≠ new + s，feed ≠ fee + d）
        assert_eq!(resolve("thing", &coca, &dict), "thing");
        assert_eq!(resolve("news", &coca, &dict), "news");
        assert_eq!(resolve("feed", &coca, &dict), "feed");
        // 词典里是独立词条、没有跳转：保持原样
        assert_eq!(resolve("blorbed", &coca, &dict), "blorbed");
        assert_eq!(resolve("take off", &coca, &dict), "take off");
    }
}
