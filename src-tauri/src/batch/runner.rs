//! 批量导出流水线（SPEC 12.3）：
//!
//! ```text
//! 待处理条目 ─► [查词 worker × N] ─► chan(8) ─► [渲染 × 1] ─► chan(16) ─► [写盘 × 1]
//! ```
//!
//! 有界 channel 形成背压；每个词的结果写盘后即丢弃。单个词的错误记到 `batch_item.note` 后继续。

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;

use super::lookup::{lookup_word, BatchSources, WordResult};
use super::parse::{safe_file_stem, unique_stem};
use super::{report, sys, BatchManager, BatchRequest, Control, RenderJob, RenderReply, RenderWord, EVENT_PROGRESS, EVENT_RENDER};
use crate::app_state::AppState;
use crate::commands::lookup::sources_for_profile;
use crate::db::batch::{self as dbb, Counts, ItemRow};
use crate::db::now_ms;
use crate::error::{Error, Result};
use crate::export;

const WORD_QUEUE: usize = 8;
const WRITE_QUEUE: usize = 16;
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
const RENDER_TIMEOUT: Duration = Duration::from_secs(180);
const VOLUME_TIMEOUT: Duration = Duration::from_secs(900);
const WINDOW_READY_TIMEOUT: Duration = Duration::from_secs(30);
const TMP_DIR: &str = ".lens-tmp";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub batch_id: String,
    /// running | paused | cancelled | done | error
    pub state: String,
    pub counts: Counts,
    /// 等待渲染的词数（查词与渲染之间的队列）
    pub render_queue: usize,
    pub mem_mb: u64,
    pub mem_limit_mb: u64,
    pub recycles: u32,
    pub started_at: i64,
    /// 本次以来状态有变化的条目
    pub changed: Vec<ItemRow>,
    pub error: Option<String>,
}

/// 流水线各阶段共享的进度标记。
struct Shared {
    batch_id: String,
    changed: Mutex<HashSet<i64>>,
    dirty: AtomicBool,
    render_queue: AtomicUsize,
    recycles: AtomicU32,
    mem_mb: std::sync::atomic::AtomicU64,
}

impl Shared {
    fn touch(&self, seq: i64) {
        if let Ok(mut c) = self.changed.lock() {
            c.insert(seq);
        }
        self.dirty.store(true, Ordering::Relaxed);
    }
}

fn set_item(state: &AppState, shared: &Shared, seq: i64, item_state: &str, note: Option<&str>, files: Option<&[String]>) {
    if let Err(e) = state.db.with_conn(|c| dbb::set_item(c, &shared.batch_id, seq, item_state, note, files)) {
        tracing::warn!(error = %e, seq, "更新批量导出条目失败");
    }
    shared.touch(seq);
}

fn emit_progress(app: &AppHandle, shared: &Shared, batch_state: &str, started_at: i64, mem_limit_mb: u64, error: Option<String>) {
    let state = app.state::<AppState>();
    let seqs: Vec<i64> = shared.changed.lock().map(|mut c| c.drain().collect()).unwrap_or_default();
    let changed = state.db.with_conn(|c| dbb::items_by_seq(c, &shared.batch_id, &seqs)).unwrap_or_default();
    let counts = state.db.with_conn(|c| dbb::counts(c, &shared.batch_id)).unwrap_or_default();
    let ev = ProgressEvent {
        batch_id: shared.batch_id.clone(),
        state: batch_state.to_string(),
        counts,
        render_queue: shared.render_queue.load(Ordering::Relaxed),
        mem_mb: shared.mem_mb.load(Ordering::Relaxed),
        mem_limit_mb,
        recycles: shared.recycles.load(Ordering::Relaxed),
        started_at,
        changed,
        error,
    };
    if let Err(e) = app.emit(EVENT_PROGRESS, ev) {
        tracing::warn!(error = %e, "推送批量导出进度失败");
    }
}

enum WriteJob {
    Word { seq: i64, word: String, notes: Vec<String>, pdf: Option<PathBuf>, html: Option<String> },
    Volume { seqs: Vec<(i64, Vec<String>)>, stem: String, pdf: Option<PathBuf>, html: Option<String> },
}

