//! 取词（SPEC 8）：全局热键 / Ctrl+C+C → UIA 取选区与语境句（超时回退剪贴板）→ 常驻弹窗。
//!
//! 线程：
//! - `lens-capture`：常驻工作线程，初始化 COM 并持有 UIA 实例，串行处理取词请求
//! - `lens-kbd-hook`：低级键盘钩子 + 消息循环（hook.rs）
//! - 每次触发起一个短命线程：投递请求、最多等 `timeoutMs`，然后显示弹窗

pub mod sentence;
#[cfg(windows)]
mod hook;
#[cfg(windows)]
pub mod uia;
#[cfg(windows)]
mod win;

use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::app_state::AppState;
use crate::error::{Error, Result};
use sentence::ContextSentence;

pub const POPUP: &str = "popup";
pub const EVENT_TRIGGERED: &str = "capture:triggered";
pub const EVENT_URL: &str = "capture:url";
/// 弹窗逻辑尺寸（画板 02）
pub const POPUP_W: f64 = 480.0;
pub const POPUP_H: f64 = 560.0;
/// 选中的文本超过这么长就截断（再长按句子模式处理，见 SPEC 7.6）
const MAX_QUERY_CHARS: usize = 200;
const BROWSERS: &[&str] = &["msedge.exe", "chrome.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// 全局快捷键：有选区就查选区，否则弹出空白查词框
    Hotkey,
    /// Ctrl+C+C：用户刚复制过，UIA 拿不到时读剪贴板
    DoubleCopy,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSource {
    /// uia | edit（Win32 编辑框）| clipboard | manual
    pub method: &'static str,
    pub app: Option<String>,
    pub window_title: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureEvent {
    pub id: u64,
    pub text: String,
    /// 已存入 context 表的语境句 id（有句子时）；收藏、历史引用它
    pub context_id: Option<i64>,
    pub sentence: Option<ContextSentence>,
    pub source: CaptureSource,
    /// 从触发到弹窗显示前的耗时
    pub ms: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    pub h: f64,
}

struct Job {
    id: u64,
    trigger: Trigger,
    reply: Sender<(CaptureEvent, Option<Anchor>)>,
}

/// 全局快捷键与 Ctrl+C+C 的当前状态，供界面提示（快捷键常被其他程序占用）。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureStatus {
    pub hotkey: String,
    pub hotkey_registered: bool,
    pub hotkey_error: Option<String>,
    pub double_copy: bool,
}

/// 注册（或改注册）全局快捷键：先注销旧的，再注册新的；失败只记录状态，不影响程序运行。
pub fn register_hotkey(app: &AppHandle, old: Option<&str>, new: &str) -> CaptureStatus {
    let gs = app.global_shortcut();
    if let Some(old) = old.and_then(|o| o.parse::<Shortcut>().ok()) {
        let _ = gs.unregister(old);
    }
    let result = new
        .parse::<Shortcut>()
        .map_err(|e| format!("快捷键格式无效：{e}"))
        .and_then(|sc| gs.register(sc).map_err(|e| e.to_string()));
    match &result {
        Ok(()) => tracing::info!(hotkey = new, "全局快捷键已注册"),
        Err(e) => tracing::warn!(hotkey = new, error = %e, "全局快捷键注册失败"),
    }
    CaptureStatus {
        hotkey: new.to_string(),
        hotkey_registered: result.is_ok(),
        hotkey_error: result.err().map(|e| {
            if e.contains("already registered") { "已被其他程序占用".to_string() } else { e }
        }),
        double_copy: false,
    }
}

/// 取词服务：持有发往工作线程的通道。
pub struct CaptureService {
    jobs: Mutex<Sender<Job>>,
    next_id: std::sync::atomic::AtomicU64,
    pub status: Mutex<CaptureStatus>,
}

fn clip_query(s: &str) -> String {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    t.chars().take(MAX_QUERY_CHARS).collect()
}

/// 启动工作线程（以及 Ctrl+C+C 钩子），返回服务句柄。
pub fn start(app: AppHandle, double_copy: bool) -> Result<CaptureService> {
    let (tx, rx) = mpsc::channel::<Job>();
    let worker_app = app.clone();
    std::thread::Builder::new()
        .name("lens-capture".into())
        .spawn(move || worker(worker_app, rx))
        .map_err(|e| Error::Other(format!("无法创建取词线程：{e}")))?;

    #[cfg(windows)]
    if double_copy {
        let (htx, hrx) = mpsc::channel::<hook::CPress>();
        hook::spawn(htx)?;
        let app = app.clone();
        std::thread::Builder::new()
            .name("lens-double-copy".into())
            .spawn(move || {
                // 确认每次 C 键确实是复制，再按时间判定是否「两次」
                let mut last = 0u32;
                while let Ok(press) = hrx.recv() {
                    if !hook::confirm_copy(press) {
                        continue;
                    }
                    let (hit, next) = hook::double_copy_step(last, press.time);
                    last = next;
                    if hit {
                        on_trigger(&app, Trigger::DoubleCopy);
                    }
                }
            })
            .map_err(|e| Error::Other(format!("无法创建线程：{e}")))?;
    }
    #[cfg(not(windows))]
    let _ = double_copy;

    Ok(CaptureService {
        jobs: Mutex::new(tx),
        next_id: std::sync::atomic::AtomicU64::new(1),
        status: Mutex::new(CaptureStatus { double_copy, ..Default::default() }),
    })
}

#[cfg(windows)]
fn worker(app: AppHandle, rx: mpsc::Receiver<Job>) {
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    // SAFETY: 本线程只初始化一次 COM，生命周期与线程相同。
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let uia = match uia::Uia::new() {
        Ok(u) => Some(u),
        Err(e) => {
            tracing::warn!(error = %e, "UIA 不可用，只能用剪贴板取词");
            None
        }
    };
    while let Ok(job) = rx.recv() {
        let fg = win::foreground();
        let selection = uia.as_ref().and_then(|u| u.selection());
        // UIA 拿不到时（经典 Win32 编辑框不提供 TextPattern）直接读编辑框
        let edit = if selection.is_none() { win::focused_control(fg.hwnd).and_then(win::edit_selection) } else { None };
        let (text, sentence, method, anchor) = match (&selection, &edit) {
            (Some(sel), _) => (
                clip_query(&sel.text),
                sentence::extract(&sel.surrounding, sel.sel16),
                "uia",
                sel.rect.map(|r| Anchor { x: r.x, y: r.y, h: r.h }),
            ),
            (None, Some(ed)) => (
                clip_query(&ed.text),
                sentence::extract(&ed.surrounding, ed.sel16),
                "edit",
                ed.anchor.map(|(x, y, h)| Anchor { x, y, h }),
            ),
            (None, None) if job.trigger == Trigger::DoubleCopy => {
                // 目标程序处理第二次 Ctrl+C 需要一点时间
                std::thread::sleep(Duration::from_millis(60));
                (win::clipboard_text().map(|t| clip_query(&t)).unwrap_or_default(), None, "clipboard", None)
            }
            (None, None) => (String::new(), None, "manual", None),
        };
        let is_browser = fg.app.as_deref().is_some_and(|a| BROWSERS.contains(&a.to_ascii_lowercase().as_str()));
        let context_id = sentence.as_ref().and_then(|ctx| save_context(&app, &text, ctx, method, &fg));
        let event = CaptureEvent {
            id: job.id,
            text,
            context_id,
            sentence,
            source: CaptureSource { method, app: fg.app.clone(), window_title: fg.title.clone(), url: None },
            ms: 0.0,
        };
        let _ = job.reply.send((event, anchor));

        // 地址栏 URL 查找可能较慢，弹窗显示后再补发
        if is_browser && method != "manual" {
            if let Some(url) = uia.as_ref().and_then(|u| u.browser_url(fg.hwnd)) {
                if let Some(cid) = context_id {
                    let state = app.state::<AppState>();
                    if let Err(e) = state.db.with_conn(|c| crate::db::history::set_context_url(c, cid, &url)) {
                        tracing::warn!(error = %e, "补写语境 URL 失败");
                    }
                }
                let _ = app.emit_to(POPUP, EVENT_URL, serde_json::json!({ "id": job.id, "url": url }));
            }
        }
    }
}

/// 把捕获到的语境句存进 context 表（SPEC 6：每次划词捕获一条）。失败只记日志。
#[cfg(windows)]
fn save_context(app: &AppHandle, text: &str, ctx: &ContextSentence, method: &str, fg: &win::Foreground) -> Option<i64> {
    use crate::db::history::{insert_context, NewContext};
    let lemma = crate::dict::index::normalize_key(&crate::query::normalize_query(text));
    if lemma.is_empty() {
        return None;
    }
    let row = NewContext {
        lemma: &lemma,
        surface: text,
        sentence: &ctx.sentence,
        hl_start: ctx.hl_start as i64,
        hl_end: ctx.hl_end as i64,
        app: fg.app.as_deref(),
        window_title: fg.title.as_deref(),
        url: None,
        method,
    };
    match app.state::<AppState>().db.with_conn(|c| insert_context(c, &row)) {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(error = %e, "保存语境句失败");
            None
        }
    }
}

