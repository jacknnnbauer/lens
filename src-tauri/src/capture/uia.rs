//! UI Automation 取词（SPEC 8.2）：焦点元素的 TextPattern → 选区 → 所在段落 → 选区在段落中的偏移。
//! 只在取词工作线程上使用（该线程已初始化 COM MTA）。

use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER, SAFEARRAY};
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound, SafeArrayUnaccessData};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationTextPattern, IUIAutomationTextRange,
    IUIAutomationValuePattern, TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character,
    TextUnit_Line, TextUnit_Paragraph, TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_EditControlTypeId,
    UIA_TextPatternId, UIA_ValuePatternId,
};

use super::win;
use crate::error::{Error, Result};

/// 段落超过这么长（如整篇文档就是一个"段落"）时，改取选区前后各 CONTEXT_CHARS 个字符
const MAX_PARAGRAPH: usize = 4000;
const CONTEXT_CHARS: i32 = 300;
const MAX_SELECTION: i32 = 2000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub text: String,
    /// 选区周边文本（段落 / 行 / 前后 300 字）
    pub surrounding: String,
    /// 选区在 surrounding 中的 UTF-16 区间
    pub sel16: (usize, usize),
    /// 选区的屏幕矩形（物理像素）
    pub rect: Option<Rect>,
}

pub struct Uia {
    uia: IUIAutomation,
    edit_condition: IUIAutomationCondition,
}

fn int_variant(n: i32) -> VARIANT {
    let mut v = VARIANT::default();
    // SAFETY: 写入 VARIANT 的类型标记与对应的 lVal 字段。
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = n;
    }
    v
}

fn bstr_text(r: &IUIAutomationTextRange, max: i32) -> Option<String> {
    // SAFETY: COM 调用；BSTR 由 windows-rs 管理释放。
    unsafe { r.GetText(max).ok().map(|b| b.to_string()) }
}

/// 读取 GetBoundingRectangles 返回的 SAFEARRAY<double>（x, y, w, h 依次排列），合并成一个外接矩形。
unsafe fn union_rect(sa: *mut SAFEARRAY) -> Option<Rect> {
    if sa.is_null() {
        return None;
    }
    let read = || -> Option<Vec<f64>> {
        let lb = SafeArrayGetLBound(sa, 1).ok()?;
        let ub = SafeArrayGetUBound(sa, 1).ok()?;
        let n = usize::try_from(ub - lb + 1).ok()?;
        let mut data = std::ptr::null_mut();
        SafeArrayAccessData(sa, &mut data).ok()?;
        let v = std::slice::from_raw_parts(data as *const f64, n).to_vec();
        let _ = SafeArrayUnaccessData(sa);
        Some(v)
    };
    let v = read();
    let _ = SafeArrayDestroy(sa);
    let v = v?;
    let mut it = v.chunks_exact(4).filter(|r| r[2] > 0.0 && r[3] > 0.0);
    let first = it.next()?;
    let (mut l, mut t, mut r, mut b) = (first[0], first[1], first[0] + first[2], first[1] + first[3]);
    for q in it {
        l = l.min(q[0]);
        t = t.min(q[1]);
        r = r.max(q[0] + q[2]);
        b = b.max(q[1] + q[3]);
    }
    Some(Rect { x: l, y: t, w: r - l, h: b - t })
}