/// 在后台运行一个批次；结束（完成 / 取消 / 出错）后释放批次锁。
pub fn spawn(app: AppHandle, batch_id: String, ctl: Arc<Control>) {
    tauri::async_runtime::spawn(async move {
        let result = run(&app, &batch_id, &ctl).await;
        let state = app.state::<AppState>();
        let manager = app.state::<BatchManager>();
        manager.release(&batch_id);
        if let Err(e) = result {
            tracing::error!(batch = %batch_id, error = %e, "批量导出中止");
            let _ = state.db.with_conn(|c| dbb::set_state(c, &batch_id, "paused", now_ms()));
            let shared = new_shared(&batch_id);
            emit_progress(&app, &shared, "error", now_ms(), 0, Some(e.to_string()));
        }
    });
}

fn new_shared(batch_id: &str) -> Shared {
    Shared {
        batch_id: batch_id.to_string(),
        changed: Mutex::new(HashSet::new()),
        dirty: AtomicBool::new(true),
        render_queue: AtomicUsize::new(0),
        recycles: AtomicU32::new(0),
        mem_mb: std::sync::atomic::AtomicU64::new(0),
    }
}

/// 输出目录里已有的文件名主干（续传、重名判断用）。
fn existing_stems(dir: &Path) -> HashSet<String> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().to_lowercase()))
                .collect()
        })
        .unwrap_or_default()
}

