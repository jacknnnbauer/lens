use serde::Serialize;
use tauri::{AppHandle, State, Window};

use crate::app_state::AppState;
use crate::db::dicts;
use crate::dict::LoadedDict;
use crate::error::Result;
use crate::query::{self, LookupHandle, Source};

/// 当前场景配置组里启用且已加载的词典（本地 + 在线），按顺序。
pub(crate) fn profile_sources(state: &AppState) -> Result<Vec<Source>> {
    let profile = state.settings.get()?.active_profile;
    sources_for_profile(state, &profile)
}

/// 指定场景配置组里启用且已加载的词典（批量导出按开始时的配置组查）。
pub fn sources_for_profile(state: &AppState, profile: &str) -> Result<Vec<Source>> {
    let ids = state.db.with_conn(|c| dicts::enabled_for_profile(c, profile))?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(d) = state.dicts.get(&id)? {
            out.push(Source::Local(d));
        } else if let Some(d) = state.online.get(&id)? {
            out.push(Source::Online(d));
        }
    }
    Ok(out)
}

pub(crate) fn locals(sources: Vec<Source>) -> Vec<LoadedDict> {
    sources
        .into_iter()
        .filter_map(|s| match s {
            Source::Local(d) => Some(d),
            Source::Online(_) => None,
        })
        .collect()
}

/// 立即返回 requestId 和参与查询的词典；结果通过 `lookup:result` 逐部推送，最后推送 `lookup:done`。
/// 新查询会取消同一窗口之前未完成的查询；事件只推送给发起的窗口。
#[tauri::command]
pub async fn lookup(app: AppHandle, window: Window, state: State<'_, AppState>, query: String) -> Result<LookupHandle> {
    let mut keys = query::candidates(&query);
    let normalized = keys.last().cloned().unwrap_or_default();
    // 不收屈折形式的词典（牛津高阶没有 boutiques）查不到原词时再查原形；收了的照常命中原词
    let base = crate::lemma::base_form(&state, &normalized);
    let other_form = (!base.is_empty() && !base.contains(' ') && base != crate::dict::index::normalize_key(&normalized)).then(|| base.clone());
    if !base.is_empty() && !keys.iter().any(|k| crate::dict::index::normalize_key(k) == base) {
        keys.push(base);
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    let sources = if keys.is_empty() { Vec::new() } else { profile_sources(&state)? };

    let handle = LookupHandle {
        request_id: request_id.clone(),
        query,
        normalized,
        base: other_form,
        dicts: sources.iter().map(Source::brief).collect(),
    };

    let label = window.label().to_string();
    let cancels = state.lookups.clone();
    let flag = cancels.register(&request_id, &label)?;
    cancels.cancel_others(&request_id, &label)?;
    let cfg = state.settings.get()?.online;
    tauri::async_runtime::spawn(query::run(app, label, cancels, request_id, keys, sources, flag, cfg));
    Ok(handle)
}

#[tauri::command]
pub async fn cancel_lookup(state: State<'_, AppState>, request_id: String) -> Result<()> {
    state.lookups.cancel(&request_id)
}

#[tauri::command]
pub async fn suggest(state: State<'_, AppState>, prefix: String, limit: usize) -> Result<Vec<String>> {
    let prefix = query::normalize_query(&prefix);
    if prefix.is_empty() {
        return Ok(Vec::new());
    }
    let dicts = locals(profile_sources(&state)?);
    Ok(query::prefix_suggestions(&dicts, &prefix, limit.clamp(1, 50)))
}

/// 语境释义（本地）：这句话里的这个词是哪个意思、依据是什么。前端把命中词典的词条 HTML 传进来（与义项图谱相同）。
#[tauri::command]
pub async fn context_sense(state: State<'_, AppState>, mut input: crate::context_sense::ContextInput) -> Result<crate::context_sense::ContextSense> {
    // 有的词典词头就是查询原文（addressed）：先还原成原形，词性判断要靠「是不是变形」
    input.word = crate::lemma::base_form(&state, &input.word);
    let coca = state.coca();
    let wordnet = state.wordnet();
    tauri::async_runtime::spawn_blocking(move || crate::context_sense::analyze(&input, coca.as_deref(), wordnet.as_deref()))
        .await
        .map_err(|e| crate::error::Error::Other(e.to_string()))
}

/// 词汇网络（离线）：派生词、同根词、易混词、近义词。只查当前配置组里的本地词典词头。
#[tauri::command]
pub async fn word_family(state: State<'_, AppState>, query: String) -> Result<crate::word_family::WordFamily> {
    let word = crate::lemma::base_form(&state, &query);
    let dicts = locals(profile_sources(&state)?);
    let coca = state.coca();
    let wordnet = state.wordnet();
    tauri::async_runtime::spawn_blocking(move || {
        // 近义词：有 COCA 用 COCA，没有就用内置 WordNet
        // 易混词按拼写找，候选词表要是常用词：只用 COCA 2 万词；WordNet 近 15 万词、没有词频，会找出一堆生僻词
        let lemmas = coca.as_ref().map(|c| c.lemmas()).unwrap_or_default();
        let synonyms = match (&coca, &wordnet) {
            (Some(c), _) => c.synonyms_of(&word),
            (None, Some(wn)) => wn.synonyms(&word, 0, 12),
            _ => Vec::new(),
        };
        // WordNet 记录的派生关系（rust ↔ rusty）：直接算派生词
        let wn_derived = wordnet.as_ref().map(|wn| wn.derived(&word)).unwrap_or_default();
        let search = |p: &str, limit: usize| query::prefix_suggestions(&dicts, p, limit).into_iter().map(|w| w.to_lowercase()).collect::<Vec<_>>();
        // 词条正文里的词：只看释义词典（同义词、搭配、词频类会列一堆不相干的词）
        let defining: Vec<&LoadedDict> = dicts.iter().filter(|d| !is_reference_dict(&d.source.meta().name)).collect();
        let mentions = |w: &str| -> std::collections::BTreeSet<String> {
            let mut out = std::collections::BTreeSet::new();
            for d in &defining {
                for e in d.source.lookup(w).unwrap_or_default() {
                    let (crate::dict::EntryBody::Html(t) | crate::dict::EntryBody::Text(t)) = &e.body;
                    out.extend(crate::word_family::entry_words(t));
                }
            }
            out
        };
        let is_lemma = |w: &str| wordnet.as_ref().is_some_and(|wn| wn.has(w));
        crate::word_family::build(&word, &search, &mentions, &lemmas, &synonyms, &wn_derived, &is_lemma)
    })
    .await
    .map_err(|e| crate::error::Error::Other(e.to_string()))
}

/// 词汇网络表格视图：每个词的词性（内置 WordNet）和第一段中文释义（当前配置组里的本地双解词典）。
#[tauri::command]
pub async fn word_glosses(state: State<'_, AppState>, words: Vec<String>) -> Result<Vec<crate::gloss::WordGloss>> {
    let dicts: Vec<LoadedDict> = locals(profile_sources(&state)?).into_iter().filter(|d| !is_reference_dict(&d.source.meta().name)).collect();
    let wordnet = state.wordnet();
    tauri::async_runtime::spawn_blocking(move || {
        words
            .into_iter()
            .take(80)
            .map(|word| {
                let pos = wordnet
                    .as_ref()
                    .and_then(|wn| crate::wordnet::pos_labels(wn.poses(&word)))
                    .or_else(|| crate::gloss::pos_by_suffix(&word).map(str::to_string));
                let gloss_of = |w: &str| {
                    dicts.iter().find_map(|d| {
                        d.source.lookup(w).ok()?.iter().find_map(|e| {
                            let (crate::dict::EntryBody::Html(t) | crate::dict::EntryBody::Text(t)) = &e.body;
                            crate::gloss::chinese_gloss(t)
                        })
                    })
                };
                // 词典没收这个变形（corroded、tarnished）：用原形（corrode、tarnish）的释义；候选原形要在 WordNet 里
                let gloss = gloss_of(&word).or_else(|| {
                    crate::batch::parse::suffix_candidates(&word)
                        .into_iter()
                        .filter(|c| c != &word && wordnet.as_ref().is_none_or(|wn| wn.has(c)))
                        .find_map(|c| gloss_of(&c))
                });
                crate::gloss::WordGloss { word, pos, gloss }
            })
            .collect()
    })
    .await
    .map_err(|e| crate::error::Error::Other(e.to_string()))
}

/// 义项图谱（实验）：把各词典命中的词条对齐成义项组。前端在查词完成后把命中卡片的词条 HTML 传进来。
#[tauri::command]
pub async fn sense_atlas(word: String, dicts: Vec<crate::senses::AtlasInput>) -> Result<crate::senses::SenseAtlas> {
    tauri::async_runtime::spawn_blocking(move || crate::senses::build(&word, &dicts))
        .await
        .map_err(|e| crate::error::Error::Other(e.to_string()))
}

/// 搭配、词频类词典（COCA 搭配之类）：背面要的是释义，自动选择时排到最后
pub(crate) fn is_reference_dict(name: &str) -> bool {
    let n = name.to_lowercase();
    ["coca", "colloc", "搭配", "词频", "frequen", "同义", "thesaurus", "词根"].iter().any(|k| n.contains(k))
}

/// 语境卡、复习卡背面显示释义用：设置 › 生词本选了词典就先查那本；
/// 否则按当前配置组顺序查本地词典，搭配、词频类排在最后。
/// 只查本地，不发事件、不取消别的查询，也不记历史。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstEntry {
    pub dict_id: String,
    pub name: String,
    pub entries: Vec<crate::dict::RawEntry>,
}

