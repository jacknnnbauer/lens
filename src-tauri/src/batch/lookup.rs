//! 批量导出的查词（SPEC 12.3 查词 worker）：本地词典 + 在线词典（令牌桶 2 次/秒/源，
//! 超时后重试 2 次，退避 0.5s / 1.5s；某源连续失败 5 次后本批跳过）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::app_state::AppState;
use crate::db::history::{self, ContextRow};
use crate::dict::index::normalize_key;
use crate::dict::{LoadedDict, RawEntry};
use crate::online::{self, OnlineDict, OnlineMode, Trigger};
use crate::query::{self, Source};
use crate::settings;

const MIN_INTERVAL: Duration = Duration::from_millis(500);
const BACKOFF: [Duration; 2] = [Duration::from_millis(500), Duration::from_millis(1500)];
const MAX_CONSECUTIVE_FAILURES: u32 = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchCard {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub entries: Vec<RawEntry>,
}

#[derive(Debug, Clone)]
pub struct WordResult {
    pub seq: i64,
    pub word: String,
    pub cards: Vec<BatchCard>,
    /// 提示（某部词典失败、用了过期缓存…），写进 batch_item.note
    pub notes: Vec<String>,
    /// 全部无结果时的拼写建议
    pub suggestions: Vec<String>,
    pub context: Option<ContextRow>,
}

/// 每个在线源的限速与连续失败计数（整批共用）。
#[derive(Default)]
struct Gate {
    next: tokio::sync::Mutex<Option<Instant>>,
    failures: AtomicU32,
    skipped: AtomicBool,
}

impl Gate {
    async fn wait_turn(&self) {
        let mut next = self.next.lock().await;
        let now = Instant::now();
        if let Some(at) = *next {
            if at > now {
                tokio::time::sleep(at - now).await;
            }
        }
        *next = Some(Instant::now() + MIN_INTERVAL);
    }
}

/// 本批参与的词典（按配置组顺序）及在线源的状态。
pub struct BatchSources {
    pub list: Vec<Source>,
    gates: HashMap<String, Arc<Gate>>,
}

impl BatchSources {
    /// `offline_only`：只用本地词典（最快）。网页模式的在线词典不参与导出。
    pub fn new(list: Vec<Source>, offline_only: bool) -> Self {
        let list: Vec<Source> = list
            .into_iter()
            .filter(|s| match s {
                Source::Local(_) => true,
                Source::Online(d) => !offline_only && d.provider.mode() == OnlineMode::Api,
            })
            .collect();
        let gates = list
            .iter()
            .filter_map(|s| match s {
                Source::Online(d) => Some((d.meta.id.clone(), Arc::new(Gate::default()))),
                Source::Local(_) => None,
            })
            .collect();
        Self { list, gates }
    }

    pub fn locals(&self) -> Vec<LoadedDict> {
        self.list
            .iter()
            .filter_map(|s| match s {
                Source::Local(d) => Some(d.clone()),
                Source::Online(_) => None,
            })
            .collect()
    }

    pub fn onlines(&self) -> Vec<OnlineDict> {
        self.list
            .iter()
            .filter_map(|s| match s {
                Source::Online(d) => Some(d.clone()),
                Source::Local(_) => None,
            })
            .collect()
    }

    /// 本地词典里有没有这个词头（解析词表时判断原形用）。
    pub fn exists(&self, word: &str) -> bool {
        let key = normalize_key(word);
        self.locals().iter().any(|d| !d.index.get(&key).is_empty())
    }
}

