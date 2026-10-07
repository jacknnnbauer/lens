//! Ctrl+C+C（SPEC 8.1）：低级键盘钩子检测 400ms 内两次 Ctrl+C。
//!
//! 回调只记录「C 键按下」的时间、当时是否能确认 Ctrl 按着、剪贴板序号，投递给另一个线程，
//! 不做任何耗时操作——回调慢了系统会把钩子摘掉，还会拖慢全系统输入。
//!
//! 为什么不能只看 Ctrl：Chromium 系程序（Chrome、Edge、Electron、WebView2）获得焦点时会装自己的
//! 低级键盘钩子并吞掉 Ctrl 事件，此时我们既收不到 Ctrl、`GetAsyncKeyState` 也读不到。
//! 所以读不到 Ctrl 时，改由消费线程确认「剪贴板随后更新了」才算一次复制（见 `confirm_copy`）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage, HC_ACTION, KBDLLHOOKSTRUCT,
    LLKHF_ALTDOWN, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::error::{Error, Result};

pub const DOUBLE_COPY_MS: u32 = 400;
/// 读不到 Ctrl 时，C 键按下后等剪贴板更新的最长时间
const CLIPBOARD_CONFIRM_MS: u64 = 200;
const VK_C: u32 = 0x43;

/// 一次 C 键按下（非自动重复、未按 Alt）。
#[derive(Debug, Clone, Copy)]
pub struct CPress {
    /// KBDLLHOOKSTRUCT.time（毫秒 tick）
    pub time: u32,
    /// 按下时能否确认 Ctrl 按着
    pub ctrl: bool,
    /// 按下时的剪贴板序号
    pub clipboard_seq: u32,
}

static SENDER: OnceLock<Sender<CPress>> = OnceLock::new();
/// C 键是否处于按下状态，用来忽略按住不放的自动重复
static C_DOWN: AtomicBool = AtomicBool::new(false);

/// 纯判定逻辑（便于测试）：本次复制是否与上一次构成「双击」。返回 (是否触发, 新的 last)。
pub fn double_copy_step(last: u32, now: u32) -> (bool, u32) {
    if last != 0 && now.wrapping_sub(last) <= DOUBLE_COPY_MS {
        (true, 0)
    } else {
        (false, now.max(1))
    }
}

/// 这次 C 键是否真的是一次复制：确认按着 Ctrl，或剪贴板在 CLIPBOARD_CONFIRM_MS 内更新了。
pub fn confirm_copy(p: CPress) -> bool {
    if p.ctrl {
        return true;
    }
    let deadline = Instant::now() + Duration::from_millis(CLIPBOARD_CONFIRM_MS);
    while Instant::now() < deadline {
        // SAFETY: 只读系统计数器。
        if unsafe { GetClipboardSequenceNumber() } != p.clipboard_seq {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if kb.vkCode == VK_C {
            let msg = wparam.0 as u32;
            if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
                let repeat = C_DOWN.swap(true, Ordering::Relaxed);
                let alt = kb.flags.0 & LLKHF_ALTDOWN.0 != 0;
                if !repeat && !alt {
                    if let Some(tx) = SENDER.get() {
                        let _ = tx.send(CPress {
                            time: kb.time,
                            ctrl: GetAsyncKeyState(i32::from(VK_CONTROL.0)) < 0,
                            clipboard_seq: GetClipboardSequenceNumber(),
                        });
                    }
                }
            } else if msg == WM_KEYUP || msg == WM_SYSKEYUP {
                C_DOWN.store(false, Ordering::Relaxed);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// 在独立线程上安装钩子并运行消息循环；每次 C 键按下往 `tx` 发一个 CPress。只能调用一次。
pub fn spawn(tx: Sender<CPress>) -> Result<()> {
    SENDER.set(tx).map_err(|_| Error::Other("键盘钩子已安装".into()))?;
    std::thread::Builder::new()
        .name("lens-kbd-hook".into())
        .spawn(|| {
            // SAFETY: 钩子在本线程安装并由本线程的消息循环驱动；进程退出时系统自动卸载。
            unsafe {
                let module = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
                match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), module, 0) {
                    Ok(_) => tracing::info!("Ctrl+C+C 键盘钩子已安装"),
                    Err(e) => {
                        tracing::error!(error = %e, "键盘钩子安装失败");
                        return;
                    }
                }
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })
        .map_err(|e| Error::Other(format!("无法创建钩子线程：{e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_copy_timing() {
        let (hit, last) = double_copy_step(0, 1000);
        assert!(!hit);
        let (hit, last) = double_copy_step(last, 1300);
        assert!(hit, "300ms 内第二次");
        assert_eq!(last, 0, "触发后重置，第三次不算");
        let (hit, last) = double_copy_step(last, 1400);
        assert!(!hit);
        let (hit, _) = double_copy_step(last, 1900);
        assert!(!hit, "超过 400ms");
        // tick 计数回绕
        let (hit, _) = double_copy_step(u32::MAX - 100, 50);
        assert!(hit);
    }

    #[test]
    fn ctrl_confirmed_presses_need_no_clipboard_change() {
        assert!(confirm_copy(CPress { time: 1, ctrl: true, clipboard_seq: 0 }));
        // 序号与当前不同 → 视为剪贴板已更新
        // SAFETY: 只读系统计数器。
        let now = unsafe { GetClipboardSequenceNumber() };
        assert!(confirm_copy(CPress { time: 1, ctrl: false, clipboard_seq: now.wrapping_add(1) }));
    }
}
