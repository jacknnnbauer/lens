//! 多格式写剪贴板（SPEC 10）：同时写入 CF_UNICODETEXT 与 CF_HTML。
//! CF_HTML 是 UTF-8，带一个记录各段字节偏移的头（StartHTML/EndHTML/StartFragment/EndFragment），
//! Word、OneNote 等按这个头取片段；偏移算错会粘贴成乱码或空白，所以单独写成纯函数测试。

use crate::error::{Error, Result};

const START_MARK: &str = "<!--StartFragment-->";
const END_MARK: &str = "<!--EndFragment-->";

/// 按 CF_HTML 规范把 HTML 片段包成完整的剪贴板数据。偏移都是相对数据开头的 UTF-8 字节数。
pub fn cf_html(fragment: &str, source_url: Option<&str>) -> String {
    // 头里的数字固定 10 位，先占位、算好偏移再填，长度不会变
    let header_tpl = |sh: usize, eh: usize, sf: usize, ef: usize| {
        let mut h = format!(
            "Version:0.9\r\nStartHTML:{sh:010}\r\nEndHTML:{eh:010}\r\nStartFragment:{sf:010}\r\nEndFragment:{ef:010}\r\n"
        );
        if let Some(u) = source_url {
            h.push_str(&format!("SourceURL:{u}\r\n"));
        }
        h
    };
    let header_len = header_tpl(0, 0, 0, 0).len();
    let prefix = format!("<html><head><meta charset=\"utf-8\"></head><body>{START_MARK}");
    let suffix = format!("{END_MARK}</body></html>");

    let start_html = header_len;
    let start_fragment = start_html + prefix.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + suffix.len();
    format!("{}{prefix}{fragment}{suffix}", header_tpl(start_html, end_html, start_fragment, end_fragment))
}

/// 写入纯文本，以及可选的 HTML。
#[cfg(windows)]
pub fn write(text: &str, html: Option<&str>) -> Result<()> {
    use windows::core::w;
    use windows::Win32::Foundation::{GlobalFree, HANDLE};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    /// 分配全局内存并拷入数据；交给 SetClipboardData 后所有权归系统。
    unsafe fn global(bytes: &[u8]) -> Result<HANDLE> {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|e| Error::Other(format!("分配剪贴板内存失败：{e}")))?;
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            let _ = GlobalFree(Some(h));
            return Err(Error::Other("锁定剪贴板内存失败".into()));
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        let _ = GlobalUnlock(h);
        Ok(HANDLE(h.0))
    }

    let mut utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    utf16.extend_from_slice(&[0, 0]);
    let html_bytes = html.map(|h| {
        let mut b = cf_html(h, None).into_bytes();
        b.push(0);
        b
    });

    // 剪贴板可能正被别的程序占用，重试几次
    let mut opened = false;
    for _ in 0..10 {
        // SAFETY: 打开成功后在本函数内一定 CloseClipboard。
        if unsafe { OpenClipboard(None) }.is_ok() {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    if !opened {
        return Err(Error::Other("剪贴板正被其他程序占用".into()));
    }
    // SAFETY: 剪贴板已打开；SetClipboardData 成功后内存归系统，失败时由系统不接管，泄漏极小且罕见。
    let result = unsafe {
        (|| -> Result<()> {
            EmptyClipboard().map_err(|e| Error::Other(format!("清空剪贴板失败：{e}")))?;
            SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(global(&utf16)?))
                .map_err(|e| Error::Other(format!("写入文本失败：{e}")))?;
            if let Some(b) = &html_bytes {
                let fmt = RegisterClipboardFormatW(w!("HTML Format"));
                SetClipboardData(fmt, Some(global(b)?)).map_err(|e| Error::Other(format!("写入 HTML 失败：{e}")))?;
            }
            Ok(())
        })()
    };
    // SAFETY: 与上面的 OpenClipboard 配对。
    let _ = unsafe { CloseClipboard() };
    result
}

#[cfg(not(windows))]
pub fn write(_text: &str, _html: Option<&str>) -> Result<()> {
    Err(Error::Other("当前平台不支持".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(data: &str, name: &str) -> usize {
        let line = data.lines().find(|l| l.starts_with(name)).expect(name);
        line[name.len() + 1..].trim().parse().expect("number")
    }

    #[test]
    fn offsets_point_at_the_right_bytes() {
        // 含多字节字符，偏移必须按 UTF-8 字节算
        let frag = "<p><b>ubiquitous</b> 无处不在的</p>";
        let d = cf_html(frag, Some("https://example.com/a"));
        let (sh, eh, sf, ef) = (field(&d, "StartHTML"), field(&d, "EndHTML"), field(&d, "StartFragment"), field(&d, "EndFragment"));
        assert_eq!(&d[sf..ef], frag);
        assert!(d[sh..].starts_with("<html>"));
        assert_eq!(eh, d.len());
        assert!(d[..sf].ends_with(START_MARK));
        assert!(d[ef..].starts_with(END_MARK));
        assert!(d.contains("SourceURL:https://example.com/a\r\n"));
    }

    #[test]
    fn header_numbers_are_fixed_width() {
        let d = cf_html("x", None);
        assert!(d.starts_with("Version:0.9\r\nStartHTML:0000000"));
        assert_eq!(field(&d, "EndFragment") - field(&d, "StartFragment"), 1);
    }
}
