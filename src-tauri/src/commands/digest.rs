//! 精读本的命令：先看要做几步、估算花费（`book_digest_plan`），前端一步步调 `book_digest_step`（可以并发、停止），
//! 都做完后 `book_digest` 拼出完整结果。每步存成文件：`digests/<书的哈希>-<模型>-v<版本>/read-<k>.json`、
//! `synth-<k>.json`、`scene-<k>.json`；公版判断存在 `rights.json`。
//! 业务逻辑在 `crate::digest`。

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::app_state::AppState;
use crate::commands::study::{deleted, effective, parts, scans, Scope};
use crate::db::articles;
use crate::digest::{self, ChapterRef, ChunkNotes, Digest, Rights, Scene, Synthesis};
use crate::error::{Error, Result};

const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(900);
/// 单次输出上限：DeepSeek flash 允许到 39 万，Claude Opus / Sonnet 都在 3.2 万以上。
/// 逐章复述一段八章约一万字，8000 不够；仍被截断时会把这一段拆小再读
const MAX_TOKENS: u32 = 32_000;

/// 一本书的精读本要用的东西：书、哈希、分好的段（每章带正文）、模型
struct Ctx {
    title: String,
    author: Option<String>,
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

/// 最近一本书的章（书的哈希 → 各章标题和正文）：每一步都要用，不重复读文件
static UNITS: std::sync::Mutex<Option<(String, std::sync::Arc<Vec<(String, String)>>)>> = std::sync::Mutex::new(None);

/// 默认范围里的正文（跳过前言、附赠内容），按书里的章号切成章（见 `digest::build_units`），按篇幅分段。
/// 章的序号是精读本自己的（从 1 起），不是 epub 里的文件序号
async fn ctx(state: &AppState, id: i64) -> Result<Ctx> {
    let article = state.db.with_conn(|c| articles::open(c, id))?.ok_or_else(deleted)?;
    let hash = state.db.with_conn(|c| articles::book_hash(c, id))?.ok_or_else(deleted)?;
    let cached = UNITS.lock().ok().and_then(|g| g.as_ref().filter(|(h, _)| *h == hash).map(|(_, u)| u.clone()));
    let units = match cached {
        Some(u) => u,
        None => {
            let info = state.db.with_conn(|c| articles::book_info(c, id))?.ok_or_else(deleted)?;
            let all = scans(state, id).await?;
            let ps = parts(&all);
            let (from, to) = effective(Scope::default(), &ps);
            let files: Vec<(String, String)> = ps
                .iter()
                .filter(|p| p.idx >= from && p.idx <= to && !p.extra)
                .filter_map(|p| info.chapters.iter().find(|c| c.idx == p.idx).map(|c| (p.title.clone(), c.href.clone())))
                .collect();
            let path = crate::commands::articles::book_file(&state.root, &hash);
            let units = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<(String, String)>> {
                let texts = files
                    .into_iter()
                    .map(|(title, href)| Ok((title, crate::epub::text_of_html(&crate::epub::read_chapter(&path, &href)?))))
                    .collect::<Result<Vec<_>>>()?;
                Ok(digest::build_units(&texts))
            })
            .await
            .map_err(|e| Error::Other(e.to_string()))??;
            let units = std::sync::Arc::new(units);
            if let Ok(mut g) = UNITS.lock() {
                *g = Some((hash.clone(), units.clone()));
            }
            units
        }
    };
    let refs: Vec<(ChapterRef, String)> = units
        .iter()
        .enumerate()
        .map(|(i, (title, text))| (ChapterRef { idx: i as i64 + 1, title: title.clone(), words: text.split_whitespace().count() }, text.clone()))
        .collect();
    let plain: Vec<ChapterRef> = refs.iter().map(|(r, _)| r.clone()).collect();
    let chunks = digest::chunks(&plain)
        .into_iter()
        .map(|g| g.into_iter().map(|r| (r.clone(), refs[(r.idx - 1) as usize].1.clone())).collect())
        .collect();
    let cfg = state.settings.get()?.ai;
    let provider = cfg.provider();
    let model = cfg.active_model().to_string();
    let dir = state.root.join("digests").join(format!("{hash}-{}-v{}", safe(&model), digest::VERSION));
    Ok(Ctx {
        title: article.meta.title,
        author: article.meta.author,
        words: plain.iter().map(|c| c.words).sum(),
        chapters: plain.len(),
        chunks,
        provider,
        model,
        dir,
    })
}

