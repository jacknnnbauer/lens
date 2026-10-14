//! 批量导出命令（SPEC 5.1、12）。

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::app_state::AppState;
use crate::batch::lookup::BatchSources;
use crate::batch::parse::{self, ParseOutput};
use crate::batch::{self, sys, BatchManager, BatchRequest, RenderReply};
use crate::commands::lookup::sources_for_profile;
use crate::db::batch::{self as dbb, Counts, ItemRow};
use crate::db::now_ms;
use crate::error::{Error, Result};
use crate::export::{self, Paper};
use crate::online;

/// 预估：本地每词约 0.3 秒、未缓存的在线查询每词约 1 秒；PDF 每词约 150KB，HTML 约 120KB。
const SEC_PER_WORD_LOCAL: f64 = 0.3;
const SEC_PER_WORD_ONLINE: f64 = 1.0;
const PDF_BYTES_PER_WORD: u64 = 150 * 1024;
const HTML_BYTES_PER_WORD: u64 = 120 * 1024;

fn sources(state: &AppState, offline_only: bool) -> Result<BatchSources> {
    let profile = state.settings.get()?.active_profile;
    Ok(BatchSources::new(sources_for_profile(state, &profile)?, offline_only))
}

/// 整理词表：分隔、去标点、按原形去重、挑出无效条目、截断到上限。
#[tauri::command]
pub async fn batch_parse(state: State<'_, AppState>, text: String) -> Result<ParseOutput> {
    let max = state.settings.get()?.batch.max_words;
    let src = sources(&state, true)?;
    Ok(parse::parse_list(&text, max, &|w| src.exists(w)))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    pub words: usize,
    pub est_seconds: u64,
    pub est_bytes: u64,
    pub free_bytes: Option<u64>,
    /// 目录可写（不存在会先创建）
    pub writable: bool,
    pub write_error: Option<String>,
    /// 剩余空间 > 估算 × 2
    pub space_ok: bool,
    /// 参与的在线词典数；为 0 时下面两项无意义
    pub online_dicts: usize,
    /// 所有在线词典都有新鲜缓存的词数
    pub online_cached: usize,
    pub online_uncached: usize,
}

/// 开始导出前：创建目录并试写。
fn check_writable(dir: &std::path::Path) -> std::result::Result<(), String> {
    require_absolute(dir)?;
    std::fs::create_dir_all(dir).map_err(|e| crate::tf!("无法创建目录：{}", "Could not create the folder: {}", e))?;
    probe_write(dir)
}

/// 相对路径会落到程序的工作目录里（如安装目录），一律拒绝
fn require_absolute(dir: &std::path::Path) -> std::result::Result<(), String> {
    if dir.is_absolute() {
        Ok(())
    } else {
        Err(crate::i18n::t("保存位置需要是完整路径（从盘符开始）", "The save location must be a full path, starting with the drive letter").into())
    }
}

fn probe_write(dir: &std::path::Path) -> std::result::Result<(), String> {
    let probe = dir.join(".lens-write-test");
    std::fs::write(&probe, b"x").map_err(|e| crate::tf!("目录不可写：{}", "The folder is not writable: {}", e))?;
    let _ = std::fs::remove_file(probe);
    Ok(())
}

/// 预检只试写，不创建目录（目录名随词数变化，预检时建会留下一堆空目录）：
/// 目录不存在时试写最近一级已存在的上级目录。
fn check_writable_no_create(dir: &std::path::Path) -> std::result::Result<(), String> {
    require_absolute(dir)?;
    let mut probe = dir.to_path_buf();
    while !probe.exists() {
        probe = probe.parent().ok_or_else(|| crate::i18n::t("目录无效", "Invalid folder").to_string())?.to_path_buf();
    }
    if !probe.is_dir() {
        return Err(crate::i18n::t("保存位置不是文件夹", "The save location is not a folder").into());
    }
    probe_write(&probe)
}