#[cfg(not(windows))]
fn worker(_app: AppHandle, rx: mpsc::Receiver<Job>) {
    while let Ok(job) = rx.recv() {
        let event = CaptureEvent {
            id: job.id,
            text: String::new(),
            context_id: None,
            sentence: None,
            source: CaptureSource::default(),
            ms: 0.0,
        };
        let _ = job.reply.send((event, None));
    }
}

/// 热键或 Ctrl+C+C 触发：在短命线程里取词（最多等 timeoutMs），然后显示弹窗。
pub fn on_trigger(app: &AppHandle, trigger: Trigger) {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("lens-capture-trigger".into()).spawn(move || {
        let started = Instant::now();
        let state = app.state::<AppState>();
        let timeout = state.settings.get().map(|s| s.capture.timeout_ms).unwrap_or(150);
        let Some(service) = app.try_state::<CaptureService>() else { return };
        let id = service.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        let sent = service.jobs.lock().map(|j| j.send(Job { id, trigger, reply: tx }).is_ok()).unwrap_or(false);

        let (mut event, anchor) = match sent.then(|| rx.recv_timeout(Duration::from_millis(timeout))) {
            Some(Ok(r)) => r,
            _ => {
                // 超时（UIA 卡住）或工作线程不可用：先把弹窗弹出来
                tracing::debug!(timeout, "取词超时，回退");
                #[cfg(windows)]
                let text = if trigger == Trigger::DoubleCopy { win::clipboard_text().map(|t| clip_query(&t)) } else { None };
                #[cfg(not(windows))]
                let text: Option<String> = None;
                let method = if text.is_some() { "clipboard" } else { "manual" };
                let event = CaptureEvent {
                    id,
                    text: text.unwrap_or_default(),
                    context_id: None,
                    sentence: None,
                    source: CaptureSource { method, ..Default::default() },
                    ms: 0.0,
                };
                (event, None)
            }
        };
        event.ms = started.elapsed().as_secs_f64() * 1000.0;
        if let Err(e) = show_popup(&app, &event, anchor) {
            tracing::warn!(error = %e, "显示弹窗失败");
        }
        tracing::info!(trigger = ?trigger, method = event.source.method, ms = event.ms, "取词");
    });
    if let Err(e) = spawned {
        tracing::warn!(error = %e, "无法创建取词线程");
    }
}