impl Uia {
    /// 调用方须已在本线程初始化 COM。
    pub fn new() -> Result<Self> {
        // SAFETY: 标准 COM 实例化。
        unsafe {
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| Error::Other(format!("UI Automation 初始化失败：{e}")))?;
            let edit_condition = uia
                .CreatePropertyCondition(UIA_ControlTypePropertyId, &int_variant(UIA_EditControlTypeId.0))
                .map_err(|e| Error::Other(format!("UIA 条件创建失败：{e}")))?;
            Ok(Self { uia, edit_condition })
        }
    }

    /// 焦点控件里的选区及其周边文本。应用不支持 TextPattern 或没有选区时返回 None。
    pub fn selection(&self) -> Option<Selection> {
        // SAFETY: 全部是 COM 方法调用，失败都转为 None。
        unsafe {
            let el = self.uia.GetFocusedElement().ok()?;
            let tp: IUIAutomationTextPattern = el.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
            let ranges = tp.GetSelection().ok()?;
            if ranges.Length().ok()? < 1 {
                return None;
            }
            let sel = ranges.GetElement(0).ok()?;
            let text = bstr_text(&sel, MAX_SELECTION)?;
            if text.trim().is_empty() {
                return None;
            }
            let rect = sel.GetBoundingRectangles().ok().and_then(|sa| union_rect(sa));

            let around = self.surrounding(&sel, &text);
            let (surrounding, sel16) = around.unwrap_or_else(|| (text.clone(), (0, text.encode_utf16().count())));
            Some(Selection { text, surrounding, sel16, rect })
        }
    }

    /// 段落 → 行 → 前后 300 字，依次尝试；返回周边文本和选区偏移（并校验偏移处确实是选中的文本）。
    unsafe fn surrounding(&self, sel: &IUIAutomationTextRange, text: &str) -> Option<(String, (usize, usize))> {
        let sel_len16 = text.encode_utf16().count();
        let try_unit = |expand: &dyn Fn(&IUIAutomationTextRange) -> bool| -> Option<(String, (usize, usize))> {
            let range = sel.Clone().ok()?;
            if !expand(&range) {
                return None;
            }
            // 选区跨段时，确保范围覆盖整个选区
            if range.CompareEndpoints(TextPatternRangeEndpoint_End, sel, TextPatternRangeEndpoint_End).ok()? < 0 {
                range.MoveEndpointByRange(TextPatternRangeEndpoint_End, sel, TextPatternRangeEndpoint_End).ok()?;
            }
            if range.CompareEndpoints(TextPatternRangeEndpoint_Start, sel, TextPatternRangeEndpoint_Start).ok()? > 0 {
                range.MoveEndpointByRange(TextPatternRangeEndpoint_Start, sel, TextPatternRangeEndpoint_Start).ok()?;
            }
            let all = bstr_text(&range, -1)?;
            if all.chars().count() > MAX_PARAGRAPH {
                return None;
            }
            let prefix = range.Clone().ok()?;
            prefix.MoveEndpointByRange(TextPatternRangeEndpoint_End, sel, TextPatternRangeEndpoint_Start).ok()?;
            let start16 = bstr_text(&prefix, -1)?.encode_utf16().count();
            let units: Vec<u16> = all.encode_utf16().collect();
            let want: Vec<u16> = text.encode_utf16().collect();
            // 有些实现会把 \r\n 规范化，偏移对不上时在附近找
            let start16 = if units.get(start16..start16 + sel_len16) == Some(&want[..]) {
                start16
            } else {
                units.windows(want.len().max(1)).position(|w| w == &want[..])?
            };
            Some((all, (start16, start16 + sel_len16)))
        };

        try_unit(&|r| r.ExpandToEnclosingUnit(TextUnit_Paragraph).is_ok())
            .or_else(|| try_unit(&|r| r.ExpandToEnclosingUnit(TextUnit_Line).is_ok()))
            .or_else(|| {
                try_unit(&|r| {
                    r.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -CONTEXT_CHARS).is_ok()
                        && r.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, CONTEXT_CHARS).is_ok()
                })
            })
    }

    /// 浏览器地址栏里的 URL：窗口内第一个 Edit 控件的 Value。
    pub fn browser_url(&self, hwnd: isize) -> Option<String> {
        // SAFETY: COM 调用，失败转为 None。
        let value = unsafe {
            let root = self.uia.ElementFromHandle(win::hwnd(hwnd)).ok()?;
            let edit = root.FindFirst(TreeScope_Descendants, &self.edit_condition).ok()?;
            let vp: IUIAutomationValuePattern = edit.GetCurrentPatternAs(UIA_ValuePatternId).ok()?;
            vp.CurrentValue().ok()?.to_string()
        };
        normalize_url(&value)
    }
}