/// 第几步存在哪个文件：先是各段通读，然后四次综合，最后几次名场面
fn step_file(c: &Ctx, step: usize) -> PathBuf {
    let n = c.chunks.len();
    if step < n {
        c.dir.join(format!("read-{step}.json"))
    } else if step < n + digest::SYNTH_STEPS {
        c.dir.join(format!("synth-{}.json", step - n))
    } else {
        c.dir.join(format!("scene-{}.json", step - n - digest::SYNTH_STEPS))
    }
}

fn total_steps(c: &Ctx) -> usize {
    c.chunks.len() + digest::AFTER_READ_STEPS
}

/// 是不是公版书：查一次维基百科（作者卒年），结果存下；查不到（没有作者、联不上网）按非公版
async fn rights(c: &Ctx) -> Rights {
    let file = c.dir.join("rights.json");
    if let Some(r) = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<Rights>(&t).ok()) {
        return r;
    }
    let died = match c.author.as_deref() {
        Some(a) => crate::wiki::author_died(a).await,
        None => Ok(None),
    };
    let year = digest::year_of_ms(crate::db::now_ms());
    let r = Rights { author_died: died.as_ref().ok().copied().flatten(), public_domain: digest::is_public_domain(died.as_ref().ok().copied().flatten(), year) };
    // 联网失败时不存，下次再查
    if died.is_ok() {
        let _ = std::fs::create_dir_all(&c.dir).and_then(|_| std::fs::write(&file, serde_json::to_string(&r).unwrap_or_default()));
    }
    r
}