async fn run(app: &AppHandle, batch_id: &str, ctl: &Arc<Control>) -> Result<()> {
    let state = app.state::<AppState>();
    let row = state.db.with_conn(|c| dbb::get(c, batch_id))?.ok_or_else(|| Error::Other(crate::i18n::t("找不到这个批次", "Batch not found").into()))?;
    let req: BatchRequest = serde_json::from_str(&row.request_json)?;
    let settings = state.settings.get()?;
    let (cfg_batch, cfg_online) = (settings.batch.clone(), settings.online.clone());
    let profile = if req.profile.is_empty() { settings.active_profile.clone() } else { req.profile.clone() };
    let sources = Arc::new(BatchSources::new(sources_for_profile(&state, &profile)?, req.offline_only));
    if sources.list.is_empty() {
        return Err(Error::Other(crate::i18n::t("当前配置组没有可用的词典", "The current profile has no usable dictionaries").into()));
    }

    state.db.with_conn(|c| {
        dbb::reset_in_flight(c, batch_id)?;
        dbb::set_state(c, batch_id, "running", now_ms())
    })?;
    let out_dir = PathBuf::from(&req.out_dir);
    let tmp_dir = out_dir.join(TMP_DIR);
    std::fs::create_dir_all(&tmp_dir)?;
    let pending: VecDeque<(i64, String)> = state.db.with_conn(|c| dbb::pending_items(c, batch_id))?.into();
    tracing::info!(batch = %batch_id, pending = pending.len(), merge = req.merge, "开始批量导出");

    let _awake = sys::KeepAwake::start();
    let started_at = now_ms();
    let shared = Arc::new(new_shared(batch_id));
    let finished = Arc::new(AtomicBool::new(false));

    // 进度推送（节流 200ms）
    let progress_task = {
        let (app, shared, ctl, finished) = (app.clone(), shared.clone(), ctl.clone(), finished.clone());
        let limit = cfg_batch.mem_limit_mb;
        tauri::async_runtime::spawn(async move {
            let mut last_mem = Instant::now() - Duration::from_secs(10);
            while !finished.load(Ordering::Relaxed) {
                tokio::time::sleep(PROGRESS_EVERY).await;
                if last_mem.elapsed() > Duration::from_secs(1) {
                    shared.mem_mb.store(tauri::async_runtime::spawn_blocking(sys::process_tree_mb).await.unwrap_or(0), Ordering::Relaxed);
                    last_mem = Instant::now();
                    shared.dirty.store(true, Ordering::Relaxed);
                }
                if shared.dirty.swap(false, Ordering::Relaxed) {
                    let s = if ctl.is_paused() { "paused" } else { "running" };
                    emit_progress(&app, &shared, s, started_at, limit, None);
                }
            }
        })
    };

    // ① 查词
    let queue = Arc::new(Mutex::new(pending));
    let (word_tx, mut word_rx) = mpsc::channel::<WordResult>(WORD_QUEUE);
    let mut workers = Vec::new();
    for _ in 0..cfg_batch.lookup_concurrency.clamp(1, 8) {
        let (app, queue, word_tx, ctl, shared, sources, cfg) =
            (app.clone(), queue.clone(), word_tx.clone(), ctl.clone(), shared.clone(), sources.clone(), cfg_online.clone());
        workers.push(tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            loop {
                ctl.wait_while_paused().await;
                if ctl.is_cancelled() {
                    break;
                }
                let Some((seq, word)) = queue.lock().ok().and_then(|mut q| q.pop_front()) else { break };
                set_item(&state, &shared, seq, dbb::QUERYING, None, None);
                let result = lookup_word(&state, &sources, &cfg, seq, &word).await;
                shared.render_queue.fetch_add(1, Ordering::Relaxed);
                if word_tx.send(result).await.is_err() {
                    break;
                }
            }
        }));
    }
    drop(word_tx);

    // ③ 写盘
    let (write_tx, write_rx) = mpsc::channel::<WriteJob>(WRITE_QUEUE);
    let writer = {
        let (app, shared) = (app.clone(), shared.clone());
        let mut used = existing_stems(&out_dir);
        let out_dir = out_dir.clone();
        tauri::async_runtime::spawn(async move { write_stage(&app, &shared, &out_dir, &mut used, write_rx).await })
    };

    // ② 渲染（本任务）
    let render_result = render_stage(app, &req, &cfg_batch, ctl, &shared, &tmp_dir, &mut word_rx, &write_tx).await;
    drop(write_tx);
    for w in workers {
        let _ = w.await;
    }
    let _ = writer.await;

    finished.store(true, Ordering::Relaxed);
    let _ = progress_task.await;
    render_result?;

    let counts = state.db.with_conn(|c| dbb::counts(c, batch_id))?;
    let final_state = if ctl.is_cancelled() {
        "cancelled"
    } else if counts.pending + counts.querying + counts.rendering == 0 {
        "done"
    } else {
        "paused"
    };
    // 已完成的部分总是生成目录（取消时也保留已导出的文件）
    let items = state.db.with_conn(|c| dbb::all_items(c, batch_id))?;
    if let Err(e) = report::write(&out_dir, &req, &items) {
        tracing::warn!(error = %e, "生成总目录失败");
    }
    if final_state != "paused" {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    state.db.with_conn(|c| dbb::set_state(c, batch_id, final_state, now_ms()))?;
    shared.dirty.store(true, Ordering::Relaxed);
    emit_progress(app, &shared, final_state, started_at, cfg_batch.mem_limit_mb, None);
    tracing::info!(batch = %batch_id, state = final_state, done = counts.done, failed = counts.failed, "批量导出结束");
    Ok(())
}

/// 因内存重建渲染窗口的最小间隔（词数）
const MIN_WORDS_PER_RECYCLE: usize = 25;
/// 渲染窗口池上限
const MAX_RENDER_WORKERS: usize = 4;

