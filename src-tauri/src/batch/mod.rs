//! 批量导出（SPEC 12、M7）：选词 → 预检 → 流水线（查词 × N → 渲染 × 1 → 写盘 × 1）→ 总目录与失败清单。
//!
//! - 进度存在 `batch` / `batch_item` 表，中途退出后可续传（已完成项跳过）
//! - 渲染借用隐藏的 `export` 窗口：Rust 发 `batch:render` 任务，窗口组装文档、打印 / 序列化后调用
//!   `batch_render_done` 回报；每 N 个词或内存超限时销毁重建窗口
//! - 同一时间只跑一个批次（渲染窗口只有一个）

pub mod lookup;
pub mod parse;
mod report;
mod runner;
pub mod sys;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::sync::{oneshot, watch, Notify};

use crate::error::{Error, Result};
use crate::export::Paper;
use lookup::BatchCard;

pub use runner::spawn;

pub const EVENT_PROGRESS: &str = "batch:progress";
pub const EVENT_RENDER: &str = "batch:render";

/// 前端提交的批量导出请求（整份存进 `batch.request_json`，续传时原样读回）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    /// history | paste | wordbook
    pub source: String,
    /// 显示用的说明，如「粘贴词表」「历史 · 最近 7 天」
    pub label: String,
    pub words: Vec<String>,
    pub pdf: bool,
    pub html: bool,
    /// 合并成一个文件（超过分卷大小自动分卷）
    pub merge: bool,
    /// cards | document
    pub layout: String,
    /// paperWhite | followTheme | inkSaving | theme:<主题 id>
    pub style: String,
    pub paper: Paper,
    /// 只用本地词典
    pub offline_only: bool,
    pub out_dir: String,
    /// 导出选项（音标、例句、语境、来源与日期），原样交给模板
    pub options: serde_json::Value,
    pub theme: String,
    pub accent: String,
    /// 开始时的场景配置组
    #[serde(default)]
    pub profile: String,
    /// 界面语言（zh | en）：导出文件里的标签跟着它
    #[serde(default)]
    pub lang: String,
}

/// 发给导出窗口的渲染任务。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderJob {
    pub job_id: String,
    /// word：单个词一个文件；volume：多个词合并成一卷（带目录与书签）
    pub mode: &'static str,
    pub title: String,
    pub words: Vec<RenderWord>,
    pub layout: String,
    pub style: String,
    pub options: serde_json::Value,
    pub theme: String,
    pub accent: String,
    pub paper: Paper,
    /// 需要 PDF 时的临时输出路径
    pub pdf_path: Option<String>,
    pub want_html: bool,
    pub lang: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderWord {
    pub word: String,
    pub cards: Vec<BatchCard>,
    pub context: Option<crate::db::history::ContextRow>,
}

#[derive(Debug)]
pub struct RenderReply {
    pub ok: bool,
    pub error: Option<String>,
    pub html: Option<String>,
}

/// 批次的暂停 / 取消开关。
#[derive(Default)]
pub struct Control {
    paused: AtomicBool,
    cancelled: AtomicBool,
    wake: Notify,
}

impl Control {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// 暂停时在这里等，直到继续或取消。
    pub async fn wait_while_paused(&self) {
        while self.is_paused() && !self.is_cancelled() {
            let notified = self.wake.notified();
            if !self.is_paused() || self.is_cancelled() {
                break;
            }
            notified.await;
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
        self.wake.notify_waiters();
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.wake.notify_waiters();
    }
}

pub struct BatchManager {
    active: Mutex<Option<(String, Arc<Control>)>>,
    replies: Mutex<HashMap<String, oneshot::Sender<RenderReply>>>,
    /// 导出窗口每报告一次就绪加一。用计数而不是 Notify 的许可：开发模式下 React 会把
    /// effect 跑两遍，多出来的许可会让下次重建窗口时没等新窗口就绪就发任务
    window_ready: watch::Sender<u64>,
    /// 已报告就绪的窗口标签（渲染窗口池里每个窗口各自就绪）
    ready_labels: Mutex<std::collections::HashSet<String>>,
}

impl Default for BatchManager {
    fn default() -> Self {
        Self { active: Mutex::default(), replies: Mutex::default(), window_ready: watch::channel(0).0, ready_labels: Mutex::default() }
    }
}

impl BatchManager {
    pub fn active(&self) -> Option<(String, Arc<Control>)> {
        self.active.lock().ok().and_then(|a| a.clone())
    }

