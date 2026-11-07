//! 全书预习的命令：分析整本书（结果缓存，换门槛不用重扫）、门槛预览、挑出的词表和每章统计。业务逻辑在 `crate::study`。

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;
use crate::db::{articles, words};
use crate::error::{Error, Result};
use crate::study::{self, BookStudy, ChapterRow, ChapterScan, ChapterText, Filter, LevelCount, Mark};

/// 最近分析过的一本书（文章 id、书的文件哈希、每章的扫描结果）：换范围、门槛、排序都不用重扫
static CACHE: Mutex<Option<Cached>> = Mutex::new(None);
type Cached = (i64, String, Arc<Vec<ChapterScan>>);

/// 预习的范围：从哪部分到哪部分（书里的章节序号，含两端；不给就用默认范围），范围里的附加内容算不算
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub include_extras: bool,
}

fn deleted() -> Error {
    Error::Other(crate::i18n::t("这本书已被删除", "This book has been deleted").into())
}

/// 读出整本书、逐章扫描（同一本书第二次直接用缓存）
async fn scans(state: &AppState, id: i64) -> Result<Arc<Vec<ChapterScan>>> {
    let (hash, info) = state.db.with_conn(|c| Ok((articles::book_hash(c, id)?, articles::book_info(c, id)?)))?;
    let (Some(hash), Some(info)) = (hash, info) else { return Err(deleted()) };
    if let Ok(cache) = CACHE.lock() {
        if let Some((_, _, s)) = cache.as_ref().filter(|(cid, chash, _)| *cid == id && *chash == hash) {
            return Ok(s.clone());
        }
    }
    let path = crate::commands::articles::book_file(&state.root, &hash);
    let (coca, wordnet) = (state.coca(), state.wordnet());
    let t = std::time::Instant::now();
    let scanned = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<ChapterScan>> {
        crate::scan::with_lexicon(coca.as_deref(), wordnet.as_deref(), |lx| {
            info.chapters
                .iter()
                .map(|c| {
                    let html = crate::epub::read_chapter(&path, &c.href)?;
                    Ok(study::scan_chapter(&ChapterText { idx: c.idx, title: c.title.clone(), text: crate::epub::text_of_html(&html) }, lx))
                })
                .collect()
        })
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))??;
    tracing::info!(id, ms = t.elapsed().as_millis() as u64, chapters = scanned.len(), "全书预习：扫描完成");
    let scanned = Arc::new(scanned);
    if let Ok(mut cache) = CACHE.lock() {
        *cache = Some((id, hash, scanned.clone()));
    }
    Ok(scanned)
}

/// 有正文的部分（书里的章节序号、标题、是不是附加内容）
fn parts(scans: &[ChapterScan]) -> Vec<Part> {
    scans.iter().filter(|c| c.tokens > 0).map(|c| Part { idx: c.idx, title: c.title.clone(), extra: study::is_extra(&c.title) }).collect()
}

/// 实际用的范围：给了就用，没给用默认范围
fn effective(scope: Scope, parts: &[Part]) -> (i64, i64) {
    let titles: Vec<(i64, &str)> = parts.iter().map(|p| (p.idx, p.title.as_str())).collect();
    let (df, dt) = study::default_range(&titles).unwrap_or((0, i64::MAX));
    let (from, to) = (scope.from.unwrap_or(df), scope.to.unwrap_or(dt));
    (from.min(to), from.max(to))
}

/// 按范围合并出要预习的词表
async fn load(state: &AppState, id: i64, scope: Scope) -> Result<(BookStudy, Vec<Part>, (i64, i64))> {
    let scans = scans(state, id).await?;
    let parts = parts(&scans);
    let (from, to) = effective(scope, &parts);
    let study = study::merge(scans.iter().filter(|c| c.idx >= from && c.idx <= to && (scope.include_extras || !study::is_extra(&c.title))));
    Ok((study, parts, (from, to)))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    pub idx: i64,
    pub title: String,
    /// 书前书后的附加内容（前言、注释…）
    pub extra: bool,
}

