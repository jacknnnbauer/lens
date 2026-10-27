//! 全文索引的命令：后台建索引（一次一部，可取消）、反查、正文搜索。业务逻辑在 `crate::fulltext`。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;
use crate::db::dicts;
use crate::dict::LoadedDict;
use crate::error::{Error, Result};
use crate::fulltext::{self, ReverseWord, Snippet};

pub const EVENT_PROGRESS: &str = "fulltext:progress";

/// 建索引的队列。`running` 的读写都在 `queue` 锁内，避免「刚判断完没在跑、工作线程就退出」
#[derive(Default)]
pub struct Jobs {
    queue: Mutex<VecDeque<String>>,
    running: AtomicBool,
    cancel: AtomicBool,
    /// 正在建的：(词典 id, 已处理, 总数)
    current: Mutex<Option<(String, u64, u64)>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FtStatus {
    pub dict_id: String,
    pub name: String,
    /// none | queued | building | ready
    pub state: &'static str,
    pub done: u64,
    pub total: u64,
    /// 索引文件大小（字节）
    pub bytes: u64,
    /// 词典文件大小（字节），给用户估计索引会有多大
    pub dict_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    dict_id: String,
    /// building | ready | error | cancelled
    state: &'static str,
    done: u64,
    total: u64,
    error: Option<String>,
}

/// 已加载的本地词典，按词典库里的顺序
fn local_dicts(state: &AppState) -> Result<Vec<LoadedDict>> {
    let rows = state.db.with_conn(dicts::list)?;
    let mut out = Vec::new();
    for r in rows {
        if let Some(d) = state.dicts.get(&r.id)? {
            out.push(d);
        }
    }
    Ok(out)
}

#[tauri::command]
pub async fn fulltext_status(state: State<'_, AppState>) -> Result<Vec<FtStatus>> {
    let dir = state.dicts.index_dir().to_path_buf();
    let queued: Vec<String> = state.fulltext.queue.lock().map_err(|_| Error::LockPoisoned)?.iter().cloned().collect();
    let current = state.fulltext.current.lock().map_err(|_| Error::LockPoisoned)?.clone();
    let rows = state.db.with_conn(dicts::list)?;
    let mut out = Vec::new();
    for d in local_dicts(&state)? {
        let meta = d.source.meta();
        let path = fulltext::index_path(&dir, &meta.id);
        let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let dict_bytes = rows
            .iter()
            .find(|r| r.id == meta.id)
            .and_then(|r| r.path.as_deref())
            .and_then(|p| std::fs::metadata(p).ok())
            .map_or(0, |m| m.len());
        let (st, done, total) = match &current {
            Some((id, done, total)) if *id == meta.id => ("building", *done, *total),
            _ if queued.contains(&meta.id) => ("queued", 0, 0),
            _ if bytes > 0 && fulltext::open(&dir, &meta.id).is_some() => ("ready", 0, 0),
            _ => ("none", 0, 0),
        };
        out.push(FtStatus { dict_id: meta.id.clone(), name: meta.name.clone(), state: st, done, total, bytes, dict_bytes });
    }
    Ok(out)
}

/// 把这些词典排进建索引的队列（已有索引的会重建）
#[tauri::command]
pub async fn fulltext_build(app: AppHandle, state: State<'_, AppState>, dict_ids: Vec<String>) -> Result<()> {
    let jobs = &state.fulltext;
    let mut queue = jobs.queue.lock().map_err(|_| Error::LockPoisoned)?;
    let current = jobs.current.lock().map_err(|_| Error::LockPoisoned)?.as_ref().map(|c| c.0.clone());
    for id in dict_ids {
        if !queue.contains(&id) && current.as_deref() != Some(id.as_str()) {
            queue.push_back(id);
        }
    }
    jobs.cancel.store(false, Ordering::Relaxed);
    if !jobs.running.load(Ordering::Relaxed) && !queue.is_empty() {
        jobs.running.store(true, Ordering::Relaxed);
        let app = app.clone();
        std::thread::spawn(move || worker(&app));
    }
    Ok(())
}

fn worker(app: &AppHandle) {
    let state = app.state::<AppState>();
    let jobs = &state.fulltext;
    loop {
        let next = match jobs.queue.lock() {
            Ok(mut q) => {
                let n = q.pop_front();
                if n.is_none() {
                    jobs.running.store(false, Ordering::Relaxed);
                }
                n
            }
            Err(_) => {
                jobs.running.store(false, Ordering::Relaxed);
                None
            }
        };
        let Some(id) = next else { break };
        let emit = |st: &'static str, done: u64, total: u64, error: Option<String>| {
            let _ = app.emit(EVENT_PROGRESS, ProgressEvent { dict_id: id.clone(), state: st, done, total, error });
        };
        let Ok(Some(dict)) = state.dicts.get(&id) else {
            emit("error", 0, 0, Some(crate::tf!("词典没有加载", "The dictionary isn't loaded")));
            continue;
        };
        let set_current = |v: Option<(String, u64, u64)>| {
            if let Ok(mut c) = jobs.current.lock() {
                *c = v;
            }
        };
        let total = dict.source.entry_total();
        set_current(Some((id.clone(), 0, total)));
        emit("building", 0, total, None);
        let started = std::time::Instant::now();
        let result = fulltext::build(dict.source.as_ref(), state.dicts.index_dir(), &jobs.cancel, &mut |done, total| {
            set_current(Some((id.clone(), done, total)));
            emit("building", done, total, None);
        });
        set_current(None);
        match result {
            Ok(true) => {
                tracing::info!(dict = %dict.source.meta().name, secs = started.elapsed().as_secs(), "全文索引已建好");
                emit("ready", total, total, None);
            }
            Ok(false) => emit("cancelled", 0, total, None),
            Err(e) => {
                tracing::warn!(dict = %dict.source.meta().name, error = %e, "建全文索引失败");
                emit("error", 0, total, Some(e.to_string()));
            }
        }
    }
}