#[tauri::command]
pub async fn batch_preflight(state: State<'_, AppState>, req: BatchRequest) -> Result<Preflight> {
    let src = sources(&state, req.offline_only)?;
    let cfg = state.settings.get()?.online;
    let onlines = src.onlines();
    let mut cached = 0;
    if !onlines.is_empty() {
        for w in &req.words {
            if onlines.iter().all(|d| online::fresh_cached(&state.db, d, w, &cfg).is_some()) {
                cached += 1;
            }
        }
    }
    let n = req.words.len();
    let uncached = if onlines.is_empty() { 0 } else { n - cached };
    let est_seconds = (n as f64 * SEC_PER_WORD_LOCAL + uncached as f64 * SEC_PER_WORD_ONLINE).ceil() as u64;
    let est_bytes = n as u64 * (if req.pdf { PDF_BYTES_PER_WORD } else { 0 } + if req.html { HTML_BYTES_PER_WORD } else { 0 });
    let dir = PathBuf::from(&req.out_dir);
    let (writable, write_error) = match tauri::async_runtime::spawn_blocking({
        let dir = dir.clone();
        move || check_writable_no_create(&dir)
    })
    .await
    {
        Ok(Ok(())) => (true, None),
        Ok(Err(e)) => (false, Some(e)),
        Err(e) => (false, Some(e.to_string())),
    };
    let free = sys::free_bytes(&dir);
    Ok(Preflight {
        words: n,
        est_seconds,
        est_bytes,
        free_bytes: free,
        writable,
        write_error,
        space_ok: free.is_none_or(|f| f > est_bytes * 2),
        online_dicts: onlines.len(),
        online_cached: cached,
        online_uncached: uncached,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub id: String,
    pub state: String,
    pub label: String,
    pub out_dir: String,
    pub pdf: bool,
    pub html: bool,
    pub merge: bool,
    pub layout: String,
    pub counts: Counts,
    pub created_at: i64,
    pub updated_at: i64,
    /// 正在本进程里运行
    pub active: bool,
}

fn summary(state: &AppState, manager: &BatchManager, row: dbb::BatchRow) -> Result<BatchSummary> {
    let req: BatchRequest = serde_json::from_str(&row.request_json)?;
    Ok(BatchSummary {
        counts: state.db.with_conn(|c| dbb::counts(c, &row.id))?,
        active: manager.active_id().as_deref() == Some(row.id.as_str()),
        id: row.id,
        state: row.state,
        label: req.label,
        out_dir: row.out_dir,
        pdf: req.pdf,
        html: req.html,
        merge: req.merge,
        layout: req.layout,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

#[tauri::command]
pub async fn start_batch(app: AppHandle, state: State<'_, AppState>, manager: State<'_, BatchManager>, mut req: BatchRequest) -> Result<String> {
    if req.words.is_empty() {
        return Err(Error::Other(crate::i18n::t("没有要导出的词", "No words to export").into()));
    }
    if !req.pdf && !req.html {
        return Err(Error::Other(crate::i18n::t("至少选择 PDF 或 HTML 一种格式", "Choose at least one format: PDF or HTML").into()));
    }
    let max = state.settings.get()?.batch.max_words;
    if req.words.len() > max {
        return Err(Error::Other(crate::tf!("单批最多 {} 个词（可在设置里调整）", "At most {} words per batch (adjustable in Settings)", max)));
    }
    if manager.active_id().is_some() {
        return Err(Error::Other(crate::i18n::t("已有一个批量导出在进行，请等它完成或取消", "Another batch export is running. Wait for it to finish or cancel it").into()));
    }
    check_writable(std::path::Path::new(&req.out_dir)).map_err(Error::Other)?;
    req.profile = state.settings.get()?.active_profile;
    let id = uuid::Uuid::new_v4().to_string();
    let json = serde_json::to_string(&req)?;
    state.db.with_conn(|c| dbb::create(c, &id, &json, &req.out_dir, &req.words, now_ms()))?;
    batch::start(&app, &manager, &id)?;
    Ok(id)
}

fn require_active(manager: &BatchManager, id: &str) -> Result<std::sync::Arc<batch::Control>> {
    match manager.active() {
        Some((active, ctl)) if active == id => Ok(ctl),
        _ => Err(Error::Other(crate::i18n::t("这个批次没有在运行", "This batch is not running").into())),
    }
}

#[tauri::command]
pub async fn pause_batch(app: AppHandle, state: State<'_, AppState>, manager: State<'_, BatchManager>, id: String) -> Result<()> {
    require_active(&manager, &id)?.set_paused(true);
    state.db.with_conn(|c| dbb::set_state(c, &id, "paused", now_ms()))?;
    let _ = app.emit(batch::EVENT_PROGRESS, serde_json::json!({ "batchId": id, "state": "paused", "partial": true }));
    Ok(())
}

/// 继续：本进程里暂停的直接放行；上次退出时没做完的重新启动流水线（已完成项跳过）。
#[tauri::command]
pub async fn resume_batch(app: AppHandle, state: State<'_, AppState>, manager: State<'_, BatchManager>, id: String) -> Result<()> {
    if let Ok(ctl) = require_active(&manager, &id) {
        ctl.set_paused(false);
        state.db.with_conn(|c| dbb::set_state(c, &id, "running", now_ms()))?;
        return Ok(());
    }
    state.db.with_conn(|c| dbb::get(c, &id))?.ok_or_else(|| Error::Other(crate::i18n::t("找不到这个批次", "Batch not found").into()))?;
    batch::start(&app, &manager, &id)
}

/// 取消：停止派发新词，正在渲染的做完；已导出的文件保留。不在运行的批次直接标记为已取消。
#[tauri::command]
pub async fn cancel_batch(state: State<'_, AppState>, manager: State<'_, BatchManager>, id: String) -> Result<()> {
    match require_active(&manager, &id) {
        Ok(ctl) => ctl.cancel(),
        Err(_) => state.db.with_conn(|c| dbb::set_state(c, &id, "cancelled", now_ms()))?,
    }
    Ok(())
}

/// 重试失败项：失败的条目退回待处理，再跑一遍（运行中则等本轮结束后再点）。
#[tauri::command]
pub async fn retry_failed_batch(app: AppHandle, state: State<'_, AppState>, manager: State<'_, BatchManager>, id: String) -> Result<usize> {
    if manager.active_id().as_deref() == Some(id.as_str()) {
        return Err(Error::Other(crate::i18n::t("请等这一轮导出结束后再重试", "Wait for this run to finish before retrying").into()));
    }
    let n = state.db.with_conn(|c| dbb::reset_failed(c, &id))?;
    if n > 0 {
        batch::start(&app, &manager, &id)?;
    }
    Ok(n)
}

#[tauri::command]
pub async fn list_unfinished_batches(state: State<'_, AppState>, manager: State<'_, BatchManager>) -> Result<Vec<BatchSummary>> {
    let rows = state.db.with_conn(dbb::unfinished)?;
    rows.into_iter().map(|r| summary(&state, &manager, r)).collect()
}

#[tauri::command]
pub async fn batch_status(state: State<'_, AppState>, manager: State<'_, BatchManager>, id: String) -> Result<BatchSummary> {
    let row = state.db.with_conn(|c| dbb::get(c, &id))?.ok_or_else(|| Error::Other(crate::i18n::t("找不到这个批次", "Batch not found").into()))?;
    summary(&state, &manager, row)
}

#[tauri::command]
pub async fn batch_items(state: State<'_, AppState>, id: String, filter: Option<String>, offset: i64, limit: i64) -> Result<Vec<ItemRow>> {
    let filter = filter.unwrap_or_else(|| "all".into());
    state.db.with_conn(|c| dbb::items(c, &id, &filter, offset.max(0), limit.clamp(1, 5000)))
}

/// 放弃一个未完成的批次（删除记录，已导出的文件保留）。
#[tauri::command]
pub async fn discard_batch(state: State<'_, AppState>, manager: State<'_, BatchManager>, id: String) -> Result<bool> {
    if manager.active_id().as_deref() == Some(id.as_str()) {
        return Err(Error::Other(crate::i18n::t("这个批次正在运行，请先取消", "This batch is running. Cancel it first").into()));
    }
    state.db.with_conn(|c| dbb::delete(c, &id))
}

/// 导出窗口：渲染任务完成（PDF 已由窗口自己打印到临时路径；HTML 以字符串返回）。
#[tauri::command]
pub async fn batch_render_done(manager: State<'_, BatchManager>, job_id: String, ok: bool, error: Option<String>, html: Option<String>) -> Result<()> {
    manager.reply(&job_id, RenderReply { ok, error, html });
    Ok(())
}

/// 导出窗口：页面加载完成，可以接任务。
#[tauri::command]
pub async fn batch_window_ready(window: tauri::WebviewWindow, manager: State<'_, BatchManager>) -> Result<()> {
    manager.window_ready(window.label());
    Ok(())
}

/// 导出窗口：打印带书签目录的 PDF（合并模式）。
#[tauri::command]
pub async fn export_print_pdf_outline(window: WebviewWindow, path: PathBuf, paper: Paper) -> Result<()> {
    export::print_pdf_with_outline(&window, &path, paper).await
}