fn saved_and_known(state: &AppState) -> Result<(HashSet<String>, HashSet<String>)> {
    let saved = state.db.with_conn(words::all_lemmas)?.into_iter().map(|(_, l)| l).collect();
    let known = state.db.with_conn(words::known_lemmas)?.into_iter().collect();
    Ok((saved, known))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyPreview {
    pub title: String,
    pub author: Option<String>,
    /// 有正文的章数、全书词次、不同的词、预计阅读分钟数
    pub chapters: u32,
    pub total: u32,
    pub unique: u32,
    pub minutes: u32,
    pub levels: Vec<LevelCount>,
    /// 排名用的词频表：coca | builtin
    pub source: &'static str,
    /// 书里有正文的各部分，和实际用的范围
    pub parts: Vec<Part>,
    pub from: i64,
    pub to: i64,
}

/// 范围内的概况和各档门槛的生词数
#[tauri::command]
pub async fn book_study_preview(state: State<'_, AppState>, id: i64, min_count: u32, scope: Scope) -> Result<StudyPreview> {
    let (study, parts, (from, to)) = load(&state, id, scope).await?;
    let (saved, known) = saved_and_known(&state)?;
    let article = state.db.with_conn(|c| articles::open(c, id))?.ok_or_else(deleted)?;
    Ok(StudyPreview {
        title: article.meta.title,
        author: article.meta.author,
        chapters: study.chapters.len() as u32,
        total: study.total,
        unique: study.lemmas.len() as u32,
        minutes: study::minutes(study.total),
        levels: study::level_counts(&study, min_count, &saved, &known),
        source: if state.coca().is_some() { "coca" } else { "builtin" },
        parts,
        from,
        to,
    })
}

/// 读前导读：用书名和作者在维基百科找这本书和作者的条目（只发书名、作者）。找不到时返回 None
#[tauri::command]
pub async fn book_guide(state: State<'_, AppState>, id: i64) -> Result<Option<crate::wiki::BookGuide>> {
    let article = state.db.with_conn(|c| articles::open(c, id))?.ok_or_else(deleted)?;
    let study = state.settings.get()?.study;
    crate::wiki::book_guide(&article.meta.title, article.meta.author.as_deref(), study.guide_lang != "en", study.spoilers).await
}

/// AI 导读存成文件：`guides/<书的哈希>-<模型>-<s|n>.json`（s = 剧透）
fn ai_guide_file(state: &AppState, hash: &str, model: &str, spoilers: bool) -> std::path::PathBuf {
    let model: String = model.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
    state.root.join("guides").join(format!("{hash}-{model}-{}.json", if spoilers { "s" } else { "n" }))
}

/// 读前导读的 AI 版。`cached_only`：只看存过的（不花钱），没有时返回 None；`force`：重新生成
#[tauri::command]
pub async fn book_guide_ai(state: State<'_, AppState>, id: i64, cached_only: bool, force: bool) -> Result<Option<crate::wiki::BookGuide>> {
    let article = state.db.with_conn(|c| articles::open(c, id))?.ok_or_else(deleted)?;
    let hash = state.db.with_conn(|c| articles::book_hash(c, id))?.ok_or_else(deleted)?;
    let settings = state.settings.get()?;
    let (cfg, spoilers) = (settings.ai, settings.study.spoilers);
    let provider = cfg.provider();
    let model = cfg.active_model().to_string();
    let file = ai_guide_file(&state, &hash, &model, spoilers);
    if !force {
        if let Some(g) = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<crate::wiki::BookGuide>(&t).ok()) {
            return Ok(Some(g));
        }
    }
    if cached_only {
        return Ok(None);
    }
    let key = crate::online::credentials::read(provider.key_id())?.ok_or_else(|| {
        Error::Online(crate::tf!("还没有填写 {} API Key（设置 › AI 例句）", "No {} API key yet (Settings › AI examples)", provider.name()))
    })?;
    // 章节标题（默认范围里的正文）和开头一段正文
    let all = scans(&state, id).await?;
    let ps = parts(&all);
    let (from, to) = effective(Scope::default(), &ps);
    let body_parts: Vec<&Part> = ps.iter().filter(|p| p.idx >= from && p.idx <= to && !p.extra).collect();
    let titles: Vec<String> = body_parts.iter().map(|p| p.title.clone()).collect();
    let info = state.db.with_conn(|c| articles::book_info(c, id))?.ok_or_else(deleted)?;
    let path = crate::commands::articles::book_file(&state.root, &hash);
    let hrefs: Vec<String> = body_parts.iter().take(3).filter_map(|p| info.chapters.iter().find(|c| c.idx == p.idx).map(|c| c.href.clone())).collect();
    let excerpt = tauri::async_runtime::spawn_blocking(move || {
        let mut text = String::new();
        for h in hrefs {
            if let Ok(html) = crate::epub::read_chapter(&path, &h) {
                text.push_str(crate::epub::text_of_html(&html).trim());
                text.push('\n');
            }
            if text.chars().count() > crate::guide_ai::EXCERPT_CHARS {
                break;
            }
        }
        text.chars().take(crate::guide_ai::EXCERPT_CHARS).collect::<String>()
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?;
    let author = article.meta.author.clone();
    let input = crate::guide_ai::Input { title: &article.meta.title, author: author.as_deref(), parts: &titles, excerpt: &excerpt, spoilers };
    let user = crate::guide_ai::prompt(&input);
    let (text, usage) = crate::ai::complete_json(
        state.online.client(),
        provider,
        &key,
        &model,
        crate::guide_ai::SYSTEM,
        &user,
        crate::guide_ai::schema(),
        crate::guide_ai::JSON_FORMAT,
        4000,
        std::time::Duration::from_secs(180),
    )
    .await?;
    let out = crate::guide_ai::parse(&text)?;
    let guide = crate::guide_ai::to_guide(&out, &article.meta.title, author.as_deref(), &model, spoilers);
    state.db.with_conn(|c| crate::ai::record(c, provider, &model, &article.meta.title, usage, crate::db::now_ms()).map(|_| ()))?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&file, serde_json::to_string(&guide)?)?;
    Ok(Some(guide))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyWord {
    pub lemma: String,
    pub forms: Vec<String>,
    pub count: u32,
    pub rank: Option<u32>,
    pub status: Mark,
    /// 第一次出现的章和那句话
    pub chapter: i64,
    pub sentence: Option<String>,
    pub hl_start: usize,
    pub hl_end: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyList {
    pub words: Vec<StudyWord>,
    pub chapters: Vec<ChapterRow>,
}

/// 按门槛挑出的生词（`order`：chapter = 按第一次出现的先后，alpha = 按字母）和每章统计
#[tauri::command]
#[allow(clippy::too_many_arguments)]
/// `limit`：最多要多少个（书里出现次数多的优先）
pub async fn book_study_list(
    state: State<'_, AppState>,
    id: i64,
    level: u32,
    min_count: u32,
    include_saved: bool,
    scope: Scope,
    order: String,
    limit: Option<u32>,
) -> Result<StudyList> {
    let (study, _, _) = load(&state, id, scope).await?;
    let (saved, known) = saved_and_known(&state)?;
    let mut picked = study::pick(&study, Filter { level, min_count, include_saved }, &saved, &known);
    if let Some(n) = limit {
        picked = study::limit(picked, n as usize);
    }
    let chapters = study::chapter_rows(&study, &picked);
    let mut words: Vec<StudyWord> = picked
        .iter()
        .map(|(l, m)| StudyWord {
            lemma: l.lemma.clone(),
            forms: l.forms.clone(),
            count: l.count,
            rank: l.rank,
            status: *m,
            chapter: l.chapter,
            sentence: l.sentence.clone(),
            hl_start: l.hl_start,
            hl_end: l.hl_end,
        })
        .collect();
    if order == "alpha" {
        words.sort_by(|a, b| a.lemma.cmp(&b.lemma));
    }
    Ok(StudyList { words, chapters })
}