/// 停下正在建的索引，清空队列
#[tauri::command]
pub async fn fulltext_cancel(state: State<'_, AppState>) -> Result<()> {
    state.fulltext.queue.lock().map_err(|_| Error::LockPoisoned)?.clear();
    state.fulltext.cancel.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn fulltext_delete(state: State<'_, AppState>, dict_id: String) -> Result<()> {
    let building = state.fulltext.current.lock().map_err(|_| Error::LockPoisoned)?.as_ref().is_some_and(|c| c.0 == dict_id);
    if building {
        return Err(Error::Other(crate::tf!("这部词典的索引正在建，先停止再删除", "This index is being built — stop it first")));
    }
    let path = fulltext::index_path(state.dicts.index_dir(), &dict_id);
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseResult {
    pub words: Vec<ReverseWord>,
    /// 参与反查的（建了索引的）词典数
    pub indexed: usize,
}

/// 反查：中文 → 英文词（各词典释义里有这段中文的词，汇总排序）
#[tauri::command]
pub async fn reverse_lookup(state: State<'_, AppState>, query: String, limit: Option<usize>) -> Result<ReverseResult> {
    let dicts = local_dicts(&state)?;
    let dir = state.dicts.index_dir().to_path_buf();
    let (coca, wordnet) = (state.coca(), state.wordnet());
    tauri::async_runtime::spawn_blocking(move || -> Result<ReverseResult> {
        let mut hits = Vec::new();
        for d in &dicts {
            let meta = d.source.meta();
            let Some(conn) = fulltext::open(&dir, &meta.id) else { continue };
            hits.push((meta.name.clone(), fulltext::reverse_hits(&conn, query.trim())?));
        }
        let freq = crate::freq::get();
        let rank = |w: &str| match &coca {
            Some(c) => c.lookup(w).first().map(|e| e.rank),
            None => freq.rank(w),
        };
        let mut words = fulltext::rank_reverse(query.trim(), &hits, &rank, limit.unwrap_or(80).min(300));
        for w in &mut words {
            w.pos = wordnet
                .as_ref()
                .and_then(|wn| crate::wordnet::pos_labels(wn.poses(&w.word)))
                .or_else(|| crate::gloss::pos_by_suffix(&w.word).map(str::to_string));
        }
        Ok(ReverseResult { words, indexed: hits.len() })
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FtHit {
    pub headword: String,
    pub snippet: Option<Snippet>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FtGroup {
    pub dict_id: String,
    pub name: String,
    pub total: u64,
    pub hits: Vec<FtHit>,
}

/// 每部词典一页显示这么多条
const PAGE: u32 = 20;

/// 正文搜索：词条正文（释义、例句）里出现这段文字的词条，按词典分组；`dict_id` 给定时只搜这一部（翻页用）
#[tauri::command]
pub async fn fulltext_search(state: State<'_, AppState>, query: String, dict_id: Option<String>, offset: Option<u32>) -> Result<Vec<FtGroup>> {
    let dicts: Vec<LoadedDict> =
        local_dicts(&state)?.into_iter().filter(|d| dict_id.as_ref().is_none_or(|id| *id == d.source.meta().id)).collect();
    let dir = state.dicts.index_dir().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<FtGroup>> {
        let mut out = Vec::new();
        for d in &dicts {
            let meta = d.source.meta();
            let Some(conn) = fulltext::open(&dir, &meta.id) else { continue };
            let (total, heads) = fulltext::search(&conn, query.trim(), PAGE, offset.unwrap_or(0))?;
            if total == 0 {
                continue;
            }
            let mut hits: Vec<FtHit> = Vec::new();
            for h in heads {
                if hits.iter().any(|x| x.headword == h) {
                    continue;
                }
                let snippet = d
                    .source
                    .lookup(&h)
                    .unwrap_or_default()
                    .iter()
                    .find_map(|e| fulltext::snippet(&fulltext::entry_text(&e.body), query.trim(), 50));
                hits.push(FtHit { headword: h, snippet });
            }
            out.push(FtGroup { dict_id: meta.id.clone(), name: meta.name.clone(), total, hits });
        }
        Ok(out)
    })
    .await
    .map_err(|e| Error::Other(e.to_string()))?
}