/// 地址栏常省略协议（example.com/a）：补上 https://；本地文件显示为 C:/… 时转成 file:///；
/// localhost 与 IP 保留原协议推断为 http。看起来不像网址的返回 None。
pub fn normalize_url(v: &str) -> Option<String> {
    let v = v.trim();
    if v.is_empty() || v.contains(char::is_whitespace) {
        return None;
    }
    if v.contains("://") || ["about:", "edge:", "chrome:", "file:"].iter().any(|p| v.starts_with(p)) {
        return Some(v.to_string());
    }
    let b = v.as_bytes();
    if b.len() > 2 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\') {
        return Some(format!("file:///{}", v.replace('\\', "/")));
    }
    let host = v.split('/').next().unwrap_or_default();
    let name = host.split(':').next().unwrap_or_default();
    if name == "localhost" || name.parse::<std::net::Ipv4Addr>().is_ok() {
        return Some(format!("http://{v}"));
    }
    (name.contains('.') && !name.starts_with('.')).then(|| format!("https://{v}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 诊断：4 秒后打印前台焦点元素的 UIA 信息与取词结果。
    /// `cargo test probe_focused -- --ignored --nocapture`，期间切到目标程序并选中文字。
    #[test]
    #[ignore]
    fn probe_focused() -> Result<()> {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        use windows::Win32::UI::Accessibility::UIA_TextPattern2Id;
        std::thread::sleep(std::time::Duration::from_secs(4));
        // SAFETY: 测试线程初始化 COM；其余为 COM 只读调用。
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let u = Uia::new()?;
            let fg = win::foreground();
            println!("foreground: {:?} / {:?}", fg.app, fg.title);
            match u.uia.GetFocusedElement() {
                Ok(el) => {
                    println!(
                        "focused: type={:?} class={:?} name={:?} framework={:?}",
                        el.CurrentControlType().map(|t| t.0),
                        el.CurrentClassName().map(|s| s.to_string()),
                        el.CurrentName().map(|s| s.to_string().chars().take(60).collect::<String>()),
                        el.CurrentFrameworkId().map(|s| s.to_string()),
                    );
                    let tp: windows::core::Result<IUIAutomationTextPattern> = el.GetCurrentPatternAs(UIA_TextPatternId);
                    println!("TextPattern: {}", tp.as_ref().map(|_| "yes".to_string()).unwrap_or_else(|e| format!("no ({e})")));
                    let tp2: windows::core::Result<IUIAutomationTextPattern> = el.GetCurrentPatternAs(UIA_TextPattern2Id);
                    println!("TextPattern2: {}", tp2.is_ok());
                    if let Ok(tp) = tp {
                        match tp.GetSelection() {
                            Ok(r) => println!("selection ranges: {:?}", r.Length()),
                            Err(e) => println!("GetSelection failed: {e}"),
                        }
                    }
                }
                Err(e) => println!("GetFocusedElement failed: {e}"),
            }
            println!("browser_url: {:?}", u.browser_url(fg.hwnd));
            if let Ok(root) = u.uia.ElementFromHandle(win::hwnd(fg.hwnd)) {
                if let Ok(edit) = root.FindFirst(TreeScope_Descendants, &u.edit_condition) {
                    let raw = edit.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId).and_then(|vp| vp.CurrentValue());
                    println!("first Edit: name={:?} value={:?}", edit.CurrentName().map(|s| s.to_string()), raw.map(|b| b.to_string()));
                }
            }
            match u.selection() {
                Some(s) => println!("selection: {:?} rect={:?}\nsurrounding: {:?}\nsel16={:?}", s.text, s.rect, s.surrounding.chars().take(200).collect::<String>(), s.sel16),
                None => println!("selection: None"),
            }
        }
        Ok(())
    }

    #[test]
    fn url_normalization() {
        assert_eq!(normalize_url("example.com/a?b=1").as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(normalize_url("https://x.org").as_deref(), Some("https://x.org"));
        assert_eq!(normalize_url("edge://settings").as_deref(), Some("edge://settings"));
        assert_eq!(normalize_url("search terms here"), None);
        assert_eq!(normalize_url("localhost:8765/a.html").as_deref(), Some("http://localhost:8765/a.html"));
        assert_eq!(normalize_url("127.0.0.1:8765/a").as_deref(), Some("http://127.0.0.1:8765/a"));
        assert_eq!(normalize_url("C:/Users/x/a.html").as_deref(), Some("file:///C:/Users/x/a.html"));
        assert_eq!(normalize_url("intranet"), None);
    }
}