    pub fn active_id(&self) -> Option<String> {
        self.active().map(|(id, _)| id)
    }

    fn claim(&self, id: &str) -> Result<Arc<Control>> {
        let mut a = self.active.lock().map_err(|_| Error::LockPoisoned)?;
        if let Some((other, _)) = a.as_ref() {
            return Err(Error::Other(if other == id { crate::i18n::t("这个批次正在导出", "This batch is already exporting").into() } else { crate::i18n::t("已有一个批量导出在进行，请等它完成或取消", "Another batch export is running. Wait for it to finish or cancel it").into() }));
        }
        let ctl = Arc::new(Control::default());
        *a = Some((id.to_string(), ctl.clone()));
        Ok(ctl)
    }

    fn release(&self, id: &str) {
        if let Ok(mut a) = self.active.lock() {
            if a.as_ref().is_some_and(|(x, _)| x == id) {
                *a = None;
            }
        }
    }

    fn expect_reply(&self, job_id: &str) -> Result<oneshot::Receiver<RenderReply>> {
        let (tx, rx) = oneshot::channel();
        self.replies.lock().map_err(|_| Error::LockPoisoned)?.insert(job_id.to_string(), tx);
        Ok(rx)
    }

    fn forget_reply(&self, job_id: &str) {
        if let Ok(mut r) = self.replies.lock() {
            r.remove(job_id);
        }
    }

    /// 导出窗口回报渲染结果。
    pub fn reply(&self, job_id: &str, reply: RenderReply) {
        let tx = self.replies.lock().ok().and_then(|mut r| r.remove(job_id));
        if let Some(tx) = tx {
            let _ = tx.send(reply);
        }
    }

    /// 导出窗口加载完成（新建或重建后）。
    pub fn window_ready(&self, label: &str) {
        if let Ok(mut s) = self.ready_labels.lock() {
            s.insert(label.to_string());
        }
        self.window_ready.send_modify(|n| *n += 1);
    }

    pub fn is_ready(&self, label: &str) -> bool {
        self.ready_labels.lock().is_ok_and(|s| s.contains(label))
    }

    /// 窗口要重建 / 新建：清掉旧的就绪记录
    pub fn forget_ready(&self, label: &str) {
        if let Ok(mut s) = self.ready_labels.lock() {
            s.remove(label);
        }
    }

    /// 在创建窗口之前调用，之后 `changed()` 只会被这之后的就绪报告唤醒。
    pub fn ready_receiver(&self) -> watch::Receiver<u64> {
        let mut rx = self.window_ready.subscribe();
        rx.mark_unchanged();
        rx
    }
}

/// 启动或续传一个批次（后台运行）。
pub fn start(app: &AppHandle, manager: &BatchManager, id: &str) -> Result<()> {
    let ctl = manager.claim(id)?;
    spawn(app.clone(), id.to_string(), ctl);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn pause_and_cancel_wake_waiters() {
        let ctl = Arc::new(Control::default());
        ctl.set_paused(true);
        let c2 = ctl.clone();
        let waiter = tokio::spawn(async move { c2.wait_while_paused().await });
        tokio::task::yield_now().await;
        ctl.set_paused(false);
        assert!(waiter.await.is_ok());
        ctl.set_paused(true);
        let c3 = ctl.clone();
        let waiter = tokio::spawn(async move { c3.wait_while_paused().await });
        tokio::task::yield_now().await;
        ctl.cancel();
        assert!(waiter.await.is_ok());
    }

    #[test]
    fn single_active_batch() -> Result<()> {
        let m = BatchManager::default();
        m.claim("a")?;
        assert!(m.claim("b").is_err());
        m.release("a");
        assert!(m.claim("b").is_ok());
        Ok(())
    }
}
