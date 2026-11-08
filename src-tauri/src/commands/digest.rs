//! 精读本的命令：先看要做几步、估算花费（`book_digest_plan`），前端一步步调 `book_digest_step`（可以并发、停止），
//! 都做完后 `book_digest` 拼出完整结果。每步存成文件：`digests/<书的哈希>-<模型>-v<版本>/read-<k>.json`、`synth-<k>.json`。
//! 业务逻辑在 `crate::digest`。

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::commands::study::{deleted, effective, parts, scans, Scope};
use crate::db::articles;
use crate::digest::{self, ChapterRef, ChunkNotes, Digest, Synthesis};
use crate::error::{Error, Result};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);
const MAX_TOKENS: u32 = 8000;

/// 一本书的精读本要用的东西：书、哈希、分好的段（每章带 href）、模型
struct Ctx {
    title: String,
    author: Option<String>,
    hash: String,
    chunks: Vec<Vec<(ChapterRef, String)>>,
    provider: crate::ai::Provider,
    model: String,
    dir: PathBuf,
    words: usize,
    chapters: usize,
}

fn safe(model: &str) -> String {
    model.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect()
}

/// 默认范围里的正文各章（跳过前言、附赠内容），按篇幅分段
async fn ctx(state: &AppState, id: i64) -> Result<Ctx> {
    let article = state.db.with_conn(|c| articles::open(c, id))?.ok_or_else(deleted)?;
    let hash = state.db.with_conn(|c| articles::book_hash(c, id))?.ok_or_else(deleted)?;
    let info = state.db.with_conn(|c| articles::book_info(c, id))?.ok_or_else(deleted)?;
    let all = scans(state, id).await?;
    let ps = parts(&all);
    let (from, to) = effective(Scope::default(), &ps);
    let mut refs = Vec::new();
    for p in ps.iter().filter(|p| p.idx >= from && p.idx <= to && !p.extra) {
        let (Some(scan), Some(ch)) = (all.iter().find(|c| c.idx == p.idx), info.chapters.iter().find(|c| c.idx == p.idx)) else { continue };
        refs.push((ChapterRef { idx: p.idx, title: p.title.clone(), words: scan.tokens as usize }, ch.href.clone()));
    }
    let plain: Vec<ChapterRef> = refs.iter().map(|(r, _)| r.clone()).collect();
    let chunks = digest::chunks(&plain)
        .into_iter()
        .map(|g| g.into_iter().map(|r| { let href = refs.iter().find(|(x, _)| x.idx == r.idx).map(|(_, h)| h.clone()).unwrap_or_default(); (r, href) }).collect())
        .collect();
    let cfg = state.settings.get()?.ai;
    let provider = cfg.provider();
    let model = cfg.active_model().to_string();
    let dir = state.root.join("digests").join(format!("{hash}-{}-v{}", safe(&model), digest::VERSION));
    Ok(Ctx {
        title: article.meta.title,
        author: article.meta.author,
        hash,
        words: plain.iter().map(|c| c.words).sum(),
        chapters: plain.len(),
        chunks,
        provider,
        model,
        dir,
    })
}

