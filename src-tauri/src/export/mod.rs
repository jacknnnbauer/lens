//! 单词导出（SPEC 11、M5）。批量导出流水线在 M7。
//!
//! 流程：主窗口组装导出文档 → 隐藏的 `export` 窗口渲染（等样式、图片、字体就绪并修正对比度）→
//! - PDF：该窗口调用 `print_pdf`，用 WebView2 的 `ICoreWebView2_7::PrintToPdf` 生成（文字可选可搜）
//! - HTML：该窗口把渲染结果（CSS 与图片已内联）序列化后调用 `save_html` 写盘

use std::path::Path;

use serde::Deserialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::error::{Error, Result};

pub const EXPORT: &str = "export";

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Paper {
    A4,
    Letter,
}

impl Paper {
    /// 纸张宽高（英寸），WebView2 打印设置以英寸为单位
    pub fn inches(self) -> (f64, f64) {
        match self {
            Paper::A4 => (8.27, 11.69),
            Paper::Letter => (8.5, 11.0),
        }
    }
}

/// 页边距（英寸），约 12.7mm
const MARGIN_IN: f64 = 0.5;

/// 确保导出窗口存在（首次导出时创建，之后常驻隐藏）。返回是否为新建。
pub fn ensure_window(app: &AppHandle) -> Result<bool> {
    if app.get_webview_window(EXPORT).is_some() {
        return Ok(false);
    }
    WebviewWindowBuilder::new(app, EXPORT, WebviewUrl::App("index.html".into()))
        .title("词镜 Lens · 导出")
        .inner_size(900.0, 1200.0)
        .visible(false)
        .skip_taskbar(true)
        .focused(false)
        .build()?;
    Ok(true)
}

/// 写 HTML 导出文件：只允许 .html / .htm，先写临时文件再改名。
pub fn save_html(path: &Path, html: &str) -> Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    if !matches!(ext.as_deref(), Some("html" | "htm")) {
        return Err(Error::Other("导出文件必须是 .html".into()));
    }
    let tmp = path.with_extension("html.part");
    std::fs::write(&tmp, html)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// 把 `window` 当前页面打印成 PDF（在 UI 线程上发起，完成回调后返回）。
#[cfg(windows)]
pub async fn print_pdf(window: &WebviewWindow, path: &Path, paper: Paper) -> Result<()> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2Environment6, ICoreWebView2_7};
    use webview2_com::PrintToPdfCompletedHandler;
    use windows::core::{Interface, HSTRING};

    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    if ext.as_deref() != Some("pdf") {
        return Err(Error::Other("导出文件必须是 .pdf".into()));
    }
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<()>>();
    let target = path.to_path_buf();
    let (w, h) = paper.inches();

    window.with_webview(move |pw| {
        // SAFETY: with_webview 在 UI 线程上执行；COM 对象由 windows-rs 管理引用计数。
        let started = unsafe {
            (|| -> windows::core::Result<()> {
                let core7: ICoreWebView2_7 = pw.controller().CoreWebView2()?.cast()?;
                let env6: ICoreWebView2Environment6 = pw.environment().cast()?;
                let settings = env6.CreatePrintSettings()?;
                settings.SetPageWidth(w)?;
                settings.SetPageHeight(h)?;
                settings.SetMarginTop(MARGIN_IN)?;
                settings.SetMarginBottom(MARGIN_IN)?;
                settings.SetMarginLeft(MARGIN_IN)?;
                settings.SetMarginRight(MARGIN_IN)?;
                settings.SetShouldPrintBackgrounds(true)?;
                settings.SetShouldPrintHeaderAndFooter(false)?;
                let mut tx = Some(tx);
                let handler = PrintToPdfCompletedHandler::create(Box::new(move |hr, ok| {
                    let result = match (hr, ok) {
                        (Ok(()), true) => Ok(()),
                        (Err(e), _) => Err(Error::Other(format!("生成 PDF 失败：{e}"))),
                        (Ok(()), false) => Err(Error::Other("生成 PDF 失败（文件可能被占用）".into())),
                    };
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(result);
                    }
                    Ok(())
                }));
                core7.PrintToPdf(&HSTRING::from(target.as_os_str()), &settings, &handler)
            })()
        };
        if let Err(e) = started {
            tracing::warn!(error = %e, "无法开始打印 PDF");
        }
    })?;

    match tokio::time::timeout(std::time::Duration::from_secs(120), rx).await {
        Ok(Ok(r)) => r,
        Ok(Err(_)) => Err(Error::Other("无法开始生成 PDF（WebView2 版本过旧？）".into())),
        Err(_) => Err(Error::Other("生成 PDF 超时".into())),
    }
}

#[cfg(not(windows))]
pub async fn print_pdf(_window: &WebviewWindow, _path: &Path, _paper: Paper) -> Result<()> {
    Err(Error::Other("当前平台不支持".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_export_requires_html_extension() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let ok = dir.path().join("w.html");
        save_html(&ok, "<p>x</p>")?;
        assert_eq!(std::fs::read_to_string(&ok)?, "<p>x</p>");
        assert!(save_html(&dir.path().join("w.exe"), "x").is_err());
        assert!(save_html(&dir.path().join("w"), "x").is_err());
        Ok(())
    }

    #[test]
    fn paper_sizes() {
        assert_eq!(Paper::A4.inches(), (8.27, 11.69));
        assert_eq!(Paper::Letter.inches().0, 8.5);
    }
}
