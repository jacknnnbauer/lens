//! 取词用到的零散 Win32 调用：前台窗口信息、光标位置、剪贴板文本，
//! 以及 UIA 不支持 TextPattern 的经典 Win32 编辑框（Win10 记事本等）的选区读取。

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HGLOBAL, HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetGUIThreadInfo, GetWindowTextW, GetWindowThreadProcessId,
    SendMessageTimeoutW, GUITHREADINFO, SMTO_ABORTIFHUNG, WM_GETTEXT, WM_GETTEXTLENGTH,
};

#[derive(Debug, Clone, Default)]
pub struct Foreground {
    /// HWND 数值（HWND 本身不是 Send）
    pub hwnd: isize,
    /// 进程名，如 msedge.exe
    pub app: Option<String>,
    pub title: Option<String>,
}

pub fn foreground() -> Foreground {
    // SAFETY: 只读查询；缓冲区长度与传入长度一致，句柄用完即关。
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return Foreground::default();
        }
        let mut title = [0u16; 512];
        let n = GetWindowTextW(hwnd, &mut title);
        let title = (n > 0).then(|| String::from_utf16_lossy(&title[..n as usize]));

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let app = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok().and_then(|h| {
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
            let _ = CloseHandle(h);
            ok.then(|| {
                let full = String::from_utf16_lossy(&buf[..len as usize]);
                full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string()
            })
        });
        Foreground { hwnd: hwnd.0 as isize, app, title }
    }
}

pub fn hwnd(v: isize) -> HWND {
    HWND(v as *mut _)
}

pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    // SAFETY: 写入本地 POINT。
    unsafe { GetCursorPos(&mut p).ok().map(|_| (p.x, p.y)) }
}