fn date_of(ms: i64) -> String {
    let days = ms.div_euclid(86_400_000);
    // 公历日期（Howard Hinnant 的 civil_from_days）
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

async fn query_online(state: &AppState, dict: &OnlineDict, gate: &Gate, word: &str, cfg: &settings::Online, notes: &mut Vec<String>) -> Option<Vec<RawEntry>> {
    if gate.skipped.load(Ordering::Relaxed) {
        return None;
    }
    if let Some(o) = online::fresh_cached(&state.db, dict, word, cfg) {
        return (o.status == "hit").then_some(o.entries);
    }
    let name = &dict.meta.name;
    let mut last_error = String::new();
    for attempt in 0..=BACKOFF.len() {
        if attempt > 0 {
            tokio::time::sleep(BACKOFF[attempt - 1]).await;
        }
        gate.wait_turn().await;
        let o = online::fetch(&state.db, state.online.client(), dict, word, cfg).await;
        match o.status {
            "hit" | "miss" => {
                gate.failures.store(0, Ordering::Relaxed);
                if o.stale {
                    if let Some(at) = o.cached_at {
                        notes.push(crate::tf!("{} 联网失败，用了 {} 的缓存", "{}: offline, used the cache from {}", name, date_of(at)));
                    }
                }
                return (o.status == "hit").then_some(o.entries);
            }
            _ => {
                last_error = o.error.unwrap_or_default();
                // Key 问题重试也没用
                if last_error.contains("Key") {
                    break;
                }
            }
        }
    }
    let failures = gate.failures.fetch_add(1, Ordering::Relaxed) + 1;
    if failures >= MAX_CONSECUTIVE_FAILURES && !gate.skipped.swap(true, Ordering::Relaxed) {
        notes.push(crate::tf!("{} 连续失败 {} 次，本批之后跳过：{}", "{}: failed {} times in a row, skipped for the rest of this batch: {}", name, failures, last_error));
    } else {
        notes.push(crate::tf!("{} 失败：{}", "{} failed: {}", name, last_error));
    }
    None
}

/// 查一个词：本地词典并行查，在线词典按查询时机（手动的不查；本地无结果才查的看本地结果）。
pub async fn lookup_word(state: &AppState, sources: &BatchSources, cfg: &settings::Online, seq: i64, word: &str) -> WordResult {
    let keys = query::candidates(word);
    let locals = sources.locals();
    let keys2 = keys.clone();
    let local_results = tokio::task::spawn_blocking(move || {
        locals
            .iter()
            .map(|d| {
                let mut hit = Vec::new();
                let mut err = None;
                for k in &keys2 {
                    match d.source.lookup(k) {
                        Ok(h) if !h.is_empty() => {
                            hit = h;
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => err = Some(e.to_string()),
                    }
                }
                (d.source.meta().id.clone(), d.source.meta().name.clone(), hit, err)
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();

    let mut notes = Vec::new();
    let mut found: HashMap<String, BatchCard> = HashMap::new();
    for (id, name, entries, err) in local_results {
        if let Some(e) = err {
            notes.push(crate::tf!("{} 出错：{}", "{} error: {}", name, e));
        }
        if !entries.is_empty() {
            found.insert(id.clone(), BatchCard { id, name, online: false, entries });
        }
    }
    let local_hit = !found.is_empty();
    let lemma_word = keys.last().cloned().unwrap_or_else(|| word.to_string());
    for d in sources.onlines() {
        let wanted = match d.trigger(cfg) {
            Trigger::Always => true,
            Trigger::LocalMiss => !local_hit,
            Trigger::Manual => false,
        };
        if !wanted {
            continue;
        }
        let Some(gate) = sources.gates.get(d.id()) else { continue };
        if let Some(entries) = query_online(state, &d, gate, &lemma_word, cfg, &mut notes).await {
            if !entries.is_empty() {
                found.insert(d.meta.id.clone(), BatchCard { id: d.meta.id.clone(), name: d.meta.name.clone(), online: true, entries });
            }
        }
    }

    // 按配置组顺序排列
    let cards: Vec<BatchCard> = sources
        .list
        .iter()
        .filter_map(|s| {
            let id = match s {
                Source::Local(d) => d.source.meta().id.clone(),
                Source::Online(d) => d.meta.id.clone(),
            };
            found.remove(&id)
        })
        .collect();
    let suggestions = if cards.is_empty() { query::fuzzy_suggestions(&sources.locals(), &lemma_word) } else { Vec::new() };
    let context = state.db.with_conn(|c| history::latest_context(c, &normalize_key(&lemma_word))).ok().flatten();
    WordResult { seq, word: word.to_string(), cards, notes, suggestions, context }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_791_430_000_000), "2026-10-08");
        assert_eq!(date_of(951_782_400_000), "2000-02-29");
    }
}
