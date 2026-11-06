//! 查询管线（SPEC 7.6、13）：归一化 → 按场景配置组顺序并发查各词典（含在线词典）→ 逐部推送结果。
//!
//! 目前只做精确查询 + 全部未命中时的模糊候选；意图识别、词形还原、SymSpell 纠错后续补上。

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::app_state::AppState;
use crate::dict::{LoadedDict, RawEntry};
use crate::error::{Error, Result};
use crate::online::{self, OnlineDict, OnlineMode, Outcome, Trigger};
use crate::settings;

pub const EVENT_RESULT: &str = "lookup:result";
pub const EVENT_DONE: &str = "lookup:done";
const MAX_SUGGESTIONS: usize = 5;

/// 依次尝试的查询键：原样（只合并空白），再是去掉首尾标点的版本。
/// 这样 `e.g.`、`Mr.` 这类带标点的词头优先按原样命中，`ubiquitous,` 也能退回到 `ubiquitous`。
pub fn candidates(q: &str) -> Vec<String> {
    let raw = q.split_whitespace().collect::<Vec<_>>().join(" ");
    let norm = normalize_query(q);
    let mut out = Vec::new();
    for k in [raw, norm] {
        if !k.is_empty() && !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

/// 输入归一化：去首尾空白和标点（保留词内的 `-` `'` `.`）、合并连续空白。
/// 大小写与 Unicode 形式由索引的 `normalize_key` 处理。
pub fn normalize_query(q: &str) -> String {
    let is_edge = |c: char| c.is_whitespace() || (c.is_ascii_punctuation() && c != '-') || "，。、；：！？“”‘’「」《》（）…—".contains(c);
    let trimmed = q.trim_matches(is_edge);
    trimmed.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 参与查询的一部词典：本地词典或在线词典。
#[derive(Clone)]
pub enum Source {
    Local(LoadedDict),
    Online(OnlineDict),
}

impl Source {
    pub fn brief(&self) -> DictBrief {
        match self {
            Self::Local(d) => DictBrief { id: d.source.meta().id.clone(), name: d.source.meta().name.clone(), source: "local" },
            Self::Online(d) => DictBrief {
                id: d.meta.id.clone(),
                name: d.meta.name.clone(),
                source: match d.provider.mode() {
                    OnlineMode::Api => "api",
                    OnlineMode::Web => "web",
                },
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictBrief {
    pub id: String,
    pub name: String,
    /// local | api（在线接口）| web（网页模式）
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupHandle {
    pub request_id: String,
    pub query: String,
    pub normalized: String,
    /// 查询词的原形和它不同时（left → leave）：词典收了 left 就不会再查 leave，界面据此提示「也可能是 leave 的变形」
    pub base: Option<String>,
    /// 本次参与查询的词典（按配置组顺序），前端据此先画骨架
    pub dicts: Vec<DictBrief>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupResultEvent {
    pub request_id: String,
    pub dict_id: String,
    /// hit | miss | error；在线词典另有 manual（等用户点击）、web（网页模式，给出地址）、skipped（本地已命中，未查）
    pub status: &'static str,
    pub entries: Vec<RawEntry>,
    pub error: Option<String>,
    pub ms: f64,
    /// 在线结果来自缓存时的获取时间
    pub cached_at: Option<i64>,
    /// 缓存已过期（联网失败后的回退）
    pub stale: bool,
    /// 网页模式的原页面地址
    pub web_url: Option<String>,
}

impl LookupResultEvent {
    fn new(request_id: &str, dict_id: &str, status: &'static str) -> Self {
        Self {
            request_id: request_id.to_string(),
            dict_id: dict_id.to_string(),
            status,
            entries: Vec::new(),
            error: None,
            ms: 0.0,
            cached_at: None,
            stale: false,
            web_url: None,
        }
    }

    pub fn from_outcome(request_id: &str, dict_id: &str, o: Outcome, ms: f64) -> Self {
        Self { entries: o.entries, error: o.error, ms, cached_at: o.cached_at, stale: o.stale, ..Self::new(request_id, dict_id, o.status) }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupDoneEvent {
    pub request_id: String,
    /// 所有词典都未命中时的拼写候选
    pub suggestions: Vec<String>,
}

/// 进行中的查询的取消标记，按发起窗口区分（主窗口和弹窗的查询互不取消）。
#[derive(Default)]
pub struct Cancels(Mutex<HashMap<String, (String, Arc<AtomicBool>)>>);

impl Cancels {
    pub fn register(&self, id: &str, window: &str) -> Result<Arc<AtomicBool>> {
        let flag = Arc::new(AtomicBool::new(false));
        self.0
            .lock()
            .map_err(|_| Error::LockPoisoned)?
            .insert(id.to_string(), (window.to_string(), flag.clone()));
        Ok(flag)
    }

    pub fn cancel(&self, id: &str) -> Result<()> {
        if let Some((_, f)) = self.0.lock().map_err(|_| Error::LockPoisoned)?.get(id) {
            f.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    /// 取消同一窗口里除 `keep` 以外的查询（新查询发出时调用）。
    pub fn cancel_others(&self, keep: &str, window: &str) -> Result<()> {
        for (id, (w, f)) in self.0.lock().map_err(|_| Error::LockPoisoned)?.iter() {
            if id != keep && w == window {
                f.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    fn finish(&self, id: &str) {
        if let Ok(mut m) = self.0.lock() {
            m.remove(id);
        }
    }
}

fn concurrency() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get())
}

/// （是否本地词典, 结果事件）
type Task = (bool, LookupResultEvent);

fn spawn_local(tasks: &mut JoinSet<Task>, sem: &Arc<Semaphore>, request_id: &str, keys: &[String], dict: LoadedDict, cancelled: &Arc<AtomicBool>) {
    let (sem, keys, cancelled, request_id) = (sem.clone(), keys.to_vec(), cancelled.clone(), request_id.to_string());
    tasks.spawn(async move {
        let _permit = sem.acquire_owned().await;
        let dict_id = dict.source.meta().id.clone();
        let mut ev = LookupResultEvent::new(&request_id, &dict_id, "miss");
        if cancelled.load(Ordering::Relaxed) {
            return (true, ev);
        }
        let t = Instant::now();
        let res = tokio::task::spawn_blocking(move || -> Result<Vec<RawEntry>> {
            for k in &keys {
                let hits = dict.source.lookup(k)?;
                if !hits.is_empty() {
                    return Ok(hits);
                }
            }
            Ok(Vec::new())
        })
        .await;
        ev.ms = t.elapsed().as_secs_f64() * 1000.0;
        match res {
            Ok(Ok(entries)) if entries.is_empty() => {}
            Ok(Ok(entries)) => {
                ev.status = "hit";
                ev.entries = entries;
            }
            Ok(Err(e)) => {
                tracing::warn!(dict = %dict_id, error = %e, "查词失败");
                ev.status = "error";
                ev.error = Some(e.to_string());
            }
            // spawn_blocking 里 panic：记日志，当作该词典出错，不影响其他词典
            Err(e) => {
                tracing::error!(dict = %dict_id, error = %e, "查词任务 panic");
                ev.status = "error";
                ev.error = Some("查词时发生内部错误".to_string());
            }
        }
        (true, ev)
    });
}

fn spawn_online(tasks: &mut JoinSet<Task>, app: &AppHandle, request_id: &str, word: &str, dict: OnlineDict, cfg: &settings::Online) {
    let (app, request_id, word, cfg) = (app.clone(), request_id.to_string(), word.to_string(), cfg.clone());
    tasks.spawn(async move {
        let state = app.state::<AppState>();
        let t = Instant::now();
        let outcome = online::fetch(&state.db, state.online.client(), &dict, &word, &cfg).await;
        (false, LookupResultEvent::from_outcome(&request_id, dict.id(), outcome, t.elapsed().as_secs_f64() * 1000.0))
    });
}

fn web_event(request_id: &str, dict: &OnlineDict, word: &str) -> LookupResultEvent {
    let mut ev = LookupResultEvent::new(request_id, dict.id(), "web");
    ev.web_url = dict.provider.page_url(word).map(|u| u.to_string());
    if ev.web_url.is_none() {
        ev.status = "error";
        ev.error = Some("网址模板无效".into());
    }
    ev
}

/// 后台执行一次查询（SPEC 7.6、13）：本地词典每部一个阻塞任务，完成一个推送一个；
/// 在线词典按查询时机——每次都查的与本地并行，本地无结果才查的等本地全部结束，手动的只推送占位。
/// 全部结束后推送 done。事件只发给发起查询的窗口 `target`。
#[allow(clippy::too_many_arguments)]
pub async fn run(
    app: AppHandle,
    target: String,
    cancels: Arc<Cancels>,
    request_id: String,
    keys: Vec<String>,
    sources: Vec<Source>,
    cancelled: Arc<AtomicBool>,
    cfg: settings::Online,
) {
    let emit = |ev: LookupResultEvent| {
        if let Err(e) = app.emit_to(target.as_str(), EVENT_RESULT, ev) {
            tracing::warn!(error = %e, "推送查词结果失败");
        }
    };
    let word = keys.last().cloned().unwrap_or_default();
    let sem = Arc::new(Semaphore::new(concurrency()));
    let mut tasks = JoinSet::new();
    let mut locals = Vec::new();
    let mut deferred = Vec::new();

    for s in &sources {
        match s {
            Source::Local(d) => {
                locals.push(d.clone());
                spawn_local(&mut tasks, &sem, &request_id, &keys, d.clone(), &cancelled);
            }
            Source::Online(d) => match (d.provider.mode(), d.trigger(&cfg)) {
                (_, Trigger::LocalMiss) => deferred.push(d.clone()),
                (OnlineMode::Web, _) => emit(web_event(&request_id, d, &word)),
                (OnlineMode::Api, Trigger::Manual) => emit(LookupResultEvent::new(&request_id, d.id(), "manual")),
                (OnlineMode::Api, Trigger::Always) => spawn_online(&mut tasks, &app, &request_id, &word, d.clone(), &cfg),
            },
        }
    }

    // 本地全部结束后处理「本地无结果时才查」的在线词典
    let start_deferred = |deferred: &mut Vec<OnlineDict>, any_hit: bool, tasks: &mut JoinSet<Task>| {
        for d in std::mem::take(deferred) {
            if any_hit {
                emit(LookupResultEvent::new(&request_id, d.id(), "skipped"));
            } else if d.provider.mode() == OnlineMode::Web {
                emit(web_event(&request_id, &d, &word));
            } else {
                spawn_online(tasks, &app, &request_id, &word, d, &cfg);
            }
        }
    };

    let mut pending_local = locals.len();
    let mut any_hit = false;
    if pending_local == 0 && !cancelled.load(Ordering::Relaxed) {
        start_deferred(&mut deferred, false, &mut tasks);
    }
    while let Some(joined) = tasks.join_next().await {
        let Ok((is_local, ev)) = joined else { continue };
        if cancelled.load(Ordering::Relaxed) {
            continue;
        }
        any_hit |= ev.status == "hit";
        emit(ev);
        if is_local {
            pending_local -= 1;
            if pending_local == 0 {
                start_deferred(&mut deferred, any_hit, &mut tasks);
            }
        }
    }

    if !cancelled.load(Ordering::Relaxed) {
        let suggestions = if any_hit { Vec::new() } else { fuzzy_suggestions(&locals, &word) };
        let done = LookupDoneEvent { request_id: request_id.clone(), suggestions };
        if let Err(e) = app.emit_to(target.as_str(), EVENT_DONE, done) {
            tracing::warn!(error = %e, "推送查词完成事件失败");
        }
    }
    cancels.finish(&request_id);
}

/// 各词典的模糊候选按词典顺序合并去重。
pub fn fuzzy_suggestions(dicts: &[LoadedDict], key: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in dicts {
        match d.index.fuzzy(key, 2, MAX_SUGGESTIONS) {
            Ok(hits) => {
                for (k, _) in hits {
                    if !out.contains(&k) {
                        out.push(k);
                    }
                }
            }
            Err(e) => tracing::debug!(error = %e, "模糊查询失败"),
        }
        if out.len() >= MAX_SUGGESTIONS {
            break;
        }
    }
    out.truncate(MAX_SUGGESTIONS);
    out
}

/// 输入框候选：各词典前缀结果合并、去重、按字典序取前 `limit` 个。
pub fn prefix_suggestions(dicts: &[LoadedDict], prefix: &str, limit: usize) -> Vec<String> {
    let mut set = BTreeSet::new();
    for d in dicts {
        set.extend(d.index.prefix(prefix, limit).into_iter().map(|(k, _)| k));
    }
    set.into_iter().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_edge_punctuation_only() {
        assert_eq!(normalize_query("  “ubiquitous,”  "), "ubiquitous");
        assert_eq!(normalize_query("(take   off)."), "take off");
        assert_eq!(normalize_query("well-being"), "well-being");
        assert_eq!(normalize_query("rock'n'roll!"), "rock'n'roll");
        assert_eq!(normalize_query("e.g."), "e.g");
        assert_eq!(normalize_query("《红楼梦》"), "红楼梦");
        assert_eq!(normalize_query(" ，。 "), "");
    }

    #[test]
    fn candidates_try_raw_first() {
        assert_eq!(candidates(" e.g. "), ["e.g.", "e.g"]);
        assert_eq!(candidates("ubiquitous"), ["ubiquitous"]);
        assert_eq!(candidates("take   off,"), ["take off,", "take off"]);
        assert!(candidates("  ").is_empty());
    }
}