#[tauri::command]
/// `dict`：先查这部词典（全书预习里用户选的释义词典），查不到再按下面的顺序
pub async fn first_entry(state: State<'_, AppState>, query: String, dict: Option<String>) -> Result<Option<FirstEntry>> {
    let keys = query::candidates(&query);
    if keys.is_empty() {
        return Ok(None);
    }
    let mut dicts = locals(profile_sources(&state)?);
    // 稳定排序：先是设置里选的那本，然后是普通词典，搭配、词频类最后
    let chosen = state.settings.get()?.wordbook.card_dict;
    dicts.sort_by_key(|d| {
        let m = d.source.meta();
        (dict.as_deref() != Some(m.id.as_str()), m.id != chosen, is_reference_dict(&m.name))
    });
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<FirstEntry>> {
        for d in &dicts {
            for k in &keys {
                let hits = match d.source.lookup(k) {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::warn!(dict = %d.source.meta().id, error = %e, "查词失败");
                        break;
                    }
                };
                if !hits.is_empty() {
                    let meta = d.source.meta();
                    return Ok(Some(FirstEntry { dict_id: meta.id.clone(), name: meta.name.clone(), entries: hits }));
                }
            }
        }
        Ok(None)
    })
    .await
    .map_err(|e| crate::error::Error::Other(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::is_reference_dict;

    #[test]
    fn spots_collocation_and_frequency_dicts() {
        assert!(is_reference_dict("COCA2W"));
        assert!(is_reference_dict("牛津英语搭配词典"));
        assert!(!is_reference_dict("American Heritage Dictionary 5th"));
        assert!(!is_reference_dict("牛津高阶英汉双解词典 第 8 版"));
    }
}