/// 弹窗位置（SPEC 8.4）：优先放在锚点下方，放不下放上方；左右夹在工作区内。全部为物理像素。
pub fn place(anchor: Anchor, size: (f64, f64), work: (f64, f64, f64, f64)) -> (f64, f64) {
    const GAP: f64 = 8.0;
    let (wx, wy, ww, wh) = work;
    let (w, h) = size;
    let below = anchor.y + anchor.h + GAP;
    let above = anchor.y - GAP - h;
    let y = if below + h <= wy + wh {
        below
    } else if above >= wy {
        above
    } else {
        (wy + wh - h).max(wy)
    };
    let x = anchor.x.min(wx + ww - w).max(wx);
    (x.round(), y.round())
}

fn show_popup(app: &AppHandle, event: &CaptureEvent, anchor: Option<Anchor>) -> Result<()> {
    let win = app.get_webview_window(POPUP).ok_or_else(|| Error::Other("弹窗窗口不存在".into()))?;
    #[cfg(windows)]
    let cursor = win::cursor_pos();
    #[cfg(not(windows))]
    let cursor: Option<(i32, i32)> = None;
    let anchor = anchor.or_else(|| cursor.map(|(x, y)| Anchor { x: f64::from(x), y: f64::from(y), h: 16.0 }));

    if let Some(a) = anchor {
        let monitor = app.monitor_from_point(a.x, a.y).ok().flatten().or_else(|| app.primary_monitor().ok().flatten());
        if let Some(m) = monitor {
            let scale = m.scale_factor();
            let wa = m.work_area();
            let size = (POPUP_W * scale, POPUP_H * scale);
            let work = (f64::from(wa.position.x), f64::from(wa.position.y), f64::from(wa.size.width), f64::from(wa.size.height));
            let (x, y) = place(a, size, work);
            win.set_size(PhysicalSize::new(size.0 as u32, size.1 as u32))?;
            win.set_position(PhysicalPosition::new(x as i32, y as i32))?;
        }
    }
    // 先推送内容再显示，避免闪一下上一次的结果
    win.emit_to(POPUP, EVENT_TRIGGERED, event)?;
    win.show()?;
    win.set_focus()?;
    Ok(())
}

impl From<tauri::Error> for Error {
    fn from(e: tauri::Error) -> Self {
        Error::Other(format!("窗口操作失败：{e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: (f64, f64, f64, f64) = (0.0, 0.0, 1920.0, 1040.0);

    #[test]
    fn popup_goes_below_then_above_and_stays_inside() {
        let a = |x, y| Anchor { x, y, h: 20.0 };
        assert_eq!(place(a(100.0, 100.0), (480.0, 560.0), WORK), (100.0, 128.0));
        // 下方放不下 → 上方
        assert_eq!(place(a(100.0, 900.0), (480.0, 560.0), WORK), (100.0, 332.0));
        // 右边超出 → 左移
        assert_eq!(place(a(1800.0, 100.0), (480.0, 560.0), WORK).0, 1440.0);
        // 第二块屏幕（负坐标）
        let work2 = (-1920.0, 0.0, 1920.0, 1040.0);
        assert_eq!(place(a(-2000.0, 100.0), (480.0, 560.0), work2).0, -1920.0);
        // 上下都放不下 → 贴底
        assert_eq!(place(a(100.0, 400.0), (480.0, 1000.0), WORK).1, 40.0);
    }

    #[test]
    fn queries_are_collapsed_and_capped() {
        assert_eq!(clip_query("  take\r\n off  "), "take off");
        assert_eq!(clip_query(&"x".repeat(500)).len(), MAX_QUERY_CHARS);
    }
}