/// 读剪贴板文本；被别的程序占用时重试几次。
pub fn clipboard_text() -> Option<String> {
    for _ in 0..5 {
        // SAFETY: 打开成功后一定 CloseClipboard；GlobalLock 返回的指针在 Unlock 前有效，按 NUL 结尾读取。
        unsafe {
            if OpenClipboard(None).is_err() {
                std::thread::sleep(std::time::Duration::from_millis(15));
                continue;
            }
            let text = GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok().and_then(|h: HANDLE| {
                let g = HGLOBAL(h.0);
                let p = GlobalLock(g) as *const u16;
                if p.is_null() {
                    return None;
                }
                let mut len = 0;
                while *p.add(len) != 0 && len < 1 << 20 {
                    len += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
                let _ = GlobalUnlock(g);
                Some(s)
            });
            let _ = CloseClipboard();
            return text;
        }
    }
    None
}

const EM_GETSEL: u32 = 0x00B0;
const EM_POSFROMCHAR: u32 = 0x00D6;
/// 跨进程 SendMessage 的超时：目标程序卡住时不拖住取词
const SEND_TIMEOUT_MS: u32 = 50;
/// 编辑框全文超过这个长度就不读（EM_GETSEL 的返回值只有 16 位，长文也读不准）
const MAX_EDIT_TEXT: usize = 65_535;
/// 一行（段）太长时只取选区前后这么多
const LINE_WINDOW: usize = 1500;

fn send(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> Option<usize> {
    let mut result = 0usize;
    // SAFETY: SendMessageTimeoutW 对 WM_GETTEXT 等标准消息会做跨进程封送；result 写入本地变量。
    let ok = unsafe {
        SendMessageTimeoutW(hwnd, msg, WPARAM(wparam), LPARAM(lparam), SMTO_ABORTIFHUNG, SEND_TIMEOUT_MS, Some(&mut result))
    };
    (ok.0 != 0).then_some(result)
}

/// 前台线程里获得键盘焦点的控件。
pub fn focused_control(foreground: isize) -> Option<isize> {
    // SAFETY: 只读查询线程 GUI 信息。
    unsafe {
        let tid = GetWindowThreadProcessId(hwnd(foreground), None);
        let mut info = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        GetGUIThreadInfo(tid, &mut info).ok()?;
        (!info.hwndFocus.0.is_null()).then_some(info.hwndFocus.0 as isize)
    }
}

pub struct EditSelection {
    pub text: String,
    /// 选区所在的行（段），供断句
    pub surrounding: String,
    pub sel16: (usize, usize),
    /// 选区起点的屏幕坐标与估计行高
    pub anchor: Option<(f64, f64, f64)>,
}

/// 选区所在的行（以换行分隔）；行太长时取选区前后 LINE_WINDOW 个 UTF-16 单元。返回 [from, to)。
pub fn line_around(units: &[u16], start: usize, end: usize) -> (usize, usize) {
    let nl = u16::from(b'\n');
    let from = units[..start].iter().rposition(|&c| c == nl).map_or(0, |i| i + 1);
    let to = units[end..].iter().position(|&c| c == nl).map_or(units.len(), |i| end + i);
    (from.max(start.saturating_sub(LINE_WINDOW)), to.min(end + LINE_WINDOW))
}

/// 经典 Edit / RichEdit 控件：WM_GETTEXT 取全文，EM_GETSEL 取选区。
pub fn edit_selection(control: isize) -> Option<EditSelection> {
    let h = hwnd(control);
    let mut class = [0u16; 64];
    // SAFETY: 缓冲区长度与传入长度一致。
    let n = unsafe { GetClassNameW(h, &mut class) };
    let class = String::from_utf16_lossy(&class[..usize::try_from(n).ok()?]).to_ascii_lowercase();
    if class != "edit" && !class.starts_with("richedit") {
        return None;
    }
    let len = send(h, WM_GETTEXTLENGTH, 0, 0)?;
    if len == 0 || len > MAX_EDIT_TEXT {
        return None;
    }
    let mut buf = vec![0u16; len + 1];
    let copied = send(h, WM_GETTEXT, buf.len(), buf.as_mut_ptr() as isize)?;
    buf.truncate(copied.min(len));
    let sel = send(h, EM_GETSEL, 0, 0)?;
    let (start, end) = (sel & 0xFFFF, (sel >> 16) & 0xFFFF);
    if start >= end || end > buf.len() {
        return None;
    }
    let text = String::from_utf16_lossy(&buf[start..end]);
    if text.trim().is_empty() {
        return None;
    }
    let (from, to) = line_around(&buf, start, end);
    let surrounding = String::from_utf16_lossy(&buf[from..to]);

    // 选区起点坐标：Edit 控件的 EM_POSFROMCHAR 返回客户区坐标（低 16 位 x、高 16 位 y）
    let anchor = (class == "edit")
        .then(|| send(h, EM_POSFROMCHAR, start, 0))
        .flatten()
        .filter(|&p| p != usize::MAX & 0xFFFF_FFFF)
        .map(|p| {
            let mut pt = POINT { x: i32::from((p & 0xFFFF) as u16 as i16), y: i32::from(((p >> 16) & 0xFFFF) as u16 as i16) };
            // SAFETY: 坐标换算，写入本地 POINT。
            let _ = unsafe { ClientToScreen(h, &mut pt) };
            (f64::from(pt.x), f64::from(pt.y), 20.0)
        });

    Some(EditSelection { text, surrounding, sel16: (start - from, end - from), anchor })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_around_selection() {
        let u: Vec<u16> = "first line\r\nThey made ubiquitous tracking.\r\nlast".encode_utf16().collect();
        let s = "first line\r\nThey made ".encode_utf16().count();
        let (a, b) = line_around(&u, s, s + 10);
        // 行尾的 \r 留给断句预处理当空白处理
        assert_eq!(String::from_utf16_lossy(&u[a..b]), "They made ubiquitous tracking.\r");
        let long: Vec<u16> = "x".repeat(5000).encode_utf16().collect();
        let (a, b) = line_around(&long, 2500, 2505);
        assert_eq!((a, b), (1000, 4005));
    }
}