/// 等导出窗口 `label` 就绪：不存在就新建；`recycle` 为真时先销毁再建（释放 WebView2 内存）。
async fn ensure_render_window(app: &AppHandle, label: &str, recycle: bool) -> Result<()> {
    let manager = app.state::<BatchManager>();
    if recycle {
        if let Some(w) = app.get_webview_window(label) {
            manager.forget_ready(label);
            w.destroy()?;
            // 等窗口真正关闭
            for _ in 0..50 {
                if app.get_webview_window(label).is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
    let mut ready = manager.ready_receiver();
    if export::ensure_window_labeled(app, label)? {
        manager.forget_ready(label);
    }
    // 新建的窗口（或单词导出建好、还没报到的窗口）要等它自己报告就绪
    let wait = async {
        while !manager.is_ready(label) {
            if ready.changed().await.is_err() {
                break;
            }
        }
    };
    if tokio::time::timeout(WINDOW_READY_TIMEOUT, wait).await.is_err() {
        return Err(Error::Other(crate::i18n::t("渲染窗口启动超时", "The render window took too long to start").into()));
    }
    Ok(())
}

async fn render(app: &AppHandle, label: &str, job: RenderJob, timeout: Duration) -> Result<RenderReply> {
    let manager = app.state::<BatchManager>();
    let job_id = job.job_id.clone();
    let rx = manager.expect_reply(&job_id)?;
    if let Err(e) = app.emit_to(label, EVENT_RENDER, job) {
        manager.forget_reply(&job_id);
        return Err(Error::Other(crate::tf!("无法发送渲染任务：{}", "Could not send the render job: {}", e)));
    }
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(r)) => Ok(r),
        Ok(Err(_)) => Err(Error::Other(crate::i18n::t("渲染窗口没有回应", "The render window did not respond").into())),
        Err(_) => {
            manager.forget_reply(&job_id);
            Err(Error::Other(crate::i18n::t("渲染超时", "Rendering timed out").into()))
        }
    }
}

fn job_base(req: &BatchRequest, mode: &'static str, title: String, words: Vec<RenderWord>, pdf_path: Option<PathBuf>) -> RenderJob {
    RenderJob {
        job_id: uuid::Uuid::new_v4().to_string(),
        mode,
        title,
        words,
        layout: req.layout.clone(),
        style: req.style.clone(),
        options: req.options.clone(),
        theme: req.theme.clone(),
        accent: req.accent.clone(),
        paper: req.paper,
        pdf_path: pdf_path.map(|p| p.to_string_lossy().into_owned()),
        want_html: req.html,
        lang: req.lang.clone(),
    }
}

fn no_result_note(r: &WordResult) -> String {
    let mut s = crate::i18n::t("所有词典均无结果", "No results in any dictionary").to_string();
    if !r.suggestions.is_empty() {
        s.push_str(&crate::tf!(" · 拼写建议：{}", " · Did you mean: {}", r.suggestions.join(crate::i18n::t("、", ", "))));
    }
    if !r.notes.is_empty() {
        s.push_str(&crate::tf!("（{}）", " ({})", r.notes.join(crate::i18n::t("；", "; "))));
    }
    s
}

#[allow(clippy::too_many_arguments)]
async fn render_stage(
    app: &AppHandle,
    req: &BatchRequest,
    cfg: &crate::settings::Batch,
    ctl: &Arc<Control>,
    shared: &Arc<Shared>,
    tmp_dir: &Path,
    word_rx: &mut mpsc::Receiver<WordResult>,
    write_tx: &mpsc::Sender<WriteJob>,
) -> Result<()> {
    if !req.merge {
        return render_pool(app, req, cfg, ctl, shared, tmp_dir, word_rx, write_tx).await;
    }
    let state = app.state::<AppState>();
    let mut volume: Vec<WordResult> = Vec::new();
    let total = state.db.with_conn(|c| dbb::counts(c, &shared.batch_id))?.total as usize;
    let volume_size = cfg.merge_volume_size.max(1);
    let volumes_total = total.div_ceil(volume_size);
    // 续传时接着已有的卷号：已完成条目引用的不同文件数 ÷ 每卷的格式数
    let formats_per_volume = (usize::from(req.pdf) + usize::from(req.html)).max(1);
    let done_files: HashSet<String> = state
        .db
        .with_conn(|c| dbb::all_items(c, &shared.batch_id))?
        .into_iter()
        .filter(|i| i.state == dbb::DONE)
        .flat_map(|i| i.files)
        .collect();
    let mut volume_no = done_files.len() / formats_per_volume;
    // 标签里已带本地日期（前端生成）
    let label_stem = safe_file_stem(&req.label);

    loop {
        let next = word_rx.recv().await;
        let end = next.is_none();
        if let Some(r) = next {
            shared.render_queue.fetch_sub(1, Ordering::Relaxed);
            if ctl.is_cancelled() {
                // 取消：已查到的不再渲染，条目退回待处理
                set_item(&state, shared, r.seq, dbb::PENDING, None, None);
                continue;
            }
            if r.cards.is_empty() {
                set_item(&state, shared, r.seq, dbb::FAILED, Some(&no_result_note(&r)), None);
                continue;
            }
            set_item(&state, shared, r.seq, dbb::RENDERING, None, None);
            volume.push(r);
        }
        // 合并模式：攒满一卷或输入结束时渲染；取消时攒着的退回待处理
        if req.merge && ctl.is_cancelled() {
            for r in volume.drain(..) {
                set_item(&state, shared, r.seq, dbb::PENDING, None, None);
            }
        }
        if req.merge && !volume.is_empty() && (volume.len() >= volume_size || end) {
            volume_no += 1;
            let stem = if volumes_total > 1 { crate::tf!("{}-第{}卷", "{}-vol{}", label_stem, volume_no) } else { label_stem.clone() };
            let title = if volumes_total > 1 { crate::tf!("{} · 第 {} 卷", "{} · Volume {}", req.label, volume_no) } else { req.label.clone() };
            // 查词并发完成顺序不定，卷内按输入顺序排
            volume.sort_by_key(|r| r.seq);
            ensure_render_window(app, export::EXPORT, true).await?;
            shared.recycles.fetch_add(u32::from(volume_no > 1), Ordering::Relaxed);
            let pdf = req.pdf.then(|| tmp_dir.join(format!("volume-{volume_no}.pdf")));
            let seqs: Vec<(i64, Vec<String>)> = volume.iter().map(|r| (r.seq, r.notes.clone())).collect();
            let words = std::mem::take(&mut volume)
                .into_iter()
                .map(|r| RenderWord { word: r.word, cards: r.cards, context: r.context })
                .collect();
            let job = job_base(req, "volume", title, words, pdf.clone());
            match render(app, export::EXPORT, job, VOLUME_TIMEOUT).await {
                Ok(RenderReply { ok: true, html, .. }) => {
                    if write_tx.send(WriteJob::Volume { seqs, stem, pdf, html }).await.is_err() {
                        break;
                    }
                }
                Ok(RenderReply { error, .. }) => {
                    let note = crate::tf!("渲染失败：{}", "Rendering failed: {}", error.unwrap_or_default());
                    for (seq, _) in seqs {
                        set_item(&state, shared, seq, dbb::FAILED, Some(&note), None);
                    }
                }
                Err(e) => {
                    for (seq, _) in seqs {
                        set_item(&state, shared, seq, dbb::FAILED, Some(&e.to_string()), None);
                    }
                }
            }
        }
        if end {
            break;
        }
    }
    Ok(())
}

/// 「每个词一个文件」：几个渲染窗口同时取词渲染、打印（WebView2 打印 PDF 每个文件约 2 秒起，串行很慢）。
#[allow(clippy::too_many_arguments)]
async fn render_pool(
    app: &AppHandle,
    req: &BatchRequest,
    cfg: &crate::settings::Batch,
    ctl: &Arc<Control>,
    shared: &Arc<Shared>,
    tmp_dir: &Path,
    word_rx: &mut mpsc::Receiver<WordResult>,
    write_tx: &mpsc::Sender<WriteJob>,
) -> Result<()> {
    let workers = cfg.render_workers.clamp(1, MAX_RENDER_WORKERS);
    // 内存上限按开始时的基线算：进程树里还有主窗口、弹窗与 GPU 进程，基线本身可能就接近上限；每多一个窗口多留一些
    let baseline = tauri::async_runtime::spawn_blocking(sys::process_tree_mb).await.unwrap_or(0);
    let extra = 250 * (workers as u64 - 1);
    let mem_limit = AtomicU64::new(cfg.mem_limit_mb.max(baseline + 400 + extra));
    tracing::info!(baseline, mem_limit = mem_limit.load(Ordering::Relaxed), workers, "批量导出内存基线");
    let rx = tokio::sync::Mutex::new(word_rx);
    let results = futures_util::future::join_all(
        (0..workers).map(|i| render_worker(app, req, cfg, ctl, shared, tmp_dir, &rx, write_tx, &mem_limit, export::pool_label(i))),
    )
    .await;
    // 多出来的窗口用完就关，0 号（单词导出也用）留着
    for i in 1..workers {
        if let Some(w) = app.get_webview_window(&export::pool_label(i)) {
            app.state::<BatchManager>().forget_ready(w.label());
            let _ = w.destroy();
        }
    }
    results.into_iter().collect::<Result<Vec<()>>>().map(|_| ())
}

#[allow(clippy::too_many_arguments)]
async fn render_worker(
    app: &AppHandle,
    req: &BatchRequest,
    cfg: &crate::settings::Batch,
    ctl: &Arc<Control>,
    shared: &Arc<Shared>,
    tmp_dir: &Path,
    rx: &tokio::sync::Mutex<&mut mpsc::Receiver<WordResult>>,
    write_tx: &mpsc::Sender<WriteJob>,
    mem_limit: &AtomicU64,
    label: String,
) -> Result<()> {
    let state = app.state::<AppState>();
    let recycle_every = cfg.render_recycle_every.max(1);
    let mut since_recycle = 0usize;
    let mut need_recycle = false;
    loop {
        // 取下一个词：锁只在取的时候持有
        let next = rx.lock().await.recv().await;
        let Some(r) = next else { break };
        shared.render_queue.fetch_sub(1, Ordering::Relaxed);
        if ctl.is_cancelled() {
            // 取消：已查到的不再渲染，条目退回待处理
            set_item(&state, shared, r.seq, dbb::PENDING, None, None);
            continue;
        }
        if r.cards.is_empty() {
            set_item(&state, shared, r.seq, dbb::FAILED, Some(&no_result_note(&r)), None);
            continue;
        }
        set_item(&state, shared, r.seq, dbb::RENDERING, None, None);
        let mem = shared.mem_mb.load(Ordering::Relaxed);
        // 内存触发的重建至少隔 MIN_WORDS_PER_RECYCLE 个词：WebView2 重建后很快又涨回原水位，
        // 不设下限就会每个词都重建一次（每次 1–2 秒）
        let over_mem = mem > mem_limit.load(Ordering::Relaxed) && since_recycle >= MIN_WORDS_PER_RECYCLE;
        let recycle = need_recycle || since_recycle >= recycle_every || over_mem;
        if recycle && since_recycle > 0 {
            shared.recycles.fetch_add(1, Ordering::Relaxed);
            tracing::info!(window = %label, since_recycle, mem, "重建渲染窗口");
        }
        if over_mem {
            // 这个水位是常态：下次超过它一截再重建
            mem_limit.fetch_max(mem + 200, Ordering::Relaxed);
        }
        if let Err(e) = ensure_render_window(app, &label, recycle && since_recycle > 0).await {
            set_item(&state, shared, r.seq, dbb::FAILED, Some(&e.to_string()), None);
            need_recycle = true;
            continue;
        }
        if recycle {
            since_recycle = 0;
            need_recycle = false;
        }
        let pdf = req.pdf.then(|| tmp_dir.join(format!("{}.pdf", r.seq)));
        let words = vec![RenderWord { word: r.word.clone(), cards: r.cards, context: r.context }];
        let job = job_base(req, "word", r.word.clone(), words, pdf.clone());
        since_recycle += 1;
        let t0 = std::time::Instant::now();
        let rendered = render(app, &label, job, RENDER_TIMEOUT).await;
        tracing::debug!(window = %label, word = %r.word, ms = t0.elapsed().as_millis() as u64, "渲染完成");
        match rendered {
            Ok(RenderReply { ok: true, html, .. }) => {
                let job = WriteJob::Word { seq: r.seq, word: r.word, notes: r.notes, pdf, html };
                if write_tx.send(job).await.is_err() {
                    break;
                }
            }
            Ok(RenderReply { error, .. }) => {
                set_item(&state, shared, r.seq, dbb::FAILED, Some(&crate::tf!("渲染失败：{}", "Rendering failed: {}", error.unwrap_or_default())), None);
            }
            Err(e) => {
                // 窗口没回应：下一个词前重建
                need_recycle = true;
                set_item(&state, shared, r.seq, dbb::FAILED, Some(&e.to_string()), None);
            }
        }
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("{}.part", path.extension().and_then(|e| e.to_str()).unwrap_or("bin")));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// 临时 PDF → 正式文件名；HTML 先写 .part 再改名。返回生成的文件名。
fn place_files(out_dir: &Path, stem: &str, pdf: Option<&Path>, html: Option<&str>) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();
    if let Some(p) = pdf {
        let name = format!("{stem}.pdf");
        std::fs::rename(p, out_dir.join(&name))?;
        files.push(name);
    }
    if let Some(h) = html {
        let name = format!("{stem}.html");
        write_atomic(&out_dir.join(&name), h.as_bytes())?;
        files.push(name);
    }
    Ok(files)
}

async fn write_stage(app: &AppHandle, shared: &Shared, out_dir: &Path, used: &mut HashSet<String>, mut rx: mpsc::Receiver<WriteJob>) {
    let state = app.state::<AppState>();
    while let Some(job) = rx.recv().await {
        match job {
            WriteJob::Word { seq, word, mut notes, pdf, html } => {
                let safe = safe_file_stem(&word);
                let stem = unique_stem(&safe, used);
                if stem != word {
                    notes.push(crate::tf!("文件名已规范为 {}", "File name normalized to {}", stem));
                }
                let (dir, stem2, pdf2) = (out_dir.to_path_buf(), stem.clone(), pdf.clone());
                let placed = tauri::async_runtime::spawn_blocking(move || place_files(&dir, &stem2, pdf2.as_deref(), html.as_deref())).await;
                match placed {
                    Ok(Ok(files)) => {
                        let note = (!notes.is_empty()).then(|| notes.join("；"));
                        set_item(&state, shared, seq, dbb::DONE, note.as_deref(), Some(&files));
                    }
                    Ok(Err(e)) => set_item(&state, shared, seq, dbb::FAILED, Some(&crate::tf!("写文件失败：{}", "Could not write the file: {}", e)), None),
                    Err(e) => set_item(&state, shared, seq, dbb::FAILED, Some(&crate::tf!("写文件失败：{}", "Could not write the file: {}", e)), None),
                }
            }
            WriteJob::Volume { seqs, stem, pdf, html } => {
                let stem = unique_stem(&stem, used);
                let (dir, stem2) = (out_dir.to_path_buf(), stem.clone());
                let placed = tauri::async_runtime::spawn_blocking(move || place_files(&dir, &stem2, pdf.as_deref(), html.as_deref())).await;
                let result = match placed {
                    Ok(r) => r.map_err(|e| e.to_string()),
                    Err(e) => Err(e.to_string()),
                };
                for (seq, notes) in seqs {
                    match &result {
                        Ok(files) => {
                            let note = (!notes.is_empty()).then(|| notes.join("；"));
                            set_item(&state, shared, seq, dbb::DONE, note.as_deref(), Some(files));
                        }
                        Err(e) => set_item(&state, shared, seq, dbb::FAILED, Some(&crate::tf!("写文件失败：{}", "Could not write the file: {}", e)), None),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_files_atomically() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let tmp = dir.path().join("1.pdf");
        std::fs::write(&tmp, b"%PDF")?;
        let files = place_files(dir.path(), "colour", Some(&tmp), Some("<p>x</p>"))?;
        assert_eq!(files, ["colour.pdf", "colour.html"]);
        assert!(!tmp.exists());
        assert_eq!(std::fs::read_to_string(dir.path().join("colour.html"))?, "<p>x</p>");
        assert!(!dir.path().join("colour.html.part").exists());
        assert!(existing_stems(dir.path()).contains("colour"));
        Ok(())
    }

    #[test]
    fn no_result_note_lists_suggestions() {
        let r = WordResult { seq: 0, word: "qwertyz".into(), cards: vec![], notes: vec![], suggestions: vec!["qwerty".into()], context: None };
        assert_eq!(no_result_note(&r), "所有词典均无结果 · 拼写建议：qwerty");
    }
}