fn step_file(c: &Ctx, step: usize) -> PathBuf {
    let n = c.chunks.len();
    if step < n {
        c.dir.join(format!("read-{step}.json"))
    } else {
        c.dir.join(format!("synth-{}.json", step - n))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestPlan {
    pub provider: &'static str,
    pub model: String,
    /// 有没有填 API Key
    pub has_key: bool,
    /// 通读的段数；总步数 = 段数 + 4
    pub chunks: usize,
    pub steps: usize,
    /// 已经做完（存了文件）的步
    pub done: Vec<usize>,
    pub chapters: usize,
    pub words: usize,
    /// 估算的总花费（美元）和还要花的
    pub cost_usd: f64,
    pub remaining_usd: f64,
}

/// 精读本要做几步、做完了哪些、估算花费
#[tauri::command]
pub async fn book_digest_plan(state: State<'_, AppState>, id: i64) -> Result<DigestPlan> {
    let c = ctx(&state, id).await?;
    let steps = c.chunks.len() + digest::SYNTH_STEPS;
    let done: Vec<usize> = (0..steps).filter(|&s| step_file(&c, s).exists()).collect();
    let (input, output) = digest::estimate_tokens(c.words, c.chapters);
    let cost = crate::ai::cost_usd(c.provider, &c.model, crate::ai::Usage { input, cached: 0, output }, crate::db::now_ms());
    let has_key = crate::online::credentials::read(c.provider.key_id()).ok().flatten().is_some();
    Ok(DigestPlan {
        provider: c.provider.name(),
        model: c.model.clone(),
        has_key,
        chunks: c.chunks.len(),
        steps,
        remaining_usd: cost * (steps - done.len()) as f64 / steps.max(1) as f64,
        done,
        chapters: c.chapters,
        words: c.words,
        cost_usd: cost,
    })
}

fn read_notes(c: &Ctx) -> Result<Vec<ChunkNotes>> {
    (0..c.chunks.len())
        .map(|k| {
            let t = std::fs::read_to_string(step_file(c, k))
                .map_err(|_| Error::Other(crate::i18n::t("通读还没做完", "The read-through is not finished yet").into()))?;
            Ok(serde_json::from_str(&t)?)
        })
        .collect()
}

/// 做第 `step` 步（已经存过就直接返回）：前面几步是逐段通读，最后四步是综合（要等通读全部做完）
#[tauri::command]
pub async fn book_digest_step(state: State<'_, AppState>, id: i64, step: usize) -> Result<()> {
    let c = ctx(&state, id).await?;
    let file = step_file(&c, step);
    if file.exists() {
        return Ok(());
    }
    let n = c.chunks.len();
    if step >= n + digest::SYNTH_STEPS {
        return Err(Error::Other(format!("step {step} out of range")));
    }
    let key = crate::online::credentials::read(c.provider.key_id())?.ok_or_else(|| {
        Error::Online(crate::tf!("还没有填写 {} API Key（设置 › AI 例句）", "No {} API key yet (Settings › AI examples)", c.provider.name()))
    })?;
    let author = c.author.as_deref();
    let (output, usage) = if step < n {
        // 读出这一段各章的正文
        let path = crate::commands::articles::book_file(&state.root, &c.hash);
        let group = c.chunks[step].clone();
        let texts = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<(ChapterRef, String)>> {
            group
                .into_iter()
                .map(|(r, href)| {
                    let html = crate::epub::read_chapter(&path, &href)?;
                    let text = crate::epub::text_of_html(&html);
                    Ok((r, digest::clip_words(&text, digest::MAX_CHAPTER_WORDS).to_string()))
                })
                .collect()
        })
        .await
        .map_err(|e| Error::Other(e.to_string()))??;
        let user = digest::read_prompt(&c.title, author, &texts);
        let (text, usage) = crate::ai::complete_json(
            state.online.client(),
            c.provider,
            &key,
            &c.model,
            digest::READ_SYSTEM,
            &user,
            digest::read_schema(),
            digest::READ_FORMAT,
            MAX_TOKENS,
            TIMEOUT,
        )
        .await?;
        let notes = digest::check_chunk(&digest::parse_read(&text)?, &texts);
        tracing::info!(id, step, quotes = notes.quotes.len(), dropped = notes.dropped, "精读本：通读一段");
        (serde_json::to_string(&notes)?, usage)
    } else {
        let s = step - n;
        let (chapters, quotes, _) = digest::merge(read_notes(&c)?);
        let notes = digest::notes_text(&c.title, author, &chapters, &quotes);
        let (text, usage) = crate::ai::complete_json(
            state.online.client(),
            c.provider,
            &key,
            &c.model,
            digest::SYNTH_SYSTEM,
            &digest::synth_prompt(&notes, s),
            digest::synth_schema(s),
            digest::synth_format(s),
            MAX_TOKENS,
            TIMEOUT,
        )
        .await?;
        // 先确认能解析再存
        digest::merge_synth(&mut Synthesis::default(), &text)?;
        (text, usage)
    };
    state.db.with_conn(|conn| crate::ai::record(conn, c.provider, &c.model, &c.title, usage, crate::db::now_ms()).map(|_| ()))?;
    std::fs::create_dir_all(&c.dir)?;
    // 先写临时文件再改名：并发的几步、中途退出都不会留下半个文件
    let tmp = file.with_extension("tmp");
    std::fs::write(&tmp, output)?;
    std::fs::rename(&tmp, &file)?;
    Ok(())
}

/// 全部步骤都做完时拼出精读本；还没做完返回 None
#[tauri::command]
pub async fn book_digest(state: State<'_, AppState>, id: i64) -> Result<Option<Digest>> {
    let c = ctx(&state, id).await?;
    let steps = c.chunks.len() + digest::SYNTH_STEPS;
    if !(0..steps).all(|s| step_file(&c, s).exists()) {
        return Ok(None);
    }
    let (chapters, quotes, dropped) = digest::merge(read_notes(&c)?);
    let mut synthesis = Synthesis::default();
    for s in 0..digest::SYNTH_STEPS {
        digest::merge_synth(&mut synthesis, &std::fs::read_to_string(step_file(&c, c.chunks.len() + s))?)?;
    }
    digest::tidy(&mut synthesis, &quotes);
    Ok(Some(Digest { model: c.model, chapters, quotes, synthesis, dropped }))
}
