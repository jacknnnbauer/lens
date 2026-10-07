//! 查询管线（SPEC 7.6）：归一化 → 按场景配置组顺序并发查各词典 → 逐部推送结果。
//!
//! 目前只做精确查询 + 全部未命中时的模糊候选；意图识别、词形还原、SymSpell 纠错后续补上。

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::Semaphore;

use crate::dict::{LoadedDict, RawEntry};
use crate::error::{Error, Result};

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictBrief {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupHandle {
    pub request_id: String,
    pub query: String,
    pub normalized: String,
    /// 本次参与查询的词典（按配置组顺序），前端据此先画骨架
    pub dicts: Vec<DictBrief>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupResultEvent {
    pub request_id: String,
    pub dict_id: String,
    /// hit | miss | error
    pub status: &'static str,
    pub entries: Vec<RawEntry>,
    pub error: Option<String>,
    pub ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupDoneEvent {
    pub request_id: String,
    /// 所有词典都未命中时的拼写候选
    pub suggestions: Vec<String>,
}

/// 进行中的查询的取消标记。
#[derive(Default)]
pub struct Cancels(Mutex<HashMap<String, Arc<AtomicBool>>>);

impl Cancels {
    pub fn register(&self, id: &str) -> Result<Arc<AtomicBool>> {
        let flag = Arc::new(AtomicBool::new(false));
        self.0.lock().map_err(|_| Error::LockPoisoned)?.insert(id.to_string(), flag.clone());
        Ok(flag)
    }

    pub fn cancel(&self, id: &str) -> Result<()> {
        if let Some(f) = self.0.lock().map_err(|_| Error::LockPoisoned)?.get(id) {
            f.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    /// 取消除 `keep` 以外的全部查询（新查询发出时调用）。
    pub fn cancel_all_except(&self, keep: &str) -> Result<()> {
        for (id, f) in self.0.lock().map_err(|_| Error::LockPoisoned)?.iter() {
            if id != keep {
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

/// 后台执行一次查询：每部词典一个阻塞任务，完成一个推送一个；全部结束后推送 done。
pub async fn run(
    app: AppHandle,
    cancels: Arc<Cancels>,
    request_id: String,
    keys: Vec<String>,
    dicts: Vec<LoadedDict>,
    cancelled: Arc<AtomicBool>,
) {
    let sem = Arc::new(Semaphore::new(concurrency()));
    let mut tasks = tokio::task::JoinSet::new();
    for (order, dict) in dicts.iter().cloned().enumerate() {
        let (sem, keys, cancelled) = (sem.clone(), keys.clone(), cancelled.clone());
        tasks.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let dict_id = dict.source.meta().id.clone();
            if cancelled.load(Ordering::Relaxed) {
                return (order, dict_id, None, 0.0);
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
            (order, dict_id, Some(res), t.elapsed().as_secs_f64() * 1000.0)
        });
    }

    let mut any_hit = false;
    while let Some(joined) = tasks.join_next().await {
        let Ok((_, dict_id, Some(res), ms)) = joined else { continue };
        if cancelled.load(Ordering::Relaxed) {
            continue;
        }
        let (status, entries, error) = match res {
            Ok(Ok(entries)) if entries.is_empty() => ("miss", entries, None),
            Ok(Ok(entries)) => {
                any_hit = true;
                ("hit", entries, None)
            }
            Ok(Err(e)) => ("error", Vec::new(), Some(e.to_string())),
            // spawn_blocking 里 panic：记日志，当作该词典出错，不影响其他词典
            Err(e) => {
                tracing::error!(dict = %dict_id, error = %e, "查词任务 panic");
                ("error", Vec::new(), Some("查词时发生内部错误".to_string()))
            }
        };
        if let Some(e) = &error {
            tracing::warn!(dict = %dict_id, error = %e, "查词失败");
        }
        let event = LookupResultEvent { request_id: request_id.clone(), dict_id, status, entries, error, ms };
        if let Err(e) = app.emit(EVENT_RESULT, event) {
            tracing::warn!(error = %e, "推送查词结果失败");
        }
    }

    if !cancelled.load(Ordering::Relaxed) {
        let last = keys.last().map(String::as_str).unwrap_or_default();
        let suggestions = if any_hit { Vec::new() } else { fuzzy_suggestions(&dicts, last) };
        if let Err(e) = app.emit(EVENT_DONE, LookupDoneEvent { request_id: request_id.clone(), suggestions }) {
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