/// 选出的名场面（非公版书只留开头），按编号
fn scenes_of(chapters: &[digest::ChapterNote], rights: &Rights) -> Vec<Scene> {
    let mut scenes = digest::select_scenes(chapters);
    if !rights.public_domain {
        scenes.iter_mut().for_each(digest::clip_scene);
    }
    scenes
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DigestPlan {
    pub provider: &'static str,
    pub model: String,
    /// 有没有填 API Key
    pub has_key: bool,
    /// 通读的段数；总步数 = 段数 + 4（综合）+ 4（名场面）
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
    let steps = total_steps(&c);
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

fn add(a: crate::ai::Usage, b: crate::ai::Usage) -> crate::ai::Usage {
    crate::ai::Usage { input: a.input + b.input, cached: a.cached + b.cached, output: a.output + b.output }
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

/// 做第 `step` 步（已经存过就直接返回）：前面几步是逐段通读，后面是综合和名场面（要等通读全部做完）
#[tauri::command]
pub async fn book_digest_step(state: State<'_, AppState>, id: i64, step: usize) -> Result<()> {
    let c = ctx(&state, id).await?;
    let file = step_file(&c, step);
    if file.exists() {
        return Ok(());
    }
    let n = c.chunks.len();
    if step >= total_steps(&c) {
        return Err(Error::Other(format!("step {step} out of range")));
    }
    // 名场面：这一步分到的几段；一段都没有就不用调模型
    let scene_part = if step >= n + digest::SYNTH_STEPS {
        let k = step - n - digest::SYNTH_STEPS;
        let (chapters, _, _) = digest::merge(read_notes(&c)?);
        let r = rights(&c).await;
        let part: Vec<Scene> = scenes_of(&chapters, &r).into_iter().skip(k * digest::SCENES_PER_STEP).take(digest::SCENES_PER_STEP).collect();
        if part.is_empty() {
            std::fs::create_dir_all(&c.dir)?;
            std::fs::write(&file, r#"{"scenes":[]}"#)?;
            return Ok(());
        }
        Some((part, chapters))
    } else {
        None
    };
    let key = crate::online::credentials::read(c.provider.key_id())?.ok_or_else(|| {
        Error::Online(crate::tf!("还没有填写 {} API Key（设置 › AI 例句）", "No {} API key yet (Settings › AI examples)", c.provider.name()))
    })?;
    let author = c.author.as_deref();
    let (output, usage) = if step < n {
        // 这一段各章的正文（太长的章只发前面一部分）
        let texts: Vec<(ChapterRef, String)> = c.chunks[step].iter().map(|(r, t)| (r.clone(), digest::clip_words(t, digest::MAX_CHAPTER_WORDS).to_string())).collect();
        // 输出超过上限被截断时把这一段拆成两半再读；只剩一章还写不下，就要求它写短一点
        let mut queue: std::collections::VecDeque<(Vec<(ChapterRef, String)>, bool)> = std::collections::VecDeque::from([(texts, false)]);
        let mut parts: Vec<ChunkNotes> = Vec::new();
        let mut usage = crate::ai::Usage::default();
        while let Some((group, shorter)) = queue.pop_front() {
            let mut user = digest::read_prompt(&c.title, author, &group);
            if shorter {
                user.push_str(digest::SHORTER);
            }
            let res = crate::ai::complete_json(
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
            .await;
            match res {
                Ok((text, u)) => {
                    usage = add(usage, u);
                    parts.push(digest::check_chunk(&digest::parse_read(&text)?, &group));
                }
                Err(e) if crate::ai::is_truncated(&e) && (group.len() > 1 || !shorter) => {
                    tracing::info!(id, step, chapters = group.len(), "精读本：输出被截断，拆小再读");
                    if group.len() > 1 {
                        let mut a = group;
                        let b = a.split_off(a.len() / 2);
                        // 放回队首，保持章节顺序
                        queue.push_front((b, false));
                        queue.push_front((a, false));
                    } else {
                        queue.push_front((group, true));
                    }
                }
                Err(e) => return Err(e),
            }
        }
        let notes = digest::join_chunks(parts);
        tracing::info!(id, step, quotes = notes.quotes.len(), dropped = notes.dropped, "精读本：通读一段");
        (serde_json::to_string(&notes)?, usage)
    } else if let Some((part, chapters)) = scene_part {
        let (text, usage) = crate::ai::complete_json(
            state.online.client(),
            c.provider,
            &key,
            &c.model,
            digest::SCENE_SYSTEM,
            &digest::scene_prompt(&c.title, author, &part, &chapters),
            digest::scene_schema(),
            digest::SCENE_FORMAT,
            MAX_TOKENS,
            TIMEOUT,
        )
        .await?;
        digest::parse_scenes(&text)?;
        (text, usage)
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
    if !(0..total_steps(&c)).all(|s| step_file(&c, s).exists()) {
        return Ok(None);
    }
    let (mut chapters, quotes, dropped) = digest::merge(read_notes(&c)?);
    digest::cap_must_read(&mut chapters);
    let mut synthesis = Synthesis::default();
    for s in 0..digest::SYNTH_STEPS {
        digest::merge_synth(&mut synthesis, &std::fs::read_to_string(step_file(&c, c.chunks.len() + s))?)?;
    }
    digest::tidy(&mut synthesis, &quotes);
    let titles: Vec<(i64, String)> = chapters.iter().map(|ch| (ch.idx, ch.title.clone())).collect();
    digest::resolve_synthesis(&mut synthesis, &titles);
    let r = rights(&c).await;
    let mut scenes = scenes_of(&chapters, &r);
    for k in 0..digest::SCENE_STEPS {
        let t = std::fs::read_to_string(step_file(&c, c.chunks.len() + digest::SYNTH_STEPS + k))?;
        digest::apply_scene_notes(&mut scenes, &digest::parse_scenes(&t)?);
    }
    // 章节里的候选名场面只是中间结果，不发给前端
    chapters.iter_mut().for_each(|ch| ch.scene = None);
    Ok(Some(Digest { model: c.model, chapters, quotes, scenes, synthesis, author_died: r.author_died, public_domain: r.public_domain, dropped }))
}
